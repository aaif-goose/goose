use crate::config::paths::Paths;
use crate::conversation::message::{Message, MessageContent};
use crate::model::ModelConfig;
use crate::providers::api_client::AuthProvider;
use crate::providers::base::{ConfigKey, MessageStream, Provider, ProviderDef, ProviderMetadata};
use crate::providers::errors::ProviderError;
use crate::providers::formats::openai_responses::responses_api_to_streaming_message;
use crate::providers::openai_compatible::handle_status_openai_compat;
use crate::providers::retry::ProviderRetry;
use crate::session_context::SESSION_ID_HEADER;
use anyhow::{anyhow, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use axum::{extract::Query, response::Html, routing::get, Router};
use base64::Engine;
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use futures::{StreamExt, TryStreamExt};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};
use reqwest::header::{HeaderName, HeaderValue};
use rmcp::model::{RawContent, Role, Tool};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Digest;
use std::io;
use std::net::SocketAddr;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use tokio::pin;
use tokio::sync::{oneshot, Mutex as TokioMutex};
use tokio_util::codec::{FramedRead, LinesCodec};
use tokio_util::io::StreamReader;

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const ISSUER: &str = "https://auth.openai.com";
const CODEX_API_ENDPOINT: &str = "https://chatgpt.com/backend-api/codex";
const OAUTH_SCOPES: &[&str] = &["openid", "profile", "email", "offline_access"];
// Canonical localhost callback port for Codex OAuth (default localhost:1455 per OpenAI docs).
// https://developers.openai.com/codex/auth/
const OAUTH_PORT: u16 = 1455;
// Allow time for users to complete the browser-based OAuth flow.
const OAUTH_TIMEOUT_SECS: u64 = 300;
const HTML_AUTO_CLOSE_TIMEOUT_MS: u64 = 2000;

const CHATGPT_CODEX_PROVIDER_NAME: &str = "chatgpt_codex";
pub const CHATGPT_CODEX_DEFAULT_MODEL: &str = "gpt-5.3-codex";

#[derive(Debug)]
pub struct ChatGptCodexModelAttrs {
    pub name: &'static str,
    pub reasoning_levels: &'static [&'static str],
}

pub const CHATGPT_CODEX_KNOWN_MODELS: &[ChatGptCodexModelAttrs] = &[
    ChatGptCodexModelAttrs {
        name: "gpt-5.4",
        reasoning_levels: &["low", "medium", "high", "xhigh"],
    },
    ChatGptCodexModelAttrs {
        name: "gpt-5.3-codex",
        reasoning_levels: &["low", "medium", "high", "xhigh"],
    },
    ChatGptCodexModelAttrs {
        name: "gpt-5.2-codex",
        reasoning_levels: &["low", "medium", "high", "xhigh"],
    },
    ChatGptCodexModelAttrs {
        name: "gpt-5.1-codex",
        reasoning_levels: &["low", "medium", "high", "xhigh"],
    },
    ChatGptCodexModelAttrs {
        name: "gpt-5.1-codex-mini",
        reasoning_levels: &["medium", "high"],
    },
    ChatGptCodexModelAttrs {
        name: "gpt-5.1-codex-max",
        reasoning_levels: &["low", "medium", "high", "xhigh"],
    },
];

const CHATGPT_CODEX_DOC_URL: &str = "https://openai.com/chatgpt";

const DEFAULT_REASONING_LEVELS: &[&str] = &["medium", "high"];

pub fn reasoning_levels_for_model(model_name: &str) -> &'static [&'static str] {
    CHATGPT_CODEX_KNOWN_MODELS
        .iter()
        .find(|m| m.name == model_name)
        .map(|m| m.reasoning_levels)
        .unwrap_or(DEFAULT_REASONING_LEVELS)
}

fn known_model_names() -> Vec<&'static str> {
    CHATGPT_CODEX_KNOWN_MODELS.iter().map(|m| m.name).collect()
}

const GPT_53_CODEX_TOOL_PREAMBLE: &str = "\
You are a coding agent. You have access to tools to accomplish tasks. \
Always use your tools to fulfill requests - do not just describe what you would do. \
Keep going until the query is completely resolved before yielding back to the user. \
Autonomously resolve the query using the tools available to you. \
Do NOT guess or make up an answer. \
Before making tool calls, send a brief message explaining what you're about to do.";

#[derive(Debug)]
struct ChatGptCodexAuthState {
    oauth_mutex: TokioMutex<()>,
    jwks_cache: TokioMutex<Option<JwkSet>>,
}

impl ChatGptCodexAuthState {
    fn new() -> Self {
        Self {
            oauth_mutex: TokioMutex::new(()),
            jwks_cache: TokioMutex::new(None),
        }
    }

    fn instance() -> Arc<Self> {
        Arc::clone(&CHATGPT_CODEX_AUTH_STATE)
    }
}

static CHATGPT_CODEX_AUTH_STATE: LazyLock<Arc<ChatGptCodexAuthState>> =
    LazyLock::new(|| Arc::new(ChatGptCodexAuthState::new()));

fn build_input_items(messages: &[Message]) -> Result<Vec<Value>> {
    let mut items = Vec::new();

    for message in messages {
        let role = match message.role {
            Role::User => Some("user"),
            Role::Assistant => Some("assistant"),
        };
        let mut content_items: Vec<Value> = Vec::new();

        let flush_text = |items: &mut Vec<Value>, role: Option<&str>, content: &mut Vec<Value>| {
            if let Some(role) = role {
                if !content.is_empty() {
                    items.push(json!({ "role": role, "content": std::mem::take(content) }));
                }
            } else {
                content.clear();
            }
        };

        for content in &message.content {
            match content {
                MessageContent::Text(text) => {
                    if !text.text.is_empty() {
                        let content_type = if message.role == Role::Assistant {
                            "output_text"
                        } else {
                            "input_text"
                        };
                        content_items.push(json!({ "type": content_type, "text": text.text }));
                    }
                }
                MessageContent::Image(img) => {
                    content_items.push(json!({
                        "type": "input_image",
                        "image_url": format!("data:{};base64,{}", img.mime_type, img.data),
                    }));
                }
                MessageContent::ToolRequest(request) => {
                    flush_text(&mut items, role, &mut content_items);
                    if let Ok(tool_call) = &request.tool_call {
                        let arguments_str = match tool_call.arguments.as_ref() {
                            Some(args) => serde_json::to_string(args)?,
                            None => "{}".to_string(),
                        };
                        items.push(json!({
                            "type": "function_call",
                            "call_id": request.id,
                            "name": tool_call.name,
                            "arguments": arguments_str
                        }));
                    }
                }
                MessageContent::ToolResponse(response) => {
                    flush_text(&mut items, role, &mut content_items);
                    match &response.tool_result {
                        Ok(contents) => {
                            let text_content: Vec<String> = contents
                                .content
                                .iter()
                                .filter_map(|c| {
                                    if let RawContent::Text(t) = c.deref() {
                                        Some(t.text.clone())
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            if !text_content.is_empty() {
                                items.push(json!({
                                    "type": "function_call_output",
                                    "call_id": response.id,
                                    "output": text_content.join("\n")
                                }));
                            }
                        }
                        Err(error_data) => {
                            items.push(json!({
                                "type": "function_call_output",
                                "call_id": response.id,
                                "output": format!("Error: {}", error_data.message)
                            }));
                        }
                    }
                }
                _ => {}
            }
        }

        flush_text(&mut items, role, &mut content_items);
    }

    Ok(items)
}

fn get_reasoning_effort(model_name: &str) -> String {
    let config = crate::config::Config::global();
    let effort = config
        .get_chatgpt_codex_reasoning_effort()
        .map(String::from)
        .unwrap_or_else(|_| "medium".to_string());

    let valid_levels = reasoning_levels_for_model(model_name);
    if valid_levels.contains(&effort.as_str()) {
        effort
    } else {
        tracing::warn!(
            "Invalid CHATGPT_CODEX_REASONING_EFFORT '{}' for model '{}', using 'medium'",
            effort,
            model_name
        );
        "medium".to_string()
    }
}

fn create_codex_request(
    model_config: &ModelConfig,
    system: &str,
    messages: &[Message],
    tools: &[Tool],
) -> Result<Value> {
    let input_items = build_input_items(messages)?;
    let reasoning_effort = get_reasoning_effort(&model_config.model_name);

    let instructions = match model_config.model_name.as_str() {
        "gpt-5.3-codex" => format!("{GPT_53_CODEX_TOOL_PREAMBLE}\n\n{system}"),
        _ => system.to_string(),
    };

    let mut payload = json!({
        "model": model_config.model_name,
        "input": input_items,
        "store": false,
        "reasoning": {"effort": reasoning_effort},
        "instructions": instructions,
    });

    let payload_obj = payload
        .as_object_mut()
        .ok_or_else(|| anyhow!("Codex payload must be a JSON object"))?;

    if !tools.is_empty() {
        let tools_spec: Vec<Value> = tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                })
            })
            .collect();

        payload_obj.insert("tools".to_string(), json!(tools_spec));
        payload_obj.insert("tool_choice".to_string(), json!("auto"));
        payload_obj.insert("parallel_tool_calls".to_string(), json!(true));
    }

    if let Some(temp) = model_config.temperature {
        payload_obj.insert("temperature".to_string(), json!(temp));
    }

    Ok(payload)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenData {
    access_token: String,
    refresh_token: String,
    id_token: Option<String>,
    expires_at: DateTime<Utc>,
    account_id: Option<String>,
}

#[derive(Debug, Clone)]
struct TokenCache {
    cache_path: PathBuf,
}

fn get_cache_path() -> PathBuf {
    Paths::in_config_dir("chatgpt_codex/tokens.json")
}

impl TokenCache {
    fn new() -> Self {
        let cache_path = get_cache_path();
        if let Some(parent) = cache_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self { cache_path }
    }

    fn load(&self) -> Option<TokenData> {
        if let Ok(contents) = std::fs::read_to_string(&self.cache_path) {
            serde_json::from_str(&contents).ok()
        } else {
            None
        }
    }

    fn save(&self, token_data: &TokenData) -> Result<()> {
        if let Some(parent) = self.cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string(token_data)?;
        std::fs::write(&self.cache_path, contents)?;
        Ok(())
    }

    fn clear(&self) {
        let _ = std::fs::remove_file(&self.cache_path);
    }
}

#[derive(Debug, Deserialize)]
struct JwtClaims {
    chatgpt_account_id: Option<String>,
    #[serde(rename = "https://api.openai.com/auth")]
    auth_claims: Option<AuthClaims>,
    organizations: Option<Vec<OrgInfo>>,
}

#[derive(Debug, Deserialize)]
struct AuthClaims {
    chatgpt_account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OrgInfo {
    id: String,
}

#[derive(Debug, Deserialize)]
struct OidcConfiguration {
    jwks_uri: String,
}

async fn fetch_jwks_for(issuer: &str) -> Result<JwkSet> {
    let client = reqwest::Client::new();
    let config_url = format!("{}/.well-known/openid-configuration", issuer);
    let config = client
        .get(config_url)
        .send()
        .await?
        .error_for_status()?
        .json::<OidcConfiguration>()
        .await?;

    let jwks = client
        .get(config.jwks_uri)
        .send()
        .await?
        .error_for_status()?
        .json::<JwkSet>()
        .await?;

    Ok(jwks)
}

async fn get_jwks(state: &ChatGptCodexAuthState) -> Result<JwkSet> {
    let mut cache = state.jwks_cache.lock().await;
    if let Some(jwks) = cache.clone() {
        return Ok(jwks);
    }
    let jwks = fetch_jwks_for(ISSUER).await?;
    *cache = Some(jwks.clone());
    Ok(jwks)
}

fn parse_jwt_claims_with_jwks(token: &str, jwks: &JwkSet) -> Result<JwtClaims> {
    let header = decode_header(token)?;
    let kid = header
        .kid
        .ok_or_else(|| anyhow!("JWT header missing kid"))?;
    let jwk = jwks
        .find(&kid)
        .ok_or_else(|| anyhow!("JWT signing key not found"))?;
    let decoding_key = DecodingKey::from_jwk(jwk)?;

    let mut validation = Validation::new(header.alg);
    validation.validate_aud = false;

    let token_data = decode::<JwtClaims>(token, &decoding_key, &validation)?;
    Ok(token_data.claims)
}

fn parse_jwt_claims_unverified(token: &str) -> Option<JwtClaims> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .ok()?;
    serde_json::from_slice(&payload).ok()
}

async fn parse_jwt_claims(token: &str, state: &ChatGptCodexAuthState) -> Option<JwtClaims> {
    if let Ok(jwks) = get_jwks(state).await {
        if let Ok(claims) = parse_jwt_claims_with_jwks(token, &jwks) {
            return Some(claims);
        }
    }
    parse_jwt_claims_unverified(token)
}

fn account_id_from_claims(claims: &JwtClaims) -> Option<String> {
    if let Some(id) = claims.chatgpt_account_id.as_ref() {
        return Some(id.clone());
    }
    if let Some(auth) = claims.auth_claims.as_ref() {
        if let Some(id) = auth.chatgpt_account_id.as_ref() {
            return Some(id.clone());
        }
    }
    if let Some(orgs) = claims.organizations.as_ref() {
        if let Some(org) = orgs.first() {
            return Some(org.id.clone());
        }
    }
    None
}

async fn extract_account_id(
    token_data: &TokenData,
    state: &ChatGptCodexAuthState,
) -> Option<String> {
    if let Some(id_token) = token_data.id_token.as_deref() {
        if let Some(claims) = parse_jwt_claims(id_token, state).await {
            if let Some(account_id) = account_id_from_claims(&claims) {
                return Some(account_id);
            }
        }
    }

    parse_jwt_claims(&token_data.access_token, state)
        .await
        .and_then(|claims| account_id_from_claims(&claims))
}

struct PkceChallenge {
    verifier: String,
    challenge: String,
}

fn generate_pkce() -> PkceChallenge {
    let verifier = nanoid::nanoid!(43);
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

fn build_authorize_url(redirect_uri: &str, pkce: &PkceChallenge, state: &str) -> Result<String> {
    let scopes = OAUTH_SCOPES.join(" ");
    let params = [
        ("response_type", "code"),
        ("client_id", CLIENT_ID),
        ("redirect_uri", redirect_uri),
        ("scope", &scopes),
        ("code_challenge", &pkce.challenge),
        ("code_challenge_method", "S256"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("state", state),
        ("originator", "goose"),
    ];
    let query = serde_urlencoded::to_string(params)?;
    Ok(format!("{}/oauth/authorize?{}", ISSUER, query))
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    id_token: Option<String>,
    expires_in: Option<i64>,
}

async fn exchange_code_for_tokens_with_issuer(
    issuer: &str,
    code: &str,
    redirect_uri: &str,
    pkce: &PkceChallenge,
) -> Result<TokenResponse> {
    let client = reqwest::Client::new();
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", CLIENT_ID),
        ("code_verifier", &pkce.verifier),
    ];

    let resp = client
        .post(format!("{}/oauth/token", issuer))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Token exchange failed ({}): {}", status, text));
    }

    Ok(resp.json().await?)
}

async fn refresh_access_token_with_issuer(
    issuer: &str,
    refresh_token: &str,
) -> Result<TokenResponse> {
    let client = reqwest::Client::new();
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", CLIENT_ID),
    ];

    let resp = client
        .post(format!("{}/oauth/token", issuer))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("Token refresh failed ({}): {}", status, text));
    }

    Ok(resp.json().await?)
}

const HTML_SUCCESS_TEMPLATE: &str = r#"<!doctype html>
<html>
  <head>
    <title>goose - ChatGPT Authorization Successful</title>
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
    let safe_error = v_htmlescape::escape(error).to_string();
    format!(
        r#"<!doctype html>
<html>
  <head>
    <title>goose - ChatGPT Authorization Failed</title>
    <style>
      body {{
        font-family: system-ui, -apple-system, sans-serif;
        display: flex;
        justify-content: center;
        align-items: center;
        height: 100vh;
        margin: 0;
        background: #131010;
        color: #f1ecec;
      }}
      .container {{ text-align: center; padding: 2rem; }}
      h1 {{ color: #fc533a; margin-bottom: 1rem; }}
      p {{ color: #b7b1b1; }}
      .error {{
        color: #ff917b;
        font-family: monospace;
        margin-top: 1rem;
        padding: 1rem;
        background: #3c140d;
        border-radius: 0.5rem;
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
        "/auth/callback",
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
    let addr = SocketAddr::from(([127, 0, 0, 1], OAUTH_PORT));
    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        if e.kind() == io::ErrorKind::AddrInUse {
            anyhow!(
                "OAuth callback server failed to bind to {}: port {} is already in use. \
                 Please stop the process using this port and try again.",
                addr,
                OAUTH_PORT
            )
        } else {
            anyhow!("OAuth callback server failed to bind to {}: {}", addr, e)
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
        .map_err(|_| anyhow!("OAuth flow timed out"))??
        .map_err(|e| anyhow!("OAuth callback error: {}", e))
}

async fn perform_oauth_flow(auth_state: &ChatGptCodexAuthState) -> Result<TokenData> {
    let _guard = auth_state.oauth_mutex.try_lock().map_err(|_| {
        anyhow!("Another OAuth flow is already in progress; please try again later")
    })?;

    let pkce = generate_pkce();
    let csrf_state = generate_state();
    let redirect_uri = format!("http://localhost:{}/auth/callback", OAUTH_PORT);
    let auth_url = build_authorize_url(&redirect_uri, &pkce, &csrf_state)?;

    let (tx, rx) = oneshot::channel::<Result<String>>();
    let tx = Arc::new(TokioMutex::new(Some(tx)));
    let app = oauth_callback_router(csrf_state.clone(), tx);
    let server_handle = spawn_oauth_server(app).await?;
    let mut server_guard = ServerHandleGuard::new(server_handle);

    if webbrowser::open(&auth_url).is_err() {
        tracing::info!("Please open this URL in your browser:\n{}", auth_url);
    }

    let code_result = wait_for_oauth_code(rx).await;
    server_guard.abort();
    let code = code_result?;

    let tokens = exchange_code_for_tokens_with_issuer(ISSUER, &code, &redirect_uri, &pkce).await?;

    let expires_at = Utc::now() + chrono::Duration::seconds(tokens.expires_in.unwrap_or(3600));

    let mut token_data = TokenData {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        id_token: tokens.id_token,
        expires_at,
        account_id: None,
    };

    token_data.account_id = extract_account_id(&token_data, auth_state).await;

    Ok(token_data)
}

#[derive(Debug)]
struct ChatGptCodexAuthProvider {
    cache: TokenCache,
    state: Arc<ChatGptCodexAuthState>,
}

impl ChatGptCodexAuthProvider {
    fn new(state: Arc<ChatGptCodexAuthState>) -> Self {
        Self {
            cache: TokenCache::new(),
            state,
        }
    }

    async fn get_valid_token(&self) -> Result<TokenData> {
        if let Some(mut token_data) = self.cache.load() {
            if token_data.expires_at > Utc::now() + chrono::Duration::seconds(60) {
                return Ok(token_data);
            }

            tracing::debug!("Token expired, attempting refresh");
            match refresh_access_token_with_issuer(ISSUER, &token_data.refresh_token).await {
                Ok(new_tokens) => {
                    token_data.access_token = new_tokens.access_token;
                    token_data.refresh_token = new_tokens.refresh_token;
                    if new_tokens.id_token.is_some() {
                        token_data.id_token = new_tokens.id_token;
                    }
                    token_data.expires_at = Utc::now()
                        + chrono::Duration::seconds(new_tokens.expires_in.unwrap_or(3600));
                    if token_data.account_id.is_none() {
                        token_data.account_id =
                            extract_account_id(&token_data, self.state.as_ref()).await;
                    }
                    self.cache.save(&token_data)?;
                    tracing::info!("Token refreshed successfully");
                    return Ok(token_data);
                }
                Err(e) => {
                    tracing::warn!("Token refresh failed, will re-authenticate: {}", e);
                    self.cache.clear();
                }
            }
        }

        tracing::info!("Starting OAuth flow for ChatGPT Codex");
        let token_data = perform_oauth_flow(self.state.as_ref()).await?;
        self.cache.save(&token_data)?;
        Ok(token_data)
    }
}

#[async_trait]
impl AuthProvider for ChatGptCodexAuthProvider {
    async fn get_auth_header(&self) -> Result<(String, String)> {
        let token_data = self.get_valid_token().await?;
        Ok((
            "Authorization".to_string(),
            format!("Bearer {}", token_data.access_token),
        ))
    }
}

#[derive(Debug, serde::Serialize)]
pub struct ChatGptCodexProvider {
    #[serde(skip)]
    auth_provider: Arc<ChatGptCodexAuthProvider>,
    model: ModelConfig,
    #[serde(skip)]
    name: String,
}

impl ChatGptCodexProvider {
    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let auth_provider = Arc::new(ChatGptCodexAuthProvider::new(
            ChatGptCodexAuthState::instance(),
        ));

        Ok(Self {
            auth_provider,
            model,
            name: CHATGPT_CODEX_PROVIDER_NAME.to_string(),
        })
    }

    async fn post_streaming(
        &self,
        session_id: Option<&str>,
        payload: &Value,
    ) -> Result<reqwest::Response, ProviderError> {
        let token_data = self
            .auth_provider
            .get_valid_token()
            .await
            .map_err(|e| ProviderError::Authentication(e.to_string()))?;

        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(account_id) = &token_data.account_id {
            headers.insert(
                reqwest::header::HeaderName::from_static("chatgpt-account-id"),
                reqwest::header::HeaderValue::from_str(account_id)
                    .map_err(|e| ProviderError::ExecutionError(e.to_string()))?,
            );
        }

        if let Some(session_id) = session_id.filter(|id| !id.is_empty()) {
            headers.insert(
                HeaderName::from_static(SESSION_ID_HEADER),
                HeaderValue::from_str(session_id)
                    .map_err(|e| ProviderError::ExecutionError(e.to_string()))?,
            );
        }

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/responses", CODEX_API_ENDPOINT))
            .header(
                "Authorization",
                format!("Bearer {}", token_data.access_token),
            )
            .header("Content-Type", "application/json")
            .headers(headers)
            .json(payload)
            .send()
            .await
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

        handle_status_openai_compat(response).await
    }
}

impl ProviderDef for ChatGptCodexProvider {
    type Provider = Self;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            CHATGPT_CODEX_PROVIDER_NAME,
            "ChatGPT Codex",
            "Use your ChatGPT Plus/Pro subscription for GPT-5 Codex models via OAuth",
            CHATGPT_CODEX_DEFAULT_MODEL,
            known_model_names(),
            CHATGPT_CODEX_DOC_URL,
            vec![ConfigKey::new_oauth(
                "CHATGPT_CODEX_TOKEN",
                true,
                true,
                None,
                false,
            )],
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
impl Provider for ChatGptCodexProvider {
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
        let mut payload = create_codex_request(model_config, system, messages, tools)
            .map_err(|e| ProviderError::ExecutionError(e.to_string()))?;
        payload["stream"] = serde_json::Value::Bool(true);

        let response = self
            .with_retry(|| async {
                let payload_clone = payload.clone();
                self.post_streaming(Some(session_id), &payload_clone).await
            })
            .await?;

        let stream = response.bytes_stream().map_err(io::Error::other);

        Ok(Box::pin(try_stream! {
            let stream_reader = StreamReader::new(stream);
            let framed = FramedRead::new(stream_reader, LinesCodec::new()).map_err(anyhow::Error::from);

            let message_stream = responses_api_to_streaming_message(framed);
            pin!(message_stream);
            while let Some(message) = message_stream.next().await {
                let (message, usage) = message.map_err(|e| ProviderError::RequestFailed(format!("Stream decode error: {}", e)))?;
                yield (message, usage);
            }
        }))
    }

    async fn configure_oauth(&self) -> Result<(), ProviderError> {
        self.auth_provider
            .get_valid_token()
            .await
            .map_err(|e| ProviderError::Authentication(format!("OAuth flow failed: {}", e)))?;
        Ok(())
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(known_model_names().into_iter().map(String::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::Message;
    use goose_test_support::TEST_IMAGE_B64;
    use jsonwebtoken::{Algorithm, EncodingKey, Header};
    use rmcp::model::{CallToolRequestParams, CallToolResult, Content, ErrorCode, ErrorData};
    use rmcp::object;
    use test_case::test_case;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn input_kinds(payload: &Value) -> Vec<String> {
        payload["input"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|item| {
                        if let Some(role) = item.get("role").and_then(|r| r.as_str()) {
                            format!("message:{role}")
                        } else {
                            item.get("type")
                                .and_then(|t| t.as_str())
                                .unwrap_or("unknown")
                                .to_string()
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test_case(
        vec![
            Message::user().with_text("user text"),
            Message::assistant().with_text("assistant prelude").with_tool_request(
                "call-1",
                Ok(CallToolRequestParams::new("tool_name").with_arguments(object!({"param": "value"}))),
            ),
            Message::user().with_tool_response(
                "call-1",
                Ok(CallToolResult::success(vec![Content::text("tool output")])),
            ),
            Message::assistant().with_text("assistant follow-up"),
        ],
        vec![
            "message:user".to_string(),
            "message:assistant".to_string(),
            "function_call".to_string(),
            "function_call_output".to_string(),
            "message:assistant".to_string(),
        ];
        "preserves order when assistant includes text"
    )]
    #[test_case(
        vec![
            Message::user().with_text("user text"),
            Message::assistant().with_tool_request(
                "call-1",
                Ok(CallToolRequestParams::new("tool_name").with_arguments(object!({"param": "value"}))),
            ),
            Message::user().with_tool_response(
                "call-1",
                Ok(CallToolResult::success(vec![Content::text("tool output")])),
            ),
            Message::assistant().with_text("assistant follow-up"),
        ],
        vec![
            "message:user".to_string(),
            "function_call".to_string(),
            "function_call_output".to_string(),
            "message:assistant".to_string(),
        ];
        "skips empty assistant message and preserves tool order"
    )]
    #[test_case(
        vec![
            Message::user().with_text("user text"),
            Message::assistant().with_tool_request(
                "call-1",
                Ok(CallToolRequestParams::new("tool_name").with_arguments(object!({"param": "value"}))),
            ),
            Message::user().with_tool_response(
                "call-1",
                Err(ErrorData::new(ErrorCode::INTERNAL_ERROR, "boom", None)),
            ),
        ],
        vec![
            "message:user".to_string(),
            "function_call".to_string(),
            "function_call_output".to_string(),
        ];
        "includes tool error output"
    )]
    #[test_case(
        vec![
            Message::user()
                .with_text("describe this")
                .with_image(TEST_IMAGE_B64, "image/png"),
        ],
        vec![
            "message:user".to_string(),
        ];
        "image content included in user message"
    )]
    fn test_codex_input_order(messages: Vec<Message>, expected: Vec<String>) {
        let items = build_input_items(&messages).unwrap();
        let payload = json!({ "input": items });
        let kinds = input_kinds(&payload);
        assert_eq!(kinds, expected);
    }

    #[test]
    fn test_image_url_format() {
        let messages = vec![Message::user().with_image(TEST_IMAGE_B64, "image/png")];
        let items = build_input_items(&messages).unwrap();
        // The image is inside the content array of the user message
        let content = items[0]["content"].as_array().unwrap();
        let image_item = &content[0];
        assert_eq!(image_item["type"], "input_image");
        let url = image_item["image_url"].as_str().unwrap();
        assert!(
            url.starts_with("data:image/png;base64,"),
            "image_url should start with data:image/png;base64, but was: {}",
            url
        );
    }

    #[test_case(
        JwtClaims {
            chatgpt_account_id: Some("account-1".to_string()),
            auth_claims: None,
            organizations: None,
        },
        Some("account-1".to_string());
        "uses top-level account id"
    )]
    #[test_case(
        JwtClaims {
            chatgpt_account_id: None,
            auth_claims: Some(AuthClaims {
                chatgpt_account_id: Some("account-2".to_string()),
            }),
            organizations: None,
        },
        Some("account-2".to_string());
        "uses auth claims account id"
    )]
    #[test_case(
        JwtClaims {
            chatgpt_account_id: None,
            auth_claims: None,
            organizations: Some(vec![OrgInfo {
                id: "org-1".to_string(),
            }]),
        },
        Some("org-1".to_string());
        "falls back to first organization"
    )]
    fn test_account_id_from_claims(claims: JwtClaims, expected: Option<String>) {
        assert_eq!(account_id_from_claims(&claims), expected);
    }

    #[tokio::test]
    async fn test_exchange_code_for_tokens() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .and(body_string_contains("grant_type=authorization_code"))
            .and(body_string_contains("code=code-123"))
            .and(body_string_contains(
                "redirect_uri=http%3A%2F%2Flocalhost%2Fcallback",
            ))
            .and(body_string_contains("code_verifier=verifier-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "access-1",
                "refresh_token": "refresh-1",
                "id_token": "id-1",
                "expires_in": 3600
            })))
            .mount(&server)
            .await;

        let pkce = PkceChallenge {
            verifier: "verifier-123".to_string(),
            challenge: "challenge-123".to_string(),
        };
        let tokens = exchange_code_for_tokens_with_issuer(
            &server.uri(),
            "code-123",
            "http://localhost/callback",
            &pkce,
        )
        .await
        .unwrap();

        assert_eq!(tokens.access_token, "access-1");
        assert_eq!(tokens.refresh_token, "refresh-1");
        assert_eq!(tokens.id_token.as_deref(), Some("id-1"));
        assert_eq!(tokens.expires_in, Some(3600));
    }

    #[tokio::test]
    async fn test_refresh_access_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains("refresh_token=refresh-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "access-2",
                "refresh_token": "refresh-2",
                "id_token": "id-2",
                "expires_in": 1800
            })))
            .mount(&server)
            .await;

        let tokens = refresh_access_token_with_issuer(&server.uri(), "refresh-123")
            .await
            .unwrap();

        assert_eq!(tokens.access_token, "access-2");
        assert_eq!(tokens.refresh_token, "refresh-2");
        assert_eq!(tokens.id_token.as_deref(), Some("id-2"));
        assert_eq!(tokens.expires_in, Some(1800));
    }

    #[derive(Serialize)]
    struct TestClaims {
        exp: usize,
        chatgpt_account_id: Option<String>,
    }

    #[tokio::test]
    async fn test_parse_jwt_claims_verified_with_issuer() {
        let server = MockServer::start().await;
        let jwks_uri = format!("{}/jwks", server.uri());
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jwks_uri": jwks_uri
            })))
            .mount(&server)
            .await;

        let secret = "test-secret";
        let key = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(secret);
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "keys": [{
                    "kty": "oct",
                    "alg": "HS256",
                    "kid": "test-kid",
                    "k": key
                }]
            })))
            .mount(&server)
            .await;

        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("test-kid".to_string());

        let claims = TestClaims {
            exp: (Utc::now() + chrono::Duration::seconds(60)).timestamp() as usize,
            chatgpt_account_id: Some("account-1".to_string()),
        };
        let token = jsonwebtoken::encode(
            &header,
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();

        let jwks = fetch_jwks_for(&server.uri()).await.unwrap();
        let claims = parse_jwt_claims_with_jwks(&token, &jwks).unwrap();

        assert_eq!(claims.chatgpt_account_id.as_deref(), Some("account-1"));
    }

    #[test_case("unknown-model", &["medium", "high"]; "unknown model gets default reasoning levels")]
    fn test_reasoning_levels_for_model(model: &str, expected: &[&str]) {
        assert_eq!(reasoning_levels_for_model(model), expected);
    }

    #[test]
    fn test_gpt53_preamble_injected() {
        let model = ModelConfig::new("gpt-5.3-codex").unwrap();
        let payload = create_codex_request(&model, "system prompt", &[], &[]).unwrap();
        let instructions = payload["instructions"].as_str().unwrap();
        assert!(instructions.contains(GPT_53_CODEX_TOOL_PREAMBLE));
        assert!(instructions.contains("system prompt"));
    }

    #[test]
    fn test_other_models_no_preamble() {
        let model = ModelConfig::new("gpt-5.4").unwrap();
        let payload = create_codex_request(&model, "system prompt", &[], &[]).unwrap();
        let instructions = payload["instructions"].as_str().unwrap();
        assert_eq!(instructions, "system prompt");
    }
}
