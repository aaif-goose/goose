use std::path::PathBuf;

use crate::routes::errors::ErrorResponse;
use crate::state::AppState;
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use futures::future::join_all;
use goose::config::paths::Paths;
use goose::download_manager::{get_download_manager, DownloadProgress};
use goose::providers::local_inference::hf_models::{self, HfModelInfo, HfQuantVariant};
use goose::providers::local_inference::{
    available_inference_memory_bytes,
    hf_models::{resolve_model_spec, resolve_model_spec_full, HfGgufFile},
    local_model_registry::{
        default_settings_for_model, featured_mmproj_spec, get_registry, is_featured_model,
        model_id_from_repo, LocalModelEntry, ModelDownloadStatus as RegistryDownloadStatus,
        ModelSettings, ShardFile, FEATURED_MODELS,
    },
    recommend_local_model,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "state")]
pub enum ModelDownloadStatus {
    NotDownloaded,
    Downloading {
        progress_percent: f32,
        bytes_downloaded: u64,
        total_bytes: u64,
        speed_bps: Option<u64>,
    },
    Downloaded,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LocalModelResponse {
    pub id: String,
    pub repo_id: String,
    pub filename: String,
    pub quantization: String,
    pub size_bytes: u64,
    pub status: ModelDownloadStatus,
    pub recommended: bool,
    pub settings: ModelSettings,
    pub vision_capable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mmproj_status: Option<ModelDownloadStatus>,
}

async fn ensure_featured_models_in_registry() -> Result<(), ErrorResponse> {
    let mut mmproj_downloads_needed: Vec<(String, String, PathBuf)> = Vec::new();

    struct PendingResolve {
        spec: &'static str,
        repo_id: String,
        quantization: String,
        model_id: String,
    }
    let mut to_resolve = Vec::new();

    for featured in FEATURED_MODELS {
        let (repo_id, quantization) = match hf_models::parse_model_spec(featured.spec) {
            Ok(parts) => parts,
            Err(_) => continue,
        };

        let model_id = model_id_from_repo(&repo_id, &quantization);

        {
            let registry = get_registry()
                .lock()
                .map_err(|_| ErrorResponse::internal("Failed to acquire registry lock"))?;
            if let Some(existing) = registry.get_model(&model_id) {
                let needs_backfill = existing.mmproj_path.is_none() && featured.mmproj.is_some();
                let needs_download = existing.is_downloaded()
                    && featured.mmproj.is_some()
                    && !existing.mmproj_path.as_ref().is_some_and(|p| p.exists());

                if needs_download {
                    if let Some(mmproj) = featured.mmproj.as_ref() {
                        let path = mmproj.local_path();
                        let url = format!(
                            "https://huggingface.co/{}/resolve/main/{}",
                            mmproj.repo, mmproj.filename
                        );
                        mmproj_downloads_needed.push((model_id.clone(), url, path));
                    }
                }

                if !needs_backfill {
                    continue;
                }
                // Fall through to resolve for backfill
            }
        }

        to_resolve.push(PendingResolve {
            spec: featured.spec,
            repo_id,
            quantization,
            model_id,
        });
    }

    let resolved: Vec<(PendingResolve, HfGgufFile)> =
        join_all(to_resolve.into_iter().map(|pending| async move {
            let hf_file = match resolve_model_spec(pending.spec).await {
                Ok((_repo, file)) => file,
                Err(_) => {
                    let filename = format!(
                        "{}-{}.gguf",
                        pending.repo_id.split('/').next_back().unwrap_or("model"),
                        pending.quantization
                    );
                    HfGgufFile {
                        filename: filename.clone(),
                        size_bytes: 0,
                        quantization: pending.quantization.to_string(),
                        download_url: format!(
                            "https://huggingface.co/{}/resolve/main/{}",
                            pending.repo_id, filename
                        ),
                    }
                }
            };
            (pending, hf_file)
        }))
        .await;

    let entries_to_add: Vec<LocalModelEntry> = resolved
        .into_iter()
        .map(|(pending, hf_file)| {
            let local_path = Paths::in_data_dir("models").join(&hf_file.filename);
            let settings = default_settings_for_model(&pending.model_id);
            LocalModelEntry {
                id: pending.model_id,
                repo_id: pending.repo_id,
                filename: hf_file.filename,
                quantization: pending.quantization,
                local_path,
                source_url: hf_file.download_url,
                settings,
                size_bytes: hf_file.size_bytes,
                mmproj_path: None,
                mmproj_source_url: None,
                mmproj_size_bytes: 0,
                shard_files: vec![],
            }
        })
        .collect();

    {
        let mut registry = get_registry()
            .lock()
            .map_err(|_| ErrorResponse::internal("Failed to acquire registry lock"))?;

        if !entries_to_add.is_empty() {
            registry.sync_with_featured(entries_to_add);
        }

        // Backfill mmproj data for all registry models and collect any
        // needed mmproj downloads for models already on disk.
        for model in registry.list_models_mut() {
            model.enrich_with_featured_mmproj();
            if model.is_downloaded() {
                if let Some(mmproj) = featured_mmproj_spec(&model.id) {
                    let path = mmproj.local_path();
                    if !path.exists() {
                        let url = format!(
                            "https://huggingface.co/{}/resolve/main/{}",
                            mmproj.repo, mmproj.filename
                        );
                        mmproj_downloads_needed.push((model.id.clone(), url, path));
                    }
                }
            }
        }
        let _ = registry.save();
    }

    // Auto-download mmproj files for models that are already downloaded.
    // Deduplicate by path since multiple quants share one mmproj file.
    let dm = get_download_manager();
    let mut started_paths = std::collections::HashSet::new();
    for (model_id, url, path) in mmproj_downloads_needed {
        if !path.exists() && started_paths.insert(path.clone()) {
            let download_id = format!("{}-mmproj", model_id);
            let dominated_by_active = dm
                .get_progress(&download_id)
                .is_some_and(|p| p.status == goose::download_manager::DownloadStatus::Downloading);
            if !dominated_by_active {
                tracing::info!(model_id = %model_id, "Auto-downloading vision encoder for existing model");
                if let Err(e) = dm.download_model(download_id, url, path, None).await {
                    tracing::warn!(model_id = %model_id, error = %e, "Failed to start mmproj download");
                }
            }
        }
    }

    Ok(())
}

#[utoipa::path(
    post,
    path = "/local-inference/sync-featured",
    responses(
        (status = 200, description = "Featured models synced to registry")
    )
)]
pub async fn sync_featured_models() -> Result<StatusCode, ErrorResponse> {
    ensure_featured_models_in_registry().await?;
    Ok(StatusCode::OK)
}

#[utoipa::path(
    get,
    path = "/local-inference/models",
    responses(
        (status = 200, description = "List of available local LLM models", body = Vec<LocalModelResponse>)
    )
)]
pub async fn list_local_models(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Result<Json<Vec<LocalModelResponse>>, ErrorResponse> {
    let runtime = state.get_inference_runtime()?;
    let recommended_id = recommend_local_model(&runtime);

    let registry = get_registry()
        .lock()
        .map_err(|_| ErrorResponse::internal("Failed to acquire registry lock"))?;

    let mut models: Vec<LocalModelResponse> = Vec::new();

    for entry in registry.list_models() {
        let goose_status = entry.download_status();

        let status = match goose_status {
            RegistryDownloadStatus::NotDownloaded => ModelDownloadStatus::NotDownloaded,
            RegistryDownloadStatus::Downloading {
                progress_percent,
                bytes_downloaded,
                total_bytes,
                speed_bps,
            } => ModelDownloadStatus::Downloading {
                progress_percent,
                bytes_downloaded,
                total_bytes,
                speed_bps: Some(speed_bps),
            },
            RegistryDownloadStatus::Downloaded => ModelDownloadStatus::Downloaded,
        };

        let size_bytes = entry.file_size();

        let vision_capable = entry.settings.vision_capable;
        let mmproj_status = if vision_capable {
            let ms = entry.mmproj_download_status();
            Some(match ms {
                RegistryDownloadStatus::NotDownloaded => ModelDownloadStatus::NotDownloaded,
                RegistryDownloadStatus::Downloading {
                    progress_percent,
                    bytes_downloaded,
                    total_bytes,
                    speed_bps,
                } => ModelDownloadStatus::Downloading {
                    progress_percent,
                    bytes_downloaded,
                    total_bytes,
                    speed_bps: Some(speed_bps),
                },
                RegistryDownloadStatus::Downloaded => ModelDownloadStatus::Downloaded,
            })
        } else {
            None
        };

        models.push(LocalModelResponse {
            id: entry.id.clone(),
            repo_id: entry.repo_id.clone(),
            filename: entry.filename.clone(),
            quantization: entry.quantization.clone(),
            size_bytes,
            status,
            recommended: recommended_id == entry.id,
            settings: entry.settings.clone(),
            vision_capable,
            mmproj_status,
        });
    }

    models.sort_by(|a, b| {
        let a_downloaded = matches!(a.status, ModelDownloadStatus::Downloaded);
        let b_downloaded = matches!(b.status, ModelDownloadStatus::Downloaded);
        match (b_downloaded, a_downloaded) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => a.id.cmp(&b.id),
        }
    });

    Ok(Json(models))
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RepoVariantsResponse {
    pub variants: Vec<HfQuantVariant>,
    pub recommended_index: Option<usize>,
    pub available_memory_bytes: u64,
    pub downloaded_quants: Vec<String>,
}

#[utoipa::path(
    get,
    path = "/local-inference/search",
    params(
        ("q" = String, Query, description = "Search query"),
        ("limit" = Option<usize>, Query, description = "Max results")
    ),
    responses(
        (status = 200, description = "Search results", body = Vec<HfModelInfo>),
        (status = 500, description = "Search failed")
    )
)]
pub async fn search_hf_models(
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<HfModelInfo>>, ErrorResponse> {
    let limit = params.limit.unwrap_or(20).min(50);
    let results = hf_models::search_gguf_models(&params.q, limit)
        .await
        .map_err(|e| ErrorResponse::internal(format!("Search failed: {}", e)))?;
    Ok(Json(results))
}

#[utoipa::path(
    get,
    path = "/local-inference/repo/{author}/{repo}/files",
    responses(
        (status = 200, description = "GGUF files in the repo", body = RepoVariantsResponse)
    )
)]
pub async fn get_repo_files(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Path((author, repo)): Path<(String, String)>,
) -> Result<Json<RepoVariantsResponse>, ErrorResponse> {
    let repo_id = format!("{}/{}", author, repo);
    let variants = hf_models::get_repo_gguf_variants(&repo_id)
        .await
        .map_err(|e| ErrorResponse::internal(format!("Failed to fetch repo files: {}", e)))?;

    let runtime = state.get_inference_runtime()?;
    let available_memory = available_inference_memory_bytes(&runtime);
    let recommended_index = hf_models::recommend_variant(&variants, available_memory);

    let downloaded_quants = {
        let registry = get_registry()
            .lock()
            .map_err(|_| ErrorResponse::internal("Failed to acquire registry lock"))?;
        registry
            .list_models()
            .iter()
            .filter(|m| m.repo_id == repo_id && m.is_downloaded())
            .map(|m| m.quantization.clone())
            .collect()
    };

    Ok(Json(RepoVariantsResponse {
        variants,
        recommended_index,
        available_memory_bytes: available_memory,
        downloaded_quants,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DownloadModelRequest {
    /// Model spec like "bartowski/Llama-3.2-3B-Instruct-GGUF:Q4_K_M"
    pub spec: String,
}

#[utoipa::path(
    post,
    path = "/local-inference/download",
    request_body = DownloadModelRequest,
    responses(
        (status = 202, description = "Download started", body = String),
        (status = 400, description = "Invalid request")
    )
)]
pub async fn download_hf_model(
    Json(req): Json<DownloadModelRequest>,
) -> Result<(StatusCode, Json<String>), ErrorResponse> {
    let (repo_id, quantization) = hf_models::parse_model_spec(&req.spec)
        .map_err(|e| ErrorResponse::bad_request(format!("Invalid spec format: {e}")))?;

    let (_repo, resolved) = resolve_model_spec_full(&req.spec)
        .await
        .map_err(|e| ErrorResponse::bad_request(format!("Invalid spec: {}", e)))?;

    let model_id = model_id_from_repo(&repo_id, &quantization);
    let models_dir = Paths::in_data_dir("models");
    let first_file = &resolved.files[0];
    let first_local_path = models_dir.join(&first_file.filename);

    let shard_files: Vec<ShardFile> = if resolved.files.len() > 1 {
        resolved
            .files
            .iter()
            .skip(1)
            .map(|f| ShardFile {
                filename: f.filename.clone(),
                local_path: models_dir.join(&f.filename),
                source_url: f.download_url.clone(),
                size_bytes: f.size_bytes,
            })
            .collect()
    } else {
        vec![]
    };

    let entry = LocalModelEntry {
        id: model_id.clone(),
        repo_id,
        filename: first_file.filename.clone(),
        quantization,
        local_path: first_local_path.clone(),
        source_url: first_file.download_url.clone(),
        settings: default_settings_for_model(&model_id),
        size_bytes: resolved.total_size,
        mmproj_path: None,
        mmproj_source_url: None,
        mmproj_size_bytes: 0,
        shard_files: shard_files.clone(),
    };

    // add_model enriches the entry with mmproj metadata from the featured table
    let mmproj_path = {
        let mut registry = get_registry()
            .lock()
            .map_err(|_| ErrorResponse::internal("Failed to acquire registry lock"))?;
        registry
            .add_model(entry)
            .map_err(|e| ErrorResponse::internal(format!("{}", e)))?;
        registry.get_model(&model_id).and_then(|e| {
            e.mmproj_path
                .as_ref()
                .zip(e.mmproj_source_url.as_ref())
                .map(|(p, u)| (p.clone(), u.clone()))
        })
    };

    let dm = get_download_manager();
    let all_files: Vec<(String, std::path::PathBuf)> = resolved
        .files
        .iter()
        .map(|f| (f.download_url.clone(), models_dir.join(&f.filename)))
        .collect();

    dm.download_model_sharded(
        format!("{}-model", model_id),
        all_files,
        resolved.total_size,
        None,
    )
    .await
    .map_err(|e| ErrorResponse::internal(format!("Download failed: {}", e)))?;

    if let Some((mmproj_path, mmproj_url)) = mmproj_path {
        if !mmproj_path.exists() {
            dm.download_model(
                format!("{}-mmproj", model_id),
                mmproj_url,
                mmproj_path,
                None,
            )
            .await
            .map_err(|e| ErrorResponse::internal(format!("mmproj download failed: {}", e)))?;
        }
    }

    Ok((StatusCode::ACCEPTED, Json(model_id)))
}

#[utoipa::path(
    get,
    path = "/local-inference/models/{model_id}/download",
    responses(
        (status = 200, description = "Download progress", body = DownloadProgress),
        (status = 404, description = "No active download")
    )
)]
pub async fn get_local_model_download_progress(
    Path(model_id): Path<String>,
) -> Result<Json<DownloadProgress>, ErrorResponse> {
    let download_id = format!("{}-model", model_id);
    debug!(model_id = %model_id, download_id = %download_id, "Getting download progress");

    let manager = get_download_manager();

    let model_progress = manager
        .get_progress(&download_id)
        .ok_or_else(|| ErrorResponse::not_found("No active download"))?;

    Ok(Json(model_progress))
}

#[utoipa::path(
    delete,
    path = "/local-inference/models/{model_id}/download",
    responses(
        (status = 200, description = "Download cancelled"),
        (status = 404, description = "No active download")
    )
)]
pub async fn cancel_local_model_download(
    Path(model_id): Path<String>,
) -> Result<StatusCode, ErrorResponse> {
    let manager = get_download_manager();
    manager
        .cancel_download(&format!("{}-model", model_id))
        .map_err(|e| ErrorResponse::internal(format!("{}", e)))?;
    let _ = manager.cancel_download(&format!("{}-mmproj", model_id));

    Ok(StatusCode::OK)
}

#[utoipa::path(
    delete,
    path = "/local-inference/models/{model_id}",
    responses(
        (status = 200, description = "Model deleted"),
        (status = 404, description = "Model not found")
    )
)]
pub async fn delete_local_model(Path(model_id): Path<String>) -> Result<StatusCode, ErrorResponse> {
    let (all_paths, primary_path, mmproj_path, other_uses_mmproj) = {
        let registry = get_registry()
            .lock()
            .map_err(|_| ErrorResponse::internal("Failed to acquire registry lock"))?;
        let entry = registry
            .get_model(&model_id)
            .ok_or_else(|| ErrorResponse::not_found("Model not found"))?;
        let paths: Vec<std::path::PathBuf> =
            entry.all_local_paths().map(|p| p.to_path_buf()).collect();
        let primary = entry.local_path.clone();
        let mp = entry.mmproj_path.clone();
        let shared = mp.as_ref().is_some_and(|target| {
            registry.list_models().iter().any(|m| {
                m.id != model_id && m.is_downloaded() && m.mmproj_path.as_ref() == Some(target)
            })
        });
        (paths, primary, mp, shared)
    };

    for path in &all_paths {
        if path.exists() {
            tokio::fs::remove_file(path)
                .await
                .map_err(|e| ErrorResponse::internal(format!("Failed to delete: {}", e)))?;
        }
    }

    // Clean up empty parent directories (e.g. BF16/ subdirectory)
    if let Some(parent) = primary_path.parent() {
        let models_dir = Paths::in_data_dir("models");
        if parent != models_dir {
            let _ = tokio::fs::remove_dir(parent).await;
        }
    }

    if !other_uses_mmproj {
        if let Some(mmproj) = mmproj_path {
            if mmproj.exists() {
                let _ = tokio::fs::remove_file(&mmproj).await;
            }
        }
    }

    // Only remove non-featured models from registry (featured ones stay as placeholders)
    if !is_featured_model(&model_id) {
        let mut registry = get_registry()
            .lock()
            .map_err(|_| ErrorResponse::internal("Failed to acquire registry lock"))?;
        registry
            .remove_model(&model_id)
            .map_err(|e| ErrorResponse::internal(format!("{}", e)))?;
    }

    Ok(StatusCode::OK)
}

#[utoipa::path(
    get,
    path = "/local-inference/models/{model_id}/settings",
    responses(
        (status = 200, description = "Model settings", body = ModelSettings),
        (status = 404, description = "Model not found")
    )
)]
pub async fn get_model_settings(
    Path(model_id): Path<String>,
) -> Result<Json<ModelSettings>, ErrorResponse> {
    let registry = get_registry()
        .lock()
        .map_err(|_| ErrorResponse::internal("Failed to acquire registry lock"))?;

    if let Some(settings) = registry.get_model_settings(&model_id) {
        return Ok(Json(settings.clone()));
    }

    Err(ErrorResponse::not_found("Model not found"))
}

#[utoipa::path(
    put,
    path = "/local-inference/models/{model_id}/settings",
    request_body = ModelSettings,
    responses(
        (status = 200, description = "Settings updated", body = ModelSettings),
        (status = 404, description = "Model not found"),
        (status = 500, description = "Failed to save settings")
    )
)]
pub async fn update_model_settings(
    Path(model_id): Path<String>,
    Json(settings): Json<ModelSettings>,
) -> Result<Json<ModelSettings>, ErrorResponse> {
    let mut registry = get_registry()
        .lock()
        .map_err(|_| ErrorResponse::internal("Failed to acquire registry lock"))?;

    registry
        .update_model_settings(&model_id, settings.clone())
        .map_err(|e| ErrorResponse::not_found(format!("{}", e)))?;

    Ok(Json(settings))
}

pub fn routes(state: Arc<AppState>) -> Router {
    let registered_paths: std::collections::HashSet<std::path::PathBuf> = get_registry()
        .lock()
        .map(|reg| {
            reg.list_models()
                .iter()
                .flat_map(|m| {
                    m.all_local_paths()
                        .map(|p| p.to_path_buf())
                        .chain(m.mmproj_path.as_deref().map(|p| p.to_path_buf()))
                })
                .collect()
        })
        .unwrap_or_default();
    goose::download_manager::cleanup_partial_downloads(
        &Paths::in_data_dir("models"),
        &registered_paths,
    );

    Router::new()
        .route("/local-inference/models", get(list_local_models))
        .route("/local-inference/sync-featured", post(sync_featured_models))
        .route("/local-inference/search", get(search_hf_models))
        .route(
            "/local-inference/repo/{author}/{repo}/files",
            get(get_repo_files),
        )
        .route("/local-inference/download", post(download_hf_model))
        .route(
            "/local-inference/models/{model_id}/download",
            get(get_local_model_download_progress),
        )
        .route(
            "/local-inference/models/{model_id}/download",
            delete(cancel_local_model_download),
        )
        .route(
            "/local-inference/models/{model_id}",
            delete(delete_local_model),
        )
        .route(
            "/local-inference/models/{model_id}/settings",
            get(get_model_settings),
        )
        .route(
            "/local-inference/models/{model_id}/settings",
            axum::routing::put(update_model_settings),
        )
        .with_state(state)
}
