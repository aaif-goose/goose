use super::api_client::{ApiClient, AuthMethod, AuthProvider};
use super::base::{ConfigKey, MessageStream, Provider, ProviderDef, ProviderMetadata};
use super::openai_compatible::OpenAiCompatibleProvider;
use super::xai::{xai_known_model_info, XAI_API_HOST, XAI_DEFAULT_MODEL};
use crate::config::paths::Paths;
use crate::conversation::message::Message;
use crate::providers::private_file::write_private_file;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use axum::{extract::Query, response::Html, routing::get, Router};
use base64::Engine;
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;
use rmcp::model::Tool;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use tokio::sync::{oneshot, Mutex as TokioMutex};

// Public Grok-CLI OAuth client. xAI's auth server rejects loopback OAuth from
// non-allowlisted clients, so we reuse the Grok-CLI client_id that xAI ships
// for desktop OAuth flows.
const CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";

const AUTHORIZE_URL: &str = "https://auth.x.ai/oauth2/authorize";
const TOKEN_URL: &str = "https://auth.x.ai/oauth2/token";
// RFC 8628 device authorization endpoint, surfaced via xAI's
// /.well-known/openid-configuration as `device_authorization_endpoint`.
const DEVICE_AUTHORIZATION_URL: &str = "https://auth.x.ai/oauth2/device/code";
const DEVICE_CODE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

const OAUTH_SCOPES: &[&str] = &[
    "openid",
    "profile",
    "email",
    "offline_access",
    "grok-cli:access",
    "api:access",
];

// xAI rejects redirect_uris that don't match what was registered for the
// Grok-CLI client. The host:port pair is part of the registration, so we have
// to bind the loopback server to this exact port.
const OAUTH_HOST: [u8; 4] = [127, 0, 0, 1];
const OAUTH_PORT: u16 = 56121;
const OAUTH_REDIRECT_PATH: &str = "/callback";

const OAUTH_TIMEOUT_SECS: u64 = 300;
const HTML_AUTO_CLOSE_TIMEOUT_MS: u64 = 2000;

// Refresh skew: refresh tokens this long before stored expiry so a single
// long-running tool call doesn't have to recover from a mid-flight 401.
const ACCESS_TOKEN_REFRESH_SKEW_SECS: i64 = 120;

// Device-code polling bounds.
const DEVICE_CODE_DEFAULT_INTERVAL_SECS: u64 = 5;
const DEVICE_CODE_MIN_INTERVAL_SECS: u64 = 1;
const DEVICE_CODE_SLOW_DOWN_INCREMENT_SECS: u64 = 5;
const DEVICE_CODE_DEFAULT_EXPIRES_SECS: u64 = 5 * 60;

const XAI_OAUTH_PROVIDER_NAME: &str = "xai_oauth";
const XAI_OAUTH_DOC_URL: &str = "https://x.ai/grok";

#[derive(Debug)]
struct XaiAuthState {
    oauth_mutex: TokioMutex<()>,
    refresh_mutex: TokioMutex<()>,
}

impl XaiAuthState {
    pub(crate) fn new() -> Self {
        Self {
            oauth_mutex: TokioMutex::new(()),
            refresh_mutex: TokioMutex::new(()),
        }
    }

    fn instance() -> Arc<Self> {
        Arc::clone(&XAI_AUTH_STATE)
    }
}

static XAI_AUTH_STATE: LazyLock<Arc<XaiAuthState>> =
    LazyLock::new(|| Arc::new(XaiAuthState::new()));

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenData {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    id_token: Option<String>,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct TokenCache {
    cache_path: PathBuf,
}

fn get_cache_path() -> PathBuf {
    Paths::in_config_dir("xai_oauth/tokens.json")
}

impl TokenCache {
    pub(crate) fn new() -> Self {
        let cache_path = get_cache_path();
        if let Some(parent) = cache_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self { cache_path }
    }

    fn load(&self) -> Option<TokenData> {
        let contents = std::fs::read_to_string(&self.cache_path).ok()?;
        serde_json::from_str(&contents).ok()
    }
    pub(crate) fn has_token(&self) -> bool {
        self.load().is_some()
    }

    fn save(&self, token_data: &TokenData) -> Result<()> {
        let contents = serde_json::to_string(token_data)?;
        write_private_file(&self.cache_path, &contents)?;
        Ok(())
    }

    fn clear(&self) {
        let _ = std::fs::remove_file(&self.cache_path);
    }
}

struct PkceChallenge {
    verifier: String,
    challenge: String,
}

fn generate_pkce() -> PkceChallenge {
    let verifier = nanoid::nanoid!(64);
    let digest = sha2::Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    PkceChallenge {
        verifier,
        challenge,
    }
}

fn generate_state() -> String {
    nanoid::nanoid!(32)
}

fn redirect_uri() -> String {
    format!(
        "http://{}.{}.{}.{}:{}{}",
        OAUTH_HOST[0], OAUTH_HOST[1], OAUTH_HOST[2], OAUTH_HOST[3], OAUTH_PORT, OAUTH_REDIRECT_PATH
    )
}

fn build_authorize_url(pkce: &PkceChallenge, state: &str, nonce: &str) -> Result<String> {
    let scopes = OAUTH_SCOPES.join(" ");
    let redirect = redirect_uri();
    // `plan=generic` opts the consent screen into xAI's generic OAuth plan
    // tier; without it, accounts.x.ai rejects loopback OAuth from
    // non-allowlisted clients. `referrer=goose` lets xAI attribute
    // goose-originated logins.
    let params = [
        ("response_type", "code"),
        ("client_id", CLIENT_ID),
        ("redirect_uri", redirect.as_str()),
        ("scope", scopes.as_str()),
        ("code_challenge", pkce.challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("state", state),
        ("nonce", nonce),
        ("plan", "generic"),
        ("referrer", "goose"),
    ];
    let query = serde_urlencoded::to_string(params)?;
    Ok(format!("{}?{}", AUTHORIZE_URL, query))
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

async fn exchange_code_for_tokens(code: &str, pkce: &PkceChallenge) -> Result<TokenResponse> {
    let client = reqwest::Client::new();
    let redirect = redirect_uri();
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect.as_str()),
        ("client_id", CLIENT_ID),
        ("code_verifier", pkce.verifier.as_str()),
    ];

    let resp = client
        .post(TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("xAI token exchange failed ({}): {}", status, text));
    }

    Ok(resp.json().await?)
}

async fn refresh_access_token(refresh_token: &str) -> Result<TokenResponse, ProviderError> {
    let client = reqwest::Client::new();
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", CLIENT_ID),
    ];

    let resp = client
        .post(TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .await
        .map_err(ProviderError::from)?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(token_refresh_error(status, text));
    }

    resp.json()
        .await
        .map_err(|error| ProviderError::RequestFailed(error.to_string()))
}

fn token_refresh_error(status: reqwest::StatusCode, body: String) -> ProviderError {
    let details = format!("xAI token refresh failed ({status}): {body}");
    let oauth_error = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|value| value.get("error")?.as_str().map(str::to_owned));

    if oauth_error.as_deref() == Some("invalid_grant") {
        return ProviderError::Authentication(details);
    }

    match status {
        reqwest::StatusCode::TOO_MANY_REQUESTS => ProviderError::RateLimitExceeded {
            details,
            retry_delay: None,
        },
        _ if status.is_server_error() => ProviderError::ServerError(details),
        _ => ProviderError::RequestFailed(details),
    }
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    interval: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
struct DeviceTokenErrorBody {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

async fn request_device_code() -> Result<DeviceCodeResponse> {
    let client = reqwest::Client::new();
    let scopes = OAUTH_SCOPES.join(" ");
    let params = [("client_id", CLIENT_ID), ("scope", scopes.as_str())];
    let resp = client
        .post(DEVICE_AUTHORIZATION_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!(
            "xAI device code request failed ({}): {}",
            status,
            text
        ));
    }
    Ok(resp.json().await?)
}

async fn poll_device_code_token(device: &DeviceCodeResponse) -> Result<TokenResponse> {
    let expires_secs = device
        .expires_in
        .filter(|v| *v > 0)
        .unwrap_or(DEVICE_CODE_DEFAULT_EXPIRES_SECS);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(expires_secs);
    let mut interval_secs = device
        .interval
        .filter(|v| *v > 0)
        .unwrap_or(DEVICE_CODE_DEFAULT_INTERVAL_SECS)
        .max(DEVICE_CODE_MIN_INTERVAL_SECS);

    let client = reqwest::Client::new();
    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!("xAI device authorization timed out"));
        }
        let params = [
            ("grant_type", DEVICE_CODE_GRANT_TYPE),
            ("client_id", CLIENT_ID),
            ("device_code", device.device_code.as_str()),
        ];
        let resp = client
            .post(TOKEN_URL)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .form(&params)
            .send()
            .await?;
        if resp.status().is_success() {
            return Ok(resp.json().await?);
        }
        let status = resp.status();
        let body: DeviceTokenErrorBody = resp.json().await.unwrap_or_default();
        match body.error.as_deref() {
            Some("authorization_pending") => {
                tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
            }
            Some("slow_down") => {
                interval_secs += DEVICE_CODE_SLOW_DOWN_INCREMENT_SECS;
                tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
            }
            Some("access_denied") | Some("authorization_denied") => {
                return Err(anyhow!("xAI device authorization was denied"));
            }
            Some("expired_token") => {
                return Err(anyhow!(
                    "xAI device code expired - please re-run goose configure"
                ));
            }
            other => {
                let detail = body
                    .error_description
                    .or_else(|| other.map(String::from))
                    .unwrap_or_default();
                return Err(anyhow!(
                    "xAI device token exchange failed ({}): {}",
                    status,
                    detail
                ));
            }
        }
    }
}

const HTML_SUCCESS_TEMPLATE: &str = r#"<!doctype html>
<html>
  <head>
    <title>goose - xAI Authorization Successful</title>
    <style>
      body {
        font-family: system-ui, -apple-system, sans-serif;
        display: flex;
        justify-content: center;
        align-items: center;
        height: 100vh;
        margin: 0;
        background: #131010;
        color: #f1ecec;
      }
      .container { text-align: center; padding: 2rem; }
      h1 { color: #f1ecec; margin-bottom: 1rem; }
      p { color: #b7b1b1; }
    </style>
  </head>
  <body>
    <div class="container">
      <h1>Authorization Successful</h1>
      <p>You can close this window and return to goose.</p>
    </div>
    <script>const AUTO_CLOSE_TIMEOUT_MS = __AUTO_CLOSE_TIMEOUT_MS__; setTimeout(() => window.close(), AUTO_CLOSE_TIMEOUT_MS)</script>
  </body>
</html>"#;

fn html_success() -> String {
    HTML_SUCCESS_TEMPLATE.replace(
        "__AUTO_CLOSE_TIMEOUT_MS__",
        &HTML_AUTO_CLOSE_TIMEOUT_MS.to_string(),
    )
}

fn html_error(error: &str) -> String {
    let safe_error = v_htmlescape::escape_fmt(error);
    format!(
        r#"<!doctype html>
<html>
  <head>
    <title>goose - xAI Authorization Failed</title>
    <style>
      body {{
        font-family: system-ui, -apple-system, sans-serif;
        display: flex; justify-content: center; align-items: center;
        height: 100vh; margin: 0; background: #131010; color: #f1ecec;
      }}
      .container {{ text-align: center; padding: 2rem; }}
      h1 {{ color: #fc533a; margin-bottom: 1rem; }}
      p {{ color: #b7b1b1; }}
      .error {{
        color: #ff917b; font-family: monospace; margin-top: 1rem;
        padding: 1rem; background: #3c140d; border-radius: 0.5rem;
      }}
    </style>
  </head>
  <body>
    <div class="container">
      <h1>Authorization Failed</h1>
      <p>An error occurred during authorization.</p>
      <div class="error">{}</div>
    </div>
  </body>
</html>"#,
        safe_error
    )
}

#[derive(Deserialize)]
struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

fn oauth_callback_router(
    expected_state: String,
    tx: Arc<TokioMutex<Option<oneshot::Sender<Result<String>>>>>,
) -> Router {
    Router::new().route(
        OAUTH_REDIRECT_PATH,
        get(move |Query(params): Query<CallbackParams>| {
            let tx = tx.clone();
            let expected = expected_state.clone();
            async move {
                if let Some(error) = params.error {
                    let msg = params.error_description.unwrap_or(error);
                    if let Some(sender) = tx.lock().await.take() {
                        let _ = sender.send(Err(anyhow!("{}", msg)));
                    }
                    return Html(html_error(&msg));
                }

                let code = match params.code {
                    Some(c) => c,
                    None => {
                        let msg = "Missing authorization code";
                        if let Some(sender) = tx.lock().await.take() {
                            let _ = sender.send(Err(anyhow!("{}", msg)));
                        }
                        return Html(html_error(msg));
                    }
                };

                if params.state.as_deref() != Some(&expected) {
                    let msg = "Invalid state - potential CSRF attack";
                    if let Some(sender) = tx.lock().await.take() {
                        let _ = sender.send(Err(anyhow!("{}", msg)));
                    }
                    return Html(html_error(msg));
                }

                if let Some(sender) = tx.lock().await.take() {
                    let _ = sender.send(Ok(code));
                }
                Html(html_success())
            }
        }),
    )
}

async fn spawn_oauth_server(app: Router) -> Result<tokio::task::JoinHandle<()>> {
    let addr = SocketAddr::from((OAUTH_HOST, OAUTH_PORT));
    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        if e.kind() == io::ErrorKind::AddrInUse {
            anyhow!(
                "xAI OAuth callback server failed to bind to {}: port {} is already in use \
                 (likely another OAuth client like Grok-CLI is bound to the same port). \
                 Stop the process using this port and try again, or use the device-code flow.",
                addr,
                OAUTH_PORT
            )
        } else {
            anyhow!(
                "xAI OAuth callback server failed to bind to {}: {}",
                addr,
                e
            )
        }
    })?;
    Ok(tokio::spawn(async move {
        let server = axum::serve(listener, app);
        let _ = server.await;
    }))
}

struct ServerHandleGuard(Option<tokio::task::JoinHandle<()>>);

impl ServerHandleGuard {
    fn new(handle: tokio::task::JoinHandle<()>) -> Self {
        Self(Some(handle))
    }

    fn abort(&mut self) {
        if let Some(handle) = self.0.take() {
            handle.abort();
        }
    }
}

impl Drop for ServerHandleGuard {
    fn drop(&mut self) {
        self.abort();
    }
}

async fn wait_for_oauth_code(rx: oneshot::Receiver<Result<String>>) -> Result<String> {
    let code_result =
        tokio::time::timeout(std::time::Duration::from_secs(OAUTH_TIMEOUT_SECS), rx).await;
    code_result
        .map_err(|_| anyhow!("xAI OAuth flow timed out"))??
        .map_err(|e| anyhow!("xAI OAuth callback error: {}", e))
}

async fn perform_loopback_oauth_flow(auth_state: &XaiAuthState) -> Result<TokenData> {
    let _guard = auth_state.oauth_mutex.try_lock().map_err(|_| {
        anyhow!("Another xAI OAuth flow is already in progress; please try again later")
    })?;

    let pkce = generate_pkce();
    let csrf_state = generate_state();
    let nonce = generate_state();
    let auth_url = build_authorize_url(&pkce, &csrf_state, &nonce)?;

    let (tx, rx) = oneshot::channel::<Result<String>>();
    let tx = Arc::new(TokioMutex::new(Some(tx)));
    let app = oauth_callback_router(csrf_state.clone(), tx);
    let server_handle = spawn_oauth_server(app).await?;
    let mut server_guard = ServerHandleGuard::new(server_handle);

    if webbrowser::open(&auth_url).is_err() {
        tracing::info!(
            "Please open this URL in your browser to authorize goose with xAI:\n{}",
            auth_url
        );
    }

    let code_result = wait_for_oauth_code(rx).await;
    server_guard.abort();
    let code = code_result?;

    let tokens = exchange_code_for_tokens(&code, &pkce).await?;
    Ok(token_data_from_response(tokens))
}

async fn perform_device_code_flow() -> Result<TokenData> {
    let device = request_device_code().await?;
    let url = device
        .verification_uri_complete
        .clone()
        .unwrap_or_else(|| device.verification_uri.clone());
    tracing::info!(
        "xAI device authorization: open {} and enter code {}",
        device.verification_uri,
        device.user_code
    );
    eprintln!(
        "\nTo authorize goose with xAI, open this URL in any browser:\n  {}\nand enter code: {}\n",
        url, device.user_code
    );
    let tokens = poll_device_code_token(&device).await?;
    Ok(token_data_from_response(tokens))
}

fn token_data_from_response(tokens: TokenResponse) -> TokenData {
    let expires_at = Utc::now() + chrono::Duration::seconds(tokens.expires_in.unwrap_or(3600));
    TokenData {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        id_token: tokens.id_token,
        expires_at,
    }
}

#[derive(Debug)]
struct XaiOAuthAuthProvider {
    cache: TokenCache,
    state: Arc<XaiAuthState>,
}

impl XaiOAuthAuthProvider {
    fn new(state: Arc<XaiAuthState>) -> Self {
        Self {
            cache: TokenCache::new(),
            state,
        }
    }

    async fn get_valid_token(&self) -> Result<TokenData, ProviderError> {
        if let Some(mut token_data) = self.cache.load() {
            if token_data.expires_at
                > Utc::now() + chrono::Duration::seconds(ACCESS_TOKEN_REFRESH_SKEW_SECS)
            {
                return Ok(token_data);
            }

            // Single-flight refresh: collapse concurrent fetches onto one
            // HTTP call so we don't replay a rotating refresh_token.
            let _refresh_guard = self.state.refresh_mutex.lock().await;
            if let Some(reloaded) = self.cache.load() {
                if reloaded.expires_at
                    > Utc::now() + chrono::Duration::seconds(ACCESS_TOKEN_REFRESH_SKEW_SECS)
                {
                    return Ok(reloaded);
                }
                token_data = reloaded;
            }

            tracing::debug!("xAI access token expiring, attempting refresh");
            match refresh_access_token(&token_data.refresh_token).await {
                Ok(new_tokens) => {
                    token_data.access_token = new_tokens.access_token;
                    if !new_tokens.refresh_token.is_empty() {
                        token_data.refresh_token = new_tokens.refresh_token;
                    }
                    if new_tokens.id_token.is_some() {
                        token_data.id_token = new_tokens.id_token;
                    }
                    token_data.expires_at = Utc::now()
                        + chrono::Duration::seconds(new_tokens.expires_in.unwrap_or(3600));
                    self.cache.save(&token_data).map_err(ProviderError::from)?;
                    tracing::info!("xAI access token refreshed");
                    return Ok(token_data);
                }
                Err(error @ ProviderError::Authentication(_)) => {
                    tracing::warn!("xAI token refresh rejected: {}", error);
                    self.cache.clear();
                    return Err(error);
                }
                Err(error) => {
                    if token_data.expires_at > Utc::now() {
                        return Ok(token_data);
                    }
                    return Err(error);
                }
            }
        }

        Err(ProviderError::NotConfigured)
    }
}

#[async_trait]
impl AuthProvider for XaiOAuthAuthProvider {
    async fn get_auth_header(&self) -> Result<(String, String)> {
        let token_data = self.get_valid_token().await?;
        Ok((
            "Authorization".to_string(),
            format!("Bearer {}", token_data.access_token),
        ))
    }
}

/// Delegating Provider that forwards chat/stream/etc. to an inner
/// `OpenAiCompatibleProvider` pointed at `https://api.x.ai/v1`, but overrides
/// `configure_oauth` so the desktop "Sign in" button (and any other caller of
/// `Provider::configure_oauth`) drives the loopback / device-code flow.
#[derive(serde::Serialize)]
pub struct XaiOAuthProvider {
    #[serde(skip)]
    inner: OpenAiCompatibleProvider,
    #[serde(skip)]
    auth_provider: Arc<XaiOAuthAuthProvider>,
}

impl XaiOAuthProvider {
    pub async fn cleanup() -> Result<()> {
        TokenCache::new().clear();
        Ok(())
    }
}

#[async_trait]
impl Provider for XaiOAuthProvider {
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
        self.auth_provider.get_valid_token().await?;
        self.inner
            .stream(model_config, system, messages, tools)
            .await
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        self.auth_provider.get_valid_token().await?;
        self.inner.fetch_supported_models().await
    }

    async fn configure_oauth(&self) -> Result<(), ProviderError> {
        // Preserve the previous token so a partially-completed sign-in
        // attempt (e.g. user closes the browser) doesn't sign them out.
        let previous_token = self.auth_provider.cache.load();
        self.auth_provider.cache.clear();

        let flow_result = match perform_loopback_oauth_flow(self.auth_provider.state.as_ref()).await
        {
            Ok(td) => Ok(td),
            Err(e) => {
                tracing::warn!(
                    "xAI loopback OAuth failed ({}); falling back to device-code flow",
                    e
                );
                perform_device_code_flow().await
            }
        };

        let save_result =
            flow_result.and_then(|token_data| self.auth_provider.cache.save(&token_data));

        if let Err(e) = save_result {
            if let Some(previous_token) = previous_token.as_ref() {
                if self.auth_provider.cache.load().is_none() {
                    let _ = self.auth_provider.cache.save(previous_token);
                }
            }
            return Err(ProviderError::Authentication(format!(
                "xAI OAuth flow failed: {}",
                e
            )));
        }
        Ok(())
    }
}

impl goose_providers::base::ProviderDescriptor for XaiOAuthProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::with_models(
            XAI_OAUTH_PROVIDER_NAME,
            "xAI (SuperGrok Subscription)",
            "Use your xAI SuperGrok subscription via OAuth instead of an API key. Falls back to a device-code flow on headless / remote machines.",
            XAI_DEFAULT_MODEL,
            xai_known_model_info(),
            XAI_OAUTH_DOC_URL,
            vec![
                ConfigKey::new_oauth("XAI_OAUTH_TOKEN", true, true, None, false),
                ConfigKey::new("XAI_HOST", false, false, Some(XAI_API_HOST), false),
            ],
        )
        .with_setup(
            crate::providers::catalog::ProviderSetupMetadata::new(
                crate::providers::catalog::ProviderSetupCategory::Model,
                crate::providers::catalog::ProviderSetupMethod::OauthBrowser,
                crate::providers::catalog::ProviderSetupGroup::Default,
            )
            .with_docs_url("https://x.ai/grok")
            .with_native_connect_query("xAI Grok")
            .with_capabilities(false, true, false),
        )
    }
}

impl ProviderDef for XaiOAuthProvider {
    type Provider = Self;

    fn from_env(
        _extensions: Vec<crate::config::ExtensionConfig>,
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(async move {
            let config = crate::config::Config::global();
            let host: String = config
                .get_param("XAI_HOST")
                .unwrap_or_else(|_| XAI_API_HOST.to_string());

            let auth_provider = Arc::new(XaiOAuthAuthProvider::new(XaiAuthState::instance()));
            let auth_for_client = Arc::clone(&auth_provider);
            let api_client = ApiClient::new_with_tls(
                host,
                AuthMethod::Custom(Box::new(SharedAuthProvider(auth_for_client))),
                tls_config,
            )?
            .with_request_builder(crate::session_context::session_id_request_builder());

            let inner = OpenAiCompatibleProvider::new(
                XAI_OAUTH_PROVIDER_NAME.to_string(),
                api_client,
                String::new(),
            );

            Ok(Self {
                inner,
                auth_provider,
            })
        })
    }
}

/// Adapter so the same `XaiOAuthAuthProvider` can be both owned by the
/// wrapper (for `configure_oauth`) and embedded as an `AuthMethod::Custom`
/// boxed `AuthProvider` in the inner `ApiClient`.
struct SharedAuthProvider(Arc<XaiOAuthAuthProvider>);

#[async_trait]
impl AuthProvider for SharedAuthProvider {
    async fn get_auth_header(&self) -> Result<(String, String)> {
        self.0.get_auth_header().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_url_safe_base64_of_sha256_of_verifier() {
        let pkce = generate_pkce();
        assert_eq!(pkce.verifier.len(), 64);
        // S256 of a 64-char ASCII verifier => 32-byte digest => 43 base64url chars (no padding).
        assert_eq!(pkce.challenge.len(), 43);
        assert!(!pkce.challenge.contains('='));
        assert!(!pkce.challenge.contains('+'));
        assert!(!pkce.challenge.contains('/'));
    }

    #[test]
    fn authorize_url_contains_required_oauth_params() {
        let pkce = PkceChallenge {
            verifier: "v".repeat(64),
            challenge: "challenge-fixture".to_string(),
        };
        let url = build_authorize_url(&pkce, "state-fixture", "nonce-fixture").unwrap();
        assert!(url.starts_with(AUTHORIZE_URL));
        assert!(url.contains(&format!("client_id={}", CLIENT_ID)));
        assert!(url.contains("code_challenge=challenge-fixture"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=state-fixture"));
        assert!(url.contains("nonce=nonce-fixture"));
        assert!(url.contains("plan=generic"));
        assert!(url.contains("referrer=goose"));
        assert!(url.contains("scope=openid"));
        assert!(url.contains("offline_access"));
        assert!(url.contains("grok-cli%3Aaccess"));
    }

    #[test]
    fn redirect_uri_matches_registered_grok_cli_value() {
        // xAI rejects mismatched redirect_uris for the Grok-CLI client_id.
        // This pins the loopback host/port that pairs with that client.
        assert_eq!(redirect_uri(), "http://127.0.0.1:56121/callback");
    }

    #[test]
    fn token_cache_path_lives_under_goose_config_dir() {
        let path = get_cache_path();
        let s = path.to_string_lossy().into_owned();
        assert!(
            s.contains("xai_oauth"),
            "expected token path under xai_oauth/, got {}",
            s
        );
        assert!(s.ends_with("tokens.json"));
    }

    #[tokio::test]
    async fn missing_token_does_not_start_oauth() {
        let directory = tempfile::tempdir().unwrap();
        let auth_provider = XaiOAuthAuthProvider {
            cache: TokenCache {
                cache_path: directory.path().join("missing.json"),
            },
            state: XaiAuthState::instance(),
        };

        let error = auth_provider.get_valid_token().await.unwrap_err();

        assert_eq!(error, ProviderError::NotConfigured);
    }

    #[tokio::test]
    async fn stream_preserves_not_configured_error() {
        let directory = tempfile::tempdir().unwrap();
        let auth_provider = Arc::new(XaiOAuthAuthProvider {
            cache: TokenCache {
                cache_path: directory.path().join("missing.json"),
            },
            state: XaiAuthState::instance(),
        });
        let api_client =
            ApiClient::new_with_tls("http://127.0.0.1:1".to_string(), AuthMethod::NoAuth, None)
                .unwrap();
        let provider = XaiOAuthProvider {
            inner: OpenAiCompatibleProvider::new(
                XAI_OAUTH_PROVIDER_NAME.to_string(),
                api_client,
                String::new(),
            ),
            auth_provider,
        };

        let error = provider
            .stream(&ModelConfig::new(XAI_DEFAULT_MODEL), "", &[], &[])
            .await
            .err()
            .unwrap();

        assert_eq!(error, ProviderError::NotConfigured);
    }

    #[test]
    fn token_refresh_errors_distinguish_rejected_and_transient_requests() {
        assert!(matches!(
            token_refresh_error(
                reqwest::StatusCode::BAD_REQUEST,
                r#"{"error":"invalid_grant"}"#.to_string()
            ),
            ProviderError::Authentication(_)
        ));
        assert!(matches!(
            token_refresh_error(
                reqwest::StatusCode::UNAUTHORIZED,
                r#"{"error":"invalid_client"}"#.to_string()
            ),
            ProviderError::RequestFailed(_)
        ));
        assert!(matches!(
            token_refresh_error(
                reqwest::StatusCode::FORBIDDEN,
                "request rejected by proxy".to_string()
            ),
            ProviderError::RequestFailed(_)
        ));
        assert!(matches!(
            token_refresh_error(reqwest::StatusCode::TOO_MANY_REQUESTS, String::new()),
            ProviderError::RateLimitExceeded { .. }
        ));
        assert!(matches!(
            token_refresh_error(reqwest::StatusCode::SERVICE_UNAVAILABLE, String::new()),
            ProviderError::ServerError(_)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn token_cache_replaces_loose_file_with_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let cache_path = directory.path().join("tokens.json");
        std::fs::write(&cache_path, "{}").unwrap();
        std::fs::set_permissions(&cache_path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let cache = TokenCache {
            cache_path: cache_path.clone(),
        };

        cache
            .save(&TokenData {
                access_token: "access".to_string(),
                refresh_token: "refresh".to_string(),
                id_token: None,
                expires_at: Utc::now() + chrono::Duration::hours(1),
            })
            .unwrap();

        let mode = std::fs::metadata(cache_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
