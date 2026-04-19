use crate::config::paths::Paths;
use crate::config::Config;
use crate::session_context::SESSION_ID_HEADER;
use anyhow::Result;
use async_stream::try_stream;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use futures::TryStreamExt;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;
use std::time::Duration as StdDuration;
use tokio::pin;
use tokio_util::io::StreamReader;
use uuid::Uuid;

use super::base::{ConfigKey, MessageStream, Provider, ProviderDef, ProviderMetadata};
use super::errors::ProviderError;
use super::formats::anthropic::{create_request, response_to_streaming_message};
use super::oauth_device_flow::{
    refresh_device_flow_token, run_device_flow, DeviceFlowConfig, DeviceFlowTokens, RequestEncoding,
};
use super::openai_compatible::handle_status;
use super::retry::ProviderRetry;
use super::utils::RequestLog;
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use futures::future::BoxFuture;
use rmcp::model::Tool;

const KIMI_CODE_PROVIDER_NAME: &str = "kimi_code";
pub const KIMI_CODE_DEFAULT_MODEL: &str = "kimi-for-coding";
pub const KIMI_CODE_DEFAULT_FAST_MODEL: &str = "kimi-for-coding";
/// Known models for the provider metadata registration. The live catalogue is
/// fetched from `/v1/models` at request time; this constant is only used for
/// `ProviderMetadata`. As of 2025-10 Kimi Code exposes a single model,
/// `kimi-for-coding`, and silently routes any other model name to it.
pub const KIMI_CODE_KNOWN_MODELS: &[&str] = &["kimi-for-coding"];

const KIMI_CODE_DOC_URL: &str = "https://www.kimi.com/code/docs/en/";
const KIMI_CODE_CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
const KIMI_AUTH_HOST: &str = "https://auth.kimi.com";
const KIMI_API_BASE: &str = "https://api.kimi.com/coding";
const KIMI_MSH_PLATFORM: &str = "kimi_cli";
const KIMI_MSH_VERSION: &str = "0.1.0";

/// Refresh the access token if it expires within this many seconds.
const REFRESH_THRESHOLD_SECS: i64 = 300;

/// Fallback access-token lifetime when the server omits `expires_in`.
const DEFAULT_TOKEN_LIFETIME_SECS: i64 = 3600;

/// Marker key written to the user config when OAuth completes successfully.
/// `check_provider_configured` (server) keys off this when an OAuth-flow
/// provider has no required secret env var.
const KIMI_CONFIGURED_MARKER: &str = "kimi_code_configured";

// ── Token persistence ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
struct KimiToken {
    access_token: String,
    refresh_token: String,
    expires_at: DateTime<Utc>,
}

/// Normalize helper output into the on-disk `KimiToken` shape. When the helper
/// returns `None` for `refresh_token` or `expires_at`, fall back to the prior
/// refresh token (per RFC 6749 §6) and a default lifetime.
fn tokens_to_kimi(tokens: DeviceFlowTokens, prior_refresh: Option<&str>) -> KimiToken {
    let refresh_token = tokens
        .refresh_token
        .or_else(|| prior_refresh.map(str::to_string))
        .unwrap_or_default();
    let expires_at = tokens
        .expires_at
        .unwrap_or_else(|| Utc::now() + Duration::seconds(DEFAULT_TOKEN_LIFETIME_SECS));
    KimiToken {
        access_token: tokens.access_token,
        refresh_token,
        expires_at,
    }
}

#[derive(Debug)]
struct TokenCache {
    path: std::path::PathBuf,
}

impl TokenCache {
    fn new() -> Self {
        Self {
            path: Paths::in_config_dir("kimicode/token.json"),
        }
    }

    async fn load(&self) -> Option<KimiToken> {
        let raw = tokio::fs::read_to_string(&self.path).await.ok()?;
        match serde_json::from_str(&raw) {
            Ok(token) => Some(token),
            Err(e) => {
                tracing::warn!(
                    "kimicode token cache at {:?} is corrupted ({}); ignoring and re-authenticating",
                    self.path,
                    e
                );
                None
            }
        }
    }

    async fn save(&self, token: &KimiToken) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&self.path, serde_json::to_string(token)?).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600)).await?;
        }
        Ok(())
    }

    async fn clear(&self) -> Result<()> {
        match tokio::fs::remove_file(&self.path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

// ── Provider ─────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct KimiCodeProvider {
    #[serde(skip)]
    client: Client,
    #[serde(skip)]
    token_cache: TokenCache,
    #[serde(skip)]
    cached_token: tokio::sync::Mutex<Option<KimiToken>>,
    #[serde(skip)]
    device_id: String,
    #[serde(skip)]
    auth_host: String,
    #[serde(skip)]
    api_base: String,
    model: ModelConfig,
    #[serde(skip)]
    name: String,
}

impl KimiCodeProvider {
    pub async fn cleanup() -> Result<()> {
        TokenCache::new().clear().await
    }

    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let model = model.with_fast(KIMI_CODE_DEFAULT_FAST_MODEL, KIMI_CODE_PROVIDER_NAME)?;
        let client = Client::builder()
            .timeout(StdDuration::from_secs(600))
            .build()?;
        let device_id = Self::get_or_create_device_id().await?;
        Ok(Self {
            client,
            token_cache: TokenCache::new(),
            cached_token: tokio::sync::Mutex::new(None),
            device_id,
            auth_host: KIMI_AUTH_HOST.to_string(),
            api_base: KIMI_API_BASE.to_string(),
            model,
            name: KIMI_CODE_PROVIDER_NAME.to_string(),
        })
    }

    fn is_valid_device_id(id: &str) -> bool {
        !id.is_empty() && HeaderValue::from_str(id).is_ok()
    }

    async fn get_or_create_device_id() -> Result<String> {
        let path = Paths::in_config_dir("kimicode/device_id");
        if let Ok(raw) = tokio::fs::read_to_string(&path).await {
            let id = raw.trim().to_string();
            if Self::is_valid_device_id(&id) {
                return Ok(id);
            }
            tracing::warn!("kimicode device_id at {:?} is invalid; regenerating", path);
        }
        let id = Uuid::new_v4().to_string().replace('-', "");
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, &id).await?;
        Ok(id)
    }

    fn kimi_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Msh-Platform",
            HeaderValue::from_static(KIMI_MSH_PLATFORM),
        );
        headers.insert("X-Msh-Version", HeaderValue::from_static(KIMI_MSH_VERSION));
        // Normally validated in `get_or_create_device_id`; skip the header if
        // that validation was bypassed (e.g. test-constructed provider).
        if let Ok(value) = HeaderValue::from_str(&self.device_id) {
            headers.insert("X-Msh-Device-Id", value);
        }
        headers
    }

    // ── Token management ─────────────────────────────────────────────────────

    /// Returns a valid access token, refreshing or re-authenticating as needed.
    async fn get_access_token(&self) -> Result<String> {
        Ok(self.ensure_token().await?.access_token)
    }

    /// Ensures we have a usable token, walking the cache → refresh → device-flow ladder.
    async fn ensure_token(&self) -> Result<KimiToken> {
        let mut guard = self.cached_token.lock().await;

        if let Some(token) = guard.clone() {
            if let Some(usable) = self.use_or_refresh(token).await {
                *guard = Some(usable.clone());
                return Ok(usable);
            }
        }

        if let Some(token) = self.token_cache.load().await {
            if let Some(usable) = self.use_or_refresh(token).await {
                *guard = Some(usable.clone());
                return Ok(usable);
            }
        }

        tracing::info!("kimicode: starting OAuth device-flow login");
        let token = self.device_flow_login().await?;
        self.token_cache.save(&token).await?;
        *guard = Some(token.clone());
        Ok(token)
    }

    /// Returns a usable token derived from `token`, or `None` if it is unusable.
    /// On a successful refresh, the new token is also persisted to disk.
    async fn use_or_refresh(&self, token: KimiToken) -> Option<KimiToken> {
        if token.expires_at - Utc::now() > Duration::seconds(REFRESH_THRESHOLD_SECS) {
            return Some(token);
        }
        match self.do_refresh_token(&token.refresh_token).await {
            Ok(refreshed) => {
                tracing::debug!("kimicode: token refreshed");
                if let Err(e) = self.token_cache.save(&refreshed).await {
                    tracing::warn!("failed to persist refreshed kimicode token: {}", e);
                }
                Some(refreshed)
            }
            Err(e) => {
                tracing::debug!("kimicode: token refresh failed: {}", e);
                if token.expires_at > Utc::now() {
                    tracing::debug!("kimicode: falling back to still-unexpired token");
                    Some(token)
                } else {
                    None
                }
            }
        }
    }

    async fn device_flow_login(&self) -> Result<KimiToken> {
        let device_auth_url = format!("{}/api/oauth/device_authorization", self.auth_host);
        let token_url = format!("{}/api/oauth/token", self.auth_host);
        let cfg = DeviceFlowConfig {
            device_auth_url: Some(&device_auth_url),
            token_url: &token_url,
            client_id: KIMI_CODE_CLIENT_ID,
            scopes: None,
            extra_headers: self.kimi_headers(),
            encoding: RequestEncoding::Form,
        };
        let tokens = run_device_flow(&self.client, &cfg).await?;
        Ok(tokens_to_kimi(tokens, None))
    }

    async fn do_refresh_token(&self, refresh_token: &str) -> Result<KimiToken> {
        let token_url = format!("{}/api/oauth/token", self.auth_host);
        let cfg = DeviceFlowConfig {
            device_auth_url: None,
            token_url: &token_url,
            client_id: KIMI_CODE_CLIENT_ID,
            scopes: None,
            extra_headers: self.kimi_headers(),
            encoding: RequestEncoding::Form,
        };
        let tokens = refresh_device_flow_token(&self.client, &cfg, refresh_token).await?;
        // RFC 6749 §6: the server MAY omit `refresh_token` from a refresh
        // response, in which case the client should keep reusing the prior one.
        Ok(tokens_to_kimi(tokens, Some(refresh_token)))
    }

    // ── HTTP ─────────────────────────────────────────────────────────────────

    async fn post(
        &self,
        session_id: Option<&str>,
        payload: &Value,
    ) -> Result<reqwest::Response, ProviderError> {
        let access_token = self.get_access_token().await.map_err(|e| {
            ProviderError::Authentication(format!("Failed to get Kimi access token: {}", e))
        })?;

        let mut builder = self
            .client
            .post(format!("{}/v1/messages", self.api_base))
            .bearer_auth(access_token)
            .headers(self.kimi_headers())
            .json(payload);

        if let Some(sid) = session_id {
            builder = builder.header(SESSION_ID_HEADER, sid);
        }

        builder
            .send()
            .await
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))
    }
}

// ── ProviderDef ───────────────────────────────────────────────────────────────

impl ProviderDef for KimiCodeProvider {
    type Provider = Self;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            KIMI_CODE_PROVIDER_NAME,
            "Kimi Code",
            "Kimi Code AI models optimized for coding tasks",
            KIMI_CODE_DEFAULT_MODEL,
            KIMI_CODE_KNOWN_MODELS.to_vec(),
            KIMI_CODE_DOC_URL,
            // Marker key — the actual token lives in ~/.config/goose/kimicode/token.json.
            // `oauth_flow=true` routes config through `configure_oauth`;
            // readiness is tracked via the `kimi_code_configured` param.
            vec![ConfigKey::new_oauth_device_code(
                "KIMI_CODE_TOKEN",
                true,
                true,
                None,
                false,
            )],
        )
        .with_setup_steps(vec![
            "Run `goose configure` and select 'Kimi Code'",
            "A browser window will open — log in to kimi.com and enter the displayed code",
            "Once authorized, Goose will save your token automatically",
        ])
    }

    fn from_env(
        model: ModelConfig,
        _extensions: Vec<crate::config::ExtensionConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(Self::from_env(model))
    }
}

// ── Provider trait ────────────────────────────────────────────────────────────

#[async_trait]
impl Provider for KimiCodeProvider {
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
        let mut payload = create_request(model_config, system, messages, tools)
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;
        payload
            .as_object_mut()
            .unwrap()
            .insert("stream".to_string(), Value::Bool(true));

        let mut log = RequestLog::start(model_config, &payload)
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

        let response = self
            .with_retry(|| async {
                let resp = self.post(Some(session_id), &payload).await?;
                handle_status(resp).await
            })
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;

        let stream = response.bytes_stream().map_err(io::Error::other);

        Ok(Box::pin(try_stream! {
            let stream_reader = StreamReader::new(stream);
            let framed = tokio_util::codec::FramedRead::new(
                stream_reader,
                tokio_util::codec::LinesCodec::new(),
            )
            .map_err(anyhow::Error::from);

            let message_stream = response_to_streaming_message(framed);
            pin!(message_stream);
            while let Some(message) = futures::StreamExt::next(&mut message_stream).await {
                let (message, usage) = message.map_err(|e| {
                    ProviderError::RequestFailed(format!("Stream decode error: {}", e))
                })?;
                log.write(&message, usage.as_ref().map(|f| f.usage).as_ref())?;
                yield (message, usage);
            }
        }))
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        #[derive(Deserialize)]
        struct ModelEntry {
            id: String,
        }
        #[derive(Deserialize)]
        struct ModelsResp {
            data: Vec<ModelEntry>,
        }

        let access_token = self.get_access_token().await.map_err(|e| {
            ProviderError::Authentication(format!("Failed to get Kimi access token: {}", e))
        })?;

        let resp = self
            .client
            .get(format!("{}/v1/models", self.api_base))
            .bearer_auth(access_token)
            .headers(self.kimi_headers())
            .send()
            .await
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;
        let resp = handle_status(resp).await?;

        let parsed: ModelsResp = resp.json().await.map_err(|e| {
            ProviderError::RequestFailed(format!("/v1/models body is not valid JSON: {}", e))
        })?;
        let mut models: Vec<String> = parsed.data.into_iter().map(|m| m.id).collect();
        models.sort();
        Ok(models)
    }

    async fn configure_oauth(&self) -> Result<(), ProviderError> {
        self.ensure_token()
            .await
            .map_err(|e| ProviderError::Authentication(format!("OAuth flow failed: {}", e)))?;

        Config::global()
            .set_param(KIMI_CONFIGURED_MARKER, Value::Bool(true))
            .map_err(|e| {
                ProviderError::ExecutionError(format!(
                    "Failed to record kimi_code configured state: {}",
                    e
                ))
            })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_provider(server_uri: &str, device_id: &str) -> KimiCodeProvider {
        KimiCodeProvider {
            client: Client::new(),
            token_cache: TokenCache {
                path: std::env::temp_dir()
                    .join(format!("goose-kimicode-test-{}.json", Uuid::new_v4())),
            },
            cached_token: tokio::sync::Mutex::new(None),
            device_id: device_id.to_string(),
            auth_host: server_uri.to_string(),
            api_base: server_uri.to_string(),
            model: ModelConfig::new(KIMI_CODE_DEFAULT_MODEL).unwrap(),
            name: KIMI_CODE_PROVIDER_NAME.to_string(),
        }
    }

    // ── KimiToken serde ───────────────────────────────────────────────────────

    #[test]
    fn kimi_token_roundtrip() {
        let token = KimiToken {
            access_token: "acc_test".to_string(),
            refresh_token: "ref_test".to_string(),
            expires_at: Utc::now() + Duration::seconds(3600),
        };
        let json = serde_json::to_string(&token).expect("serialize");
        let decoded: KimiToken = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.access_token, token.access_token);
        assert_eq!(decoded.refresh_token, token.refresh_token);
        assert_eq!(decoded.expires_at.timestamp(), token.expires_at.timestamp());
    }

    // ── Headers ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn kimi_headers_contains_required_fields() {
        let provider = test_provider("http://localhost", "testdeviceid");
        let headers = provider.kimi_headers();
        assert_eq!(
            headers.get("X-Msh-Platform").and_then(|v| v.to_str().ok()),
            Some(KIMI_MSH_PLATFORM)
        );
        assert_eq!(
            headers.get("X-Msh-Version").and_then(|v| v.to_str().ok()),
            Some(KIMI_MSH_VERSION)
        );
        assert_eq!(
            headers.get("X-Msh-Device-Id").and_then(|v| v.to_str().ok()),
            Some("testdeviceid")
        );
    }

    #[tokio::test]
    async fn kimi_headers_skips_invalid_device_id_without_panic() {
        // U+0000 is an invalid header byte; must not panic.
        let provider = test_provider("http://localhost", "bad\u{0000}id");
        let headers = provider.kimi_headers();
        assert!(headers.get("X-Msh-Device-Id").is_none());
        assert!(headers.contains_key("X-Msh-Platform"));
    }

    #[test]
    fn validates_device_id_rejects_invalid_bytes() {
        assert!(KimiCodeProvider::is_valid_device_id("abc123"));
        assert!(!KimiCodeProvider::is_valid_device_id(""));
        assert!(!KimiCodeProvider::is_valid_device_id("bad\u{0000}id"));
        assert!(!KimiCodeProvider::is_valid_device_id("bad\nid"));
    }

    // ── Metadata ──────────────────────────────────────────────────────────────

    #[test]
    fn metadata_has_oauth_device_code_key() {
        let meta = KimiCodeProvider::metadata();
        let key = meta
            .config_keys
            .iter()
            .find(|k| k.name == "KIMI_CODE_TOKEN")
            .expect("KIMI_CODE_TOKEN config key should exist");
        assert!(key.oauth_flow, "should be an OAuth flow key");
        assert!(key.device_code_flow, "should use device code flow");
        assert!(key.secret, "token should be stored securely");
    }

    #[test]
    fn metadata_has_setup_steps() {
        let meta = KimiCodeProvider::metadata();
        assert!(
            !meta.setup_steps.is_empty(),
            "setup_steps should be populated"
        );
    }

    // ── Refresh / poll behavior ──────────────────────────────────────────────

    #[tokio::test]
    async fn use_or_refresh_returns_fresh_token_without_calling_endpoint() {
        let server = MockServer::start().await;
        // Refusing all requests proves no network call was made.
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let provider = test_provider(&server.uri(), "abc");
        let fresh = KimiToken {
            access_token: "acc".to_string(),
            refresh_token: "ref".to_string(),
            expires_at: Utc::now() + Duration::seconds(REFRESH_THRESHOLD_SECS + 600),
        };

        let usable = provider.use_or_refresh(fresh.clone()).await.unwrap();
        assert_eq!(usable.access_token, "acc");
    }

    #[tokio::test]
    async fn use_or_refresh_falls_back_to_existing_token_when_refresh_fails() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/oauth/token"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let provider = test_provider(&server.uri(), "abc");
        // Inside refresh threshold but not yet expired.
        let near_stale = KimiToken {
            access_token: "still_good".to_string(),
            refresh_token: "ref".to_string(),
            expires_at: Utc::now() + Duration::seconds(60),
        };

        let usable = provider.use_or_refresh(near_stale).await.unwrap();
        assert_eq!(usable.access_token, "still_good");
    }

    #[tokio::test]
    async fn use_or_refresh_returns_new_token_on_successful_refresh() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/oauth/token"))
            .and(body_string_contains("grant_type=refresh_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "new_access",
                "refresh_token": "new_refresh",
                "expires_in": 3600,
            })))
            .mount(&server)
            .await;

        let provider = test_provider(&server.uri(), "abc");
        let near_stale = KimiToken {
            access_token: "old".to_string(),
            refresh_token: "old_refresh".to_string(),
            expires_at: Utc::now() + Duration::seconds(60),
        };

        let usable = provider.use_or_refresh(near_stale).await.unwrap();
        assert_eq!(usable.access_token, "new_access");
        assert_eq!(usable.refresh_token, "new_refresh");
    }

    // NOTE: RFC 8628 polling behavior (authorization_pending, slow_down, missing
    // refresh_token, HTTP errors during polling) is covered by
    // `providers::oauth_device_flow` tests. Tests here focus on Kimi-specific
    // integration — token cache, refresh-fallback when server omits refresh_token.

    #[tokio::test]
    async fn use_or_refresh_preserves_refresh_token_when_server_omits_it() {
        // RFC 6749 §6: if the refresh response omits `refresh_token`, the
        // client should keep reusing the prior one.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/oauth/token"))
            .and(body_string_contains("grant_type=refresh_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "new_access",
                "expires_in": 3600,
            })))
            .mount(&server)
            .await;

        let provider = test_provider(&server.uri(), "abc");
        let near_stale = KimiToken {
            access_token: "old".to_string(),
            refresh_token: "original_refresh".to_string(),
            expires_at: Utc::now() + Duration::seconds(60),
        };

        let usable = provider.use_or_refresh(near_stale).await.unwrap();
        assert_eq!(usable.access_token, "new_access");
        assert_eq!(usable.refresh_token, "original_refresh");
    }

    // ── fetch_supported_models ────────────────────────────────────────────────

    async fn seed_fresh_token(provider: &KimiCodeProvider) {
        *provider.cached_token.lock().await = Some(KimiToken {
            access_token: "fresh-access".to_string(),
            refresh_token: "fresh-refresh".to_string(),
            expires_at: Utc::now() + Duration::seconds(3600),
        });
    }

    #[tokio::test]
    async fn fetch_supported_models_returns_server_catalogue() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [
                    {"id": "kimi-for-coding"},
                    {"id": "future-model"}
                ],
                "object": "list",
            })))
            .mount(&server)
            .await;

        let provider = test_provider(&server.uri(), "abc");
        seed_fresh_token(&provider).await;

        let models = provider.fetch_supported_models().await.unwrap();
        // Results are sorted alphabetically, matching peer providers.
        assert_eq!(
            models,
            vec!["future-model".to_string(), "kimi-for-coding".to_string()]
        );
    }

    #[tokio::test]
    async fn fetch_supported_models_propagates_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let provider = test_provider(&server.uri(), "abc");
        seed_fresh_token(&provider).await;

        let err = provider.fetch_supported_models().await.unwrap_err();
        assert!(
            matches!(err, ProviderError::ServerError(_)),
            "expected ServerError, got {:?}",
            err
        );
    }
}
