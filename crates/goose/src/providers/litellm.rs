use anyhow::Result;
use async_trait::async_trait;
use futures::future::BoxFuture;
use serde_json::{json, Value};
use std::collections::HashMap;

use super::api_client::{ApiClient, AuthMethod};
use super::base::{
    ConfigKey, MessageStream, ModelInfo, Provider, ProviderDef, ProviderMetadata, ProviderUsage,
    DEFAULT_PROVIDER_TIMEOUT_SECS,
};
use super::embedding::EmbeddingCapable;
use super::errors::ProviderError;
use super::openai_compatible::handle_response_openai_compat;
use super::retry::ProviderRetry;
use super::utils::{get_model, ImageFormat, RequestLog};
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use rmcp::model::Tool;

const LITELLM_PROVIDER_NAME: &str = "litellm";
pub const LITELLM_DEFAULT_MODEL: &str = "gpt-4o-mini";
pub const LITELLM_DOC_URL: &str = "https://docs.litellm.ai/docs/";

#[derive(Debug, serde::Serialize)]
pub struct LiteLLMProvider {
    #[serde(skip)]
    api_client: ApiClient,
    base_path: String,
    model: ModelConfig,
    #[serde(skip)]
    name: String,
}

impl LiteLLMProvider {
    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let config = crate::config::Config::global();
        let secrets = config
            .get_secrets("LITELLM_API_KEY", &["LITELLM_CUSTOM_HEADERS"])
            .unwrap_or_default();
        let api_key = secrets.get("LITELLM_API_KEY").cloned().unwrap_or_default();
        let host: String = config
            .get_param("LITELLM_HOST")
            .unwrap_or_else(|_| "https://api.litellm.ai".to_string());
        let base_path: String = config
            .get_param("LITELLM_BASE_PATH")
            .unwrap_or_else(|_| "v1/chat/completions".to_string());
        let custom_headers: Option<HashMap<String, String>> = secrets
            .get("LITELLM_CUSTOM_HEADERS")
            .cloned()
            .map(parse_custom_headers);
        let timeout_secs: u64 = config
            .get_param("LITELLM_TIMEOUT")
            .unwrap_or(DEFAULT_PROVIDER_TIMEOUT_SECS);

        let auth = if api_key.is_empty() {
            AuthMethod::NoAuth
        } else {
            AuthMethod::BearerToken(api_key)
        };

        let mut api_client =
            ApiClient::with_timeout(host, auth, std::time::Duration::from_secs(timeout_secs))?;

        if let Some(headers) = custom_headers {
            let mut header_map = reqwest::header::HeaderMap::new();
            for (key, value) in headers {
                let header_name = reqwest::header::HeaderName::from_bytes(key.as_bytes())?;
                let header_value = reqwest::header::HeaderValue::from_str(&value)?;
                header_map.insert(header_name, header_value);
            }
            api_client = api_client.with_headers(header_map)?;
        }

        Ok(Self {
            api_client,
            base_path,
            model,
            name: LITELLM_PROVIDER_NAME.to_string(),
        })
    }

    async fn fetch_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let response = self
            .api_client
            .request(None, "model/info")
            .response_get()
            .await?;

        if !response.status().is_success() {
            return Err(ProviderError::RequestFailed(format!(
                "Models endpoint returned status: {}",
                response.status()
            )));
        }

        let response_json: Value = response.json().await.map_err(|e| {
            ProviderError::RequestFailed(format!("Failed to parse models response: {}", e))
        })?;

        let models_data = response_json["data"].as_array().ok_or_else(|| {
            ProviderError::RequestFailed("Missing data field in models response".to_string())
        })?;

        let mut models = Vec::new();
        for model_data in models_data {
            if let Some(model_name) = model_data["model_name"].as_str() {
                if model_name.contains("/*") {
                    continue;
                }

                let model_info = &model_data["model_info"];
                let context_length =
                    model_info["max_input_tokens"].as_u64().unwrap_or(128000) as usize;
                let supports_cache_control = model_info["supports_prompt_caching"].as_bool();

                let mut model_info_obj = ModelInfo::new(model_name, context_length);
                model_info_obj.supports_cache_control = supports_cache_control;
                models.push(model_info_obj);
            }
        }

        Ok(models)
    }

    async fn post(
        &self,
        session_id: Option<&str>,
        payload: &Value,
    ) -> Result<Value, ProviderError> {
        let response = self
            .api_client
            .response_post(session_id, &self.base_path, payload)
            .await?;
        handle_response_openai_compat(response).await
    }
}

impl ProviderDef for LiteLLMProvider {
    type Provider = Self;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            LITELLM_PROVIDER_NAME,
            "LiteLLM",
            "LiteLLM proxy supporting multiple models with automatic prompt caching",
            LITELLM_DEFAULT_MODEL,
            vec![],
            LITELLM_DOC_URL,
            vec![
                ConfigKey::new("LITELLM_API_KEY", true, true, None, true),
                ConfigKey::new(
                    "LITELLM_HOST",
                    true,
                    false,
                    Some("http://localhost:4000"),
                    true,
                ),
                ConfigKey::new(
                    "LITELLM_BASE_PATH",
                    true,
                    false,
                    Some("v1/chat/completions"),
                    false,
                ),
                ConfigKey::new("LITELLM_CUSTOM_HEADERS", false, true, None, false),
                ConfigKey::new("LITELLM_TIMEOUT", false, false, Some("600"), false),
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
impl Provider for LiteLLMProvider {
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
        let session_id = if session_id.is_empty() {
            None
        } else {
            Some(session_id)
        };
        let mut payload = super::formats::openai::create_request(
            model_config,
            system,
            messages,
            tools,
            &ImageFormat::OpenAi,
            false,
        )?;

        if self.supports_cache_control().await {
            payload = update_request_for_cache_control(&payload);
        }

        let response = self
            .with_retry(|| async {
                let payload_clone = payload.clone();
                self.post(session_id, &payload_clone).await
            })
            .await?;

        let message = super::formats::openai::response_to_message(&response)?;
        let usage = super::formats::openai::get_usage(&response);
        let response_model = get_model(&response);
        let mut log = RequestLog::start(model_config, &payload)?;
        log.write(&response, Some(&usage))?;
        let provider_usage = ProviderUsage::new(response_model, usage);
        Ok(super::base::stream_from_single_message(
            message,
            provider_usage,
        ))
    }

    fn supports_embeddings(&self) -> bool {
        true
    }

    async fn supports_cache_control(&self) -> bool {
        if let Ok(models) = self.fetch_models().await {
            if let Some(model_info) = models.iter().find(|m| m.name == self.model.model_name) {
                return model_info.supports_cache_control.unwrap_or(false);
            }
        }

        self.model.model_name.to_lowercase().contains("claude")
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        let models = self.fetch_models().await.map_err(|e| {
            ProviderError::RequestFailed(format!("Failed to fetch models from LiteLLM: {}", e))
        })?;
        Ok(models.into_iter().map(|m| m.name).collect())
    }
}

#[async_trait]
impl EmbeddingCapable for LiteLLMProvider {
    async fn create_embeddings(
        &self,
        session_id: &str,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, anyhow::Error> {
        let embedding_model = std::env::var("GOOSE_EMBEDDING_MODEL")
            .unwrap_or_else(|_| "text-embedding-3-small".to_string());

        let payload = json!({
            "input": texts,
            "model": embedding_model,
            "encoding_format": "float"
        });

        let response = self
            .api_client
            .response_post(Some(session_id), "v1/embeddings", &payload)
            .await?;
        let response_text = response.text().await?;
        let response_json: Value = serde_json::from_str(&response_text)?;

        let data = response_json["data"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Missing data field"))?;

        let mut embeddings = Vec::new();
        for item in data {
            let embedding: Vec<f32> = item["embedding"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Missing embedding field"))?
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                .collect();
            embeddings.push(embedding);
        }

        Ok(embeddings)
    }
}

/// Updates the request payload to include cache control headers for automatic prompt caching
/// Adds ephemeral cache control to the last 2 user messages, system message, and last tool
pub fn update_request_for_cache_control(original_payload: &Value) -> Value {
    let mut payload = original_payload.clone();

    if let Some(messages_spec) = payload
        .as_object_mut()
        .and_then(|obj| obj.get_mut("messages"))
        .and_then(|messages| messages.as_array_mut())
    {
        let mut user_count = 0;
        for message in messages_spec.iter_mut().rev() {
            if message.get("role") == Some(&json!("user")) {
                if let Some(content) = message.get_mut("content") {
                    if let Some(content_str) = content.as_str() {
                        *content = json!([{
                            "type": "text",
                            "text": content_str,
                            "cache_control": { "type": "ephemeral" }
                        }]);
                    }
                }
                user_count += 1;
                if user_count >= 2 {
                    break;
                }
            }
        }

        if let Some(system_message) = messages_spec
            .iter_mut()
            .find(|msg| msg.get("role") == Some(&json!("system")))
        {
            if let Some(content) = system_message.get_mut("content") {
                if let Some(content_str) = content.as_str() {
                    *system_message = json!({
                        "role": "system",
                        "content": [{
                            "type": "text",
                            "text": content_str,
                            "cache_control": { "type": "ephemeral" }
                        }]
                    });
                }
            }
        }
    }

    if let Some(tools_spec) = payload
        .as_object_mut()
        .and_then(|obj| obj.get_mut("tools"))
        .and_then(|tools| tools.as_array_mut())
    {
        if let Some(last_tool) = tools_spec.last_mut() {
            if let Some(function) = last_tool.get_mut("function") {
                function
                    .as_object_mut()
                    .unwrap()
                    .insert("cache_control".to_string(), json!({ "type": "ephemeral" }));
            }
        }
    }
    payload
}

fn parse_custom_headers(headers_str: String) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    for line in headers_str.lines() {
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    headers
}
