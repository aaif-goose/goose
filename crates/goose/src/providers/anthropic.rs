use anyhow::Result;
use async_stream::try_stream;
use async_trait::async_trait;
use futures::TryStreamExt;
use reqwest::StatusCode;
use serde_json::Value;
use std::io;
use tokio::pin;
use tokio_util::io::StreamReader;

use super::api_client::{ApiClient, AuthMethod};
use super::base::{ConfigKey, MessageStream, ModelInfo, Provider, ProviderDef, ProviderMetadata};
use super::errors::ProviderError;
use super::formats::anthropic::{
    create_request, response_to_streaming_message, thinking_type, ThinkingType,
};
use super::openai_compatible::handle_status_openai_compat;
use super::openai_compatible::map_http_error_to_provider_error;
use super::retry::ProviderRetry;
use crate::config::declarative_providers::DeclarativeProviderConfig;
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::providers::utils::RequestLog;
use futures::future::BoxFuture;
use rmcp::model::Tool;

const ANTHROPIC_PROVIDER_NAME: &str = "anthropic";
pub const ANTHROPIC_DEFAULT_MODEL: &str = "claude-sonnet-4-5";
const ANTHROPIC_DEFAULT_FAST_MODEL: &str = "claude-haiku-4-5";
const ANTHROPIC_KNOWN_MODELS: &[&str] = &[
    // Claude 4.6 models
    "claude-opus-4-6",
    "claude-sonnet-4-6",
    // Claude 4.5 models with aliases
    "claude-sonnet-4-5",
    "claude-sonnet-4-5-20250929",
    "claude-haiku-4-5",
    "claude-haiku-4-5-20251001",
    "claude-opus-4-5",
    "claude-opus-4-5-20251101",
    // Legacy Claude 4.0 models
    "claude-sonnet-4-0",
    "claude-sonnet-4-20250514",
    "claude-opus-4-0",
    "claude-opus-4-20250514",
];

const ANTHROPIC_DOC_URL: &str = "https://docs.anthropic.com/en/docs/about-claude/models";
const ANTHROPIC_API_VERSION: &str = "2023-06-01";

#[derive(serde::Serialize)]
pub struct AnthropicProvider {
    #[serde(skip)]
    api_client: ApiClient,
    model: ModelConfig,
    supports_streaming: bool,
    name: String,
    custom_models: Option<Vec<String>>,
}

impl AnthropicProvider {
    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let model = model.with_fast(ANTHROPIC_DEFAULT_FAST_MODEL, ANTHROPIC_PROVIDER_NAME)?;

        let config = crate::config::Config::global();
        let api_key: String = config.get_secret("ANTHROPIC_API_KEY")?;
        let host: String = config
            .get_param("ANTHROPIC_HOST")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());

        let auth = AuthMethod::ApiKey {
            header_name: "x-api-key".to_string(),
            key: api_key,
        };

        let api_client =
            ApiClient::new(host, auth)?.with_header("anthropic-version", ANTHROPIC_API_VERSION)?;

        Ok(Self {
            api_client,
            model,
            supports_streaming: true,
            name: ANTHROPIC_PROVIDER_NAME.to_string(),
            custom_models: None,
        })
    }

    pub fn from_custom_config(
        model: ModelConfig,
        config: DeclarativeProviderConfig,
    ) -> Result<Self> {
        let global_config = crate::config::Config::global();
        let api_key: String = global_config
            .get_secret(&config.api_key_env)
            .map_err(|_| anyhow::anyhow!("Missing API key: {}", config.api_key_env))?;

        let auth = AuthMethod::ApiKey {
            header_name: "x-api-key".to_string(),
            key: api_key,
        };

        let mut api_client = ApiClient::new(config.base_url, auth)?
            .with_header("anthropic-version", ANTHROPIC_API_VERSION)?;

        if let Some(headers) = &config.headers {
            let mut header_map = reqwest::header::HeaderMap::new();
            for (key, value) in headers {
                let header_name = reqwest::header::HeaderName::from_bytes(key.as_bytes())?;
                let header_value = reqwest::header::HeaderValue::from_str(value)?;
                header_map.insert(header_name, header_value);
            }
            api_client = api_client.with_headers(header_map)?;
        }

        let supports_streaming = config.supports_streaming.unwrap_or(true);

        if !supports_streaming {
            return Err(anyhow::anyhow!(
                "Anthropic provider does not support non-streaming mode. All Claude models support streaming. \
                Please remove 'supports_streaming: false' from your provider configuration."
            ));
        }

        let custom_models = if !config.models.is_empty() {
            Some(config.models.iter().map(|m| m.name.clone()).collect())
        } else {
            None
        };

        let model = if let Some(ref fast_model_name) = config.fast_model {
            model.with_fast(fast_model_name, &config.name)?
        } else {
            model
        };

        Ok(Self {
            api_client,
            model,
            supports_streaming,
            name: config.name.clone(),
            custom_models,
        })
    }

    fn get_conditional_headers(&self) -> Vec<(&str, &str)> {
        let mut headers = Vec::new();

        if self.model.model_name.starts_with("claude-3-7-sonnet-") {
            if thinking_type(&self.model) == ThinkingType::Enabled {
                headers.push(("anthropic-beta", "output-128k-2025-02-19"));
            }
            headers.push(("anthropic-beta", "token-efficient-tools-2025-02-19"));
        }

        headers
    }

    async fn fetch_models_from_api(&self) -> Result<Vec<String>, ProviderError> {
        let response = self.api_client.request(None, "v1/models").api_get().await?;

        if response.status == StatusCode::NOT_FOUND {
            let msg = response
                .payload
                .as_ref()
                .and_then(|p| p.get("error").and_then(|e| e.get("message")))
                .and_then(|m| m.as_str())
                .unwrap_or("models endpoint not found")
                .to_string();
            return Err(ProviderError::EndpointNotFound(msg));
        }

        if response.status != StatusCode::OK {
            return Err(map_http_error_to_provider_error(
                response.status,
                response.payload,
            ));
        }

        let json = response.payload.unwrap_or_default();
        let arr = json.get("data").and_then(|v| v.as_array()).ok_or_else(|| {
            ProviderError::RequestFailed(
                "Missing 'data' array in Anthropic models response".to_string(),
            )
        })?;

        let mut models: Vec<String> = arr
            .iter()
            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(str::to_string))
            .collect();
        models.sort();
        Ok(models)
    }
}

impl ProviderDef for AnthropicProvider {
    type Provider = Self;

    fn metadata() -> ProviderMetadata {
        let models: Vec<ModelInfo> = ANTHROPIC_KNOWN_MODELS
            .iter()
            .map(|&model_name| ModelInfo::new(model_name, 200_000))
            .collect();

        ProviderMetadata::with_models(
            ANTHROPIC_PROVIDER_NAME,
            "Anthropic",
            "Claude and other models from Anthropic",
            ANTHROPIC_DEFAULT_MODEL,
            models,
            ANTHROPIC_DOC_URL,
            vec![
                ConfigKey::new("ANTHROPIC_API_KEY", true, true, None, true),
                ConfigKey::new(
                    "ANTHROPIC_HOST",
                    true,
                    false,
                    Some("https://api.anthropic.com"),
                    false,
                ),
            ],
        )
        .with_setup_steps(vec![
            "Go to https://platform.claude.com/settings/keys",
            "Click 'Create Key'",
            "Copy the key and paste it above",
        ])
    }

    fn from_env(
        model: ModelConfig,
        _extensions: Vec<crate::config::ExtensionConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(Self::from_env(model))
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model.clone()
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        if let Some(custom_models) = &self.custom_models {
            match self.fetch_models_from_api().await {
                Ok(models) => return Ok(models),
                Err(e) if e.is_endpoint_not_found() => {
                    tracing::debug!(
                        "Models endpoint not implemented for provider '{}' ({}), using predefined list",
                        self.name,
                        e
                    );
                    return Ok(custom_models.clone());
                }
                Err(e) => return Err(e),
            }
        }

        self.fetch_models_from_api().await
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        session_id: &str,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let mut payload = create_request(model_config, system, messages, tools)?;
        payload
            .as_object_mut()
            .unwrap()
            .insert("stream".to_string(), Value::Bool(true));

        let conditional_headers = self.get_conditional_headers();
        let mut log = RequestLog::start(model_config, &payload)?;

        let response = self
            .with_retry(|| async {
                let mut request = self.api_client.request(Some(session_id), "v1/messages");
                for (key, value) in &conditional_headers {
                    request = request.header(key, value)?;
                }
                let resp = request.response_post(&payload).await?;
                handle_status_openai_compat(resp).await
            })
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;

        let stream = response.bytes_stream().map_err(io::Error::other);

        Ok(Box::pin(try_stream! {
            let stream_reader = StreamReader::new(stream);
            let framed = tokio_util::codec::FramedRead::new(stream_reader, tokio_util::codec::LinesCodec::new()).map_err(anyhow::Error::from);

            let message_stream = response_to_streaming_message(framed);
            pin!(message_stream);
            while let Some(message) = futures::StreamExt::next(&mut message_stream).await {
                let (message, usage) = message.map_err(|e| ProviderError::RequestFailed(format!("Stream decode error: {}", e)))?;
                log.write(&message, usage.as_ref().map(|f| f.usage).as_ref())?;
                yield (message, usage);
            }
        }))
    }
}
