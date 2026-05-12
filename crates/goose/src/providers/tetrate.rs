use super::api_client::{ApiClient, AuthMethod};
use super::base::{ConfigKey, MessageStream, Provider, ProviderDef, ProviderMetadata};
use super::errors::ProviderError;
use super::openai_compatible::{
    handle_response_openai_compat, handle_status, map_http_error_to_provider_error,
    stream_openai_compat,
};
use super::retry::ProviderRetry;
use super::utils::RequestLog;
use crate::config::signup_tetrate::TETRATE_DEFAULT_MODEL;
use crate::conversation::message::Message;
use anyhow::Result;
use async_trait::async_trait;
use futures::future::BoxFuture;

use crate::model::ModelConfig;
use crate::providers::formats::openai::create_request;
use rmcp::model::Tool;
use serde_json::Value;

const TETRATE_PROVIDER_NAME: &str = "tetrate";
pub const TETRATE_DOC_URL: &str = "https://router.tetrate.ai";
pub const TETRATE_BILLING_URL: &str = "https://router.tetrate.ai/billing";

pub const TETRATE_KNOWN_MODELS: &[&str] = &[
    "claude-opus-4-1",
    "claude-3-7-sonnet-latest",
    "claude-sonnet-4-20250514",
    "gemini-2.5-pro",
    "gemini-2.0-flash",
    "gemini-2.0-flash-lite",
    "gpt-5",
    "gpt-5-mini",
    "gpt-5-nano",
    "gpt-4.1",
];

#[derive(serde::Serialize)]
pub struct TetrateProvider {
    #[serde(skip)]
    api_client: ApiClient,
    model: ModelConfig,
    supports_streaming: bool,
    #[serde(skip)]
    name: String,
}

impl TetrateProvider {
    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let config = crate::config::Config::global();
        let api_key: String = config.get_secret("TETRATE_API_KEY")?;
        let host: String = config
            .get_param("TETRATE_HOST")
            .unwrap_or_else(|_| "https://api.router.tetrate.ai".to_string());

        let auth = AuthMethod::BearerToken(api_key);
        let api_client = ApiClient::new(host, auth)?
            .with_header("HTTP-Referer", "https://goose-docs.ai")?
            .with_header("X-Title", "goose")?;

        Ok(Self {
            api_client,
            model,
            supports_streaming: true,
            name: TETRATE_PROVIDER_NAME.to_string(),
        })
    }

    fn enrich_credits_error(err: ProviderError) -> ProviderError {
        match err {
            ProviderError::CreditsExhausted { details, .. } => ProviderError::CreditsExhausted {
                details,
                top_up_url: Some(TETRATE_BILLING_URL.to_string()),
            },
            other => other,
        }
    }

    fn error_from_tetrate_error_payload(payload: Value) -> ProviderError {
        let code = payload
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_u64())
            .unwrap_or(500) as u16;
        let status = reqwest::StatusCode::from_u16(code)
            .unwrap_or(reqwest::StatusCode::INTERNAL_SERVER_ERROR);
        Self::enrich_credits_error(map_http_error_to_provider_error(status, Some(payload)))
    }
}

impl ProviderDef for TetrateProvider {
    type Provider = Self;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            TETRATE_PROVIDER_NAME,
            "Tetrate Agent Router Service",
            "Enterprise router for AI models",
            TETRATE_DEFAULT_MODEL,
            TETRATE_KNOWN_MODELS.to_vec(),
            TETRATE_DOC_URL,
            vec![
                ConfigKey::new("TETRATE_API_KEY", true, true, None, true),
                ConfigKey::new(
                    "TETRATE_HOST",
                    false,
                    false,
                    Some("https://api.router.tetrate.ai"),
                    false,
                ),
            ],
        )
    }

    fn from_env(
        model: ModelConfig,
        _extensions: Vec<crate::config::ExtensionConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(Self::from_env(model))
    }
}

#[async_trait]
impl Provider for TetrateProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model.clone()
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        session_id: &str,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let payload = create_request(
            model_config,
            system,
            messages,
            tools,
            &super::utils::ImageFormat::OpenAi,
            true,
        )?;

        let mut log = RequestLog::start(model_config, &payload)?;

        let response = self
            .with_retry(|| async {
                let resp = self
                    .api_client
                    .response_post(Some(session_id), "v1/chat/completions", &payload)
                    .await?;
                let resp = handle_status(resp)
                    .await
                    .map_err(Self::enrich_credits_error)?;

                let is_json = resp
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v.to_ascii_lowercase())
                    .is_some_and(|v| v.contains("json"));

                if is_json {
                    // Streaming responses should be SSE; when we get JSON instead, parse it to map
                    // explicit error payloads and otherwise fail as a protocol mismatch.
                    let body = handle_response_openai_compat(resp)
                        .await
                        .map_err(Self::enrich_credits_error)?;
                    if body.get("error").is_some() {
                        return Err(Self::error_from_tetrate_error_payload(body));
                    }

                    return Err(ProviderError::ExecutionError(
                        "Expected streaming response but received non-streaming payload"
                            .to_string(),
                    ));
                }

                Ok(resp)
            })
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;

        stream_openai_compat(response, log)
    }

    /// Fetch supported models from Tetrate Agent Router Service API
    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        let response = self
            .api_client
            .response_get(None, "v1/models")
            .await
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;
        let json = handle_response_openai_compat(response).await?;

        // Tetrate can return errors in 200 OK responses, so check explicitly
        if json.get("error").is_some() {
            return Err(Self::error_from_tetrate_error_payload(json));
        }

        let arr = json.get("data").and_then(|v| v.as_array()).ok_or_else(|| {
            ProviderError::RequestFailed("Missing 'data' array in models response".to_string())
        })?;
        let mut models: Vec<String> = arr
            .iter()
            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(str::to_string))
            .collect();
        models.sort();
        Ok(models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn enrich_adds_dashboard_url() {
        let err = ProviderError::CreditsExhausted {
            details: "out of credits".to_string(),
            top_up_url: None,
        };
        match TetrateProvider::enrich_credits_error(err) {
            ProviderError::CreditsExhausted { top_up_url, .. } => {
                assert_eq!(
                    top_up_url.as_deref(),
                    Some("https://router.tetrate.ai/billing")
                );
            }
            _ => panic!("Expected CreditsExhausted variant"),
        }
    }

    #[test]
    fn enrich_passes_through_other_errors() {
        let err = ProviderError::ServerError("boom".to_string());
        assert!(matches!(
            TetrateProvider::enrich_credits_error(err),
            ProviderError::ServerError(_)
        ));
    }

    #[test]
    fn error_payload_maps_credits_and_adds_billing_url() {
        let payload = json!({
            "error": {
                "code": 402,
                "message": "Insufficient credits"
            }
        });
        match TetrateProvider::error_from_tetrate_error_payload(payload) {
            ProviderError::CreditsExhausted {
                details,
                top_up_url,
            } => {
                assert!(details.contains("Insufficient credits"));
                assert_eq!(top_up_url.as_deref(), Some(TETRATE_BILLING_URL));
            }
            other => panic!("Expected CreditsExhausted, got {other:?}"),
        }
    }

    #[test]
    fn error_payload_maps_authentication() {
        let payload = json!({
            "error": {
                "code": 401,
                "message": "Invalid API key"
            }
        });
        match TetrateProvider::error_from_tetrate_error_payload(payload) {
            ProviderError::Authentication(msg) => {
                assert!(msg.contains("Invalid API key"));
            }
            other => panic!("Expected Authentication, got {other:?}"),
        }
    }
}
