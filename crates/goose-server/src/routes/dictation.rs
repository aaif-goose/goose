use crate::routes::errors::ErrorResponse;
use crate::state::AppState;
use axum::{
    extract::DefaultBodyLimit,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
#[cfg(feature = "local-inference")]
use axum::{extract::Path, routing::delete};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
#[cfg(feature = "local-inference")]
use goose::dictation::providers::transcribe_local;
use goose::dictation::providers::{
    all_providers, is_configured, transcribe_with_provider, DictationProvider,
};
#[cfg(feature = "local-inference")]
use goose::dictation::whisper;
#[cfg(feature = "local-inference")]
use goose::download_manager::{get_download_manager, DownloadProgress};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

const MAX_AUDIO_SIZE_BYTES: usize = 50 * 1024 * 1024;

#[cfg(feature = "local-inference")]
#[derive(Debug, Serialize, ToSchema)]
pub struct WhisperModelResponse {
    #[serde(flatten)]
    #[schema(inline)]
    model: &'static whisper::WhisperModel,
    downloaded: bool,
    recommended: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TranscribeRequest {
    /// Base64 encoded audio data
    pub audio: String,
    /// MIME type of the audio (e.g., "audio/webm", "audio/wav")
    pub mime_type: String,
    /// Transcription provider to use
    pub provider: DictationProvider,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TranscribeResponse {
    /// Transcribed text from the audio
    pub text: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DictationProviderStatus {
    /// Whether the provider is fully configured and ready to use
    pub configured: bool,
    /// Custom host URL if configured (only for providers that support it)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Description of what this provider does
    pub description: String,
    /// Whether this provider uses the main provider config (true) or has its own key (false)
    pub uses_provider_config: bool,
    /// Path to settings if uses_provider_config is true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings_path: Option<String>,
    /// Config key name if uses_provider_config is false
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_key: Option<String>,
}

fn validate_audio(audio: &str, mime_type: &str) -> Result<(Vec<u8>, &'static str), ErrorResponse> {
    let audio_bytes = BASE64
        .decode(audio)
        .map_err(|_| ErrorResponse::bad_request("Invalid base64 audio data"))?;

    let extension = match mime_type {
        "audio/webm" | "audio/webm;codecs=opus" => "webm",
        "audio/mp4" => "mp4",
        "audio/mpeg" | "audio/mpga" => "mp3",
        "audio/m4a" => "m4a",
        "audio/wav" | "audio/x-wav" => "wav",
        _ => {
            return Err(ErrorResponse {
                message: format!("Unsupported audio format: {}", mime_type),
                status: StatusCode::UNSUPPORTED_MEDIA_TYPE,
            })
        }
    };

    Ok((audio_bytes, extension))
}

fn convert_error(e: anyhow::Error) -> ErrorResponse {
    let error_msg = e.to_string();

    if error_msg.contains("Invalid API key") {
        ErrorResponse {
            message: error_msg,
            status: StatusCode::UNAUTHORIZED,
        }
    } else if error_msg.contains("Rate limit exceeded") || error_msg.contains("quota") {
        ErrorResponse {
            message: error_msg,
            status: StatusCode::TOO_MANY_REQUESTS,
        }
    } else if error_msg.contains("not configured") {
        ErrorResponse {
            message: error_msg,
            status: StatusCode::PRECONDITION_FAILED,
        }
    } else if error_msg.contains("timeout") {
        ErrorResponse {
            message: error_msg,
            status: StatusCode::GATEWAY_TIMEOUT,
        }
    } else if error_msg.contains("API error") {
        ErrorResponse {
            message: error_msg,
            status: StatusCode::BAD_GATEWAY,
        }
    } else {
        ErrorResponse::internal(error_msg)
    }
}

#[utoipa::path(
    post,
    path = "/dictation/transcribe",
    request_body = TranscribeRequest,
    responses(
        (status = 200, description = "Audio transcribed successfully", body = TranscribeResponse),
        (status = 400, description = "Invalid request (bad base64 or unsupported format)"),
        (status = 401, description = "Invalid API key"),
        (status = 412, description = "Provider not configured"),
        (status = 413, description = "Audio file too large (max 50MB)"),
        (status = 429, description = "Rate limit exceeded"),
        (status = 500, description = "Internal server error"),
        (status = 502, description = "Provider API error"),
        (status = 503, description = "Service unavailable"),
        (status = 504, description = "Request timeout")
    )
)]
pub async fn transcribe_dictation(
    Json(request): Json<TranscribeRequest>,
) -> Result<Json<TranscribeResponse>, ErrorResponse> {
    let (audio_bytes, extension) = validate_audio(&request.audio, &request.mime_type)?;

    let text = match request.provider {
        DictationProvider::OpenAI => transcribe_with_provider(
            DictationProvider::OpenAI,
            "model".to_string(),
            "whisper-1".to_string(),
            audio_bytes,
            extension,
            &request.mime_type,
        )
        .await
        .map_err(convert_error)?,
        DictationProvider::Groq => transcribe_with_provider(
            DictationProvider::Groq,
            "model".to_string(),
            "whisper-large-v3-turbo".to_string(),
            audio_bytes,
            extension,
            &request.mime_type,
        )
        .await
        .map_err(convert_error)?,
        DictationProvider::ElevenLabs => transcribe_with_provider(
            DictationProvider::ElevenLabs,
            "model_id".to_string(),
            "scribe_v1".to_string(),
            audio_bytes,
            extension,
            &request.mime_type,
        )
        .await
        .map_err(convert_error)?,
        #[cfg(feature = "local-inference")]
        DictationProvider::Local => transcribe_local(audio_bytes).await.map_err(convert_error)?,
    };

    Ok(Json(TranscribeResponse { text }))
}

#[utoipa::path(
    get,
    path = "/dictation/config",
    responses(
        (status = 200, description = "Audio transcription provider configurations", body = HashMap<String, DictationProviderStatus>)
    )
)]
pub async fn get_dictation_config(
) -> Result<Json<HashMap<DictationProvider, DictationProviderStatus>>, ErrorResponse> {
    let config = goose::config::Config::global();
    let mut providers = HashMap::new();

    for def in all_providers() {
        let provider = def.provider;
        let configured = is_configured(provider);

        let host = if let Some(host_key) = def.host_key {
            config
                .get(host_key, false)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        } else {
            None
        };

        providers.insert(
            provider,
            DictationProviderStatus {
                configured,
                host,
                description: def.description.to_string(),
                uses_provider_config: def.uses_provider_config,
                settings_path: def.settings_path.map(|s| s.to_string()),
                config_key: if !def.uses_provider_config {
                    Some(def.config_key.to_string())
                } else {
                    None
                },
            },
        );
    }

    Ok(Json(providers))
}

#[cfg(feature = "local-inference")]
#[utoipa::path(
    get,
    path = "/dictation/models",
    responses(
        (status = 200, description = "List of available Whisper models", body = Vec<WhisperModelResponse>)
    )
)]
pub async fn list_models() -> Result<Json<Vec<WhisperModelResponse>>, ErrorResponse> {
    let recommended_id = whisper::recommend_model();
    let models = whisper::available_models()
        .iter()
        .map(|m| WhisperModelResponse {
            model: m,
            downloaded: m.is_downloaded(),
            recommended: m.id == recommended_id,
        })
        .collect();

    Ok(Json(models))
}

#[cfg(feature = "local-inference")]
#[utoipa::path(
    post,
    path = "/dictation/models/{model_id}/download",
    responses(
        (status = 202, description = "Download started"),
        (status = 400, description = "Download already in progress"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn download_model(Path(model_id): Path<String>) -> Result<StatusCode, ErrorResponse> {
    let model = whisper::get_model(&model_id)
        .ok_or_else(|| ErrorResponse::bad_request("Model not found"))?;

    let manager = get_download_manager();
    let model_id_for_config = model.id.to_string();
    manager
        .download_model(
            model.id.to_string(),
            model.url.to_string(),
            model.local_path(),
            Some(Box::new(move || {
                let _ = goose::config::Config::global()
                    .set_param(whisper::LOCAL_WHISPER_MODEL_CONFIG_KEY, model_id_for_config);
            })),
        )
        .await
        .map_err(convert_error)?;

    Ok(StatusCode::ACCEPTED)
}

#[cfg(feature = "local-inference")]
#[utoipa::path(
    get,
    path = "/dictation/models/{model_id}/download",
    responses(
        (status = 200, description = "Download progress", body = DownloadProgress),
        (status = 404, description = "Download not found")
    )
)]
pub async fn get_download_progress(
    Path(model_id): Path<String>,
) -> Result<Json<DownloadProgress>, ErrorResponse> {
    let manager = get_download_manager();
    let progress = manager
        .get_progress(&model_id)
        .ok_or_else(|| ErrorResponse::bad_request("Download not found"))?;

    Ok(Json(progress))
}

#[cfg(feature = "local-inference")]
#[utoipa::path(
    delete,
    path = "/dictation/models/{model_id}/download",
    responses(
        (status = 200, description = "Download cancelled"),
        (status = 404, description = "Download not found")
    )
)]
pub async fn cancel_download(Path(model_id): Path<String>) -> Result<StatusCode, ErrorResponse> {
    let manager = get_download_manager();
    manager.cancel_download(&model_id).map_err(convert_error)?;
    Ok(StatusCode::OK)
}

#[cfg(feature = "local-inference")]
#[utoipa::path(
    delete,
    path = "/dictation/models/{model_id}",
    responses(
        (status = 200, description = "Model deleted"),
        (status = 404, description = "Model not found or not downloaded"),
        (status = 500, description = "Failed to delete model")
    )
)]
pub async fn delete_model(Path(model_id): Path<String>) -> Result<StatusCode, ErrorResponse> {
    let model = whisper::get_model(&model_id)
        .ok_or_else(|| ErrorResponse::bad_request("Model not found"))?;

    let path = model.local_path();

    if !path.exists() {
        return Err(ErrorResponse::bad_request("Model not downloaded"));
    }

    tokio::fs::remove_file(&path)
        .await
        .map_err(|e| ErrorResponse::internal(format!("Failed to delete model: {}", e)))?;

    Ok(StatusCode::OK)
}

pub fn routes(state: Arc<AppState>) -> Router {
    let router = Router::new()
        .route("/dictation/transcribe", post(transcribe_dictation))
        .route("/dictation/config", get(get_dictation_config));

    #[cfg(feature = "local-inference")]
    let router = router
        .route("/dictation/models", get(list_models))
        .route(
            "/dictation/models/{model_id}/download",
            post(download_model),
        )
        .route(
            "/dictation/models/{model_id}/download",
            get(get_download_progress),
        )
        .route(
            "/dictation/models/{model_id}/download",
            delete(cancel_download),
        )
        .route("/dictation/models/{model_id}", delete(delete_model));

    router
        .layer(DefaultBodyLimit::max(MAX_AUDIO_SIZE_BYTES))
        .with_state(state)
}
