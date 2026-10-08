use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use aws_config;
use aws_sdk_bedrockruntime::config::ProvideCredentials;
use aws_sdk_sagemakerruntime::Client as SageMakerClient;
use rmcp::model::Tool;
use serde_json::{json, Value};
use smithy_transport_reqwest::ReqwestHttpClient;

use super::base::{ConfigKey, MessageStream, Provider, ProviderDef, ProviderMetadata};
use super::retry::ProviderRetry;
use crate::conversation::message::{Message, MessageContent};
use crate::session_context::SESSION_ID_HEADER;
use goose_providers::errors::ProviderError;

use chrono::Utc;
use futures::future::BoxFuture;
use goose_providers::conversation::token_usage::{ProviderUsage, Usage};
use goose_providers::model::ModelConfig;
use goose_providers::request_log::{start_log, LoggerHandleExt};
use rmcp::model::Role;

const SAGEMAKER_TGI_PROVIDER_NAME: &str = "sagemaker_tgi";
pub const SAGEMAKER_TGI_DOC_LINK: &str =
    "https://docs.aws.amazon.com/sagemaker/latest/dg/realtime-endpoints.html";

pub const SAGEMAKER_TGI_DEFAULT_MODEL: &str = "sagemaker-tgi-endpoint";

#[derive(Debug, serde::Serialize)]
pub struct SageMakerTgiProvider {
    #[serde(skip)]
    sagemaker_client: SageMakerClient,
    endpoint_name: String,
    #[serde(skip)]
    name: String,
}

impl SageMakerTgiProvider {
    pub async fn from_env(
        _tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> Result<Self> {
        let config = crate::config::Config::global();

        // Get SageMaker endpoint name (just the name, not full URL)
        let endpoint_name: String = config.get_param("SAGEMAKER_ENDPOINT_NAME").map_err(|_| {
            anyhow::anyhow!("SAGEMAKER_ENDPOINT_NAME is required for SageMaker TGI provider")
        })?;

        // Attempt to load config and secrets to get AWS_ prefixed keys
        let set_aws_env_vars = |res: Result<HashMap<String, Value>, _>| {
            if let Ok(map) = res {
                map.into_iter()
                    .filter(|(key, _)| key.starts_with("AWS_"))
                    .filter_map(|(key, value)| value.as_str().map(|s| (key, s.to_string())))
                    .for_each(|(key, s)| std::env::set_var(key, s));
            }
        };

        set_aws_env_vars(config.all_values());
        set_aws_env_vars(config.all_secrets());

        let aws_config = aws_config::from_env()
            .http_client(ReqwestHttpClient::new())
            .load()
            .await;

        // Validate credentials
        aws_config
            .credentials_provider()
            .unwrap()
            .provide_credentials()
            .await?;

        // Create client with longer timeout for model initialization
        let timeout_config = aws_config::timeout::TimeoutConfig::builder()
            .operation_timeout(Duration::from_secs(300)) // 5 minutes for cold starts
            .build();

        let config_with_timeout = aws_config
            .into_builder()
            .timeout_config(timeout_config)
            .build();

        let sagemaker_client = SageMakerClient::new(&config_with_timeout);

        Ok(Self {
            sagemaker_client,
            endpoint_name,
            name: SAGEMAKER_TGI_PROVIDER_NAME.to_string(),
        })
    }

    fn create_tgi_request(
        &self,
        model: &ModelConfig,
        system: &str,
        messages: &[Message],
    ) -> Result<Value> {
        // Create a simplified prompt for TGI models using recent user and assistant messages.
        // Uses a minimal system prompt and avoids HTML or tool-related formatting.
        let mut prompt = String::new();

        // Use a very simple system prompt if provided, but ensure it doesn't contain HTML instructions
        if !system.is_empty()
            && !system.contains("Available tools")
            && system.len() < 200
            && !system.contains("HTML")
            && !system.contains("markdown")
        {
            prompt.push_str(&format!("System: {}\n\n", system));
        } else {
            // Use a minimal system prompt for TGI that explicitly avoids HTML
            prompt.push_str("System: You are a helpful AI assistant. Provide responses in plain text only. Do not use HTML tags, markup, or formatting.\n\n");
        }

        // Only include the most recent user messages to avoid overwhelming the model
        let recent_messages: Vec<_> = messages.iter().rev().take(3).collect();
        for message in recent_messages.iter().rev() {
            match &message.role {
                Role::User => {
                    prompt.push_str("User: ");
                    for content in &message.content {
                        if let MessageContent::Text(text) = content {
                            prompt.push_str(&text.text);
                        }
                    }
                    prompt.push_str("\n\n");
                }
                Role::Assistant => {
                    prompt.push_str("Assistant: ");
                    for content in &message.content {
                        if let MessageContent::Text(text) = content {
                            // Skip responses that look like tool descriptions or contain HTML
                            if !text.text.contains("__")
                                && !text.text.contains("Available tools")
                                && !text.text.contains("<")
                            {
                                prompt.push_str(&text.text);
                            }
                        }
                    }
                    prompt.push_str("\n\n");
                }
            }
        }

        prompt.push_str("Assistant: ");

        // Skip tool descriptions entirely for TGI models to avoid confusion
        // TGI models don't support tools natively and including tool descriptions
        // causes them to mimic that format in their responses

        // Build TGI request with reasonable parameters
        let request = json!({
            "inputs": prompt,
            "parameters": {
                "max_new_tokens": model.max_output_tokens(),
                "temperature": model.temperature.unwrap_or(0.7),
                "do_sample": true,
                "return_full_text": false
            }
        });

        Ok(request)
    }

    async fn invoke_endpoint(
        &self,
        session_id: Option<&str>,
        payload: Value,
    ) -> Result<Value, ProviderError> {
        let body = serde_json::to_string(&payload).map_err(|e| {
            ProviderError::RequestFailed(format!("Failed to serialize request: {}", e))
        })?;

        let mut request = self
            .sagemaker_client
            .invoke_endpoint()
            .endpoint_name(&self.endpoint_name)
            .content_type("application/json")
            .body(body.into_bytes().into());

        if let Some(session_id) = session_id.filter(|id| !id.is_empty()) {
            request = request.custom_attributes(format!("{SESSION_ID_HEADER}={session_id}"));
        }

        let response = request
            .send()
            .await
            .map_err(|e| ProviderError::RequestFailed(format!("SageMaker invoke failed: {}", e)))?;

        let response_body = response
            .body
            .as_ref()
            .ok_or_else(|| ProviderError::RequestFailed("Empty response body".to_string()))?;
        let response_text = std::str::from_utf8(response_body.as_ref()).map_err(|e| {
            ProviderError::RequestFailed(format!("Failed to decode response: {}", e))
        })?;

        serde_json::from_str(response_text).map_err(|e| {
            ProviderError::RequestFailed(format!("Failed to parse response JSON: {}", e))
        })
    }

    fn parse_tgi_response(&self, response: Value) -> Result<Message, ProviderError> {
        // Handle standard TGI response: [{"generated_text": "..."}]
        let response_array = response
            .as_array()
            .ok_or_else(|| ProviderError::RequestFailed("Expected array response".to_string()))?;

        if response_array.is_empty() {
            return Err(ProviderError::RequestFailed(
                "Empty response array".to_string(),
            ));
        }

        let first_result = &response_array[0];
        let generated_text = first_result
            .get("generated_text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ProviderError::RequestFailed("No generated_text in response".to_string())
            })?;

        // Strip any HTML tags that might have been generated
        let clean_text = self.strip_html_tags(generated_text);

        Ok(Message::new(
            Role::Assistant,
            Utc::now().timestamp(),
            vec![MessageContent::text(clean_text)],
        ))
    }

    /// Strip HTML tags from text to ensure clean output
    fn strip_html_tags(&self, text: &str) -> String {
        // Simple regex-free approach to strip common HTML tags
        let mut result = text.to_string();

        // Remove common HTML tags like <b>, <i>, <strong>, <em>, etc.
        let tags_to_remove = [
            "<b>",
            "</b>",
            "<i>",
            "</i>",
            "<strong>",
            "</strong>",
            "<em>",
            "</em>",
            "<u>",
            "</u>",
            "<br>",
            "<br/>",
            "<p>",
            "</p>",
            "<div>",
            "</div>",
            "<span>",
            "</span>",
        ];

        for tag in &tags_to_remove {
            result = result.replace(tag, "");
        }

        // Remove any remaining HTML-like tags using a simple pattern
        // This is a basic implementation - for production use, consider using a proper HTML parser
        while let Some(start) = result.find('<') {
            if let Some(end) = result.get(start..).and_then(|s| s.find('>')) {
                result.replace_range(start..start + end + 1, "");
            } else {
                break;
            }
        }

        result.trim().to_string()
    }
}

impl goose_providers::base::ProviderDescriptor for SageMakerTgiProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            SAGEMAKER_TGI_PROVIDER_NAME,
            "Amazon SageMaker TGI",
            "Run Text Generation Inference models through Amazon SageMaker endpoints. Requires AWS credentials and a SageMaker endpoint URL.",
            SAGEMAKER_TGI_DEFAULT_MODEL,
            vec![SAGEMAKER_TGI_DEFAULT_MODEL],
            SAGEMAKER_TGI_DOC_LINK,
            vec![
                ConfigKey::new("SAGEMAKER_ENDPOINT_NAME", true, false, None, true),
                ConfigKey::new("AWS_REGION", false, false, Some("us-east-1"), true),
                ConfigKey::new("AWS_PROFILE", false, false, Some("default"), true),
            ],
        )
    }
}

impl ProviderDef for SageMakerTgiProvider {
    type Provider = Self;

    fn from_env(
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(Self::from_env(tls_config))
    }
}

#[async_trait]
impl Provider for SageMakerTgiProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let session_id = crate::session_context::current_session_id().unwrap_or_default();
        let session_id = if session_id.is_empty() {
            None
        } else {
            Some(session_id.as_str())
        };
        let model_name = &model_config.model_name;

        let request_payload = self
            .create_tgi_request(model_config, system, messages)
            .map_err(|e| {
                ProviderError::RequestFailed(format!("Failed to create request: {}", e))
            })?;

        let response = self
            .with_retry(|| self.invoke_endpoint(session_id, request_payload.clone()))
            .await?;

        let message = self.parse_tgi_response(response)?;

        // TGI doesn't provide usage statistics, so we estimate
        let usage = Usage::new(
            Some(0), // Would need to tokenize input to get accurate count
            Some(0), // Would need to tokenize output to get accurate count
            Some(0),
        );

        // Add debug trace
        let debug_payload = serde_json::json!({
            "system": system,
            "messages": messages,
            "tools": tools
        });
        let mut log = start_log(model_config, &debug_payload)?;
        log.write(
            &serde_json::to_value(&message).unwrap_or_default(),
            Some(&usage),
        )?;

        let provider_usage = ProviderUsage::new(model_name.to_string(), usage);
        Ok(super::base::stream_from_single_message(
            message,
            provider_usage,
        ))
    }
}
