use super::api_client::{ApiClient, AuthMethod};
use super::base::{ConfigKey, MessageStream, Provider, ProviderDef, ProviderMetadata};
use super::openai_compatible::{handle_status, stream_openai_compat};
use super::retry::ProviderRetry;
use crate::conversation::message::Message;
use anyhow::Result;
use async_trait::async_trait;
use futures::future::BoxFuture;
use goose_providers::errors::ProviderError;
use goose_providers::formats::openai::create_request;
use goose_providers::images::ImageFormat;
use goose_providers::model::ModelConfig;
use goose_providers::request_log::{start_log, LoggerHandleExt};
use rmcp::model::Tool;

pub const NANOGPT_PROVIDER_NAME: &str = "nano-gpt";
pub const NANOGPT_API_HOST: &str = "https://nano-gpt.com/api/v1";
pub const NANOGPT_SUBSCRIPTION_HOST: &str = "https://nano-gpt.com/api/subscription/v1";
pub const NANOGPT_DEFAULT_MODEL: &str = "anthropic/claude-sonnet-4.6";
pub const NANOGPT_DOC_URL: &str = "https://docs.nano-gpt.com/";
const NANOGPT_API_KEY: &str = "NANOGPT_API_KEY";

#[derive(serde::Serialize)]
pub struct NanoGptProvider {
    #[serde(skip)]
    api_client: ApiClient,
    #[serde(skip)]
    name: String,
}

impl NanoGptProvider {
    fn build_client(
        host: &str,
        api_key: &str,
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> Result<ApiClient> {
        ApiClient::new_with_tls(
            host.to_string(),
            AuthMethod::BearerToken(api_key.to_string()),
            tls_config,
        )?
        .with_header("x-client", "goose")
    }

    async fn check_subscription(
        api_key: &str,
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> bool {
        let client = match Self::build_client(NANOGPT_SUBSCRIPTION_HOST, api_key, tls_config) {
            Ok(c) => c,
            Err(_) => return false,
        };

        match client.response_get("usage").await {
            Ok(resp) => resp
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|json| json.get("active")?.as_bool())
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    pub async fn from_env(
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> Result<Self> {
        let config = crate::config::Config::global();
        let api_key: String = config.get_secret(NANOGPT_API_KEY)?;

        let is_subscription = Self::check_subscription(&api_key, tls_config.clone()).await;
        let host = if is_subscription {
            tracing::debug!("NanoGPT subscription active, using subscription endpoint");
            NANOGPT_SUBSCRIPTION_HOST.to_string()
        } else {
            tracing::debug!("NanoGPT using pay-as-you-go endpoint");
            NANOGPT_API_HOST.to_string()
        };

        let api_client = Self::build_client(&host, &api_key, tls_config)?
            .with_request_builder(crate::session_context::session_id_request_builder());

        Ok(Self {
            api_client,
            name: NANOGPT_PROVIDER_NAME.to_string(),
        })
    }
}

impl goose_providers::base::ProviderDescriptor for NanoGptProvider {
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
}

impl ProviderDef for NanoGptProvider {
    type Provider = Self;

    fn from_env(
        _extensions: Vec<crate::config::ExtensionConfig>,
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(Self::from_env(tls_config))
    }
}

#[async_trait]
impl Provider for NanoGptProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        let response = self
            .api_client
            .request("models?detailed=true")
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

        let mut log = start_log(model_config, &payload)?;

        let response = self
            .with_retry(|| async {
                let resp = self
                    .api_client
                    .request("chat/completions")
                    .model_headers(model_config)?
                    .streaming(true)
                    .response_post(&payload)
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
    use goose_providers::base::ProviderDescriptor as _;

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
