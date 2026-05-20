use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::api_client::{ApiClient, AuthMethod};
use super::base::{
    ConfigKey, MessageStream, Provider, ProviderDef, ProviderMetadata, ProviderUsage,
};
use super::errors::ProviderError;
use super::formats::snowflake::{create_request, get_usage, response_to_message};
use super::openai_compatible::{map_http_error_to_provider_error, sanitize_url};
use super::retry::ProviderRetry;
use super::utils::{get_model, ImageFormat, RequestLog};
use crate::config::ConfigError;
use crate::conversation::message::Message;

use crate::model::ModelConfig;
use futures::future::BoxFuture;
use rmcp::model::Tool;

const SNOWFLAKE_PROVIDER_NAME: &str = "snowflake";
pub const SNOWFLAKE_DEFAULT_MODEL: &str = "claude-sonnet-4-5";
pub const SNOWFLAKE_KNOWN_MODELS: &[&str] = &[
    // Claude 4.5 series
    "claude-sonnet-4-5",
    "claude-haiku-4-5",
    // Claude 4 series
    "claude-4-sonnet",
    "claude-4-opus",
    // Claude 3 series
    "claude-3-7-sonnet",
    "claude-3-5-sonnet",
];

pub const SNOWFLAKE_DOC_URL: &str =
    "https://docs.snowflake.com/user-guide/snowflake-cortex/aisql#choosing-a-model";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SnowflakeAuth {
    Token(String),
}

impl SnowflakeAuth {
    pub fn token(token: String) -> Self {
        Self::Token(token)
    }
}

#[derive(Debug, serde::Serialize)]
pub struct SnowflakeProvider {
    #[serde(skip)]
    api_client: ApiClient,
    model: ModelConfig,
    image_format: ImageFormat,
    #[serde(skip)]
    name: String,
}

impl SnowflakeProvider {
    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let config = crate::config::Config::global();
        let mut host: Result<String, ConfigError> = config.get_param("SNOWFLAKE_HOST");
        if host.is_err() {
            host = config.get_secret("SNOWFLAKE_HOST")
        }
        if host.is_err() {
            return Err(ConfigError::NotFound(
                "Did not find SNOWFLAKE_HOST in either config file or keyring".to_string(),
            )
            .into());
        }

        let mut host = host?;

        // Convert host to lowercase
        host = host.to_lowercase();

        // Ensure host ends with snowflakecomputing.com
        if !host.ends_with("snowflakecomputing.com") {
            host = format!("{}.snowflakecomputing.com", host);
        }

        let mut token: Result<String, ConfigError> = config.get_param("SNOWFLAKE_TOKEN");

        if token.is_err() {
            token = config.get_secret("SNOWFLAKE_TOKEN")
        }

        if token.is_err() {
            return Err(ConfigError::NotFound(
                "Did not find SNOWFLAKE_TOKEN in either config file or keyring".to_string(),
            )
            .into());
        }

        // Ensure host has https:// prefix
        let base_url = if !host.starts_with("https://") && !host.starts_with("http://") {
            format!("https://{}", host)
        } else {
            host
        };

        let auth = AuthMethod::BearerToken(token?);
        let api_client = ApiClient::new(base_url, auth)?.with_header("User-Agent", "goose")?;

        Ok(Self {
            api_client,
            model,
            image_format: ImageFormat::OpenAi,
            name: SNOWFLAKE_PROVIDER_NAME.to_string(),
        })
    }

    async fn post(
        &self,
        session_id: Option<&str>,
        payload: &Value,
    ) -> Result<Value, ProviderError> {
        let response = self
            .api_client
            .response_post(session_id, "api/v2/cortex/inference:complete", payload)
            .await?;

        let status = response.status();
        let url = sanitize_url(response.url().as_str());
        let payload_text: String = response.text().await.ok().unwrap_or_default();

        if status.is_success() {
            if let Ok(payload) = serde_json::from_str::<Value>(&payload_text) {
                if payload.get("code").is_some() {
                    let code = payload
                        .get("code")
                        .and_then(|c| c.as_str())
                        .unwrap_or("Unknown code");
                    let message = payload
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Unknown message");
                    return Err(ProviderError::RequestFailed(format!(
                        "{} - {}",
                        code, message
                    )));
                }
            }
        }

        let lines = payload_text.lines().collect::<Vec<_>>();

        let mut text = String::new();
        let mut tool_name = String::new();
        let mut tool_input = String::new();
        let mut tool_use_id = String::new();
        for line in lines.iter() {
            if line.is_empty() {
                continue;
            }

            let json_str = match line.strip_prefix("data: ") {
                Some(s) => s,
                None => continue,
            };

            if let Ok(json_line) = serde_json::from_str::<Value>(json_str) {
                let choices = match json_line.get("choices").and_then(|c| c.as_array()) {
                    Some(choices) => choices,
                    None => {
                        continue;
                    }
                };

                let choice = match choices.first() {
                    Some(choice) => choice,
                    None => {
                        continue;
                    }
                };

                let delta = match choice.get("delta") {
                    Some(delta) => delta,
                    None => {
                        continue;
                    }
                };

                // Track if we found text in content_list to avoid duplication
                let mut found_text_in_content_list = false;

                // Handle content_list array first
                if let Some(content_list) = delta.get("content_list").and_then(|cl| cl.as_array()) {
                    for content_item in content_list {
                        match content_item.get("type").and_then(|t| t.as_str()) {
                            Some("text") => {
                                if let Some(text_content) =
                                    content_item.get("text").and_then(|t| t.as_str())
                                {
                                    text.push_str(text_content);
                                    found_text_in_content_list = true;
                                }
                            }
                            Some("tool_use") => {
                                if let Some(tool_id) =
                                    content_item.get("tool_use_id").and_then(|id| id.as_str())
                                {
                                    tool_use_id.push_str(tool_id);
                                }
                                if let Some(name) =
                                    content_item.get("name").and_then(|n| n.as_str())
                                {
                                    tool_name.push_str(name);
                                }
                                if let Some(input) =
                                    content_item.get("input").and_then(|i| i.as_str())
                                {
                                    tool_input.push_str(input);
                                }
                            }
                            _ => {
                                // Handle content items without explicit type but with tool information
                                if let Some(name) =
                                    content_item.get("name").and_then(|n| n.as_str())
                                {
                                    tool_name.push_str(name);
                                }
                                if let Some(tool_id) =
                                    content_item.get("tool_use_id").and_then(|id| id.as_str())
                                {
                                    tool_use_id.push_str(tool_id);
                                }
                                if let Some(input) =
                                    content_item.get("input").and_then(|i| i.as_str())
                                {
                                    tool_input.push_str(input);
                                }
                            }
                        }
                    }
                }

                // Handle direct content field (for text) only if we didn't find text in content_list
                if !found_text_in_content_list {
                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                        text.push_str(content);
                    }
                }
            }
        }

        // Build the appropriate response structure
        let mut content_list = Vec::new();

        // Add text content if available
        if !text.is_empty() {
            content_list.push(json!({
                "type": "text",
                "text": text
            }));
        }

        // Add tool use content only if we have complete tool information
        if !tool_use_id.is_empty() && !tool_name.is_empty() {
            // Parse tool input as JSON if it's not empty
            let parsed_input = if tool_input.is_empty() {
                json!({})
            } else {
                serde_json::from_str::<Value>(&tool_input)
                    .unwrap_or_else(|_| json!({"raw_input": tool_input}))
            };

            content_list.push(json!({
                "type": "tool_use",
                "tool_use_id": tool_use_id,
                "name": tool_name,
                "input": parsed_input
            }));
        }

        // Ensure we always have at least some content
        if content_list.is_empty() {
            content_list.push(json!({
                "type": "text",
                "text": ""
            }));
        }

        let answer_payload = json!({
            "role": "assistant",
            "content": text,
            "content_list": content_list
        });

        if status.is_success() {
            Ok(answer_payload)
        } else {
            let error_json = serde_json::from_str::<Value>(&payload_text).ok();
            Err(map_http_error_to_provider_error(status, error_json, &url))
        }
    }
}

impl ProviderDef for SnowflakeProvider {
    type Provider = Self;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            SNOWFLAKE_PROVIDER_NAME,
            "Snowflake",
            "Access the latest models using Snowflake Cortex services.",
            SNOWFLAKE_DEFAULT_MODEL,
            SNOWFLAKE_KNOWN_MODELS.to_vec(),
            SNOWFLAKE_DOC_URL,
            vec![
                ConfigKey::new("SNOWFLAKE_HOST", true, false, None, true),
                ConfigKey::new("SNOWFLAKE_TOKEN", true, true, None, true),
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
impl Provider for SnowflakeProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model.clone()
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(SNOWFLAKE_KNOWN_MODELS
            .iter()
            .map(|s| s.to_string())
            .collect())
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
        let payload = create_request(model_config, system, messages, tools)?;

        let mut log = RequestLog::start(&self.model, &payload)?;

        let response = self
            .with_retry(|| async {
                let payload_clone = payload.clone();
                self.post(session_id, &payload_clone).await
            })
            .await?;

        let message = response_to_message(&response)?;
        let usage = get_usage(&response)?;
        let response_model = get_model(&response);

        log.write(&response, Some(&usage))?;

        let provider_usage = ProviderUsage::new(response_model, usage);
        Ok(super::base::stream_from_single_message(
            message,
            provider_usage,
        ))
    }
}
