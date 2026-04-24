use super::api_client::{ApiClient, AuthMethod};
use super::base::{ConfigKey, MessageStream, Provider, ProviderDef, ProviderMetadata};
use super::errors::ProviderError;
use super::openai_compatible::{handle_status, stream_openai_compat};
use super::retry::ProviderRetry;
use super::utils::{ImageFormat, RequestLog};
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::providers::formats::openai::create_request;
use anyhow::Result;
use async_trait::async_trait;
use futures::future::BoxFuture;
use rmcp::model::Tool;

const NANOGPT_PROVIDER_NAME: &str = "nano-gpt";
pub const NANOGPT_API_HOST: &str = "https://nano-gpt.com/api/v1";
pub const NANOGPT_SUBSCRIPTION_HOST: &str = "https://nano-gpt.com/api/subscription/v1";
pub const NANOGPT_DEFAULT_MODEL: &str = "anthropic/claude-sonnet-4.6";
pub const NANOGPT_DOC_URL: &str = "https://docs.nano-gpt.com/";
const NANOGPT_API_KEY: &str = "NANOGPT_API_KEY";

#[derive(serde::Serialize)]
pub struct NanoGptProvider {
    #[serde(skip)]
    api_client: ApiClient,
    model: ModelConfig,
    #[serde(skip)]
    name: String,
}

impl NanoGptProvider {
    fn build_client(host: &str, api_key: &str) -> Result<ApiClient> {
        ApiClient::new(
            host.to_string(),
            AuthMethod::BearerToken(api_key.to_string()),
        )?
        .with_header("x-client", "goose")
    }

    async fn check_subscription(api_key: &str) -> bool {
        let client = match Self::build_client(NANOGPT_SUBSCRIPTION_HOST, api_key) {
            Ok(c) => c,
            Err(_) => return false,
        };

        match client.response_get(None, "usage").await {
            Ok(resp) => resp
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|json| json.get("active")?.as_bool())
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let config = crate::config::Config::global();
        let api_key: String = config.get_secret(NANOGPT_API_KEY)?;

        let is_subscription = Self::check_subscription(&api_key).await;
        let host = if is_subscription {
            tracing::debug!("NanoGPT subscription active, using subscription endpoint");
            NANOGPT_SUBSCRIPTION_HOST.to_string()
        } else {
            tracing::debug!("NanoGPT using pay-as-you-go endpoint");
            NANOGPT_API_HOST.to_string()
        };

        let api_client = Self::build_client(&host, &api_key)?;

        Ok(Self {
            api_client,
            model,
            name: NANOGPT_PROVIDER_NAME.to_string(),
        })
    }
}

impl ProviderDef for NanoGptProvider {
    type Provider = Self;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            NANOGPT_PROVIDER_NAME,
            "NanoGPT",
            "Access multiple AI models through NanoGPT's unified API",
            NANOGPT_DEFAULT_MODEL,
            vec![NANOGPT_DEFAULT_MODEL],
            NANOGPT_DOC_URL,
            vec![ConfigKey::new(NANOGPT_API_KEY, true, true, None, true)],
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
impl Provider for NanoGptProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model.clone()
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        let response = self
            .api_client
            .request(None, "models?detailed=true")
            .response_get()
            .await
            .map_err(|e| {
                ProviderError::RequestFailed(format!(
                    "Failed to fetch models from NanoGPT API: {}",
                    e
                ))
            })?;

        let json: serde_json::Value = response.json().await.map_err(|e| {
            ProviderError::RequestFailed(format!(
                "Failed to parse NanoGPT models API response as JSON: {}",
                e
            ))
        })?;

        if let Some(err_obj) = json.get("error") {
            let msg = err_obj
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(ProviderError::RequestFailed(format!(
                "NanoGPT API returned an error: {}",
                msg
            )));
        }

        let data = json.get("data").and_then(|v| v.as_array()).ok_or_else(|| {
            ProviderError::RequestFailed("Missing 'data' field in JSON response".into())
        })?;

        let mut models: Vec<String> = data
            .iter()
            .filter_map(|model| {
                let id = model.get("id").and_then(|v| v.as_str())?;
                let supports_tool_calling = model
                    .get("capabilities")
                    .and_then(|c| c.get("tool_calling"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if supports_tool_calling {
                    Some(id.to_string())
                } else {
                    None
                }
            })
            .collect();

        models.sort();
        Ok(models)
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
            &ImageFormat::OpenAi,
            true,
        )?;

        let mut log = RequestLog::start(model_config, &payload)?;

        let response = self
            .with_retry(|| async {
                let resp = self
                    .api_client
                    .response_post(Some(session_id), "chat/completions", &payload)
                    .await?;
                handle_status(resp).await
            })
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;

        stream_openai_compat(response, log)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata() {
        let metadata = NanoGptProvider::metadata();
        assert_eq!(metadata.name, "nano-gpt");
        assert_eq!(metadata.default_model, "anthropic/claude-sonnet-4.6");
        assert_eq!(metadata.config_keys[0].name, NANOGPT_API_KEY);
        assert!(metadata.config_keys[0].required);
        assert!(metadata.config_keys[0].secret);
    }
}
