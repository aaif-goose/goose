use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use futures::future::BoxFuture;
use goose_providers::anthropic::{
    AnthropicProvider, AnthropicProviderBuilder, ANTHROPIC_API_VERSION,
};
use goose_providers::api_client::{ApiClient, AuthMethod, AuthProvider, TlsConfig};
use goose_providers::base::{
    model_info_for_provider_model, ConfigKey, MessageStream, ModelInfo, Provider,
    ProviderDescriptor, ProviderMetadata,
};
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use reqwest::Client;
use rmcp::model::Tool;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::sync::Mutex as TokioMutex;

use crate::config::paths::Paths;
use crate::config::Config;
use crate::conversation::message::Message;
use crate::providers::base::ProviderDef;
use crate::providers::oauth_device_flow::{
    refresh_device_flow_token, run_device_flow, DeviceFlowConfig, DeviceFlowTokenRefreshError,
    DeviceFlowTokens, RequestEncoding,
};
use crate::providers::private_file::write_private_file;

pub const MUSE_CODE_PROVIDER_NAME: &str = "muse_code";
pub const MUSE_CODE_API_HOST: &str = "https://api.meta.ai";
pub const MUSE_CODE_AUTH_HOST: &str = "https://auth.meta.com";
pub const MUSE_CODE_CLIENT_ID: &str = "1031625952748946";
const MUSE_CODE_DOC_URL: &str = "https://ai.developer.meta.com/docs/muse-code/subscriptions";
const MUSE_CODE_USER_AGENT: &str = "goose-muse-code";

const MUSE_CODE_KNOWN_MODELS: &[&str] = &[
    "muse-spark-1.2",
    "muse-spark-1.1",
    "muse-spark-1.2-contributor",
];

const REFRESH_THRESHOLD_SECS: i64 = 300;
const DEFAULT_TOKEN_LIFETIME_SECS: i64 = 3600;

static GLOBAL_MUSE_REFRESH_MUTEX: std::sync::OnceLock<TokioMutex<()>> = std::sync::OnceLock::new();

fn global_muse_refresh_mutex() -> &'static TokioMutex<()> {
    GLOBAL_MUSE_REFRESH_MUTEX.get_or_init(|| TokioMutex::new(()))
}

static MUSE_TOKEN_GENERATION: AtomicU64 = AtomicU64::new(0);

pub struct MuseCodeProviderDef;

pub struct MuseCodeProvider {
    inner: AnthropicProvider,
    auth: Arc<MuseCodeAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct MuseToken {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    api_key: String,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct MintedKey {
    api_key: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct TokenCache {
    path: PathBuf,
}

struct MuseCodeAuth {
    cache: TokenCache,
    client: Client,
    api_host: String,
    auth_host: String,
    client_id: String,
}

struct SharedAuthProvider(Arc<MuseCodeAuth>);

fn known_models() -> Vec<ModelInfo> {
    MUSE_CODE_KNOWN_MODELS
        .iter()
        .map(|&name| model_info_for_provider_model(MUSE_CODE_PROVIDER_NAME, name))
        .collect()
}

fn tokens_to_muse(tokens: DeviceFlowTokens, prior_refresh: Option<&str>) -> MuseToken {
    let refresh_token = tokens
        .refresh_token
        .or_else(|| prior_refresh.map(str::to_string))
        .unwrap_or_default();
    let expires_at = tokens
        .expires_at
        .unwrap_or_else(|| Utc::now() + Duration::seconds(DEFAULT_TOKEN_LIFETIME_SECS));
    MuseToken {
        access_token: tokens.access_token,
        refresh_token,
        api_key: String::new(),
        expires_at,
    }
}

fn muse_refresh_error(error: anyhow::Error) -> ProviderError {
    let refresh_error = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<DeviceFlowTokenRefreshError>());
    let status = refresh_error.map(|error| error.status).or_else(|| {
        error
            .chain()
            .find_map(|cause| cause.downcast_ref::<reqwest::Error>())
            .and_then(reqwest::Error::status)
    });
    let details = error.to_string();

    if refresh_error.and_then(|error| error.error.as_deref()) == Some("invalid_grant") {
        return ProviderError::Authentication(details);
    }

    match status {
        Some(reqwest::StatusCode::TOO_MANY_REQUESTS) => ProviderError::RateLimitExceeded {
            details,
            retry_delay: None,
        },
        Some(status) if status.is_server_error() => ProviderError::ServerError(details),
        Some(_) => ProviderError::RequestFailed(details),
        _ => ProviderError::from(error),
    }
}

fn join_url(host: &str, path: &str) -> String {
    format!("{}{}", host.trim_end_matches('/'), path)
}

pub(crate) fn has_configured_token() -> bool {
    TokenCache::new().has_token() || muse_cli_token().is_some()
}

fn muse_cli_auth_path() -> PathBuf {
    if let Ok(path) = std::env::var("MUSE_AUTH_PATH") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("muse/auth.json");
        }
    }
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
        .join(".config/muse/auth.json")
}

fn muse_cli_keychain_token() -> Option<MuseToken> {
    #[cfg(feature = "system-keyring")]
    {
        if std::env::var("MUSE_AUTH_PATH").is_ok() {
            return None;
        }

        #[derive(Deserialize)]
        struct Stored {
            #[serde(default)]
            api_key: String,
            #[serde(default)]
            access_token: String,
        }

        let entry = keyring::Entry::new("ai.meta.dev.credentials", "meta").ok()?;
        let raw = entry.get_password().ok()?;
        let stored: Stored = serde_json::from_str(&raw).ok()?;
        if stored.api_key.is_empty() {
            return None;
        }
        Some(MuseToken {
            access_token: stored.access_token,
            refresh_token: String::new(),
            api_key: stored.api_key,
            expires_at: Utc::now() + Duration::seconds(DEFAULT_TOKEN_LIFETIME_SECS),
        })
    }

    #[cfg(not(feature = "system-keyring"))]
    {
        None
    }
}

fn muse_cli_token() -> Option<MuseToken> {
    if let Some(token) = muse_cli_keychain_token() {
        return Some(token);
    }

    #[derive(Deserialize)]
    struct AuthFile {
        providers: Providers,
    }
    #[derive(Deserialize)]
    struct Providers {
        meta: MetaSlot,
    }
    #[derive(Deserialize)]
    struct MetaSlot {
        mechanism: String,
        #[serde(default)]
        access_token: String,
        #[serde(default)]
        api_key: String,
        expires_at: Option<f64>,
    }

    let raw = std::fs::read_to_string(muse_cli_auth_path()).ok()?;
    let parsed: AuthFile = serde_json::from_str(&raw).ok()?;
    if parsed.providers.meta.mechanism != "oauth" {
        return None;
    }
    let api_key = parsed.providers.meta.api_key;
    let access_token = parsed.providers.meta.access_token;
    if api_key.is_empty() && access_token.is_empty() {
        return None;
    }
    let expires_at = parsed
        .providers
        .meta
        .expires_at
        .and_then(|ts| DateTime::from_timestamp(ts as i64, 0))
        .unwrap_or_else(|| Utc::now() + Duration::seconds(DEFAULT_TOKEN_LIFETIME_SECS));
    Some(MuseToken {
        access_token,
        refresh_token: String::new(),
        api_key,
        expires_at,
    })
}

impl TokenCache {
    fn new() -> Self {
        Self {
            path: Paths::in_config_dir("muse_code/token.json"),
        }
    }

    fn load(&self) -> Option<MuseToken> {
        let raw = std::fs::read_to_string(&self.path).ok()?;
        match serde_json::from_str(&raw) {
            Ok(token) => Some(token),
            Err(e) => {
                tracing::warn!(
                    "muse_code token cache at {:?} is corrupted ({}); ignoring",
                    self.path,
                    e
                );
                None
            }
        }
    }

    pub(crate) fn has_token(&self) -> bool {
        self.load().is_some()
    }

    fn save(&self, token: &MuseToken) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_private_file(&self.path, &serde_json::to_string(token)?)?;
        Ok(())
    }

    fn clear(&self) -> anyhow::Result<()> {
        let result = match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        };
        if result.is_ok() {
            MUSE_TOKEN_GENERATION.fetch_add(1, Ordering::SeqCst);
        }
        result
    }
}

impl MuseCodeAuth {
    fn oauth_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(USER_AGENT, HeaderValue::from_static(MUSE_CODE_USER_AGENT));
        headers
    }

    fn device_flow_config<'a>(
        &'a self,
        device_auth_url: &'a str,
        token_url: &'a str,
    ) -> DeviceFlowConfig<'a> {
        DeviceFlowConfig {
            device_auth_url: Some(device_auth_url),
            token_url,
            client_id: &self.client_id,
            scopes: None,
            extra_headers: Self::oauth_headers(),
            encoding: RequestEncoding::Form,
        }
    }

    async fn device_flow_login(&self) -> Result<MuseToken> {
        let device_auth_url = join_url(&self.auth_host, "/oidc/device/authorization/");
        let token_url = join_url(&self.auth_host, "/oidc/device/token/");
        let cfg = self.device_flow_config(&device_auth_url, &token_url);
        let tokens = run_device_flow(&self.client, &cfg).await?;
        let mut token = tokens_to_muse(tokens, None);
        token.api_key = self.mint_api_key(&token.access_token).await?;
        Ok(token)
    }

    async fn mint_api_key(&self, access_token: &str) -> Result<String> {
        let url = join_url(&self.api_host, "/muse-code/key");
        let response = self
            .client
            .post(url)
            .header(ACCEPT, "application/json")
            .header(USER_AGENT, MUSE_CODE_USER_AGENT)
            .header("x-api-version", "1.0.0")
            .bearer_auth(access_token)
            .json(&serde_json::json!({}))
            .send()
            .await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        if !status.is_success() {
            anyhow::bail!(
                "key mint failed ({status}): {}",
                String::from_utf8_lossy(&bytes)
            );
        }
        let minted: MintedKey = serde_json::from_slice(&bytes)?;
        minted
            .api_key
            .filter(|key| !key.is_empty())
            .ok_or_else(|| anyhow::anyhow!("mint response missing api_key"))
    }

    async fn do_refresh_token(&self, refresh_token: &str) -> Result<MuseToken> {
        let token_url = join_url(&self.auth_host, "/oidc/device/token/");
        let cfg = DeviceFlowConfig {
            device_auth_url: None,
            token_url: &token_url,
            client_id: &self.client_id,
            scopes: None,
            extra_headers: Self::oauth_headers(),
            encoding: RequestEncoding::Form,
        };
        let tokens = refresh_device_flow_token(&self.client, &cfg, refresh_token).await?;
        Ok(tokens_to_muse(tokens, Some(refresh_token)))
    }

    async fn get_valid_token(&self) -> Result<MuseToken, ProviderError> {
        if let Some(token) = self.cache.load() {
            match self.use_or_refresh(token).await {
                Ok(token) => return self.ensure_api_key(token).await,
                Err(ProviderError::NotConfigured | ProviderError::Authentication(_)) => {}
                Err(error) => return Err(error),
            }
        }

        if let Some(token) = muse_cli_token() {
            if token.expires_at > Utc::now() {
                return self.ensure_api_key(token).await;
            }
        }

        Err(ProviderError::NotConfigured)
    }

    async fn ensure_api_key(&self, mut token: MuseToken) -> Result<MuseToken, ProviderError> {
        if !token.api_key.is_empty() {
            return Ok(token);
        }
        if token.access_token.is_empty() {
            return Err(ProviderError::NotConfigured);
        }
        let generation = MUSE_TOKEN_GENERATION.load(Ordering::SeqCst);
        token.api_key = self
            .mint_api_key(&token.access_token)
            .await
            .map_err(|error| {
                ProviderError::Authentication(format!("Failed to mint Muse API key: {error}"))
            })?;
        if MUSE_TOKEN_GENERATION.load(Ordering::SeqCst) != generation {
            return Err(ProviderError::Authentication(
                "Muse Code credential was removed during mint".to_string(),
            ));
        }
        if let Err(error) = self.cache.save(&token) {
            tracing::warn!("failed to persist minted muse_code api key: {error}");
        }
        Ok(token)
    }

    async fn use_or_refresh(&self, token: MuseToken) -> Result<MuseToken, ProviderError> {
        if token.expires_at - Utc::now() > Duration::seconds(REFRESH_THRESHOLD_SECS) {
            return Ok(token);
        }

        if token.refresh_token.is_empty() {
            if token.expires_at > Utc::now() {
                return Ok(token);
            }
            let _ = self.cache.clear();
            return Err(ProviderError::NotConfigured);
        }

        let _guard = global_muse_refresh_mutex().lock().await;

        if let Some(reloaded) = self.cache.load() {
            if reloaded != token
                && reloaded.expires_at - Utc::now() > Duration::seconds(REFRESH_THRESHOLD_SECS)
            {
                return Ok(reloaded);
            }
            if reloaded != token && !reloaded.refresh_token.is_empty() {
                return self.refresh_with_token(reloaded).await;
            }
        }

        self.refresh_with_token(token).await
    }

    async fn refresh_with_token(&self, token: MuseToken) -> Result<MuseToken, ProviderError> {
        let refresh_token = token.refresh_token.clone();
        match self.do_refresh_token(&refresh_token).await {
            Ok(refreshed) => {
                if let Err(e) = self.cache.save(&refreshed) {
                    tracing::warn!("failed to persist refreshed muse_code token: {}", e);
                }
                Ok(refreshed)
            }
            Err(error) => {
                let mapped = muse_refresh_error(error);
                if matches!(mapped, ProviderError::Authentication(_)) {
                    self.cache.clear();
                    return Err(mapped);
                }
                if token.expires_at > Utc::now() {
                    return Ok(token);
                }
                Err(mapped)
            }
        }
    }
}

#[async_trait]
impl AuthProvider for SharedAuthProvider {
    async fn get_auth_header(&self) -> Result<(String, String)> {
        let token = self.0.get_valid_token().await?;
        if token.api_key.is_empty() {
            anyhow::bail!("muse_code api key is missing; sign in again");
        }
        Ok((
            "Authorization".to_string(),
            format!("Bearer {}", token.api_key),
        ))
    }
}

impl MuseCodeProvider {
    pub async fn cleanup() -> Result<()> {
        let _guard = global_muse_refresh_mutex().lock().await;
        TokenCache::new().clear()?;
        Ok(())
    }
}

async fn from_env(tls_config: Option<TlsConfig>) -> Result<MuseCodeProvider> {
    let config = Config::global();
    let host: String = config
        .get_param("MUSE_CODE_HOST")
        .unwrap_or_else(|_| MUSE_CODE_API_HOST.to_string());
    let auth_host: String = config
        .get_param("MUSE_CODE_AUTH_HOST")
        .unwrap_or_else(|_| MUSE_CODE_AUTH_HOST.to_string());
    let client_id: String = config
        .get_param("MUSE_CODE_CLIENT_ID")
        .unwrap_or_else(|_| MUSE_CODE_CLIENT_ID.to_string());

    let auth = Arc::new(MuseCodeAuth {
        cache: TokenCache::new(),
        client: Client::builder()
            .connect_timeout(StdDuration::from_secs(
                crate::providers::base::DEFAULT_CONNECT_TIMEOUT_SECS,
            ))
            .build()?,
        api_host: host.clone(),
        auth_host,
        client_id,
    });

    let api_client = ApiClient::with_timeout_and_tls(
        host,
        AuthMethod::Custom(Box::new(SharedAuthProvider(Arc::clone(&auth)))),
        StdDuration::from_secs(crate::providers::base::DEFAULT_PROVIDER_TIMEOUT_SECS),
        tls_config,
    )?
    .with_request_builder(crate::session_context::session_id_request_builder())
    .with_header("anthropic-version", ANTHROPIC_API_VERSION)?;

    Ok(MuseCodeProvider {
        inner: AnthropicProviderBuilder::new(api_client)
            .name(MUSE_CODE_PROVIDER_NAME)
            .custom_models(Some(known_models()))
            .build(),
        auth,
    })
}

impl ProviderDescriptor for MuseCodeProviderDef {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::with_models(
            MUSE_CODE_PROVIDER_NAME,
            "Meta Muse Code",
            "Muse Spark models from a Meta Muse Code subscription",
            "muse-spark-1.2",
            known_models(),
            MUSE_CODE_DOC_URL,
            vec![
                ConfigKey::new_oauth_device_code("MUSE_CODE_TOKEN", true, true, None, false),
                ConfigKey::new(
                    "MUSE_CODE_HOST",
                    false,
                    false,
                    Some(MUSE_CODE_API_HOST),
                    false,
                ),
                ConfigKey::new(
                    "MUSE_CODE_AUTH_HOST",
                    false,
                    false,
                    Some(MUSE_CODE_AUTH_HOST),
                    false,
                ),
                ConfigKey::new(
                    "MUSE_CODE_CLIENT_ID",
                    false,
                    false,
                    Some(MUSE_CODE_CLIENT_ID),
                    false,
                ),
            ],
        )
        .with_setup_steps(vec![
            "Run `goose configure` and select 'Meta Muse Code'",
            "A browser window will open — sign in to Meta and confirm the displayed code",
            "Once authorized, Goose will save your token automatically",
        ])
        .with_setup(
            crate::providers::catalog::ProviderSetupMetadata::new(
                crate::providers::catalog::ProviderSetupCategory::Model,
                crate::providers::catalog::ProviderSetupMethod::OauthDeviceCode,
                crate::providers::catalog::ProviderSetupGroup::Default,
            )
            .with_docs_url(MUSE_CODE_DOC_URL)
            .with_native_connect_query("Meta Muse Code")
            .with_capabilities(false, true, false),
        )
    }
}

impl ProviderDef for MuseCodeProviderDef {
    type Provider = MuseCodeProvider;

    fn from_env(
        _extensions: Vec<crate::config::ExtensionConfig>,
        tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(from_env(tls_config))
    }
}

#[async_trait]
impl Provider for MuseCodeProvider {
    fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        self.auth.get_valid_token().await?;
        self.inner
            .stream(model_config, system, messages, tools)
            .await
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        self.auth.get_valid_token().await?;
        self.inner.fetch_supported_models().await
    }

    async fn configure_oauth(&self) -> Result<(), ProviderError> {
        let token = self
            .auth
            .device_flow_login()
            .await
            .map_err(|e| ProviderError::Authentication(format!("OAuth flow failed: {e}")))?;
        let _guard = global_muse_refresh_mutex().lock().await;
        self.auth.cache.save(&token).map_err(|e| {
            ProviderError::Authentication(format!("Failed to save OAuth token: {e}"))
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn fresh_token(access: &str) -> MuseToken {
        MuseToken {
            access_token: access.to_string(),
            refresh_token: "refresh".to_string(),
            api_key: access.to_string(),
            expires_at: Utc::now() + Duration::hours(1),
        }
    }

    #[test]
    fn metadata_declares_subscription_models_and_oauth() {
        let metadata = MuseCodeProviderDef::metadata();
        assert_eq!(metadata.name, "muse_code");
        assert_eq!(metadata.default_model, "muse-spark-1.2");
        assert_eq!(
            metadata
                .known_models
                .iter()
                .map(|model| model.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "muse-spark-1.2",
                "muse-spark-1.1",
                "muse-spark-1.2-contributor"
            ]
        );
        assert_eq!(
            metadata
                .known_models
                .iter()
                .map(|model| model.context_limit)
                .collect::<Vec<_>>(),
            vec![Some(1_048_576), Some(1_000_000), Some(1_048_576)]
        );
        assert!(
            metadata.known_models.iter().all(|model| model.reasoning),
            "muse-spark models should resolve as reasoning models via the canonical registry"
        );
        let key = metadata
            .config_keys
            .iter()
            .find(|key| key.name == "MUSE_CODE_TOKEN")
            .expect("MUSE_CODE_TOKEN config key should exist");
        assert!(key.oauth_flow);
        assert!(key.device_code_flow);
        assert!(key.secret);
    }

    #[tokio::test]
    async fn from_env_builds_provider_against_configured_host_with_bearer_auth() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data": [{"id": "muse-spark-1.2"}]})),
            )
            .mount(&server)
            .await;

        let _guard = env_lock::lock_env([
            ("GOOSE_PATH_ROOT", None::<&str>),
            ("META_MODEL_API_KEY", None::<&str>),
            ("MUSE_CODE_HOST", None::<&str>),
            ("MUSE_CODE_AUTH_HOST", None::<&str>),
            ("MUSE_CODE_CLIENT_ID", None::<&str>),
            ("MUSE_AUTH_PATH", None::<&str>),
            ("XDG_CONFIG_HOME", None::<&str>),
        ]);
        let temp_dir = tempfile::tempdir().expect("tempdir should be created");
        std::env::set_var("GOOSE_PATH_ROOT", temp_dir.path());
        std::env::set_var("MUSE_CODE_HOST", server.uri());
        TokenCache::new()
            .save(&fresh_token("test-key"))
            .expect("token should save");

        let provider = MuseCodeProviderDef::from_env(Vec::new(), None)
            .await
            .expect("provider should build");

        let models = provider
            .fetch_supported_models()
            .await
            .expect("models should be fetched from the configured host");
        assert_eq!(models, vec!["muse-spark-1.2".to_string()]);

        let requests = server.received_requests().await.expect("requests recorded");
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0]
                .headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer test-key")
        );
    }

    #[tokio::test]
    async fn fetch_supported_models_does_not_authenticate_when_unconfigured() {
        let server = MockServer::start().await;
        let _guard = env_lock::lock_env([
            ("GOOSE_PATH_ROOT", None::<&str>),
            ("MUSE_CODE_HOST", None::<&str>),
            ("MUSE_AUTH_PATH", None::<&str>),
            ("XDG_CONFIG_HOME", None::<&str>),
        ]);
        let temp_dir = tempfile::tempdir().expect("tempdir should be created");
        std::env::set_var("GOOSE_PATH_ROOT", temp_dir.path());
        std::env::set_var("MUSE_CODE_HOST", server.uri());
        std::env::set_var(
            "MUSE_AUTH_PATH",
            temp_dir
                .path()
                .join("missing-auth.json")
                .to_string_lossy()
                .as_ref(),
        );

        let provider = MuseCodeProviderDef::from_env(Vec::new(), None)
            .await
            .expect("provider should build without a token");
        let err = provider.fetch_supported_models().await.unwrap_err();
        assert_eq!(err, ProviderError::NotConfigured);
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn configure_oauth_stores_device_flow_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oidc/device/authorization/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "device_code": "dc-1",
                "user_code": "GSWR-PTCX",
                "verification_uri": format!("{}/oauth/device/", server.uri()),
                "verification_uri_complete": format!("{}/oauth/device/?code=GSWR-PTCX", server.uri()),
                "expires_in": 600,
                "interval": 1
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/oidc/device/token/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "muse-access",
                "refresh_token": "muse-refresh",
                "expires_in": 3600
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/muse-code/key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "api_key": "LLM-muse-key"
            })))
            .mount(&server)
            .await;

        let _guard = env_lock::lock_env([
            ("GOOSE_PATH_ROOT", None::<&str>),
            ("MUSE_CODE_HOST", None::<&str>),
            ("MUSE_CODE_AUTH_HOST", None::<&str>),
            ("MUSE_CODE_CLIENT_ID", None::<&str>),
            ("MUSE_AUTH_PATH", None::<&str>),
            ("XDG_CONFIG_HOME", None::<&str>),
        ]);
        let temp_dir = tempfile::tempdir().expect("tempdir should be created");
        std::env::set_var("GOOSE_PATH_ROOT", temp_dir.path());
        std::env::set_var("MUSE_CODE_HOST", server.uri());
        std::env::set_var("MUSE_CODE_AUTH_HOST", server.uri());
        std::env::set_var(
            "MUSE_AUTH_PATH",
            temp_dir
                .path()
                .join("missing-auth.json")
                .to_string_lossy()
                .as_ref(),
        );

        let provider = MuseCodeProviderDef::from_env(Vec::new(), None)
            .await
            .expect("provider should build");
        provider
            .configure_oauth()
            .await
            .expect("device flow should complete");

        let stored = TokenCache::new().load().expect("token should be cached");
        assert_eq!(stored.access_token, "muse-access");
        assert_eq!(stored.refresh_token, "muse-refresh");
        assert_eq!(stored.api_key, "LLM-muse-key");
    }

    #[tokio::test]
    async fn configure_oauth_reauths_even_when_a_cached_token_exists() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oidc/device/authorization/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "device_code": "dc-2",
                "user_code": "ABCD-EFGH",
                "verification_uri": format!("{}/oauth/device/", server.uri()),
                "verification_uri_complete": format!("{}/oauth/device/?code=ABCD-EFGH", server.uri()),
                "expires_in": 600,
                "interval": 1
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/oidc/device/token/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "new-access",
                "refresh_token": "new-refresh",
                "expires_in": 3600
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/muse-code/key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "api_key": "LLM-new-key"
            })))
            .mount(&server)
            .await;

        let _guard = env_lock::lock_env([
            ("GOOSE_PATH_ROOT", None::<&str>),
            ("MUSE_CODE_HOST", None::<&str>),
            ("MUSE_CODE_AUTH_HOST", None::<&str>),
            ("MUSE_CODE_CLIENT_ID", None::<&str>),
            ("MUSE_AUTH_PATH", None::<&str>),
            ("XDG_CONFIG_HOME", None::<&str>),
        ]);
        let temp_dir = tempfile::tempdir().expect("tempdir should be created");
        std::env::set_var("GOOSE_PATH_ROOT", temp_dir.path());
        std::env::set_var("MUSE_CODE_HOST", server.uri());
        std::env::set_var("MUSE_CODE_AUTH_HOST", server.uri());
        std::env::set_var(
            "MUSE_AUTH_PATH",
            temp_dir
                .path()
                .join("missing-auth.json")
                .to_string_lossy()
                .as_ref(),
        );
        TokenCache::new()
            .save(&fresh_token("old-access"))
            .expect("existing token should save");

        let provider = MuseCodeProviderDef::from_env(Vec::new(), None)
            .await
            .expect("provider should build");
        provider
            .configure_oauth()
            .await
            .expect("device flow should re-run");

        let stored = TokenCache::new().load().expect("token should be cached");
        assert_eq!(stored.access_token, "new-access");
        assert_eq!(stored.refresh_token, "new-refresh");
        assert_eq!(stored.api_key, "LLM-new-key");
    }

    #[tokio::test]
    async fn configure_oauth_restores_previous_token_when_sign_in_fails() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oidc/device/authorization/"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let _guard = env_lock::lock_env([
            ("GOOSE_PATH_ROOT", None::<&str>),
            ("MUSE_CODE_HOST", None::<&str>),
            ("MUSE_CODE_AUTH_HOST", None::<&str>),
            ("MUSE_CODE_CLIENT_ID", None::<&str>),
            ("MUSE_AUTH_PATH", None::<&str>),
            ("XDG_CONFIG_HOME", None::<&str>),
        ]);
        let temp_dir = tempfile::tempdir().expect("tempdir should be created");
        std::env::set_var("GOOSE_PATH_ROOT", temp_dir.path());
        std::env::set_var("MUSE_CODE_HOST", server.uri());
        std::env::set_var("MUSE_CODE_AUTH_HOST", server.uri());
        std::env::set_var(
            "MUSE_AUTH_PATH",
            temp_dir
                .path()
                .join("missing-auth.json")
                .to_string_lossy()
                .as_ref(),
        );
        TokenCache::new()
            .save(&fresh_token("old-access"))
            .expect("existing token should save");

        let provider = MuseCodeProviderDef::from_env(Vec::new(), None)
            .await
            .expect("provider should build");
        let err = provider.configure_oauth().await.unwrap_err();
        assert!(matches!(err, ProviderError::Authentication(_)));

        let stored = TokenCache::new()
            .load()
            .expect("previous token should remain");
        assert_eq!(stored.access_token, "old-access");
    }

    #[tokio::test]
    async fn uses_muse_cli_auth_json_when_goose_cache_is_empty() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data": [{"id": "muse-spark-1.2"}]})),
            )
            .mount(&server)
            .await;

        let _guard = env_lock::lock_env([
            ("GOOSE_PATH_ROOT", None::<&str>),
            ("MUSE_CODE_HOST", None::<&str>),
            ("MUSE_AUTH_PATH", None::<&str>),
            ("XDG_CONFIG_HOME", None::<&str>),
        ]);
        let temp_dir = tempfile::tempdir().expect("tempdir should be created");
        let cli_path = temp_dir.path().join("muse-auth.json");
        std::fs::write(
            &cli_path,
            serde_json::to_string(&json!({
                "providers": {
                    "meta": {
                        "mechanism": "oauth",
                        "access_token": "cli-access",
                        "api_key": "LLM-cli-key",
                        "expires_at": (Utc::now() + Duration::hours(1)).timestamp()
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        std::env::set_var("GOOSE_PATH_ROOT", temp_dir.path());
        std::env::set_var("MUSE_CODE_HOST", server.uri());
        std::env::set_var("MUSE_AUTH_PATH", cli_path);

        let provider = MuseCodeProviderDef::from_env(Vec::new(), None)
            .await
            .expect("provider should build");
        provider
            .fetch_supported_models()
            .await
            .expect("cli token should authorize");

        let requests = server.received_requests().await.expect("requests recorded");
        assert_eq!(
            requests[0]
                .headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer LLM-cli-key")
        );
    }
}
