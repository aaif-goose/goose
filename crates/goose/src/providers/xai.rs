use super::api_client::{ApiClient, AuthMethod};
use super::base::{ConfigKey, MessageStream, Provider, ProviderDef, ProviderMetadata};
use super::errors::ProviderError;
use super::oauth_device_flow::{
    refresh_device_flow_token, run_device_flow, DeviceFlowConfig, DeviceFlowTokens, RequestEncoding,
};
use super::openai_compatible::OpenAiCompatibleProvider;
use crate::config::paths::Paths;
use crate::config::Config;
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use axum::{extract::Query, response::Html, routing::get, Router};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration, Utc};
use futures::future::BoxFuture;
use rand::{distributions::Alphanumeric, Rng};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, USER_AGENT};
use reqwest::Client;
use rmcp::model::Tool;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Duration as StdDuration;
use tokio::sync::oneshot;

const XAI_PROVIDER_NAME: &str = "xai";
pub const XAI_API_HOST: &str = "https://api.x.ai/v1";
pub const XAI_DEFAULT_MODEL: &str = "grok-4.3";
pub const XAI_KNOWN_MODELS: &[&str] = &[
    "grok-4.3",
    "grok-4.20-beta-latest-reasoning",
    "grok-4.20-beta-latest-non-reasoning",
    "grok-code-fast-1",
    "grok-4-0709",
    "grok-3",
    "grok-3-fast",
    "grok-3-mini",
    "grok-3-mini-fast",
    "grok-2-vision-1212",
    "grok-2-image-1212",
    "grok-3-latest",
    "grok-3-fast-latest",
    "grok-3-mini-latest",
    "grok-3-mini-fast-latest",
    "grok-2-vision",
    "grok-2-vision-latest",
    "grok-2-image",
    "grok-2-image-latest",
    "grok-2",
    "grok-2-latest",
];

pub const XAI_DOC_URL: &str = "https://docs.x.ai/docs/overview";

const XAI_OAUTH_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const XAI_OAUTH_SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";
const XAI_OAUTH_ISSUER: &str = "https://auth.x.ai";
const XAI_OAUTH_DISCOVERY_URL: &str = "https://auth.x.ai/.well-known/openid-configuration";
const XAI_OAUTH_CALLBACK_HOST: &str = "127.0.0.1";
const XAI_OAUTH_CALLBACK_PORT: u16 = 56121;
const XAI_OAUTH_CALLBACK_PATH: &str = "/callback";
const XAI_CONFIGURED_MARKER: &str = "xai_configured";
const REFRESH_THRESHOLD_SECS: i64 = 300;
const DEFAULT_TOKEN_LIFETIME_SECS: i64 = 3600;
const OAUTH_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone, Deserialize)]
struct XaiOAuthDiscovery {
    authorization_endpoint: String,
    token_endpoint: String,
    device_authorization_endpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct XaiToken {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: DateTime<Utc>,
    token_endpoint: String,
    issuer: String,
}

fn tokens_to_xai(
    tokens: DeviceFlowTokens,
    token_endpoint: String,
    prior_refresh: Option<&str>,
) -> XaiToken {
    XaiToken {
        access_token: tokens.access_token,
        refresh_token: tokens
            .refresh_token
            .or_else(|| prior_refresh.map(str::to_string)),
        expires_at: tokens
            .expires_at
            .unwrap_or_else(|| Utc::now() + Duration::seconds(DEFAULT_TOKEN_LIFETIME_SECS)),
        token_endpoint,
        issuer: XAI_OAUTH_ISSUER.to_string(),
    }
}

#[derive(Debug)]
struct TokenCache {
    path: PathBuf,
}

impl TokenCache {
    fn new() -> Self {
        Self {
            path: Paths::in_config_dir("xai/token.json"),
        }
    }

    async fn load(&self) -> Option<XaiToken> {
        let raw = tokio::fs::read_to_string(&self.path).await.ok()?;
        match serde_json::from_str(&raw) {
            Ok(token) => Some(token),
            Err(e) => {
                tracing::warn!(
                    "xAI token cache at {:?} is corrupted ({}); ignoring",
                    self.path,
                    e
                );
                None
            }
        }
    }

    async fn save(&self, token: &XaiToken) -> Result<()> {
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

    fn exists() -> bool {
        Self::new().path.exists()
    }
}

fn xai_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(USER_AGENT, HeaderValue::from_static("goose"));
    headers
}

fn require_trusted_xai_endpoint(endpoint: &str, label: &str) -> Result<String> {
    let url = url::Url::parse(endpoint)?;
    if url.scheme() != "https" {
        return Err(anyhow!("xAI OAuth discovery returned untrusted {label}"));
    }
    let Some(host) = url.host_str() else {
        return Err(anyhow!("xAI OAuth discovery returned untrusted {label}"));
    };
    if host == "x.ai" || host.ends_with(".x.ai") {
        Ok(endpoint.to_string())
    } else {
        Err(anyhow!("xAI OAuth discovery returned untrusted {label}"))
    }
}

async fn fetch_discovery(client: &Client) -> Result<XaiOAuthDiscovery> {
    #[derive(Deserialize)]
    struct RawDiscovery {
        authorization_endpoint: String,
        token_endpoint: String,
        device_authorization_endpoint: Option<String>,
    }

    let raw: RawDiscovery = client
        .get(XAI_OAUTH_DISCOVERY_URL)
        .headers(xai_headers())
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(XaiOAuthDiscovery {
        authorization_endpoint: require_trusted_xai_endpoint(
            &raw.authorization_endpoint,
            "authorization endpoint",
        )?,
        token_endpoint: require_trusted_xai_endpoint(&raw.token_endpoint, "token endpoint")?,
        device_authorization_endpoint: raw
            .device_authorization_endpoint
            .map(|endpoint| {
                require_trusted_xai_endpoint(&endpoint, "device authorization endpoint")
            })
            .transpose()?,
    })
}

#[derive(Debug, Clone)]
struct PkceChallenge {
    verifier: String,
    challenge: String,
}

fn generate_pkce() -> PkceChallenge {
    let verifier: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(64)
        .map(char::from)
        .collect();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    PkceChallenge {
        verifier,
        challenge,
    }
}

fn random_hex(byte_count: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes: Vec<u8> = (0..byte_count).map(|_| rand::random::<u8>()).collect();
    let mut out = String::with_capacity(byte_count * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn callback_url() -> String {
    format!(
        "http://{}:{}{}",
        XAI_OAUTH_CALLBACK_HOST, XAI_OAUTH_CALLBACK_PORT, XAI_OAUTH_CALLBACK_PATH
    )
}

fn build_authorize_url(discovery: &XaiOAuthDiscovery, pkce: &PkceChallenge, state: &str) -> String {
    let mut url = url::Url::parse(&discovery.authorization_endpoint)
        .expect("trusted xAI authorization endpoint should be a URL");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", XAI_OAUTH_CLIENT_ID)
        .append_pair("redirect_uri", &callback_url())
        .append_pair("scope", XAI_OAUTH_SCOPE)
        .append_pair("state", state)
        .append_pair("nonce", &random_hex(16))
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("plan", "generic")
        .append_pair("referrer", "goose");
    url.to_string()
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

async fn start_oauth_callback(
    expected_state: String,
) -> Result<(
    tokio::task::JoinHandle<()>,
    oneshot::Receiver<Result<String, String>>,
)> {
    let (code_tx, code_rx) = oneshot::channel::<Result<String, String>>();
    let tx = std::sync::Arc::new(tokio::sync::Mutex::new(Some(code_tx)));

    let app = Router::new().route(
        XAI_OAUTH_CALLBACK_PATH,
        get(move |Query(params): Query<CallbackQuery>| {
            let tx = tx.clone();
            let expected_state = expected_state.clone();
            async move {
                // Ignore callbacks that don't carry our state (e.g. a stale
                // tab from a prior attempt) so the real redirect still wins.
                if params.state.as_deref() != Some(expected_state.as_str()) {
                    return Html(
                        "<h2>xAI OAuth</h2><p>Unexpected callback; you can close this window.</p>"
                            .to_string(),
                    );
                }

                let result = if let Some(error) = params.error {
                    Err(format!("xAI OAuth failed: {error}"))
                } else if let Some(code) = params.code {
                    Ok(code)
                } else {
                    Err("xAI OAuth callback missing code".to_string())
                };

                if let Some(sender) = tx.lock().await.take() {
                    let _ = sender.send(result.clone());
                }

                let html = match result {
                    Ok(_) => {
                        "<h2>xAI OAuth complete</h2><p>You can close this window.</p>".to_string()
                    }
                    Err(error) => {
                        format!("<h2>xAI OAuth failed</h2><p>{}</p>", escape_html(&error))
                    }
                };
                Html(html)
            }
        }),
    );

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], XAI_OAUTH_CALLBACK_PORT));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    Ok((server, code_rx))
}

async fn wait_for_oauth_code(
    server: tokio::task::JoinHandle<()>,
    code_rx: oneshot::Receiver<Result<String, String>>,
) -> Result<String> {
    let result = tokio::time::timeout(StdDuration::from_secs(OAUTH_TIMEOUT_SECS), code_rx).await;
    server.abort();
    result
        .map_err(|_| anyhow!("xAI OAuth timed out"))?
        .map_err(|_| anyhow!("xAI OAuth callback server stopped"))?
        .map_err(|e| anyhow!(e))
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    error: Option<String>,
    error_description: Option<String>,
}

fn parse_token_response(
    response: TokenResponse,
    token_endpoint: String,
    prior_refresh: Option<&str>,
) -> Result<XaiToken> {
    let access_token = response.access_token.ok_or_else(|| {
        let message = response
            .error_description
            .or(response.error)
            .unwrap_or_else(|| "missing access_token".to_string());
        anyhow!("xAI OAuth token response failed: {message}")
    })?;
    let refresh_token = response
        .refresh_token
        .or_else(|| prior_refresh.map(str::to_string));
    Ok(XaiToken {
        access_token,
        refresh_token,
        expires_at: Utc::now()
            + Duration::seconds(response.expires_in.unwrap_or(DEFAULT_TOKEN_LIFETIME_SECS)),
        token_endpoint,
        issuer: XAI_OAUTH_ISSUER.to_string(),
    })
}

async fn exchange_oauth_code(
    client: &Client,
    discovery: &XaiOAuthDiscovery,
    code: &str,
    pkce: &PkceChallenge,
) -> Result<XaiToken> {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &callback_url()),
        ("client_id", XAI_OAUTH_CLIENT_ID),
        ("code_verifier", &pkce.verifier),
        ("code_challenge", &pkce.challenge),
        ("code_challenge_method", "S256"),
    ];

    let response: TokenResponse = client
        .post(&discovery.token_endpoint)
        .headers(xai_headers())
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .form(&params)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let token = parse_token_response(response, discovery.token_endpoint.clone(), None)?;
    if token.refresh_token.is_none() {
        return Err(anyhow!("xAI OAuth token response is missing refresh_token"));
    }
    Ok(token)
}

async fn refresh_xai_token(client: &Client, token: &XaiToken) -> Result<XaiToken> {
    if token.issuer != XAI_OAUTH_ISSUER {
        return Err(anyhow!("xAI OAuth credential has unexpected issuer"));
    }
    let token_endpoint = require_trusted_xai_endpoint(&token.token_endpoint, "token endpoint")?;
    let refresh_token = token
        .refresh_token
        .as_deref()
        .ok_or_else(|| anyhow!("xAI OAuth credential is missing refresh token"))?;
    let cfg = DeviceFlowConfig {
        device_auth_url: None,
        token_url: &token_endpoint,
        client_id: XAI_OAUTH_CLIENT_ID,
        scopes: Some(XAI_OAUTH_SCOPE),
        extra_headers: xai_headers(),
        encoding: RequestEncoding::Form,
    };
    let tokens = refresh_device_flow_token(client, &cfg, refresh_token).await?;
    Ok(tokens_to_xai(tokens, token_endpoint, Some(refresh_token)))
}

async fn run_browser_oauth(client: &Client) -> Result<XaiToken> {
    let discovery = fetch_discovery(client).await?;
    let pkce = generate_pkce();
    let state = random_hex(32);
    let authorize_url = build_authorize_url(&discovery, &pkce, &state);

    let (server, code_rx) = start_oauth_callback(state).await?;
    if let Err(e) = webbrowser::open(&authorize_url) {
        eprintln!("Failed to open browser automatically: {e}");
        eprintln!("Open this xAI OAuth URL in your browser:\n{authorize_url}");
    }
    let code = wait_for_oauth_code(server, code_rx).await?;
    exchange_oauth_code(client, &discovery, &code, &pkce).await
}

async fn run_device_oauth(client: &Client) -> Result<XaiToken> {
    let discovery = fetch_discovery(client).await?;
    let device_auth_url = discovery.device_authorization_endpoint.ok_or_else(|| {
        anyhow!("xAI OAuth discovery response is missing device_authorization_endpoint")
    })?;
    let cfg = DeviceFlowConfig {
        device_auth_url: Some(&device_auth_url),
        token_url: &discovery.token_endpoint,
        client_id: XAI_OAUTH_CLIENT_ID,
        scopes: Some(XAI_OAUTH_SCOPE),
        extra_headers: xai_headers(),
        encoding: RequestEncoding::Form,
    };
    let tokens = run_device_flow(client, &cfg).await?;
    let token = tokens_to_xai(tokens, discovery.token_endpoint, None);
    if token.refresh_token.is_none() {
        return Err(anyhow!(
            "xAI device-code token response is missing refresh_token"
        ));
    }
    Ok(token)
}

async fn usable_oauth_token(client: &Client, cache: &TokenCache) -> Option<XaiToken> {
    let token = cache.load().await?;
    if token.expires_at - Utc::now() > Duration::seconds(REFRESH_THRESHOLD_SECS) {
        return Some(token);
    }
    match refresh_xai_token(client, &token).await {
        Ok(refreshed) => {
            if let Err(e) = cache.save(&refreshed).await {
                tracing::warn!("failed to persist refreshed xAI token: {}", e);
            }
            Some(refreshed)
        }
        Err(e) => {
            tracing::debug!("xAI token refresh failed: {}", e);
            if token.expires_at > Utc::now() {
                Some(token)
            } else {
                None
            }
        }
    }
}

pub struct XaiProvider {
    model: ModelConfig,
    host: String,
    client: Client,
    token_cache: TokenCache,
}

impl XaiProvider {
    pub async fn cleanup() -> Result<()> {
        TokenCache::new().clear().await?;
        let config = Config::global();
        let _ = config.delete(XAI_CONFIGURED_MARKER);
        if let Some(mut entry) = crate::config::get_provider_entry(config, XAI_PROVIDER_NAME) {
            if entry.configured {
                entry.configured = false;
                let _ = crate::config::set_provider_entry(config, XAI_PROVIDER_NAME, &entry);
            }
        }
        Ok(())
    }

    async fn from_env(model: ModelConfig) -> Result<Self> {
        let config = Config::global();
        let host: String = config
            .get_param("XAI_HOST")
            .unwrap_or_else(|_| XAI_API_HOST.to_string());
        let client = Client::builder()
            .timeout(StdDuration::from_secs(
                super::base::DEFAULT_PROVIDER_TIMEOUT_SECS,
            ))
            .build()?;
        let token_cache = TokenCache::new();

        Ok(Self {
            model,
            host,
            client,
            token_cache,
        })
    }

    async fn current_auth(&self) -> AuthMethod {
        if let Some(token) = usable_oauth_token(&self.client, &self.token_cache).await {
            AuthMethod::BearerToken(token.access_token)
        } else if let Ok(api_key) = Config::global().get_secret::<String>("XAI_API_KEY") {
            AuthMethod::BearerToken(api_key)
        } else {
            AuthMethod::NoAuth
        }
    }

    async fn current_provider(&self) -> Result<OpenAiCompatibleProvider, ProviderError> {
        let api_client = ApiClient::new(self.host.clone(), self.current_auth().await)
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;
        Ok(OpenAiCompatibleProvider::new(
            XAI_PROVIDER_NAME.to_string(),
            api_client,
            self.model.clone(),
            String::new(),
        ))
    }

    async fn configure_browser_oauth(&self) -> Result<()> {
        let token = run_browser_oauth(&self.client).await?;
        self.token_cache.save(&token).await?;
        Config::global().set_param(XAI_CONFIGURED_MARKER, Value::Bool(true))?;
        Ok(())
    }

    async fn configure_device_oauth(&self) -> Result<()> {
        let token = run_device_oauth(&self.client).await?;
        self.token_cache.save(&token).await?;
        Config::global().set_param(XAI_CONFIGURED_MARKER, Value::Bool(true))?;
        Ok(())
    }
}

impl ProviderDef for XaiProvider {
    type Provider = Self;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            XAI_PROVIDER_NAME,
            "xAI",
            "Grok models from xAI, including SuperGrok/X Premium OAuth login",
            XAI_DEFAULT_MODEL,
            XAI_KNOWN_MODELS.to_vec(),
            XAI_DOC_URL,
            vec![
                ConfigKey::new("XAI_API_KEY", true, true, None, true),
                ConfigKey::new_oauth("XAI_OAUTH", false, true, None, true),
                ConfigKey::new_oauth_device_code("XAI_DEVICE_CODE", false, true, None, true),
                ConfigKey::new("XAI_HOST", false, false, Some(XAI_API_HOST), false),
            ],
        )
        .with_setup_steps(vec![
            "Choose browser OAuth or device-code OAuth to use a SuperGrok/X Premium subscription",
            "Use XAI_API_KEY instead if you prefer xAI Console API keys",
        ])
    }

    fn from_env(
        model: ModelConfig,
        _extensions: Vec<crate::config::ExtensionConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(Self::from_env(model))
    }

    fn inventory_configured() -> bool {
        TokenCache::exists() || Config::global().get_secret::<String>("XAI_API_KEY").is_ok()
    }
}

#[async_trait]
impl Provider for XaiProvider {
    fn get_name(&self) -> &str {
        XAI_PROVIDER_NAME
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
        self.current_provider()
            .await?
            .stream(model_config, session_id, system, messages, tools)
            .await
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        self.current_provider()
            .await?
            .fetch_supported_models()
            .await
    }

    async fn configure_oauth(&self) -> Result<(), ProviderError> {
        self.configure_browser_oauth()
            .await
            .map_err(|e| ProviderError::Authentication(format!("xAI OAuth failed: {}", e)))
    }

    async fn configure_oauth_device_code(&self) -> Result<(), ProviderError> {
        self.configure_device_oauth().await.map_err(|e| {
            ProviderError::Authentication(format!("xAI device-code OAuth failed: {}", e))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_model_is_grok_43() {
        assert_eq!(XAI_DEFAULT_MODEL, "grok-4.3");
    }

    #[test]
    fn known_models_include_openclaw_models() {
        assert!(XAI_KNOWN_MODELS.contains(&"grok-4.3"));
        assert!(XAI_KNOWN_MODELS.contains(&"grok-4.20-beta-latest-reasoning"));
        assert!(XAI_KNOWN_MODELS.contains(&"grok-4.20-beta-latest-non-reasoning"));
    }

    #[test]
    fn builds_openclaw_style_authorize_url() {
        let discovery = XaiOAuthDiscovery {
            authorization_endpoint: "https://auth.x.ai/oauth2/auth".to_string(),
            token_endpoint: "https://auth.x.ai/oauth2/token".to_string(),
            device_authorization_endpoint: None,
        };
        let pkce = PkceChallenge {
            verifier: "verifier".to_string(),
            challenge: "challenge".to_string(),
        };
        let url = build_authorize_url(&discovery, &pkce, "state");
        assert!(url.contains("client_id=b1a00492-073a-47ea-816f-4c329264a828"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("code_challenge=challenge"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("plan=generic"));
        assert!(url.contains("referrer=goose"));
    }
}
