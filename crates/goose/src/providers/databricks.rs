use anyhow::Result;
use async_stream::try_stream;
use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::{StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::pin;
use tokio_util::codec::{FramedRead, LinesCodec};
use tokio_util::io::StreamReader;

use super::api_client::{ApiClient, AuthMethod, AuthProvider};
use super::base::{ConfigKey, MessageStream, Provider, ProviderDef, ProviderMetadata};
use super::embedding::EmbeddingCapable;
use super::errors::ProviderError;
use super::formats::databricks::create_request;
use super::formats::openai_responses::{
    create_responses_request, responses_api_to_streaming_message,
};
use super::oauth;
use super::openai_compatible::{
    handle_response_openai_compat, handle_status_openai_compat, map_http_error_to_provider_error,
    stream_openai_compat,
};
use super::retry::ProviderRetry;
use super::utils::{ImageFormat, RequestLog};
use crate::config::ConfigError;
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::providers::retry::{
    RetryConfig, DEFAULT_BACKOFF_MULTIPLIER, DEFAULT_INITIAL_RETRY_INTERVAL_MS,
    DEFAULT_MAX_RETRIES, DEFAULT_MAX_RETRY_INTERVAL_MS,
};
use rmcp::model::Tool;
use serde_json::json;

const DEFAULT_CLIENT_ID: &str = "databricks-cli";
const DEFAULT_REDIRECT_URL: &str = "http://localhost";
const DEFAULT_SCOPES: &[&str] = &["all-apis", "offline_access"];
const DEFAULT_TIMEOUT_SECS: u64 = 600;

const DATABRICKS_PROVIDER_NAME: &str = "databricks";
pub const DATABRICKS_DEFAULT_MODEL: &str = "databricks-claude-sonnet-4";
const DATABRICKS_DEFAULT_FAST_MODEL: &str = "databricks-claude-haiku-4-5";
pub const DATABRICKS_KNOWN_MODELS: &[&str] = &[
    "databricks-claude-sonnet-4-5",
    "databricks-meta-llama-3-3-70b-instruct",
    "databricks-meta-llama-3-1-405b-instruct",
];

pub const DATABRICKS_DOC_URL: &str =
    "https://docs.databricks.com/en/generative-ai/external-models/index.html";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DatabricksAuth {
    Token(String),
    OAuth {
        host: String,
        client_id: String,
        redirect_url: String,
        scopes: Vec<String>,
    },
}

impl DatabricksAuth {
    pub fn oauth(host: String) -> Self {
        Self::OAuth {
            host,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            redirect_url: DEFAULT_REDIRECT_URL.to_string(),
            scopes: DEFAULT_SCOPES.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn token(token: String) -> Self {
        Self::Token(token)
    }
}

struct DatabricksAuthProvider {
    auth: DatabricksAuth,
    token_cache: Arc<Mutex<Option<String>>>,
}

#[async_trait]
impl AuthProvider for DatabricksAuthProvider {
    async fn get_auth_header(&self) -> Result<(String, String)> {
        let token = match &self.auth {
            DatabricksAuth::Token(original) => {
                let cached = self.token_cache.lock().unwrap().clone();
                match cached {
                    Some(t) => t,
                    None => {
                        // Cache was cleared by refresh_credentials(); re-read
                        // from config which may have a sidecar-rotated token.
                        // Fall back to the constructor-provided token if config
                        // lookup fails (e.g. from_params usage).
                        let fresh = crate::config::Config::global()
                            .get_secret::<String>("DATABRICKS_TOKEN")
                            .unwrap_or_else(|_| original.clone());
                        *self.token_cache.lock().unwrap() = Some(fresh.clone());
                        fresh
                    }
                }
            }
            DatabricksAuth::OAuth {
                host,
                client_id,
                redirect_url,
                scopes,
            } => oauth::get_oauth_token_async(host, client_id, redirect_url, scopes).await?,
        };
        Ok(("Authorization".to_string(), format!("Bearer {}", token)))
    }
}

#[derive(Debug, serde::Serialize)]
pub struct DatabricksProvider {
    #[serde(skip)]
    api_client: ApiClient,
    auth: DatabricksAuth,
    model: ModelConfig,
    image_format: ImageFormat,
    #[serde(skip)]
    retry_config: RetryConfig,
    #[serde(skip)]
    fast_retry_config: RetryConfig,
    #[serde(skip)]
    name: String,
    #[serde(skip)]
    token_cache: Arc<Mutex<Option<String>>>,
}

impl DatabricksProvider {
    pub async fn cleanup() -> Result<()> {
        super::oauth::cleanup_oauth_cache()
    }

    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let config = crate::config::Config::global();

        let mut host: Result<String, ConfigError> = config.get_param("DATABRICKS_HOST");
        if host.is_err() {
            host = config.get_secret("DATABRICKS_HOST")
        }

        if host.is_err() {
            return Err(ConfigError::NotFound(
                "Did not find DATABRICKS_HOST in either config file or keyring".to_string(),
            )
            .into());
        }

        let host = host?;
        let retry_config = Self::load_retry_config(config);
        let fast_retry_config = Self::load_fast_retry_config(config);

        let auth = if let Ok(api_key) = config.get_secret("DATABRICKS_TOKEN") {
            DatabricksAuth::token(api_key)
        } else {
            DatabricksAuth::oauth(host.clone())
        };

        let token_cache = Arc::new(Mutex::new(match &auth {
            DatabricksAuth::Token(t) => Some(t.clone()),
            _ => None,
        }));

        let auth_method = AuthMethod::Custom(Box::new(DatabricksAuthProvider {
            auth: auth.clone(),
            token_cache: token_cache.clone(),
        }));

        let api_client =
            ApiClient::with_timeout(host, auth_method, Duration::from_secs(DEFAULT_TIMEOUT_SECS))?;

        let mut provider = Self {
            api_client,
            auth,
            model: model.clone(),
            image_format: ImageFormat::OpenAi,
            retry_config,
            fast_retry_config,
            name: DATABRICKS_PROVIDER_NAME.to_string(),
            token_cache,
        };
        provider.model =
            model.with_fast(DATABRICKS_DEFAULT_FAST_MODEL, DATABRICKS_PROVIDER_NAME)?;
        Ok(provider)
    }

    fn load_retry_config(config: &crate::config::Config) -> RetryConfig {
        let max_retries = config
            .get_param("DATABRICKS_MAX_RETRIES")
            .ok()
            .and_then(|v: String| v.parse::<usize>().ok())
            .unwrap_or(DEFAULT_MAX_RETRIES);

        let initial_interval_ms = config
            .get_param("DATABRICKS_INITIAL_RETRY_INTERVAL_MS")
            .ok()
            .and_then(|v: String| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_INITIAL_RETRY_INTERVAL_MS);

        let backoff_multiplier = config
            .get_param("DATABRICKS_BACKOFF_MULTIPLIER")
            .ok()
            .and_then(|v: String| v.parse::<f64>().ok())
            .unwrap_or(DEFAULT_BACKOFF_MULTIPLIER);

        let max_interval_ms = config
            .get_param("DATABRICKS_MAX_RETRY_INTERVAL_MS")
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

    fn load_fast_retry_config(_config: &crate::config::Config) -> RetryConfig {
        // Fast models are hardcoded to 0 retries for quick failure on Databricks
        RetryConfig::new(0, 0, 1.0, 0)
    }

    pub fn from_params(host: String, api_key: String, model: ModelConfig) -> Result<Self> {
        let token_cache = Arc::new(Mutex::new(Some(api_key.clone())));
        let auth = DatabricksAuth::token(api_key);
        let auth_method = AuthMethod::Custom(Box::new(DatabricksAuthProvider {
            auth: auth.clone(),
            token_cache: token_cache.clone(),
        }));

        let api_client = ApiClient::with_timeout(host, auth_method, Duration::from_secs(600))?;

        Ok(Self {
            api_client,
            auth,
            model,
            image_format: ImageFormat::OpenAi,
            retry_config: RetryConfig::default(),
            fast_retry_config: RetryConfig::new(0, 0, 1.0, 0),
            name: DATABRICKS_PROVIDER_NAME.to_string(),
            token_cache,
        })
    }

    fn is_responses_model(model_name: &str) -> bool {
        let normalized = model_name.to_ascii_lowercase();
        normalized.contains("codex")
    }

    fn get_endpoint_path(&self, model_name: &str, is_embedding: bool) -> String {
        if is_embedding {
            "serving-endpoints/text-embedding-3-small/invocations".to_string()
        } else if Self::is_responses_model(model_name) {
            "serving-endpoints/responses".to_string()
        } else {
            format!("serving-endpoints/{}/invocations", model_name)
        }
    }

    async fn post(
        &self,
        session_id: Option<&str>,
        payload: Value,
        model_name: Option<&str>,
    ) -> Result<Value, ProviderError> {
        let is_embedding = payload.get("input").is_some() && payload.get("messages").is_none();
        let model_to_use = model_name.unwrap_or(&self.model.model_name);
        let path = self.get_endpoint_path(model_to_use, is_embedding);

        let response = self
            .api_client
            .response_post(session_id, &path, &payload)
            .await?;
        handle_response_openai_compat(response).await
    }
}

impl ProviderDef for DatabricksProvider {
    type Provider = Self;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            DATABRICKS_PROVIDER_NAME,
            "Databricks",
            "Models on Databricks AI Gateway",
            DATABRICKS_DEFAULT_MODEL,
            DATABRICKS_KNOWN_MODELS.to_vec(),
            DATABRICKS_DOC_URL,
            vec![
                ConfigKey::new("DATABRICKS_HOST", true, false, None, true),
                ConfigKey::new("DATABRICKS_TOKEN", false, true, None, true),
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
impl Provider for DatabricksProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn retry_config(&self) -> RetryConfig {
        self.retry_config.clone()
    }

    async fn refresh_credentials(&self) -> Result<(), ProviderError> {
        crate::config::Config::global().invalidate_secrets_cache();
        *self.token_cache.lock().unwrap() = None;
        tracing::info!("Invalidated secrets cache and token cache for credential refresh");
        Ok(())
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
        let path = self.get_endpoint_path(&model_config.model_name, false);

        if Self::is_responses_model(&model_config.model_name) {
            let mut payload = create_responses_request(model_config, system, messages, tools)?;
            payload["stream"] = Value::Bool(true);

            let mut log = RequestLog::start(model_config, &payload)?;

            let response = self
                .with_retry(|| async {
                    let payload_clone = payload.clone();
                    let resp = self
                        .api_client
                        .response_post(Some(session_id), &path, &payload_clone)
                        .await?;
                    handle_status_openai_compat(resp).await
                })
                .await
                .inspect_err(|e| {
                    let _ = log.error(e);
                })?;

            let stream = response.bytes_stream().map_err(io::Error::other);

            Ok(Box::pin(try_stream! {
                let stream_reader = StreamReader::new(stream);
                let framed = FramedRead::new(stream_reader, LinesCodec::new()).map_err(anyhow::Error::from);

                let message_stream = responses_api_to_streaming_message(framed);
                pin!(message_stream);
                while let Some(message) = message_stream.next().await {
                    let (message, usage) = message.map_err(|e| ProviderError::RequestFailed(format!("Stream decode error: {}", e)))?;
                    log.write(&message, usage.as_ref().map(|f| f.usage).as_ref())?;
                    yield (message, usage);
                }
            }))
        } else {
            let mut payload =
                create_request(model_config, system, messages, tools, &self.image_format)?;
            payload
                .as_object_mut()
                .expect("payload should have model key")
                .remove("model");

            payload
                .as_object_mut()
                .unwrap()
                .insert("stream".to_string(), Value::Bool(true));

            if let Some(opts) = payload
                .get_mut("stream_options")
                .and_then(|v| v.as_object_mut())
            {
                opts.entry("include_usage").or_insert(json!(true));
            } else {
                payload
                    .as_object_mut()
                    .unwrap()
                    .insert("stream_options".to_string(), json!({"include_usage": true}));
            }

            let mut log = RequestLog::start(model_config, &payload)?;
            let response = self
                .with_retry(|| async {
                    let resp = self
                        .api_client
                        .response_post(Some(session_id), &path, &payload)
                        .await?;
                    if !resp.status().is_success() {
                        let status = resp.status();
                        let error_text = resp.text().await.unwrap_or_default();

                        let json_payload = serde_json::from_str::<Value>(&error_text).ok();
                        return Err(map_http_error_to_provider_error(status, json_payload));
                    }
                    Ok(resp)
                })
                .await;

            let response = match response {
                Err(e) if e.to_string().contains("stream_options") => {
                    payload.as_object_mut().unwrap().remove("stream_options");
                    self.with_retry(|| async {
                        let resp = self
                            .api_client
                            .response_post(Some(session_id), &path, &payload)
                            .await?;
                        if !resp.status().is_success() {
                            let status = resp.status();
                            let error_text = resp.text().await.unwrap_or_default();
                            let json_payload = serde_json::from_str::<Value>(&error_text).ok();
                            return Err(map_http_error_to_provider_error(status, json_payload));
                        }
                        Ok(resp)
                    })
                    .await
                    .inspect_err(|e| {
                        let _ = log.error(e);
                    })?
                }
                Err(e) => {
                    let _ = log.error(&e);
                    return Err(e);
                }
                Ok(resp) => resp,
            };

            stream_openai_compat(response, log)
        }
    }

    fn supports_embeddings(&self) -> bool {
        true
    }

    async fn create_embeddings(
        &self,
        session_id: &str,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, ProviderError> {
        EmbeddingCapable::create_embeddings(self, session_id, texts)
            .await
            .map_err(|e| ProviderError::ExecutionError(e.to_string()))
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        let response = self
            .api_client
            .request(None, "api/2.0/serving-endpoints")
            .response_get()
            .await
            .map_err(|e| {
                ProviderError::RequestFailed(format!("Failed to fetch Databricks models: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let detail = response.text().await.unwrap_or_default();
            return Err(ProviderError::RequestFailed(format!(
                "Failed to fetch Databricks models: {} {}",
                status, detail
            )));
        }

        let json: Value = response.json().await.map_err(|e| {
            ProviderError::RequestFailed(format!("Failed to parse Databricks API response: {}", e))
        })?;

        let endpoints = json
            .get("endpoints")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                ProviderError::RequestFailed(
                    "Unexpected response format from Databricks API: missing 'endpoints' array"
                        .to_string(),
                )
            })?;

        let models: Vec<String> = endpoints
            .iter()
            .filter_map(|endpoint| {
                endpoint
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|name| name.to_string())
            })
            .collect();

        Ok(models)
    }
}

#[async_trait]
impl EmbeddingCapable for DatabricksProvider {
    async fn create_embeddings(
        &self,
        session_id: &str,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let request = json!({
            "input": texts,
        });

        let response = self
            .with_retry_config(
                || self.post(Some(session_id), request.clone(), None),
                self.fast_retry_config.clone(),
            )
            .await?;

        let embeddings = response["data"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Invalid response format: missing data array"))?
            .iter()
            .map(|item| {
                item["embedding"]
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("Invalid embedding format"))?
                    .iter()
                    .map(|v| v.as_f64().map(|f| f as f32))
                    .collect::<Option<Vec<f32>>>()
                    .ok_or_else(|| anyhow::anyhow!("Invalid embedding values"))
            })
            .collect::<Result<Vec<Vec<f32>>>>()?;

        Ok(embeddings)
    }
}
