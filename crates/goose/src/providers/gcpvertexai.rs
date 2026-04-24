use std::io;
use std::time::Duration;

use anyhow::Result;
use async_stream::try_stream;
use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::StreamExt;
use futures::TryStreamExt;
use once_cell::sync::Lazy;
use reqwest::{Client, StatusCode};
use serde_json::Value;
use tokio::time::sleep;
use tokio_util::io::StreamReader;
use url::Url;

use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::providers::base::{ConfigKey, MessageStream, Provider, ProviderDef, ProviderMetadata};

use crate::providers::errors::ProviderError;
use crate::providers::formats::gcpvertexai::{
    create_request, response_to_streaming_message, GcpLocation, ModelProvider, RequestContext,
    DEFAULT_MODEL, KNOWN_MODELS,
};
use crate::providers::gcpauth::GcpAuth;
use crate::providers::openai_compatible::map_http_error_to_provider_error;
use crate::providers::retry::RetryConfig;
use crate::providers::utils::RequestLog;
use crate::session_context::SESSION_ID_HEADER;
use rmcp::model::Tool;

const GCP_VERTEX_AI_PROVIDER_NAME: &str = "gcp_vertex_ai";
/// Base URL for GCP Vertex AI documentation
const GCP_VERTEX_AI_DOC_URL: &str = "https://cloud.google.com/vertex-ai";
/// Default timeout for API requests in seconds
const DEFAULT_TIMEOUT_SECS: u64 = 600;
/// Default initial interval for retry (in milliseconds)
const DEFAULT_INITIAL_RETRY_INTERVAL_MS: u64 = 5000;
/// Default maximum number of retries
const DEFAULT_MAX_RETRIES: usize = 6;
/// Default retry backoff multiplier
const DEFAULT_BACKOFF_MULTIPLIER: f64 = 2.0;
/// Default maximum interval for retry (in milliseconds)
const DEFAULT_MAX_RETRY_INTERVAL_MS: u64 = 320_000;
/// Status code for Anthropic's API overloaded error (529)
static STATUS_API_OVERLOADED: Lazy<StatusCode> =
    Lazy::new(|| StatusCode::from_u16(529).expect("Valid status code 529 for API_OVERLOADED"));

fn rate_limit_error_message(response_text: &str) -> String {
    let cite = "See https://cloud.google.com/vertex-ai/generative-ai/docs/error-code-429";
    if response_text.contains("Exceeded the Provisioned Throughput") {
        format!("Exceeded the Provisioned Throughput: {cite}")
    } else {
        format!("Pay-as-you-go resource exhausted: {cite}")
    }
}

const OVERLOADED_ERROR_MSG: &str =
    "Vertex AI Provider API is temporarily overloaded. This is similar to a rate limit \
     error but indicates backend processing capacity issues.";

fn build_vertex_url(
    host: &str,
    configured_location: &str,
    project_id: &str,
    model_name: &str,
    provider: ModelProvider,
    target_location: &str,
    streaming: bool,
) -> Result<Url, GcpVertexAIError> {
    let host_url = if configured_location == target_location {
        host.to_string()
    } else {
        GcpVertexAIProvider::build_host_url(target_location)
    };

    let base_url =
        Url::parse(&host_url).map_err(|e| GcpVertexAIError::InvalidUrl(e.to_string()))?;

    let endpoint = match (&provider, streaming) {
        (ModelProvider::Anthropic, true) => "streamRawPredict",
        (ModelProvider::Anthropic, false) => "rawPredict",
        (ModelProvider::Google, true) => "streamGenerateContent",
        (ModelProvider::Google, false) => "generateContent",
        (ModelProvider::MaaS(_), true) => "streamGenerateContent",
        (ModelProvider::MaaS(_), false) => "generateContent",
    };

    // Anthropic's Vertex AI docs specify v1 for rawPredict endpoints.
    // The official google-genai SDK uses v1beta1 for Google models,
    // and the global endpoint requires it.
    let api_version = match &provider {
        ModelProvider::Anthropic => "v1",
        ModelProvider::Google | ModelProvider::MaaS(_) => "v1beta1",
    };

    let path = format!(
        "{}/projects/{}/locations/{}/publishers/{}/models/{}:{}",
        api_version,
        project_id,
        target_location,
        provider.as_str(),
        model_name,
        endpoint
    );

    let mut url = base_url
        .join(&path)
        .map_err(|e| GcpVertexAIError::InvalidUrl(e.to_string()))?;

    if streaming && !matches!(provider, ModelProvider::Anthropic) {
        url.set_query(Some("alt=sse"));
    }

    Ok(url)
}

/// Represents errors specific to GCP Vertex AI operations.
#[derive(Debug, thiserror::Error)]
enum GcpVertexAIError {
    /// Error when URL construction fails
    #[error("Invalid URL configuration: {0}")]
    InvalidUrl(String),

    /// Error during GCP authentication
    #[error("Authentication error: {0}")]
    AuthError(String),
}

/// Provider implementation for Google Cloud Platform's Vertex AI service.
///
/// This provider enables interaction with various AI models hosted on GCP Vertex AI,
/// including Claude and Gemini model families. It handles authentication, request routing,
/// and response processing for the Vertex AI API endpoints.
#[derive(Debug, serde::Serialize)]
pub struct GcpVertexAIProvider {
    /// HTTP client for making API requests
    #[serde(skip)]
    client: Client,
    /// GCP authentication handler
    #[serde(skip)]
    auth: GcpAuth,
    /// Base URL for the Vertex AI API
    host: String,
    /// GCP project identifier
    project_id: String,
    /// GCP region for model deployment
    location: String,
    /// Configuration for the specific model being used
    model: ModelConfig,
    /// Retry configuration for handling rate limit errors
    #[serde(skip)]
    retry_config: RetryConfig,
    #[serde(skip)]
    name: String,
}

impl GcpVertexAIProvider {
    /// Creates a new provider instance from environment configuration.
    ///
    /// This is a convenience method that initializes the provider using
    /// environment variables and default settings.
    ///
    /// # Arguments
    /// * `model` - Configuration for the model to be used
    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let config = crate::config::Config::global();
        let project_id = config.get_param("GCP_PROJECT_ID")?;
        let location = Self::determine_location(config)?;
        let host = Self::build_host_url(&location);

        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()?;

        let auth = GcpAuth::new().await?;

        // Load optional retry configuration from environment
        let retry_config = Self::load_retry_config(config);

        Ok(Self {
            client,
            auth,
            host,
            project_id,
            location,
            model,
            retry_config,
            name: GCP_VERTEX_AI_PROVIDER_NAME.to_string(),
        })
    }

    /// Loads retry configuration from environment variables or uses defaults.
    fn load_retry_config(config: &crate::config::Config) -> RetryConfig {
        // Load max retries for 429 rate limit errors
        let max_retries = config
            .get_param("GCP_MAX_RETRIES")
            .ok()
            .and_then(|v: String| v.parse::<usize>().ok())
            .unwrap_or(DEFAULT_MAX_RETRIES);

        let initial_interval_ms = config
            .get_param("GCP_INITIAL_RETRY_INTERVAL_MS")
            .ok()
            .and_then(|v: String| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_INITIAL_RETRY_INTERVAL_MS);

        let backoff_multiplier = config
            .get_param("GCP_BACKOFF_MULTIPLIER")
            .ok()
            .and_then(|v: String| v.parse::<f64>().ok())
            .unwrap_or(DEFAULT_BACKOFF_MULTIPLIER);

        let max_interval_ms = config
            .get_param("GCP_MAX_RETRY_INTERVAL_MS")
            .ok()
            .and_then(|v: String| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_MAX_RETRY_INTERVAL_MS);

        RetryConfig::new(
            max_retries,
            initial_interval_ms,
            backoff_multiplier,
            max_interval_ms,
        )
    }

    /// Determines the appropriate GCP location for model deployment.
    ///
    /// Location is determined in the following order:
    /// 1. Custom location from GCP_LOCATION environment variable
    /// 2. Global default location (Iowa)
    fn determine_location(config: &crate::config::Config) -> Result<String> {
        Ok(config
            .get_param("GCP_LOCATION")
            .ok()
            .filter(|location: &String| !location.trim().is_empty())
            .unwrap_or_else(|| GcpLocation::Iowa.to_string()))
    }

    fn build_host_url(location: &str) -> String {
        if location == "global" {
            "https://aiplatform.googleapis.com".to_string()
        } else {
            format!("https://{}-aiplatform.googleapis.com", location)
        }
    }

    /// Retrieves an authentication token for API requests.
    async fn get_auth_header(&self) -> Result<String, GcpVertexAIError> {
        self.auth
            .get_token()
            .await
            .map(|token| format!("Bearer {}", token.token_value))
            .map_err(|e| GcpVertexAIError::AuthError(e.to_string()))
    }

    fn build_request_url(
        &self,
        provider: ModelProvider,
        location: &str,
        streaming: bool,
    ) -> Result<Url, GcpVertexAIError> {
        build_vertex_url(
            &self.host,
            &self.location,
            &self.project_id,
            &self.model.model_name,
            provider,
            location,
            streaming,
        )
    }

    async fn send_request_with_retry(
        &self,
        session_id: Option<&str>,
        url: Url,
        payload: &Value,
    ) -> Result<reqwest::Response, ProviderError> {
        let mut rate_limit_attempts = 0;
        let mut overloaded_attempts = 0;
        let mut last_error = None;
        let max_retries = self.retry_config.max_retries;

        loop {
            if rate_limit_attempts > max_retries && overloaded_attempts > max_retries {
                return Err(
                    last_error.unwrap_or_else(|| ProviderError::RateLimitExceeded {
                        details: format!("Exceeded maximum retry attempts ({max_retries})"),
                        retry_delay: None,
                    }),
                );
            }

            let auth_header = self
                .get_auth_header()
                .await
                .map_err(|e| ProviderError::Authentication(e.to_string()))?;

            let mut request = self
                .client
                .post(url.clone())
                .json(payload)
                .header("Authorization", auth_header);

            if let Some(session_id) = session_id.filter(|id| !id.is_empty()) {
                request = request.header(SESSION_ID_HEADER, session_id);
            }

            let response = request
                .send()
                .await
                .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

            let status = response.status();

            if status == StatusCode::TOO_MANY_REQUESTS {
                rate_limit_attempts += 1;
                if rate_limit_attempts > max_retries {
                    return Err(
                        last_error.unwrap_or_else(|| ProviderError::RateLimitExceeded {
                            details: format!("Exceeded max retries ({max_retries}) for 429"),
                            retry_delay: None,
                        }),
                    );
                }
                let msg = rate_limit_error_message(&response.text().await.unwrap_or_default());
                tracing::warn!("429 (attempt {rate_limit_attempts}/{max_retries}): {msg}");
                last_error = Some(ProviderError::RateLimitExceeded {
                    details: msg,
                    retry_delay: None,
                });
                sleep(self.retry_config.delay_for_attempt(rate_limit_attempts)).await;
            } else if status == *STATUS_API_OVERLOADED {
                overloaded_attempts += 1;
                if overloaded_attempts > max_retries {
                    return Err(
                        last_error.unwrap_or_else(|| ProviderError::RateLimitExceeded {
                            details: format!("Exceeded max retries ({max_retries}) for 529"),
                            retry_delay: None,
                        }),
                    );
                }
                tracing::warn!(
                    "529 (attempt {overloaded_attempts}/{max_retries}): {OVERLOADED_ERROR_MSG}"
                );
                last_error = Some(ProviderError::RateLimitExceeded {
                    details: OVERLOADED_ERROR_MSG.to_string(),
                    retry_delay: None,
                });
                sleep(self.retry_config.delay_for_attempt(overloaded_attempts)).await;
            } else if status == StatusCode::OK {
                return Ok(response);
            } else if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                return Err(ProviderError::Authentication(format!(
                    "Authentication failed with status: {status}"
                )));
            } else {
                let response_text = response.text().await.unwrap_or_default();
                let payload = serde_json::from_str::<Value>(&response_text).ok();
                return Err(map_http_error_to_provider_error(status, payload));
            }
        }
    }

    async fn post_stream_with_location(
        &self,
        session_id: Option<&str>,
        payload: &Value,
        context: &RequestContext,
        location: &str,
    ) -> Result<reqwest::Response, ProviderError> {
        let url = self
            .build_request_url(context.provider(), location, true)
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

        self.send_request_with_retry(session_id, url, payload).await
    }

    async fn post_stream(
        &self,
        session_id: Option<&str>,
        payload: &Value,
        context: &RequestContext,
    ) -> Result<reqwest::Response, ProviderError> {
        let result = self
            .post_stream_with_location(session_id, payload, context, &self.location)
            .await;

        if self.location == context.model.known_location().to_string() || result.is_ok() {
            return result;
        }

        match &result {
            Err(ProviderError::RequestFailed(msg)) => {
                let model_name = context.model.to_string();
                let configured_location = &self.location;
                let known_location = context.model.known_location().to_string();

                tracing::warn!(
                    "Trying known location {known_location} for {model_name} instead of {configured_location}: {msg}"
                );

                self.post_stream_with_location(session_id, payload, context, &known_location)
                    .await
            }
            _ => result,
        }
    }

    async fn filter_by_org_policy(&self, models: Vec<String>) -> Vec<String> {
        let Ok(auth_header) = self.get_auth_header().await else {
            tracing::debug!("Could not get auth header for org policy check, returning all models");
            return models;
        };

        let url = format!(
            "https://cloudresourcemanager.googleapis.com/v1/projects/{}:getEffectiveOrgPolicy",
            self.project_id
        );

        let payload = serde_json::json!({
            "constraint": "constraints/vertexai.allowedModels"
        });

        let response = match self
            .client
            .post(&url)
            .header("Authorization", &auth_header)
            .json(&payload)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!("Failed to fetch org policy: {e}, returning all models");
                return models;
            }
        };

        let json = match response.json::<Value>().await {
            Ok(j) => j,
            Err(e) => {
                tracing::debug!("Failed to parse org policy response: {e}, returning all models");
                return models;
            }
        };

        let allowed_patterns: Vec<String> = json
            .get("listPolicy")
            .and_then(|lp| lp.get("allowedValues"))
            .and_then(|av| av.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();

        if allowed_patterns.is_empty() {
            return models;
        }

        models
            .into_iter()
            .filter(|model| Self::is_model_allowed(model, &allowed_patterns))
            .collect()
    }

    fn is_model_allowed(model: &str, allowed_patterns: &[String]) -> bool {
        let publisher = if model.starts_with("claude-") {
            "anthropic"
        } else if model.starts_with("gemini-") {
            "google"
        } else {
            return true;
        };

        for pattern in allowed_patterns {
            if pattern.contains(&format!("publishers/{publisher}/models/*")) {
                return true;
            }

            let pattern_model = pattern
                .split("/models/")
                .nth(1)
                .map(|s| s.trim_end_matches(":predict").trim_end_matches(":*"));

            if let Some(pattern_model) = pattern_model {
                if model == pattern_model || model.starts_with(&format!("{pattern_model}@")) {
                    return true;
                }
            }
        }

        false
    }
}

impl ProviderDef for GcpVertexAIProvider {
    type Provider = Self;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            GCP_VERTEX_AI_PROVIDER_NAME,
            "GCP Vertex AI",
            "Access variety of AI models such as Claude, Gemini through Vertex AI",
            DEFAULT_MODEL,
            KNOWN_MODELS.to_vec(),
            GCP_VERTEX_AI_DOC_URL,
            vec![
                ConfigKey::new("GCP_PROJECT_ID", true, false, None, true),
                ConfigKey::new(
                    "GCP_LOCATION",
                    true,
                    false,
                    Some(&GcpLocation::Iowa.to_string()),
                    true,
                ),
                ConfigKey::new(
                    "GCP_MAX_RETRIES",
                    false,
                    false,
                    Some(&DEFAULT_MAX_RETRIES.to_string()),
                    false,
                ),
                ConfigKey::new(
                    "GCP_INITIAL_RETRY_INTERVAL_MS",
                    false,
                    false,
                    Some(&DEFAULT_INITIAL_RETRY_INTERVAL_MS.to_string()),
                    false,
                ),
                ConfigKey::new(
                    "GCP_BACKOFF_MULTIPLIER",
                    false,
                    false,
                    Some(&DEFAULT_BACKOFF_MULTIPLIER.to_string()),
                    false,
                ),
                ConfigKey::new(
                    "GCP_MAX_RETRY_INTERVAL_MS",
                    false,
                    false,
                    Some(&DEFAULT_MAX_RETRY_INTERVAL_MS.to_string()),
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
impl Provider for GcpVertexAIProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    /// Completes a model interaction by sending a request and processing the response.
    ///
    /// # Arguments
    /// * `system` - System prompt or context
    /// * `messages` - Array of previous messages in the conversation
    /// * `tools` - Array of available tools for the model
    /// Returns the current model configuration.
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
        let (mut request, context) = create_request(model_config, system, messages, tools)?;

        if matches!(context.provider(), ModelProvider::Anthropic) {
            if let Some(obj) = request.as_object_mut() {
                obj.insert("stream".to_string(), Value::Bool(true));
            }
        }

        let mut log = RequestLog::start(model_config, &request)?;

        let response = self
            .post_stream(Some(session_id), &request, &context)
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;

        let stream = response.bytes_stream().map_err(io::Error::other);

        let context_clone = context.clone();
        Ok(Box::pin(try_stream! {
            let stream_reader = StreamReader::new(stream);
            let framed = tokio_util::codec::FramedRead::new(
                stream_reader,
                tokio_util::codec::LinesCodec::new(),
            )
            .map_err(anyhow::Error::from);

            let mut message_stream = response_to_streaming_message(framed, &context_clone);

            while let Some(message) = message_stream.next().await {
                let (message, usage) = message
                    .map_err(|e| ProviderError::RequestFailed(format!("Stream decode error: {}", e)))?;
                log.write(&message, usage.as_ref().map(|u| &u.usage))?;
                yield (message, usage);
            }
        }))
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        let models: Vec<String> = KNOWN_MODELS.iter().map(|s| s.to_string()).collect();
        let filtered = self.filter_by_org_policy(models).await;
        Ok(filtered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    #[test]
    fn test_retry_config_delay_calculation() {
        let config = RetryConfig::new(5, 1000, 2.0, 32000);

        // First attempt has no delay
        let delay0 = config.delay_for_attempt(0);
        assert_eq!(delay0.as_millis(), 0);

        // First retry should be around initial_interval with jitter
        let delay1 = config.delay_for_attempt(1);
        assert!(delay1.as_millis() >= 800 && delay1.as_millis() <= 1200);

        // Second retry should be around initial_interval * multiplier^1 with jitter
        let delay2 = config.delay_for_attempt(2);
        assert!(delay2.as_millis() >= 1600 && delay2.as_millis() <= 2400);

        // Check that max interval is respected
        let delay10 = config.delay_for_attempt(10);
        assert!(delay10.as_millis() <= 38400); // max_interval_ms * 1.2 (max jitter)
    }

    #[test]
    fn test_status_overloaded_code() {
        // Test that we correctly handle the 529 status code

        // Verify the custom status code is created correctly
        assert_eq!(STATUS_API_OVERLOADED.as_u16(), 529);

        // This is not a standard HTTP status code, so it's classified as server error
        assert!(STATUS_API_OVERLOADED.is_server_error());

        // Should be different from TOO_MANY_REQUESTS (429)
        assert_ne!(*STATUS_API_OVERLOADED, StatusCode::TOO_MANY_REQUESTS);

        // Should be different from SERVICE_UNAVAILABLE (503)
        assert_ne!(*STATUS_API_OVERLOADED, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn test_model_provider_conversion() {
        assert_eq!(ModelProvider::Anthropic.as_str(), "anthropic".to_string());
        assert_eq!(ModelProvider::Google.as_str(), "google".to_string());
        assert_eq!(
            ModelProvider::MaaS("qwen".to_string()).as_str(),
            "qwen".to_string()
        );
    }

    #[test]
    fn test_build_vertex_url_endpoints() {
        let anthropic_url = build_vertex_url(
            "https://us-east5-aiplatform.googleapis.com",
            "us-east5",
            "test-project",
            "claude-sonnet-4@20250514",
            ModelProvider::Anthropic,
            "us-east5",
            false,
        )
        .unwrap();
        assert!(anthropic_url.as_str().contains(":rawPredict"));

        let anthropic_stream = build_vertex_url(
            "https://us-east5-aiplatform.googleapis.com",
            "us-east5",
            "test-project",
            "claude-sonnet-4@20250514",
            ModelProvider::Anthropic,
            "us-east5",
            true,
        )
        .unwrap();
        assert!(anthropic_stream.as_str().contains(":streamRawPredict"));
        assert!(anthropic_stream.query().is_none());

        let google_stream = build_vertex_url(
            "https://us-central1-aiplatform.googleapis.com",
            "us-central1",
            "test-project",
            "gemini-2.5-flash",
            ModelProvider::Google,
            "us-central1",
            true,
        )
        .unwrap();
        assert!(google_stream.as_str().contains(":streamGenerateContent"));
        assert_eq!(google_stream.query(), Some("alt=sse"));
    }

    #[test]
    fn test_build_vertex_url_location_replacement() {
        let url = build_vertex_url(
            "https://us-east5-aiplatform.googleapis.com",
            "us-east5",
            "test-project",
            "claude-sonnet-4@20250514",
            ModelProvider::Anthropic,
            "europe-west1",
            false,
        )
        .unwrap();

        assert!(url
            .as_str()
            .contains("europe-west1-aiplatform.googleapis.com"));
        assert!(url.as_str().contains("locations/europe-west1"));
    }

    #[test]
    fn test_build_vertex_url_global_location_fallback() {
        // When configured_location is "global", the host is
        // "https://aiplatform.googleapis.com" which does not contain "global".
        // Falling back to a regional target_location must rebuild the host
        // via build_host_url rather than string replacement.
        let url = build_vertex_url(
            "https://aiplatform.googleapis.com",
            "global",
            "test-project",
            "claude-haiku-4-5@20251001",
            ModelProvider::Anthropic,
            "us-east5",
            true,
        )
        .unwrap();

        assert!(
            url.as_str()
                .starts_with("https://us-east5-aiplatform.googleapis.com"),
            "Expected regional host for us-east5, got: {}",
            url
        );
        assert!(
            url.as_str().contains("locations/us-east5"),
            "Expected locations/us-east5 in path, got: {}",
            url
        );
        assert!(url.as_str().contains(":streamRawPredict"));
    }

    #[test]
    fn test_build_vertex_url_global_location_same() {
        // When both configured and target are "global", the host should stay as-is.
        let url = build_vertex_url(
            "https://aiplatform.googleapis.com",
            "global",
            "test-project",
            "claude-haiku-4-5@20251001",
            ModelProvider::Anthropic,
            "global",
            true,
        )
        .unwrap();

        assert!(
            url.as_str()
                .starts_with("https://aiplatform.googleapis.com"),
            "Expected global host, got: {}",
            url
        );
        assert!(url.as_str().contains("locations/global"));
    }

    #[test]
    fn test_provider_metadata() {
        let metadata = GcpVertexAIProvider::metadata();
        assert!(!metadata.known_models.is_empty());
        assert_eq!(metadata.default_model, "gemini-2.5-flash");
        assert_eq!(metadata.config_keys.len(), 6);
    }
}
