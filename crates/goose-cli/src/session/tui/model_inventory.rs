use super::model_picker::{
    MetadataProvenance, MetadataValue, ModelAvailability, ModelDownloadSpec, ModelInventoryEntry,
    ModelMetadata, ModelSelection, ModelSource,
};
use anyhow::Result;
use goose::config::Config;
use goose::providers::base::ModelInfo;
use goose::providers::provider_registry::ProviderEntry;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct ModelInventory {
    pub entries: Vec<ModelInventoryEntry>,
    pub diagnostics: Vec<String>,
}

struct ConfiguredModels {
    provider: String,
    default_model: String,
    known_models: Vec<ModelInfo>,
    installed_models: Result<Vec<String>, String>,
}

struct LocalModel {
    id: String,
    size_bytes: u64,
    vision_capable: bool,
}

struct CatalogModel {
    repo_id: String,
    model_name: String,
    variants: Vec<CatalogVariant>,
}

struct CatalogVariant {
    backend_id: String,
    variant_id: String,
    label: String,
    size_bytes: Option<u64>,
}

pub(super) async fn catalog_variant_fit(
    selection: &ModelSelection,
) -> Result<super::model_fit::ModelFitAssessment> {
    #[cfg(feature = "local-inference")]
    {
        let download = selection
            .download
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Select a catalog variant to inspect memory fit"))?;
        let response = goose::providers::local_inference::management::huggingface_repo_variants(
            download.spec.clone(),
        )
        .await?;
        let variants = response
            .variants
            .into_iter()
            .map(|variant| CatalogVariant {
                backend_id: variant.backend_id,
                variant_id: variant.variant_id,
                label: variant.label,
                size_bytes: (variant.size_bytes > 0).then_some(variant.size_bytes),
            })
            .collect::<Vec<_>>();
        Ok(catalog_variant_fit_from_data(
            &download.backend_id,
            &download.variant_id,
            &variants,
            response.recommended_index,
            response.available_memory_bytes,
        ))
    }
    #[cfg(not(feature = "local-inference"))]
    {
        let _ = selection;
        Ok(super::model_fit::assess_model_fit(None, None, None, false))
    }
}

fn catalog_variant_fit_from_data(
    backend_id: &str,
    variant_id: &str,
    variants: &[CatalogVariant],
    recommended_index: Option<usize>,
    available_memory_bytes: u64,
) -> super::model_fit::ModelFitAssessment {
    use super::model_fit::assess_model_fit;

    if backend_id != "llamacpp" || available_memory_bytes == 0 {
        return assess_model_fit(None, None, None, false);
    }

    let candidates = variants
        .iter()
        .filter(|variant| variant.backend_id == "llamacpp")
        .collect::<Vec<_>>();
    let Some(selected_index) = candidates
        .iter()
        .position(|variant| variant.variant_id == variant_id)
    else {
        return assess_model_fit(None, None, None, false);
    };
    let selected = candidates[selected_index];
    let recommended = recommended_index.map(|index| index == selected_index);
    let backend_no_fit = !candidates.is_empty() && recommended_index.is_none();
    assess_model_fit(
        selected.size_bytes,
        Some(available_memory_bytes),
        recommended,
        backend_no_fit,
    )
}

#[derive(Clone, Debug)]
pub(super) struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub progress_percent: f32,
    pub speed_bps: Option<u64>,
    pub eta_seconds: Option<u64>,
    pub error: Option<String>,
}

pub(super) async fn load_model_inventory(
    config: &Config,
    active_provider: &str,
    active_model: &str,
) -> Result<ModelInventory> {
    let providers = config
        .get_param::<HashMap<String, goose::config::ProviderEntry>>("providers")
        .unwrap_or_default();
    let active = Some(ModelSelection::new(active_provider, active_model));
    let mut configured = Vec::new();
    let mut diagnostics = Vec::new();

    for (provider, provider_config) in providers {
        if !provider_config.configured || provider_config.model.trim().is_empty() {
            continue;
        }
        let registry_entry = match goose::providers::get_from_registry(&provider).await {
            Ok(entry) => entry,
            Err(error) => {
                diagnostics.push(format!("{provider}: {error}"));
                continue;
            }
        };
        let known_models = registry_entry.metadata().known_models.clone();
        let installed_models = if provider.eq_ignore_ascii_case("ollama") {
            list_ollama_models(&registry_entry).await
        } else {
            Ok(Vec::new())
        };
        configured.push(ConfiguredModels {
            provider,
            default_model: provider_config.model,
            known_models,
            installed_models,
        });
    }

    #[cfg(feature = "local-inference")]
    let local_models = match goose::providers::local_inference::management::list_models().await {
        Ok(response) => response
            .models
            .into_iter()
            .map(|model| LocalModel {
                id: model.id,
                size_bytes: model.size_bytes,
                vision_capable: model.vision_capable,
            })
            .collect(),
        Err(error) => {
            diagnostics.push(format!("Goose local models: {error}"));
            Vec::new()
        }
    };
    #[cfg(not(feature = "local-inference"))]
    let local_models = Vec::new();

    let mut inventory = inventory_from_sources(configured, local_models, active);
    if !inventory.entries.iter().any(|entry| {
        entry.provider.eq_ignore_ascii_case(active_provider)
            && entry.model.eq_ignore_ascii_case(active_model)
    }) {
        inventory.entries.push(entry_from_model_info(
            active_provider,
            active_model,
            ModelSource::Configured,
            ModelAvailability::Configured,
            None,
            Some(&ModelSelection::new(active_provider, active_model)),
            false,
        ));
    }
    inventory.diagnostics = diagnostics;
    Ok(inventory)
}

pub(super) async fn browse_local_catalog(query: &str) -> Result<Vec<ModelInventoryEntry>> {
    #[cfg(feature = "local-inference")]
    {
        let response = goose::providers::local_inference::management::search_huggingface_models(
            query.to_string(),
            Some(20),
        )
        .await?;
        let models = response
            .models
            .into_iter()
            .map(|model| CatalogModel {
                repo_id: model.repo_id,
                model_name: model.model_name,
                variants: model
                    .variants
                    .into_iter()
                    .filter(|variant| variant.supported)
                    .map(|variant| CatalogVariant {
                        backend_id: variant.backend_id,
                        variant_id: variant.variant_id,
                        label: variant.label,
                        size_bytes: (variant.size_bytes > 0).then_some(variant.size_bytes),
                    })
                    .collect(),
            })
            .collect();
        Ok(catalog_entries(models))
    }
    #[cfg(not(feature = "local-inference"))]
    {
        let _ = query;
        anyhow::bail!("The local model catalog requires Goose local inference support")
    }
}

pub(super) async fn download_model(
    selection: &ModelSelection,
    progress: impl Fn(&DownloadProgress),
    cancel: tokio_util::sync::CancellationToken,
) -> Result<String> {
    #[cfg(feature = "local-inference")]
    {
        let download = selection
            .download
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Select a catalog model before downloading"))?;
        let response = goose::providers::local_inference::management::download_model(
            goose_sdk_types::custom_requests::LocalInferenceModelDownloadRequest {
                spec: download.spec.clone(),
                backend_id: Some(download.backend_id.clone()),
                variant_id: Some(download.variant_id.clone()),
            },
        )
        .await?;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    goose::providers::local_inference::management::cancel_download(&response.model_id)?;
                    anyhow::bail!("Model download cancelled");
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(300)) => {}
            }

            let state = goose::providers::local_inference::management::download_progress(
                &response.model_id,
            )?
            .ok_or_else(|| anyhow::anyhow!("Model download status is unavailable"))?;
            let status = state.status.clone();
            let update = DownloadProgress {
                bytes_downloaded: state.bytes_downloaded,
                total_bytes: state.total_bytes,
                progress_percent: state.progress_percent,
                speed_bps: state.speed_bps,
                eta_seconds: state.eta_seconds,
                error: state.error,
            };
            progress(&update);

            match status.as_str() {
                "completed" => return Ok(response.model_id),
                "failed" => anyhow::bail!(
                    "{}",
                    update
                        .error
                        .unwrap_or_else(|| "Model download failed".into())
                ),
                "cancelled" => anyhow::bail!("Model download cancelled"),
                _ => {}
            }
        }
    }
    #[cfg(not(feature = "local-inference"))]
    {
        let _ = (selection, progress, cancel);
        anyhow::bail!("Model downloads require Goose local inference support")
    }
}

fn catalog_entries(models: Vec<CatalogModel>) -> Vec<ModelInventoryEntry> {
    models
        .into_iter()
        .flat_map(|model| {
            model.variants.into_iter().map(move |variant| {
                let mut entry = entry_from_model_info(
                    "local",
                    &format!("{} · {}", model.model_name, variant.label),
                    ModelSource::Catalog,
                    ModelAvailability::Downloadable,
                    None,
                    None,
                    false,
                );
                entry.metadata.size_bytes = variant.size_bytes.map(|value| MetadataValue {
                    value,
                    provenance: MetadataProvenance::Catalog,
                });
                entry.download = Some(ModelDownloadSpec {
                    spec: model.repo_id.clone(),
                    backend_id: variant.backend_id,
                    variant_id: variant.variant_id,
                });
                entry
            })
        })
        .collect()
}

async fn list_ollama_models(entry: &ProviderEntry) -> Result<Vec<String>, String> {
    let provider = entry
        .create(Vec::new())
        .await
        .map_err(|error| error.to_string())?;
    provider
        .fetch_supported_models()
        .await
        .map_err(|error| error.to_string())
}

fn inventory_from_sources(
    configured_models: Vec<ConfiguredModels>,
    local_models: Vec<LocalModel>,
    active: Option<ModelSelection>,
) -> ModelInventory {
    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();

    for provider in configured_models {
        let default_name = provider.default_model;
        let default_info = provider
            .known_models
            .iter()
            .find(|model| model.name.eq_ignore_ascii_case(&default_name));
        entries.push(entry_from_model_info(
            &provider.provider,
            &default_name,
            ModelSource::Configured,
            ModelAvailability::Configured,
            default_info,
            active.as_ref(),
            true,
        ));

        for model in &provider.known_models {
            if !model.name.eq_ignore_ascii_case(&default_name) {
                entries.push(entry_from_model_info(
                    &provider.provider,
                    &model.name,
                    ModelSource::Configured,
                    ModelAvailability::Configured,
                    Some(model),
                    active.as_ref(),
                    false,
                ));
            }
        }

        match provider.installed_models {
            Ok(models) => {
                let source = if provider.provider.eq_ignore_ascii_case("ollama") {
                    ModelSource::Ollama
                } else {
                    continue;
                };
                for model in models {
                    entries.push(entry_from_model_info(
                        &provider.provider,
                        &model,
                        source,
                        ModelAvailability::Installed,
                        provider
                            .known_models
                            .iter()
                            .find(|known| known.name.eq_ignore_ascii_case(&model)),
                        active.as_ref(),
                        false,
                    ));
                }
            }
            Err(error) => diagnostics.push(format!("{}: {error}", provider.provider)),
        }
    }

    for model in local_models {
        let mut entry = entry_from_model_info(
            "local",
            &model.id,
            ModelSource::GooseLocal,
            ModelAvailability::Installed,
            None,
            active.as_ref(),
            false,
        );
        entry.metadata.size_bytes = Some(MetadataValue {
            value: model.size_bytes,
            provenance: MetadataProvenance::Catalog,
        });
        entry.metadata.supports_vision = Some(MetadataValue {
            value: model.vision_capable,
            provenance: MetadataProvenance::Catalog,
        });
        entries.push(entry);
    }

    ModelInventory {
        entries: super::model_picker::ModelPickerState::new(entries, active)
            .entries()
            .to_vec(),
        diagnostics,
    }
}

fn entry_from_model_info(
    provider: &str,
    model_name: &str,
    source: ModelSource,
    availability: ModelAvailability,
    model_info: Option<&ModelInfo>,
    active: Option<&ModelSelection>,
    is_default: bool,
) -> ModelInventoryEntry {
    let info = model_info;
    ModelInventoryEntry {
        provider: provider.to_string(),
        model: model_name.to_string(),
        source,
        availability,
        is_active: active.is_some_and(|active| {
            active.provider.eq_ignore_ascii_case(provider)
                && active.model.eq_ignore_ascii_case(model_name)
        }),
        is_default,
        metadata: ModelMetadata {
            context_length: info.and_then(|info| {
                info.context_limit.map(|value| MetadataValue {
                    value: value as u64,
                    provenance: MetadataProvenance::Provider,
                })
            }),
            supports_tools: None,
            supports_vision: info.and_then(|info| {
                info.supports_vision.map(|value| MetadataValue {
                    value,
                    provenance: MetadataProvenance::Provider,
                })
            }),
            size_bytes: None,
            performance: None,
            fit: None,
        },
        download: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::tui::model_fit::{ModelFitEvidence, ModelFitStatus};
    use crate::session::tui::model_picker::{
        MetadataProvenance, ModelAvailability, ModelSelection, ModelSource,
    };

    #[test]
    fn configured_model_is_retained_when_runtime_lookup_fails() {
        let inventory = inventory_from_sources(
            vec![ConfiguredModels {
                provider: "ollama".into(),
                default_model: "qwen3:8b".into(),
                known_models: Vec::new(),
                installed_models: Err("connection refused".into()),
            }],
            Vec::new(),
            Some(ModelSelection::new("ollama", "qwen3:8b")),
        );

        assert_eq!(inventory.entries.len(), 1);
        assert!(inventory.entries[0].is_active);
        assert!(inventory.diagnostics[0].contains("connection refused"));
    }

    #[test]
    fn runtime_models_are_marked_installed_and_duplicate_names_merge() {
        let inventory = inventory_from_sources(
            vec![ConfiguredModels {
                provider: "ollama".into(),
                default_model: "qwen3:8b".into(),
                known_models: Vec::new(),
                installed_models: Ok(vec!["qwen3:8b".into(), "llama3.2:3b".into()]),
            }],
            Vec::new(),
            Some(ModelSelection::new("ollama", "qwen3:8b")),
        );

        assert_eq!(inventory.entries.len(), 2);
        assert_eq!(inventory.entries[0].source, ModelSource::Ollama);
        assert_eq!(
            inventory.entries[0].availability,
            ModelAvailability::Installed
        );
        assert!(inventory.entries[0].is_active);
        assert!(inventory.entries[0].is_default);
        assert_eq!(
            inventory.entries[1].availability,
            ModelAvailability::Installed
        );
    }

    #[test]
    fn goose_local_models_are_included_without_catalog_requests() {
        let inventory = inventory_from_sources(
            Vec::new(),
            vec![LocalModel {
                id: "bartowski/llama-3.2-3b-instruct-gguf".into(),
                size_bytes: 2_000_000_000,
                vision_capable: false,
            }],
            None,
        );

        assert_eq!(inventory.entries.len(), 1);
        assert_eq!(inventory.entries[0].source, ModelSource::GooseLocal);
        assert_eq!(
            inventory.entries[0].availability,
            ModelAvailability::Installed
        );
        assert_eq!(
            inventory.entries[0]
                .metadata
                .size_bytes
                .as_ref()
                .unwrap()
                .provenance,
            MetadataProvenance::Catalog
        );
    }

    #[test]
    fn ollama_fit_remains_unknown_without_runtime_memory_evidence() {
        let variants = vec![CatalogVariant {
            backend_id: "llamacpp".into(),
            variant_id: "Q4_K_M".into(),
            label: "Q4_K_M".into(),
            size_bytes: Some(6_000),
        }];

        let ollama = catalog_variant_fit_from_data("ollama", "Q4_K_M", &variants, Some(0), 10_000);
        let unavailable =
            catalog_variant_fit_from_data("llamacpp", "Q4_K_M", &variants, Some(0), 0);

        assert_eq!(ollama.status, ModelFitStatus::Unknown);
        assert_eq!(unavailable.status, ModelFitStatus::Unknown);
        assert!(!ollama.selection_blocked);
        assert!(!unavailable.selection_blocked);
    }

    #[test]
    fn backend_recommended_variant_keeps_reported_available_memory() {
        let variants = vec![
            CatalogVariant {
                backend_id: "llamacpp".into(),
                variant_id: "Q4_K_M".into(),
                label: "Q4_K_M".into(),
                size_bytes: Some(6_000),
            },
            CatalogVariant {
                backend_id: "llamacpp".into(),
                variant_id: "Q8_0".into(),
                label: "Q8_0".into(),
                size_bytes: Some(8_000),
            },
        ];

        let fit = catalog_variant_fit_from_data("llamacpp", "Q8_0", &variants, Some(1), 10_000);

        assert_eq!(fit.status, ModelFitStatus::Recommended);
        assert_eq!(fit.required_bytes, Some(8_000));
        assert_eq!(fit.available_bytes, Some(10_000));
        assert_eq!(fit.effective_budget_bytes, Some(8_500));
        assert_eq!(fit.evidence, ModelFitEvidence::BackendRecommendation);
    }

    #[test]
    fn catalog_entries_are_downloadable_and_do_not_invent_unknown_sizes() {
        let entries = catalog_entries(vec![CatalogModel {
            repo_id: "org/model-gguf".into(),
            model_name: "Model".into(),
            variants: vec![CatalogVariant {
                backend_id: "llamacpp".into(),
                variant_id: "Q4_K_M".into(),
                label: "Q4_K_M".into(),
                size_bytes: None,
            }],
        }]);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, ModelSource::Catalog);
        assert_eq!(entries[0].availability, ModelAvailability::Downloadable);
        assert!(entries[0].metadata.size_bytes.is_none());
        assert!(entries[0].download.is_some());
    }
}
