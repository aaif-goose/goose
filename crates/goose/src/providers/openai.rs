use super::api_client::{ApiClient, AuthMethod};
use super::base::{ConfigKey, ModelInfo, Provider, ProviderDef, ProviderMetadata};
use super::embedding::{EmbeddingCapable, EmbeddingRequest, EmbeddingResponse};
use super::errors::ProviderError;
use super::formats::openai::{create_request, get_usage, response_to_message};
use super::formats::openai_responses::{
    create_responses_request, get_responses_usage, responses_api_to_message,
    responses_api_to_streaming_message, ResponsesApiResponse,
};
use super::inventory::{config_secret_value, InventoryIdentityInput};
use super::openai_compatible::{
    handle_response_openai_compat, handle_status, stream_openai_compat,
};
use super::retry::ProviderRetry;
use super::utils::ImageFormat;
use crate::config::declarative_providers::DeclarativeProviderConfig;
use crate::conversation::message::Message;
use anyhow::Result;
use async_stream::try_stream;
use async_trait::async_trait;
use futures::future::BoxFuture;
use futures::{StreamExt, TryStreamExt};
use reqwest::StatusCode;
use std::collections::HashMap;
use std::io;
use tokio::pin;
use tokio_util::codec::{FramedRead, LinesCodec};
use tokio_util::io::StreamReader;

use crate::model::ModelConfig;
use crate::providers::base::MessageStream;
use crate::providers::utils::RequestLog;
use rmcp::model::Tool;

const OPEN_AI_PROVIDER_NAME: &str = "openai";
const OPEN_AI_DEFAULT_BASE_PATH: &str = "v1/chat/completions";
const OPEN_AI_VERSIONLESS_BASE_PATH: &str = "chat/completions";
const OPEN_AI_DEFAULT_RESPONSES_PATH: &str = "v1/responses";
const OPEN_AI_DEFAULT_MODELS_PATH: &str = "v1/models";
const OPEN_AI_DEFAULT_EMBEDDINGS_PATH: &str = "v1/embeddings";
pub const OPEN_AI_DEFAULT_MODEL: &str = "gpt-4o";
pub const OPEN_AI_DEFAULT_FAST_MODEL: &str = "gpt-4o-mini";
pub const OPEN_AI_KNOWN_MODELS: &[(&str, usize)] = &[
    ("gpt-4o", 128_000),
    ("gpt-4o-mini", 128_000),
    ("gpt-4.1", 128_000),
    ("gpt-4.1-mini", 128_000),
    ("o1", 200_000),
    ("o3", 200_000),
    ("gpt-3.5-turbo", 16_385),
    ("gpt-4-turbo", 128_000),
    ("o4-mini", 128_000),
    ("gpt-5", 400_000),
    ("gpt-5-mini", 400_000),
    ("gpt-5-nano", 400_000),
    ("gpt-5-pro", 400_000),
    ("gpt-5-codex", 400_000),
    ("gpt-5.1", 400_000),
    ("gpt-5.1-codex", 400_000),
    ("gpt-5.2", 400_000),
    ("gpt-5.2-codex", 400_000),
    ("gpt-5.2-pro", 400_000),
    ("gpt-5.3-codex", 400_000),
    ("gpt-5.4", 1_050_000),
    ("gpt-5.4-mini", 400_000),
    ("gpt-5.4-nano", 400_000),
    ("gpt-5.4-pro", 1_050_000),
];

pub const OPEN_AI_DOC_URL: &str = "https://platform.openai.com/docs/models";

type OpenAiBaseUrlParts = (String, Vec<(String, String)>, bool);

/// Components extracted from an `OPENAI_BASE_URL` value.
struct ParsedBaseUrl {
    /// The host (scheme + authority + any path prefix before `/v1`).
    host: String,
    /// Query parameters to forward on every request.
    query_params: Vec<(String, String)>,
    /// Whether the URL path ended with `/v1`.
    has_v1: bool,
    /// `true` when the host was derived from `OPENAI_BASE_URL`.
    /// Controls whether `OPENAI_BASE_PATH` is read from env only
    /// (to avoid persisted desktop defaults shadowing URL-derived paths)
    /// or from config too (to honour Docker Model Runner setups).
    from_base_url: bool,
}

pub(crate) fn parse_openai_base_url(raw_url: &str) -> Result<OpenAiBaseUrlParts> {
    let parsed = url::Url::parse(raw_url)
        .map_err(|e| anyhow::anyhow!("Invalid OPENAI_BASE_URL '{}': {}", raw_url, e))?;

    let authority = parsed[..url::Position::BeforePath].to_string();
    let query_params: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    let path = parsed.path().trim_end_matches('/');
    if path.is_empty() || path == "/" {
        return Ok((authority, query_params, true));
    }

    if path == "/v1" {
        return Ok((authority, query_params, true));
    }
    if let Some(prefix) = path.strip_suffix("/v1") {
        return Ok((format!("{}{}", authority, prefix), query_params, true));
    }

    Ok((format!("{}{}", authority, path), query_params, false))
}

#[derive(Debug, serde::Serialize)]
pub struct OpenAiProvider {
    #[serde(skip)]
    api_client: ApiClient,
    base_path: String,
    organization: Option<String>,
    project: Option<String>,
    model: ModelConfig,
    custom_headers: Option<HashMap<String, String>>,
    supports_streaming: bool,
    name: String,
    custom_models: Option<Vec<String>>,
    skip_canonical_filtering: bool,
}

impl OpenAiProvider {
    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let config = crate::config::Config::global();

        // Resolve host and base_path.
        //
        // Priority (highest first):
        //   1. OPENAI_HOST env var — session override (deprecated but still
        //      honoured so that `OPENAI_HOST=… goose` keeps working)
        //   2. OPENAI_BASE_URL (env or config) — ecosystem-standard
        //   3. OPENAI_HOST from config file — persisted by `goose configure`
        //   4. Default "https://api.openai.com"
        //
        // OPENAI_BASE_URL is parsed into host + query params + a flag
        // indicating whether the URL included a /v1 path segment.  When /v1
        // is present the default base_path is "v1/chat/completions";
        // otherwise "chat/completions" to match the OpenAI SDK convention.
        //
        // OPENAI_BASE_PATH always wins when set explicitly.
        let parsed = if let Ok(h) = std::env::var("OPENAI_HOST") {
            // OPENAI_HOST env var takes priority as a session override so
            // that existing scripts like `OPENAI_HOST=… goose` still work
            // even after OPENAI_BASE_URL is persisted in config.
            ParsedBaseUrl {
                host: h,
                query_params: vec![],
                has_v1: true,
                from_base_url: false,
            }
        } else if let Some(raw_url) = config
            .get_param::<String>("OPENAI_BASE_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            Self::parse_base_url(&raw_url)?
        } else {
            let h: String = config
                .get_param("OPENAI_HOST")
                .unwrap_or_else(|_| "https://api.openai.com".to_string());
            ParsedBaseUrl {
                host: h,
                query_params: vec![],
                has_v1: true,
                from_base_url: false,
            }
        };

        // When the host was derived from OPENAI_BASE_URL, read
        // OPENAI_BASE_PATH from env only so that the desktop UI's persisted
        // default ("v1/chat/completions") doesn't shadow the versionless
        // path.  When the host came from OPENAI_HOST (env or config), read
        // from config too — Docker Model Runner and similar setups persist a
        // custom base_path that must be honoured.
        let default_bp = || {
            if parsed.has_v1 {
                OPEN_AI_DEFAULT_BASE_PATH.to_string()
            } else {
                OPEN_AI_VERSIONLESS_BASE_PATH.to_string()
            }
        };
        let base_path: String = if parsed.from_base_url {
            std::env::var("OPENAI_BASE_PATH").unwrap_or_else(|_| default_bp())
        } else {
            config
                .get_param("OPENAI_BASE_PATH")
                .unwrap_or_else(|_| default_bp())
        };

        // Only apply the default fast model when talking to OpenAI directly.
        // Custom/compatible endpoints likely don't serve gpt-4o-mini, so
        // leave fast_model unset (complete_fast will fall back to the main model).
        // Parse the URL and compare the hostname exactly to avoid false positives
        // (e.g. https://api.openai.com.local:8000 or proxy paths containing api.openai.com).
        let host = parsed.host.clone();

        // Only apply the default fast model when talking to OpenAI directly.
        // Custom/compatible endpoints likely don't serve gpt-4o-mini, so
        // leave fast_model unset (complete_fast will fall back to the main model).
        // Parse the URL and compare the hostname exactly to avoid false positives
        // (e.g. https://api.openai.com.local:8000 or proxy paths containing api.openai.com).
        let is_openai = url::Url::parse(&host)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
            .map(|h| h == "api.openai.com" || h.ends_with(".api.openai.com"))
            .unwrap_or(false);
        let model = if is_openai {
            model.with_fast(OPEN_AI_DEFAULT_FAST_MODEL, OPEN_AI_PROVIDER_NAME)?
        } else {
            model
        };

        let secrets = config
            .get_secrets("OPENAI_API_KEY", &["OPENAI_CUSTOM_HEADERS"])
            .unwrap_or_default();
        let api_key: Option<String> = secrets.get("OPENAI_API_KEY").cloned();
        let custom_headers: Option<HashMap<String, String>> = secrets
            .get("OPENAI_CUSTOM_HEADERS")
            .cloned()
            .map(parse_custom_headers);

        let organization: Option<String> = config.get_param("OPENAI_ORGANIZATION").ok();
        let project: Option<String> = config.get_param("OPENAI_PROJECT").ok();
        let timeout_secs: u64 = config.get_param("OPENAI_TIMEOUT").unwrap_or(600);

        let auth = match api_key {
            Some(key) if !key.is_empty() => AuthMethod::BearerToken(key),
            _ => AuthMethod::NoAuth,
        };
        let mut api_client = ApiClient::with_timeout(
            parsed.host,
            auth,
            std::time::Duration::from_secs(timeout_secs),
        )?;

        if !parsed.query_params.is_empty() {
            api_client = api_client.with_query(parsed.query_params);
        }

        if let Some(org) = &organization {
            api_client = api_client.with_header("OpenAI-Organization", org)?;
        }

        if let Some(project) = &project {
            api_client = api_client.with_header("OpenAI-Project", project)?;
        }

        if let Some(headers) = &custom_headers {
            let mut header_map = reqwest::header::HeaderMap::new();
            for (key, value) in headers {
                let header_name = reqwest::header::HeaderName::from_bytes(key.as_bytes())?;
                let header_value = reqwest::header::HeaderValue::from_str(value)?;
                header_map.insert(header_name, header_value);
            }
            api_client = api_client.with_headers(header_map)?;
        }

        Ok(Self {
            api_client,
            base_path,
            organization,
            project,
            model,
            custom_headers,
            supports_streaming: true,
            name: OPEN_AI_PROVIDER_NAME.to_string(),
            custom_models: None,
            skip_canonical_filtering: false,
        })
    }

    #[doc(hidden)]
    pub fn new(api_client: ApiClient, model: ModelConfig) -> Self {
        Self {
            api_client,
            base_path: OPEN_AI_DEFAULT_BASE_PATH.to_string(),
            organization: None,
            project: None,
            model,
            custom_headers: None,
            supports_streaming: true,
            name: OPEN_AI_PROVIDER_NAME.to_string(),
            custom_models: None,
            skip_canonical_filtering: false,
        }
    }

    pub fn from_custom_config(
        model: ModelConfig,
        config: DeclarativeProviderConfig,
    ) -> Result<Self> {
        let global_config = crate::config::Config::global();

        let api_key: Option<String> = if config.requires_auth && !config.api_key_env.is_empty() {
            Some(global_config.get_secret::<String>(&config.api_key_env).map_err(|e| {
                use crate::config::ConfigError;
                match e {
                    ConfigError::NotFound(_) => anyhow::anyhow!(
                        "Required API key {} is not set. Configure it via `goose configure` or set the {} environment variable.",
                        config.api_key_env,
                        config.api_key_env
                    ),
                    other => anyhow::anyhow!("Failed to read {}: {}", config.api_key_env, other),
                }
            })?)
        } else {
            None
        };

        let url = url::Url::parse(&config.base_url)
            .map_err(|e| anyhow::anyhow!("Invalid base URL '{}': {}", config.base_url, e))?;

        let host = if let Some(port) = url.port() {
            format!(
                "{}://{}:{}",
                url.scheme(),
                url.host_str().unwrap_or(""),
                port
            )
        } else {
            format!("{}://{}", url.scheme(), url.host_str().unwrap_or(""))
        };
        let base_path = if let Some(ref explicit_path) = config.base_path {
            explicit_path.trim_start_matches('/').to_string()
        } else {
            Self::derive_base_path(url.path())
        };

        let timeout_secs = config.timeout_seconds.unwrap_or(600);

        let auth = match api_key {
            Some(key) if !key.is_empty() => AuthMethod::BearerToken(key),
            _ => AuthMethod::NoAuth,
        };
        let mut api_client =
            ApiClient::with_timeout(host, auth, std::time::Duration::from_secs(timeout_secs))?;

        // Add custom headers if present
        if let Some(headers) = &config.headers {
            let mut header_map = reqwest::header::HeaderMap::new();
            for (key, value) in headers {
                let header_name = reqwest::header::HeaderName::from_bytes(key.as_bytes())?;
                let header_value = reqwest::header::HeaderValue::from_str(value)?;
                header_map.insert(header_name, header_value);
            }
            api_client = api_client.with_headers(header_map)?;
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
            base_path,
            organization: None,
            project: None,
            model,
            custom_headers: config.headers,
            supports_streaming: config.supports_streaming.unwrap_or(true),
            name: config.name.clone(),
            custom_models,
            skip_canonical_filtering: config.skip_canonical_filtering,
        })
    }

    fn parse_base_url(raw_url: &str) -> Result<ParsedBaseUrl> {
        let (host, query_params, has_v1) = parse_openai_base_url(raw_url)?;
        Ok(ParsedBaseUrl {
            host,
            query_params,
            has_v1,
            from_base_url: true,
        })
    }

    fn derive_base_path(url_path: &str) -> String {
        let stripped = url_path.trim_start_matches('/');
        let normalized = stripped.trim_end_matches('/');
        if normalized.is_empty() {
            "v1/chat/completions".to_string()
        } else if normalized == "v1" || normalized.ends_with("/v1") {
            format!("{}/chat/completions", normalized)
        } else {
            stripped.to_string()
        }
    }

    fn normalize_base_path(base_path: &str) -> String {
        if let Some(path) = base_path.strip_prefix('/') {
            format!("/{}", path.trim_end_matches('/'))
        } else {
            base_path.trim_end_matches('/').to_string()
        }
    }

    fn is_chat_completions_path(base_path: &str) -> bool {
        let normalized = Self::normalize_base_path(base_path).to_ascii_lowercase();
        normalized.contains("chat/completions")
    }

    fn is_responses_path(base_path: &str) -> bool {
        let normalized = Self::normalize_base_path(base_path).to_ascii_lowercase();
        normalized.ends_with("responses") || normalized.contains("/responses")
    }

    fn is_responses_model(model_name: &str) -> bool {
        super::utils::is_openai_responses_model(model_name)
    }

    fn should_use_responses_api(model_name: &str, base_path: &str) -> bool {
        let normalized_base_path = Self::normalize_base_path(base_path);
        // Only the standard "v1/chat/completions" is treated as a default
        // path that defers to model-based routing.  The versionless
        // "chat/completions" (derived from an OPENAI_BASE_URL without /v1)
        // is treated as custom because versionless gateways typically do not
        // support the Responses API.
        let has_custom_base_path = normalized_base_path != OPEN_AI_DEFAULT_BASE_PATH;

        if has_custom_base_path {
            if Self::is_responses_path(&normalized_base_path) {
                return true;
            }
            if Self::is_chat_completions_path(&normalized_base_path) {
                return false;
            }
        }

        Self::is_responses_model(model_name)
    }

    /// Providers known to reject `max_completion_tokens` and require
    /// the legacy `max_tokens` field instead.
    const PROVIDERS_NEEDING_MAX_TOKENS_REMAP: &[&str] = &[
        "cerebras",
        "custom_deepseek",
        "groq",
        "inception",
        "kimi",
        "lmstudio",
        "mistral",
        "moonshot",
        "ovhcloud",
    ];

    fn sanitize_request_for_compat(&self, mut payload: serde_json::Value) -> serde_json::Value {
        if !Self::PROVIDERS_NEEDING_MAX_TOKENS_REMAP.contains(&self.name.as_str()) {
            return payload;
        }

        if let Some(obj) = payload.as_object_mut() {
            if let Some(value) = obj.remove("max_completion_tokens") {
                obj.entry("max_tokens").or_insert(value);
            }
        }

        payload
    }

    fn map_base_path(base_path: &str, target: &str, fallback: &str) -> String {
        let normalized = Self::normalize_base_path(base_path);
        if normalized.ends_with(target) || normalized.contains(&format!("/{target}")) {
            return normalized;
        }

        if Self::is_chat_completions_path(&normalized) {
            return normalized.replacen("chat/completions", target, 1);
        }

        if Self::is_responses_path(&normalized) {
            return normalized.replacen("responses", target, 1);
        }

        if normalized.starts_with('/') {
            format!("/{}", fallback.trim_start_matches('/'))
        } else {
            fallback.to_string()
        }
    }

    async fn fetch_models_from_api(&self) -> Result<Vec<String>, ProviderError> {
        let models_path =
            Self::map_base_path(&self.base_path, "models", OPEN_AI_DEFAULT_MODELS_PATH);
        let response = self
            .api_client
            .request(None, &models_path)
            .response_get()
            .await?;

        if response.status() == StatusCode::NOT_FOUND {
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::EndpointNotFound(body));
        }

        let json = handle_response_openai_compat(response).await?;
        if let Some(err_obj) = json.get("error") {
            let msg = err_obj
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(ProviderError::Authentication(msg.to_string()));
        }

        let data = json.get("data").and_then(|v| v.as_array()).ok_or_else(|| {
            ProviderError::UsageError("Missing data field in JSON response".into())
        })?;
        let mut models: Vec<String> = data
            .iter()
            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(str::to_string))
            .collect();
        models.sort();
        Ok(models)
    }
}

impl ProviderDef for OpenAiProvider {
    type Provider = Self;

    fn metadata() -> ProviderMetadata {
        let models = OPEN_AI_KNOWN_MODELS
            .iter()
            .map(|(name, limit)| ModelInfo::new(*name, *limit))
            .collect();
        ProviderMetadata::with_models(
            OPEN_AI_PROVIDER_NAME,
            "OpenAI",
            "GPT-4 and other OpenAI models, including OpenAI compatible ones",
            OPEN_AI_DEFAULT_MODEL,
            models,
            OPEN_AI_DOC_URL,
            vec![
                ConfigKey::new("OPENAI_API_KEY", false, true, None, true),
                ConfigKey::new("OPENAI_BASE_URL", false, false, None, false),
                ConfigKey::new(
                    "OPENAI_HOST",
                    true,
                    false,
                    Some("https://api.openai.com"),
                    false,
                ),
                ConfigKey::new(
                    "OPENAI_BASE_PATH",
                    true,
                    false,
                    Some("v1/chat/completions"),
                    false,
                ),
                ConfigKey::new("OPENAI_ORGANIZATION", false, false, None, false),
                ConfigKey::new("OPENAI_PROJECT", false, false, None, false),
                ConfigKey::new("OPENAI_CUSTOM_HEADERS", false, true, None, false),
                ConfigKey::new("OPENAI_TIMEOUT", false, false, Some("600"), false),
            ],
        )
        .with_setup_steps(vec![
            "Go to https://platform.openai.com and sign up or log in",
            "Navigate to API Keys in the left sidebar",
            "Click 'Create new secret key'",
            "Copy the key and paste it above",
        ])
    }

    fn from_env(
        model: ModelConfig,
        _extensions: Vec<crate::config::ExtensionConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(Self::from_env(model))
    }

    fn supports_inventory_refresh() -> bool {
        true
    }

    fn inventory_configured() -> bool {
        let config = crate::config::Config::global();
        // If the host is explicitly set to something non-default, trust the user's
        // custom setup (e.g. a local server that doesn't require an API key).
        if let Ok(host) = config.get_param::<String>("OPENAI_HOST") {
            if host != "https://api.openai.com" {
                return true;
            }
        }
        // Standard OpenAI endpoint requires an API key.
        config
            .get_secret::<serde_json::Value>("OPENAI_API_KEY")
            .is_ok()
    }

    fn inventory_identity() -> Result<InventoryIdentityInput> {
        let config = crate::config::Config::global();
        let mut identity =
            InventoryIdentityInput::new(OPEN_AI_PROVIDER_NAME, OPEN_AI_PROVIDER_NAME)
                .with_public(
                    "host",
                    config
                        .get_param::<String>("OPENAI_HOST")
                        .unwrap_or_else(|_| "https://api.openai.com".to_string()),
                )
                .with_public(
                    "base_path",
                    config
                        .get_param::<String>("OPENAI_BASE_PATH")
                        .unwrap_or_else(|_| OPEN_AI_DEFAULT_BASE_PATH.to_string()),
                );

        if let Ok(organization) = config.get_param::<String>("OPENAI_ORGANIZATION") {
            identity = identity.with_public("organization", organization);
        }
        if let Ok(project) = config.get_param::<String>("OPENAI_PROJECT") {
            identity = identity.with_public("project", project);
        }
        if let Some(api_key) = config_secret_value(config, "OPENAI_API_KEY") {
            identity = identity.with_secret("api_key", api_key);
        }
        if let Some(custom_headers) = config_secret_value(config, "OPENAI_CUSTOM_HEADERS") {
            identity = identity.with_secret("custom_headers", custom_headers);
        }

        Ok(identity)
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn skip_canonical_filtering(&self) -> bool {
        self.skip_canonical_filtering
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

    async fn stream(
        &self,
        model_config: &ModelConfig,
        session_id: &str,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        if Self::should_use_responses_api(&model_config.model_name, &self.base_path) {
            let mut payload = create_responses_request(model_config, system, messages, tools)?;
            payload["stream"] = serde_json::Value::Bool(self.supports_streaming);

            let mut log = RequestLog::start(model_config, &payload)?;

            let response = self
                .with_retry(|| async {
                    let payload_clone = payload.clone();
                    let resp = self
                        .api_client
                        .response_post(
                            Some(session_id),
                            &Self::map_base_path(
                                &self.base_path,
                                "responses",
                                OPEN_AI_DEFAULT_RESPONSES_PATH,
                            ),
                            &payload_clone,
                        )
                        .await?;
                    handle_status(resp).await
                })
                .await
                .inspect_err(|e| {
                    let _ = log.error(e);
                })?;

            if self.supports_streaming {
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
                let json: serde_json::Value = response.json().await.map_err(|e| {
                    ProviderError::RequestFailed(format!("Failed to parse JSON: {}", e))
                })?;

                let responses_api_response: ResponsesApiResponse =
                    serde_json::from_value(json.clone()).map_err(|e| {
                        ProviderError::ExecutionError(format!(
                            "Failed to parse responses API response: {}",
                            e
                        ))
                    })?;

                let message = responses_api_to_message(&responses_api_response)?;
                let usage_data = get_responses_usage(&responses_api_response);
                let usage =
                    super::base::ProviderUsage::new(model_config.model_name.clone(), usage_data);

                log.write(
                    &serde_json::to_value(&message).unwrap_or_default(),
                    Some(&usage_data),
                )?;

                Ok(super::base::stream_from_single_message(message, usage))
            }
        } else {
            let payload = create_request(
                model_config,
                system,
                messages,
                tools,
                &ImageFormat::OpenAi,
                self.supports_streaming,
            )?;
            let payload = self.sanitize_request_for_compat(payload);
            let mut log = RequestLog::start(model_config, &payload)?;

            let response = self
                .with_retry(|| async {
                    let resp = self
                        .api_client
                        .response_post(Some(session_id), &self.base_path, &payload)
                        .await?;
                    handle_status(resp).await
                })
                .await
                .inspect_err(|e| {
                    let _ = log.error(e);
                })?;

            if self.supports_streaming {
                stream_openai_compat(response, log)
            } else {
                let json: serde_json::Value = response.json().await.map_err(|e| {
                    ProviderError::RequestFailed(format!("Failed to parse JSON: {}", e))
                })?;

                let message = response_to_message(&json).map_err(|e| {
                    ProviderError::RequestFailed(format!("Failed to parse message: {}", e))
                })?;

                let usage_data = get_usage(json.get("usage").unwrap_or(&serde_json::Value::Null));
                let usage =
                    super::base::ProviderUsage::new(model_config.model_name.clone(), usage_data);

                log.write(
                    &serde_json::to_value(&message).unwrap_or_default(),
                    Some(&usage_data),
                )?;

                Ok(super::base::stream_from_single_message(message, usage))
            }
        }
    }
}

fn parse_custom_headers(s: String) -> HashMap<String, String> {
    s.split(',')
        .filter_map(|header| {
            let mut parts = header.splitn(2, '=');
            let key = parts.next().map(|s| s.trim().to_string())?;
            let value = parts.next().map(|s| s.trim().to_string())?;
            Some((key, value))
        })
        .collect()
}

#[async_trait]
impl EmbeddingCapable for OpenAiProvider {
    async fn create_embeddings(
        &self,
        session_id: &str,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let embedding_model = std::env::var("GOOSE_EMBEDDING_MODEL")
            .unwrap_or_else(|_| "text-embedding-3-small".to_string());

        let request = EmbeddingRequest {
            input: texts,
            model: embedding_model,
        };

        let response = self
            .with_retry(|| async {
                let request_clone = EmbeddingRequest {
                    input: request.input.clone(),
                    model: request.model.clone(),
                };
                let request_value = serde_json::to_value(request_clone)
                    .map_err(|e| ProviderError::ExecutionError(e.to_string()))?;
                let embeddings_path = Self::map_base_path(
                    &self.base_path,
                    "embeddings",
                    OPEN_AI_DEFAULT_EMBEDDINGS_PATH,
                );
                self.api_client
                    .api_post(Some(session_id), &embeddings_path, &request_value)
                    .await
                    .map_err(|e| ProviderError::ExecutionError(e.to_string()))
            })
            .await?;

        if response.status != StatusCode::OK {
            let error_text = response
                .payload
                .as_ref()
                .and_then(|p| p.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("Embedding API error: {}", error_text));
        }

        let embedding_response: EmbeddingResponse = serde_json::from_value(
            response
                .payload
                .ok_or_else(|| anyhow::anyhow!("Empty response body"))?,
        )?;

        Ok(embedding_response
            .data
            .into_iter()
            .map(|d| d.embedding)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_provider(name: &str) -> OpenAiProvider {
        OpenAiProvider {
            api_client: ApiClient::new("http://localhost".to_string(), AuthMethod::NoAuth).unwrap(),
            base_path: "v1/chat/completions".to_string(),
            organization: None,
            project: None,
            model: ModelConfig::new_or_fail("test-model"),
            custom_headers: None,
            supports_streaming: true,
            name: name.to_string(),
            custom_models: None,
            skip_canonical_filtering: false,
        }
    }

    #[test]
    fn sanitize_remaps_max_completion_tokens_for_compat_provider() {
        let provider = make_provider("mistral");
        let payload = json!({
            "model": "mistral-medium-latest",
            "messages": [],
            "max_completion_tokens": 16384
        });

        let result = provider.sanitize_request_for_compat(payload);
        let obj = result.as_object().unwrap();

        assert!(!obj.contains_key("max_completion_tokens"));
        assert_eq!(obj.get("max_tokens").unwrap(), &json!(16384));
    }

    #[test]
    fn sanitize_preserves_existing_max_tokens_for_compat_provider() {
        let provider = make_provider("mistral");
        let payload = json!({
            "model": "mistral-medium-latest",
            "messages": [],
            "max_tokens": 4096,
            "max_completion_tokens": 16384
        });

        let result = provider.sanitize_request_for_compat(payload);
        let obj = result.as_object().unwrap();

        assert!(!obj.contains_key("max_completion_tokens"));
        assert_eq!(obj.get("max_tokens").unwrap(), &json!(4096));
    }

    #[test]
    fn sanitize_noop_for_native_openai_provider() {
        let provider = make_provider("openai");
        let payload = json!({
            "model": "o3",
            "messages": [],
            "max_completion_tokens": 16384
        });

        let result = provider.sanitize_request_for_compat(payload);
        let obj = result.as_object().unwrap();

        assert!(obj.contains_key("max_completion_tokens"));
        assert!(!obj.contains_key("max_tokens"));
    }

    #[test]
    fn sanitize_noop_for_unknown_provider() {
        let provider = make_provider("some_future_provider");
        let payload = json!({
            "model": "future-model",
            "messages": [],
            "max_completion_tokens": 16384
        });

        let result = provider.sanitize_request_for_compat(payload);
        let obj = result.as_object().unwrap();

        assert!(obj.contains_key("max_completion_tokens"));
        assert!(!obj.contains_key("max_tokens"));
    }

    #[test]
    fn sanitize_no_token_params() {
        let provider = make_provider("groq");
        let payload = json!({
            "model": "llama-3.3-70b-versatile",
            "messages": []
        });

        let result = provider.sanitize_request_for_compat(payload.clone());
        assert_eq!(result, payload);
    }

    #[test]
    fn responses_api_routing_uses_model_family_unless_path_forces_chat() {
        for (model_name, base_path, expected) in [
            ("gpt-5.4", "v1/chat/completions", true),
            ("gpt-5.4-xhigh", "v1/chat/completions", true),
            ("gpt-5.2-pro-2025-12-11", "v1/chat/completions", true),
            ("gpt-4o", "v1/chat/completions", false),
            ("gpt-5.2-codex", "openai/v1/chat/completions", false),
        ] {
            assert_eq!(
                OpenAiProvider::should_use_responses_api(model_name, base_path),
                expected,
                "unexpected routing for {model_name} via {base_path}"
            );
        }
    }

    #[test]
    fn custom_chat_path_maps_to_responses_path() {
        let responses_path = OpenAiProvider::map_base_path(
            "openai/v1/chat/completions",
            "responses",
            "v1/responses",
        );
        assert_eq!(responses_path, "openai/v1/responses");
    }

    #[test]
    fn responses_path_maps_to_models_path() {
        let models_path =
            OpenAiProvider::map_base_path("openai/v1/responses", "models", "v1/models");
        assert_eq!(models_path, "openai/v1/models");
    }

    #[test]
    fn unknown_path_falls_back_to_default_models_path() {
        let models_path = OpenAiProvider::map_base_path("custom/path", "models", "v1/models");
        assert_eq!(models_path, "v1/models");
    }

    #[test]
    fn absolute_chat_path_maps_to_absolute_responses_path() {
        let responses_path =
            OpenAiProvider::map_base_path("/v1/chat/completions", "responses", "v1/responses");
        assert_eq!(responses_path, "/v1/responses");
    }

    #[test]
    fn unknown_absolute_path_falls_back_to_absolute_models_path() {
        let models_path = OpenAiProvider::map_base_path("/custom/path", "models", "v1/models");
        assert_eq!(models_path, "/v1/models");
    }
    #[test]
    fn parse_base_url_strips_v1_from_standard_openai_url() {
        let r = OpenAiProvider::parse_base_url("https://api.openai.com/v1").unwrap();
        assert_eq!(r.host, "https://api.openai.com");
        assert!(r.query_params.is_empty());
        assert!(r.has_v1);
    }

    #[test]
    fn parse_base_url_preserves_prefix_before_v1() {
        let r = OpenAiProvider::parse_base_url("https://gateway.example.com/openai/v1").unwrap();
        assert_eq!(r.host, "https://gateway.example.com/openai");
        assert!(r.has_v1);
    }

    #[test]
    fn parse_base_url_handles_no_path() {
        let r = OpenAiProvider::parse_base_url("https://api.openai.com").unwrap();
        assert_eq!(r.host, "https://api.openai.com");
        assert!(r.has_v1);
    }

    #[test]
    fn parse_base_url_handles_trailing_slash() {
        let r = OpenAiProvider::parse_base_url("https://api.openai.com/v1/").unwrap();
        assert_eq!(r.host, "https://api.openai.com");
        assert!(r.has_v1);
    }

    #[test]
    fn parse_base_url_preserves_port() {
        let r = OpenAiProvider::parse_base_url("https://localhost:8080/v1").unwrap();
        assert_eq!(r.host, "https://localhost:8080");
        assert!(r.has_v1);
    }

    #[test]
    fn parse_base_url_preserves_non_v1_path() {
        let r = OpenAiProvider::parse_base_url("https://example.com/custom/api").unwrap();
        assert_eq!(r.host, "https://example.com/custom/api");
        assert!(!r.has_v1);
    }

    #[test]
    fn parse_base_url_preserves_query_params() {
        let r = OpenAiProvider::parse_base_url("https://gw.example.com/v1?api-version=2024-02-01")
            .unwrap();
        assert_eq!(r.host, "https://gw.example.com");
        assert_eq!(
            r.query_params,
            vec![("api-version".to_string(), "2024-02-01".to_string())]
        );
        assert!(r.has_v1);
    }

    #[test]
    fn parse_base_url_preserves_multiple_query_params() {
        let r = OpenAiProvider::parse_base_url("https://example.com/v1?key=val&foo=bar").unwrap();
        assert_eq!(r.query_params.len(), 2);
        assert_eq!(r.query_params[0], ("key".to_string(), "val".to_string()));
        assert_eq!(r.query_params[1], ("foo".to_string(), "bar".to_string()));
    }

    #[test]
    fn parse_base_url_preserves_credentials() {
        let r = OpenAiProvider::parse_base_url("https://user:pass@gateway.example.com/v1").unwrap();
        assert_eq!(r.host, "https://user:pass@gateway.example.com");
        assert!(r.has_v1);
    }

    #[test]
    fn parse_base_url_rejects_empty_string() {
        assert!(OpenAiProvider::parse_base_url("").is_err());
    }

    #[test]
    fn parse_base_url_rejects_whitespace_only() {
        assert!(OpenAiProvider::parse_base_url("  ").is_err());
    }

    #[test]
    fn versionless_base_path_opts_out_of_responses_for_codex_models() {
        assert!(!OpenAiProvider::should_use_responses_api(
            "gpt-5-codex",
            "chat/completions"
        ));
    }
}
