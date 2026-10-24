use crate::config::paths::Paths;
use crate::conversation::message::{Message, MessageContent};
use crate::providers::api_client::{AuthProvider, RequestBuilderDecorator};
use crate::providers::base::{
    ConfigKey, MessageStream, Provider, ProviderDef, ProviderMetadata,
    DEFAULT_CONNECT_TIMEOUT_SECS, DEFAULT_PROVIDER_TIMEOUT_SECS,
};
use crate::providers::openai_compatible::handle_status;
use crate::providers::private_file::write_private_file;
use crate::providers::retry::ProviderRetry;
use anyhow::{anyhow, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use axum::{extract::Query, response::Html, routing::get, Router};
use base64::Engine;
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use futures::{StreamExt, TryStreamExt};
use goose_providers::errors::ProviderError;
use goose_providers::formats::openai_responses::responses_api_to_streaming_message;
use goose_providers::model::ModelConfig;
use goose_providers::thinking::{
    ThinkingEffortCapability, ThinkingEffortOption, ThinkingEffortSupport,
};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, DecodingKey, Validation};
use rmcp::model::{ContentBlock, Role, Tool};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Digest;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use tokio::pin;
use tokio::sync::{oneshot, watch, Mutex as TokioMutex};
use tokio_util::codec::{FramedRead, LinesCodec};
use tokio_util::io::StreamReader;

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const ISSUER: &str = "https://auth.openai.com";
const CODEX_API_ENDPOINT: &str = "https://chatgpt.com/backend-api/codex";
const CODEX_MODELS_ENDPOINT: &str = "https://chatgpt.com/backend-api/codex/models";
const OAUTH_SCOPES: &[&str] = &["openid", "profile", "email", "offline_access"];
// Canonical localhost callback port for Codex OAuth (default localhost:1455 per OpenAI docs).
// https://developers.openai.com/codex/auth/
const OAUTH_PORT: u16 = 1455;
// Allow time for users to complete the browser-based OAuth flow.
const OAUTH_TIMEOUT_SECS: u64 = 300;
const HTML_AUTO_CLOSE_TIMEOUT_MS: u64 = 2000;

const CHATGPT_CODEX_PROVIDER_NAME: &str = "chatgpt_codex";
pub const CHATGPT_CODEX_DEFAULT_MODEL: &str = "gpt-5.6-sol";

pub(crate) fn default_model() -> Option<String> {
    match TokenCache::new().catalog_snapshot() {
        Some(snapshot) => snapshot.models.first().map(|model| model.slug.clone()),
        None => Some(CHATGPT_CODEX_DEFAULT_MODEL.to_string()),
    }
}

/// Inventory identity used when an OAuth response carries no account id. It is
/// deliberately not derived from the refresh token: that token rotates on every
/// refresh, so keying the inventory on it would move the snapshot out from
/// under the refresh job that just fetched it. Accounts we cannot tell apart
/// share one key, which is all the information the credentials carry.
pub(crate) const UNKNOWN_ACCOUNT_IDENTITY: &str = "unknown-account";

#[derive(Debug)]
pub struct ChatGptCodexModelAttrs {
    pub name: &'static str,
    pub reasoning_levels: &'static [&'static str],
}

pub const CHATGPT_CODEX_KNOWN_MODELS: &[ChatGptCodexModelAttrs] = &[
    ChatGptCodexModelAttrs {
        name: "gpt-5.6-sol",
        reasoning_levels: &["none", "low", "medium", "high", "xhigh"],
    },
    ChatGptCodexModelAttrs {
        name: "gpt-5.6-terra",
        reasoning_levels: &["none", "low", "medium", "high", "xhigh"],
    },
    ChatGptCodexModelAttrs {
        name: "gpt-5.6-luna",
        reasoning_levels: &["none", "low", "medium", "high", "xhigh"],
    },
    ChatGptCodexModelAttrs {
        name: "gpt-5.6",
        reasoning_levels: &["none", "low", "medium", "high", "xhigh"],
    },
    ChatGptCodexModelAttrs {
        name: "gpt-5.5",
        reasoning_levels: &["low", "medium", "high", "xhigh"],
    },
    ChatGptCodexModelAttrs {
        name: "gpt-5.4",
        reasoning_levels: &["low", "medium", "high", "xhigh"],
    },
];

const CHATGPT_CODEX_DOC_URL: &str = "https://openai.com/chatgpt";

const DEFAULT_REASONING_LEVELS: &[&str] = &["medium", "high"];
pub(crate) const CODEX_REASONING_OPTION_ID: &str = "codex_reasoning_effort";

static CODEX_CATALOG_UPDATES: LazyLock<watch::Sender<ThinkingEffortSupport>> =
    LazyLock::new(|| watch::channel(ThinkingEffortSupport::Unspecified).0);
static CODEX_CATALOG_POLLING: AtomicBool = AtomicBool::new(false);

struct CatalogPollGuard;

impl Drop for CatalogPollGuard {
    fn drop(&mut self) {
        CODEX_CATALOG_POLLING.store(false, Ordering::Release);
    }
}

fn catalog_file_state() -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let cache = TokenCache::new();
    (
        std::fs::read(&cache.cache_path).ok(),
        std::fs::read(cache.catalog_path()).ok(),
    )
}

fn start_catalog_polling() {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    if CODEX_CATALOG_POLLING.swap(true, Ordering::AcqRel) {
        return;
    }
    let mut updates = CODEX_CATALOG_UPDATES.subscribe();
    let mut last_state = catalog_file_state();
    handle.spawn(async move {
        let _guard = CatalogPollGuard;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        interval.tick().await;
        loop {
            tokio::select! {
                changed = updates.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    last_state = catalog_file_state();
                }
                _ = interval.tick() => {
                    if CODEX_CATALOG_UPDATES.receiver_count() <= 1 {
                        continue;
                    }
                    let state = catalog_file_state();
                    if state != last_state {
                        last_state = state;
                        notify_catalog_inventory_update();
                    }
                }
            }
        }
    });
}

pub(crate) fn notify_catalog_inventory_update() {
    CODEX_CATALOG_UPDATES.send_replace(ThinkingEffortSupport::Unspecified);
}

pub fn reasoning_levels_for_model(model_name: &str) -> Vec<String> {
    if let Some(levels) = TokenCache::new().catalog_snapshot().and_then(|snapshot| {
        snapshot
            .models
            .into_iter()
            .find(|model| model.slug == model_name)
            .map(|model| {
                model
                    .supported_reasoning_levels
                    .into_iter()
                    .map(|level| level.effort)
                    .collect()
            })
    }) {
        return levels;
    }
    CHATGPT_CODEX_KNOWN_MODELS
        .iter()
        .find(|m| m.name == model_name)
        .map(|m| m.reasoning_levels)
        .unwrap_or(DEFAULT_REASONING_LEVELS)
        .iter()
        .map(|level| (*level).to_string())
        .collect()
}

pub(crate) fn catalog_model_has_reasoning(model_name: &str) -> Option<bool> {
    TokenCache::new().catalog_snapshot().and_then(|snapshot| {
        snapshot
            .models
            .into_iter()
            .find(|model| model.slug == model_name)
            .map(|model| !model.supported_reasoning_levels.is_empty())
    })
}

fn known_model_names() -> Vec<&'static str> {
    CHATGPT_CODEX_KNOWN_MODELS.iter().map(|m| m.name).collect()
}

/// The Codex catalog endpoint requires a semver `client_version` and withholds
/// entries whose minimum client version the caller does not satisfy. goose
/// reports its own version, which is ahead of the Codex CLI release line, so the
/// account's whole catalog is listed rather than a snapshot of it.
fn codex_client_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CodexCatalogEntry {
    slug: String,
    #[serde(default)]
    visibility: Option<String>,
    /// An entry that omits this flag never asserted API support, so it is
    /// treated as unsupported instead of being advertised and failing later at
    /// request time.
    #[serde(default)]
    supported_in_api: bool,
    #[serde(default)]
    priority: Option<i64>,
    #[serde(default)]
    supported_reasoning_levels: Vec<CodexReasoningLevel>,
    #[serde(default)]
    default_reasoning_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CodexReasoningLevel {
    effort: String,
}

/// `models` is required. A successful response that omits it - an error
/// envelope or a schema change - must fail to deserialize so the refresh job
/// keeps the last good inventory rather than storing an empty one.
#[derive(Debug, Deserialize)]
struct CodexCatalog {
    models: Vec<CodexCatalogEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CodexCatalogSnapshot {
    account_id: Option<String>,
    models: Vec<CodexCatalogEntry>,
}

/// Entries marked `hide` are Codex-internal (legacy or review-only models) and
/// are not offered to users, so they are left out of the selectable list.
fn selectable_models(catalog: CodexCatalog) -> Vec<CodexCatalogEntry> {
    let mut entries: Vec<CodexCatalogEntry> = catalog
        .models
        .into_iter()
        .filter(|model| model.visibility.as_deref() != Some("hide"))
        .filter(|model| model.supported_in_api)
        .collect();
    entries.sort_by_key(|model| (model.priority.unwrap_or(i64::MAX), model.slug.clone()));
    entries
}

#[cfg(test)]
fn selectable_model_names(catalog: CodexCatalog) -> Vec<String> {
    selectable_models(catalog)
        .into_iter()
        .map(|model| model.slug)
        .collect()
}

#[derive(Debug)]
struct ChatGptCodexAuthState {
    oauth_mutex: TokioMutex<()>,
    token_mutex: TokioMutex<u64>,
    jwks_cache: TokioMutex<Option<JwkSet>>,
}

impl ChatGptCodexAuthState {
    fn new() -> Self {
        Self {
            oauth_mutex: TokioMutex::new(()),
            token_mutex: TokioMutex::new(0),
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
                                    if let ContentBlock::Text(t) = c {
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
    let valid_levels = reasoning_levels_for_model(model_name);
    let catalog_default = TokenCache::new()
        .catalog_snapshot()
        .and_then(|snapshot| {
            snapshot
                .models
                .into_iter()
                .find(|model| model.slug == model_name)
                .and_then(|model| model.default_reasoning_level)
        })
        .filter(|level| valid_levels.contains(level));
    let effort = config
        .get_chatgpt_codex_reasoning_effort()
        .map(String::from)
        .unwrap_or_else(|_| {
            catalog_default
                .clone()
                .unwrap_or_else(|| "medium".to_string())
        });
    if valid_levels.iter().any(|level| level == &effort) {
        effort
    } else {
        let fallback = catalog_default
            .or_else(|| {
                valid_levels
                    .iter()
                    .find(|level| *level == "medium")
                    .cloned()
            })
            .or_else(|| valid_levels.first().cloned())
            .unwrap_or_else(|| "medium".to_string());
        tracing::warn!(
            "Invalid CHATGPT_CODEX_REASONING_EFFORT '{}' for model '{}', using '{}'",
            effort,
            model_name,
            fallback
        );
        fallback
    }
}

fn reasoning_effort_for_config(model_config: &ModelConfig) -> Option<String> {
    use goose_providers::thinking::ThinkingEffort;

    if reasoning_levels_for_model(&model_config.model_name).is_empty() {
        return None;
    }

    if let Some(raw_effort) = model_config.request_param::<String>("thinking_effort") {
        if reasoning_levels_for_model(&model_config.model_name).contains(&raw_effort) {
            return Some(raw_effort);
        }
    }

    model_config
        .thinking_effort()
        .and_then(|effort| {
            let valid_levels = reasoning_levels_for_model(&model_config.model_name);
            let preferred_levels: &[&str] = match effort {
                ThinkingEffort::Off => {
                    return Some(
                        valid_levels
                            .iter()
                            .find(|level| *level == "none")
                            .or_else(|| valid_levels.first())
                            .cloned()
                            .unwrap_or_else(|| "medium".to_string()),
                    );
                }
                ThinkingEffort::Low => &["low", "medium", "high", "xhigh"],
                ThinkingEffort::Medium => &["medium", "high", "low", "xhigh"],
                ThinkingEffort::High => &["high", "medium", "xhigh", "low"],
                ThinkingEffort::Max => &["max", "xhigh", "high", "medium", "low"],
            };

            preferred_levels
                .iter()
                .find(|level| valid_levels.iter().any(|valid| valid.as_str() == **level))
                .map(|level| (*level).to_string())
        })
        .or_else(|| Some(get_reasoning_effort(&model_config.model_name)))
}

fn create_codex_request(
    model_config: &ModelConfig,
    system: &str,
    messages: &[Message],
    tools: &[Tool],
) -> Result<Value> {
    let input_items = build_input_items(messages)?;
    let reasoning_effort = reasoning_effort_for_config(model_config);

    let mut payload = json!({
        "model": model_config.model_name,
        "input": input_items,
        "store": false,
        "instructions": system,
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

    if let Some(reasoning_effort) = reasoning_effort {
        payload_obj.insert(
            "reasoning".to_string(),
            json!({ "effort": reasoning_effort }),
        );
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
pub(crate) struct TokenCache {
    cache_path: PathBuf,
}

fn get_cache_path() -> PathBuf {
    Paths::in_config_dir("chatgpt_codex/tokens.json")
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
        if let Ok(contents) = std::fs::read_to_string(&self.cache_path) {
            serde_json::from_str(&contents).ok()
        } else {
            None
        }
    }
    pub(crate) fn has_token(&self) -> bool {
        self.load().is_some()
    }

    pub(crate) fn inventory_identity(&self) -> Option<String> {
        self.load().map(|token| {
            token
                .account_id
                .unwrap_or_else(|| UNKNOWN_ACCOUNT_IDENTITY.to_string())
        })
    }

    fn save(&self, token_data: &TokenData) -> Result<()> {
        let contents = serde_json::to_string(token_data)?;
        write_private_file(&self.cache_path, &contents)?;
        Ok(())
    }

    fn clear(&self) {
        let _ = std::fs::remove_file(&self.cache_path);
        let _ = std::fs::remove_file(self.catalog_path());
    }

    fn bind_catalog_to_account(&self, account_id: &str) -> Result<()> {
        let Ok(contents) = std::fs::read_to_string(self.catalog_path()) else {
            return Ok(());
        };
        let Ok(mut snapshot) = serde_json::from_str::<CodexCatalogSnapshot>(&contents) else {
            return Ok(());
        };
        if snapshot.account_id.is_none() {
            snapshot.account_id = Some(account_id.to_string());
            write_private_file(&self.catalog_path(), &serde_json::to_string(&snapshot)?)?;
        }
        Ok(())
    }

    fn commit_oauth_token(&self, token: &TokenData) -> Result<()> {
        let previous_account = self.load().and_then(|previous| previous.account_id);
        self.save(token)?;
        if previous_account.is_none() || previous_account != token.account_id {
            let _ = std::fs::remove_file(self.catalog_path());
            notify_catalog_inventory_update();
        }
        Ok(())
    }

    fn catalog_path(&self) -> PathBuf {
        self.cache_path.with_extension("json.models")
    }

    fn catalog_snapshot(&self) -> Option<CodexCatalogSnapshot> {
        let account_id = self.load()?.account_id;
        let contents = std::fs::read_to_string(self.catalog_path()).ok()?;
        let snapshot: CodexCatalogSnapshot = serde_json::from_str(&contents).ok()?;
        (snapshot.account_id == account_id).then_some(snapshot)
    }

    pub(crate) fn catalog_matches_models(&self, models: &[String]) -> bool {
        self.catalog_snapshot().is_some_and(|snapshot| {
            snapshot.models.len() == models.len()
                && snapshot
                    .models
                    .iter()
                    .zip(models)
                    .all(|(model, name)| model.slug.as_str() == name.as_str())
        })
    }

    /// Deletion marker for the cache, in its own sidecar so `clear()` can remove
    /// the credentials without erasing the record that they were deleted. An
    /// OAuth flow releases the cache lock for the browser round trip, which can
    /// take minutes; without this, a delete in another process would be undone
    /// by a flow that started before it.
    fn generation_path(&self) -> PathBuf {
        self.cache_path.with_extension("json.generation")
    }

    fn generation(&self) -> u64 {
        std::fs::read_to_string(self.generation_path())
            .ok()
            .and_then(|contents| contents.trim().parse().ok())
            .unwrap_or(0)
    }

    fn bump_generation(&self) -> Result<()> {
        let next = self.generation().wrapping_add(1);
        write_private_file(&self.generation_path(), &next.to_string())?;
        Ok(())
    }

    /// Lock file for the cache. It is a sidecar, never the cache file itself,
    /// so the lock can be held without disturbing readers of the cache.
    fn lock_path(&self) -> PathBuf {
        self.cache_path.with_extension("json.lock")
    }

    /// Acquires the cross-process cache lock. The blocking `flock` runs on a
    /// blocking thread so waiting for another process never stalls the runtime.
    async fn lock_exclusive(&self) -> Result<TokenCacheLock> {
        use fs2::FileExt as _;

        let path = self.lock_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        let file = tokio::task::spawn_blocking(move || {
            file.lock_exclusive()?;
            Ok::<_, io::Error>(file)
        })
        .await??;
        Ok(TokenCacheLock { file })
    }
}

/// Held across a whole load/refresh/save of the token cache.
///
/// The in-process mutex does not cover the desktop app and a CLI session that
/// share this config directory: both can load the same refresh token and rotate
/// it concurrently, and the loser gets `invalid_grant` and may then clear the
/// credentials the winner just saved. An OS advisory lock covers every process,
/// and the kernel releases it if the holder dies.
#[derive(Debug)]
struct TokenCacheLock {
    file: std::fs::File,
}

impl Drop for TokenCacheLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
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
    let safe_error = v_htmlescape::escape_fmt(error);
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

async fn perform_oauth_flow<F, Fut, T>(
    auth_state: &ChatGptCodexAuthState,
    on_started: F,
) -> Result<(Result<TokenData>, T)>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
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
    let started = on_started().await?;

    let result = async {
        if webbrowser::open(&auth_url).is_err() {
            tracing::info!("Please open this URL in your browser:\n{}", auth_url);
        }

        let code_result = wait_for_oauth_code(rx).await;
        server_guard.abort();
        let code = code_result?;

        let tokens =
            exchange_code_for_tokens_with_issuer(ISSUER, &code, &redirect_uri, &pkce).await?;

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
    .await;
    Ok((result, started))
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

    async fn refresh_token(&self, token_data: &mut TokenData) -> Result<()> {
        let account_was_unknown = token_data.account_id.is_none();
        let new_tokens =
            refresh_access_token_with_issuer(ISSUER, &token_data.refresh_token).await?;
        token_data.access_token = new_tokens.access_token;
        token_data.refresh_token = new_tokens.refresh_token;
        if new_tokens.id_token.is_some() {
            token_data.id_token = new_tokens.id_token;
        }
        token_data.expires_at =
            Utc::now() + chrono::Duration::seconds(new_tokens.expires_in.unwrap_or(3600));
        if token_data.account_id.is_none() {
            token_data.account_id = extract_account_id(token_data, self.state.as_ref()).await;
        }
        if account_was_unknown {
            if let Some(account_id) = token_data.account_id.as_deref() {
                self.cache.bind_catalog_to_account(account_id)?;
            }
        }
        self.cache.save(token_data)?;
        Ok(())
    }

    /// Token for read-only model discovery: refreshes an expired token but never
    /// starts the interactive OAuth flow, so listing models cannot open a browser.
    async fn get_cached_token(&self) -> Result<TokenData, ProviderError> {
        let _guard = self.state.token_mutex.lock().await;
        let _cache_lock = self
            .cache
            .lock_exclusive()
            .await
            .map_err(|e| ProviderError::ExecutionError(e.to_string()))?;
        let Some(mut token_data) = self.cache.load() else {
            return Err(ProviderError::NotConfigured);
        };
        if token_data.expires_at <= Utc::now() + chrono::Duration::seconds(60) {
            self.refresh_token(&mut token_data)
                .await
                .map_err(|e| ProviderError::Authentication(e.to_string()))?;
        }
        Ok(token_data)
    }

    async fn get_valid_token(&self) -> Result<TokenData> {
        let guard = self.state.token_mutex.lock().await;
        {
            let _cache_lock = self.cache.lock_exclusive().await?;
            if let Some(mut token_data) = self.cache.load() {
                if token_data.expires_at > Utc::now() + chrono::Duration::seconds(60) {
                    return Ok(token_data);
                }

                tracing::debug!("Token expired, attempting refresh");
                match self.refresh_token(&mut token_data).await {
                    Ok(()) => {
                        tracing::info!("Token refreshed successfully");
                        return Ok(token_data);
                    }
                    Err(e) => {
                        tracing::warn!("Token refresh failed, will re-authenticate: {}", e);
                    }
                }
            }
        }

        let generation = *guard;
        drop(guard);
        tracing::info!("Starting OAuth flow for ChatGPT Codex");
        let (token_data, cache_generation) = perform_oauth_flow(self.state.as_ref(), || async {
            let _cache_lock = self.cache.lock_exclusive().await?;
            self.cache.bump_generation()?;
            Ok(self.cache.generation())
        })
        .await?;
        let token_data = token_data?;
        let guard = self.state.token_mutex.lock().await;
        if *guard != generation {
            return Err(anyhow!(
                "ChatGPT Codex credentials were removed during OAuth"
            ));
        }
        drop(guard);
        let _cache_lock = self.cache.lock_exclusive().await?;
        if self.cache.generation() != cache_generation {
            return Err(anyhow!(
                "ChatGPT Codex credentials were removed during OAuth"
            ));
        }
        self.cache.bump_generation()?;
        self.cache.commit_oauth_token(&token_data)?;
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

#[derive(serde::Serialize)]
pub struct ChatGptCodexProvider {
    #[serde(skip)]
    auth_provider: Arc<ChatGptCodexAuthProvider>,
    #[serde(skip)]
    name: String,
    #[serde(skip)]
    models_endpoint: String,
    #[serde(skip)]
    request_builder: RequestBuilderDecorator,
}

impl ChatGptCodexProvider {
    pub async fn cleanup() -> Result<()> {
        let state = ChatGptCodexAuthState::instance();
        let mut generation = state.token_mutex.lock().await;
        *generation = generation.wrapping_add(1);
        let cache = TokenCache::new();
        let _cache_lock = cache.lock_exclusive().await?;
        cache.bump_generation()?;
        cache.clear();
        Ok(())
    }

    pub async fn from_env(
        _tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> Result<Self> {
        let auth_provider = Arc::new(ChatGptCodexAuthProvider::new(
            ChatGptCodexAuthState::instance(),
        ));

        Ok(Self {
            auth_provider,
            name: CHATGPT_CODEX_PROVIDER_NAME.to_string(),
            models_endpoint: CODEX_MODELS_ENDPOINT.to_string(),
            request_builder: crate::session_context::session_id_request_builder(),
        })
    }

    async fn fetch_catalog(&self) -> Result<Vec<String>, ProviderError> {
        let token_data = self.auth_provider.get_cached_token().await?;

        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS))
            .timeout(std::time::Duration::from_secs(
                DEFAULT_PROVIDER_TIMEOUT_SECS,
            ))
            .build()
            .map_err(|e| ProviderError::ExecutionError(e.to_string()))?;

        let mut url = reqwest::Url::parse(&self.models_endpoint)
            .map_err(|e| ProviderError::ExecutionError(e.to_string()))?;
        url.query_pairs_mut()
            .append_pair("client_version", codex_client_version());

        let mut request = client.get(url).bearer_auth(&token_data.access_token);
        if let Some(account_id) = &token_data.account_id {
            request = request.header("chatgpt-account-id", account_id);
        }

        let response = request
            .send()
            .await
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;
        let response = handle_status(response).await?;
        let catalog: CodexCatalog =
            goose_providers::http_status::read_json_response(response).await?;
        let models = selectable_models(catalog);
        let cache = &self.auth_provider.cache;
        let _cache_lock = cache.lock_exclusive().await?;
        if !cache.load().is_some_and(|token| {
            token.account_id == token_data.account_id
                && token.access_token == token_data.access_token
                && token.refresh_token == token_data.refresh_token
        }) {
            return Err(ProviderError::Authentication(
                "ChatGPT Codex account changed during model discovery".to_string(),
            ));
        }
        write_private_file(
            &cache.catalog_path(),
            &serde_json::to_string(&CodexCatalogSnapshot {
                account_id: token_data.account_id,
                models: models.clone(),
            })
            .map_err(|error| ProviderError::ExecutionError(error.to_string()))?,
        )
        .map_err(|error| ProviderError::ExecutionError(error.to_string()))?;
        notify_catalog_inventory_update();
        Ok(models.into_iter().map(|model| model.slug).collect())
    }

    async fn post_streaming(&self, payload: &Value) -> Result<reqwest::Response, ProviderError> {
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

        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS))
            .read_timeout(std::time::Duration::from_secs(
                DEFAULT_PROVIDER_TIMEOUT_SECS,
            ))
            .build()
            .map_err(|e| ProviderError::ExecutionError(e.to_string()))?;
        let request = client
            .post(format!("{}/responses", CODEX_API_ENDPOINT))
            .header(
                "Authorization",
                format!("Bearer {}", token_data.access_token),
            )
            .header("Content-Type", "application/json")
            .headers(headers)
            .json(payload);

        let request = (self.request_builder)(request)
            .map_err(|e| ProviderError::ExecutionError(e.to_string()))?;
        let response = goose_providers::http_status::send_bounded(
            request,
            std::time::Duration::from_secs(DEFAULT_PROVIDER_TIMEOUT_SECS),
        )
        .await?;

        handle_status(response).await
    }
}

impl goose_providers::base::ProviderDescriptor for ChatGptCodexProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            CHATGPT_CODEX_PROVIDER_NAME,
            "ChatGPT Codex",
            "Use your ChatGPT Plus/Pro subscription for GPT-5 Codex models via OAuth",
            &default_model().unwrap_or_default(),
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
        .with_setup(
            crate::providers::catalog::ProviderSetupMetadata::new(
                crate::providers::catalog::ProviderSetupCategory::Model,
                crate::providers::catalog::ProviderSetupMethod::OauthDeviceCode,
                crate::providers::catalog::ProviderSetupGroup::Default,
            )
            .with_docs_url("https://chatgpt.com")
            .with_native_connect_query("ChatGPT Codex")
            .with_capabilities(false, true, false),
        )
    }
}

impl ProviderDef for ChatGptCodexProvider {
    type Provider = Self;

    fn from_env(
        _extensions: Vec<crate::config::ExtensionConfig>,
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(Self::from_env(tls_config))
    }
}

pub(crate) fn thinking_effort_support_for_model(
    provider: &dyn Provider,
    model_config: &ModelConfig,
) -> ThinkingEffortSupport {
    if provider.get_name() != CHATGPT_CODEX_PROVIDER_NAME {
        return provider.thinking_effort_support();
    }
    let levels = reasoning_levels_for_model(&model_config.model_name);
    if levels.is_empty() {
        return ThinkingEffortSupport::Unsupported;
    }
    let current = reasoning_effort_for_config(model_config);
    ThinkingEffortSupport::Options(ThinkingEffortCapability {
        option_id: CODEX_REASONING_OPTION_ID.to_string(),
        values: levels
            .into_iter()
            .map(|level| ThinkingEffortOption {
                value: level.clone(),
                label: level,
            })
            .collect(),
        current,
    })
}

pub(crate) async fn set_thinking_effort_for_model(
    provider: &dyn Provider,
    session_id: &str,
    model_config: &ModelConfig,
    value: &str,
) -> Result<bool, ProviderError> {
    if provider.get_name() != CHATGPT_CODEX_PROVIDER_NAME {
        return provider.set_thinking_effort(session_id, value).await;
    }
    let levels = reasoning_levels_for_model(&model_config.model_name);
    if (levels.is_empty() && value == "off") || levels.iter().any(|level| level == value) {
        Ok(true)
    } else {
        Err(ProviderError::InvalidValue(format!(
            "Invalid thinking effort '{value}' for model '{}'",
            model_config.model_name
        )))
    }
}

#[async_trait]
impl Provider for ChatGptCodexProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn subscribe_thinking_effort_support(&self) -> Option<watch::Receiver<ThinkingEffortSupport>> {
        let rx = CODEX_CATALOG_UPDATES.subscribe();
        start_catalog_polling();
        Some(rx)
    }

    /// The Codex catalog is already scoped to the account's API-supported
    /// models and carries the endpoint's own priority order; the canonical
    /// registry would re-sort recognized models by release date and lose it.
    fn skip_canonical_filtering(&self) -> bool {
        true
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
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
                self.post_streaming(&payload_clone).await
            })
            .await?;

        let stream = response.bytes_stream().map_err(io::Error::other);

        Ok(Box::pin(try_stream! {
            let stream_reader = StreamReader::new(stream);
            let framed = FramedRead::new(stream_reader, LinesCodec::new()).map_err(anyhow::Error::from);

            let message_stream = responses_api_to_streaming_message(framed);
            pin!(message_stream);
            while let Some(message) = message_stream.next().await {
                let (message, usage) = message.map_err(|e| {
                    e.downcast::<ProviderError>()
                        .unwrap_or_else(ProviderError::stream_decode_error)
                })?;
                yield (message, usage);
            }
        }))
    }

    async fn configure_oauth(&self) -> Result<(), ProviderError> {
        let (result, (generation, cache_generation)) =
            perform_oauth_flow(self.auth_provider.state.as_ref(), || async {
                let guard = self.auth_provider.state.token_mutex.lock().await;
                let _cache_lock = self.auth_provider.cache.lock_exclusive().await?;
                self.auth_provider.cache.bump_generation()?;
                let cache_generation = self.auth_provider.cache.generation();
                Ok((*guard, cache_generation))
            })
            .await?;
        let guard = self.auth_provider.state.token_mutex.lock().await;
        if *guard != generation {
            return Err(ProviderError::Authentication(
                "ChatGPT Codex credentials were removed during OAuth".to_string(),
            ));
        }
        drop(guard);
        let _cache_lock = self.auth_provider.cache.lock_exclusive().await?;
        if self.auth_provider.cache.generation() != cache_generation {
            return Err(ProviderError::Authentication(
                "ChatGPT Codex credentials were removed during OAuth".to_string(),
            ));
        }
        result
            .and_then(|token_data| {
                self.auth_provider.cache.bump_generation()?;
                self.auth_provider.cache.commit_oauth_token(&token_data)?;
                Ok(())
            })
            .map_err(|e| ProviderError::Authentication(format!("OAuth flow failed: {e}")))
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        self.fetch_catalog().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::Message;
    use goose_test_support::TEST_IMAGE_B64;
    use jsonwebtoken::{Algorithm, EncodingKey, Header};
    use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, ErrorCode, ErrorData};
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

    #[test]
    #[serial_test::serial]
    fn inventory_configured_uses_oauth_token_cache() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_string_lossy().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path.as_str()))]);

        TokenCache::new().clear();
        assert!(!TokenCache::new().has_token());

        TokenCache::new()
            .save(&TokenData {
                access_token: "access".to_string(),
                refresh_token: "refresh".to_string(),
                id_token: None,
                expires_at: Utc::now() + chrono::Duration::hours(1),
                account_id: Some("account".to_string()),
            })
            .unwrap();

        assert!(TokenCache::new().has_token());
    }

    #[test]
    #[serial_test::serial]
    fn inventory_identity_is_stable_across_token_refresh() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_string_lossy().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path.as_str()))]);
        let cache = TokenCache::new();
        let resolve =
            crate::providers::inventory::registrations::chatgpt_codex_inventory().identity;

        let save = |account_id: &str, refresh_token: &str| {
            cache
                .save(&TokenData {
                    access_token: "access".to_string(),
                    refresh_token: refresh_token.to_string(),
                    id_token: None,
                    expires_at: Utc::now() + chrono::Duration::hours(1),
                    account_id: Some(account_id.to_string()),
                })
                .unwrap();
        };

        save("account-a", "refresh-a");
        let account_a = resolve().unwrap().into_identity().unwrap();
        save("account-a", "rotated-refresh-a");
        assert_eq!(account_a, resolve().unwrap().into_identity().unwrap());
        save("account-b", "refresh-b");
        assert_ne!(account_a, resolve().unwrap().into_identity().unwrap());

        // With no account id the fallback must not move when the refresh token
        // rotates: the refresh job stores the catalog it fetched under the key
        // it planned with, and the next read resolves that same key.
        let mut token = cache.load().unwrap();
        token.account_id = None;
        cache.save(&token).unwrap();
        let unknown_account = resolve().unwrap().into_identity().unwrap();
        assert_ne!(account_a, unknown_account);
        token.refresh_token = "rotated-refresh-b".to_string();
        cache.save(&token).unwrap();
        assert_eq!(unknown_account, resolve().unwrap().into_identity().unwrap());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn token_cache_lock_serializes_independent_handles() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_string_lossy().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path.as_str()))]);

        // Two handles on one cache are two independent opens of the lock file,
        // exactly as two goose processes are, and flock makes them contend.
        let held = TokenCache::new().lock_exclusive().await.unwrap();
        let waiter = {
            let second = TokenCache::new();
            tokio::spawn(async move { second.lock_exclusive().await })
        };

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(
            !waiter.is_finished(),
            "lock was acquired while another handle still held it"
        );

        drop(held);
        let acquired = tokio::time::timeout(std::time::Duration::from_secs(5), waiter)
            .await
            .expect("second handle never acquired the released lock")
            .unwrap();
        assert!(acquired.is_ok());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn cleanup_advances_the_deletion_generation_for_other_handles() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_string_lossy().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path.as_str()))]);

        TokenCache::new()
            .save(&TokenData {
                access_token: "access".to_string(),
                refresh_token: "refresh".to_string(),
                id_token: None,
                expires_at: Utc::now() + chrono::Duration::hours(1),
                account_id: Some("account".to_string()),
            })
            .unwrap();

        // Separate handles are separate opens of the same files, exactly as two
        // goose processes are, so this is the desktop-deletes-while-the-cli-runs
        // case an in-process counter cannot see.
        let generation = TokenCache::new().generation();
        ChatGptCodexProvider::cleanup().await.unwrap();

        assert!(!TokenCache::new().has_token());
        assert_ne!(
            generation,
            TokenCache::new().generation(),
            "an OAuth flow that started before a deletion in another handle must be able to see it"
        );
    }

    #[test]
    #[serial_test::serial]
    fn empty_catalog_has_no_default_model() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_string_lossy().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path.as_str()))]);
        let cache = TokenCache::new();
        let token = TokenData {
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            id_token: None,
            expires_at: Utc::now() + chrono::Duration::hours(1),
            account_id: Some("account".to_string()),
        };
        cache.save(&token).unwrap();
        write_private_file(
            &cache.catalog_path(),
            &serde_json::to_string(&CodexCatalogSnapshot {
                account_id: token.account_id,
                models: vec![],
            })
            .unwrap(),
        )
        .unwrap();

        assert_eq!(default_model(), None);
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
                account_id: None,
            })
            .unwrap();

        let mode = std::fs::metadata(cache_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
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
                Ok(CallToolResult::success(vec![ContentBlock::text("tool output")])),
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
                Ok(CallToolResult::success(vec![ContentBlock::text("tool output")])),
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

    #[test]
    fn test_create_codex_request_reasoning_effort_from_unified_thinking() {
        let mut params = std::collections::HashMap::new();
        params.insert("thinking_effort".to_string(), json!("max"));
        let mut config = ModelConfig::new("gpt-5.5");
        config.request_params = Some(params);

        let payload = create_codex_request(&config, "sys", &[], &[]).unwrap();
        assert_eq!(payload["reasoning"]["effort"], "xhigh");
        assert!(payload.get("reasoning_effort").is_none());
    }

    #[test]
    fn test_create_codex_request_caps_unified_thinking_to_supported_level() {
        let mut params = std::collections::HashMap::new();
        params.insert("thinking_effort".to_string(), json!("max"));
        let mut config = ModelConfig::new("unknown-model");
        config.request_params = Some(params);

        let payload = create_codex_request(&config, "sys", &[], &[]).unwrap();
        assert_eq!(payload["reasoning"]["effort"], "high");
        assert!(payload.get("reasoning_effort").is_none());
    }

    #[test]
    fn test_create_codex_request_off_sets_none_for_gpt_5_6_models() {
        let mut params = std::collections::HashMap::new();
        params.insert("thinking_effort".to_string(), json!("off"));
        let mut config = ModelConfig::new("gpt-5.6-sol");
        config.request_params = Some(params);

        let payload = create_codex_request(&config, "sys", &[], &[]).unwrap();
        assert_eq!(payload["reasoning"]["effort"], "none");
        assert!(payload.get("reasoning_effort").is_none());
    }

    // ChatGPT Codex does not support temperature and will return an error
    #[test]
    fn test_create_codex_request_omits_temperature() {
        let config = ModelConfig::new("gpt-5.5").with_temperature(Some(0.2));

        let payload = create_codex_request(&config, "sys", &[], &[]).unwrap();
        assert!(payload.get("temperature").is_none());
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

    #[test_case("gpt-5.6-sol", &["none", "low", "medium", "high", "xhigh"]; "gpt 5.6 sol supports extended reasoning levels")]
    #[test_case("gpt-5.6-terra", &["none", "low", "medium", "high", "xhigh"]; "gpt 5.6 terra supports extended reasoning levels")]
    #[test_case("gpt-5.6-luna", &["none", "low", "medium", "high", "xhigh"]; "gpt 5.6 luna supports extended reasoning levels")]
    #[test_case("gpt-5.6", &["none", "low", "medium", "high", "xhigh"]; "gpt 5.6 supports extended reasoning levels")]
    #[test_case("unknown-model", &["medium", "high"]; "unknown model gets default reasoning levels")]
    #[serial_test::serial]
    fn test_reasoning_levels_for_model(model: &str, expected: &[&str]) {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_string_lossy().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path.as_str()))]);
        assert_eq!(
            reasoning_levels_for_model(model),
            expected
                .iter()
                .map(|level| level.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn catalog_sets_default_and_model_reasoning_levels() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_string_lossy().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path.as_str()))]);
        let cache = TokenCache::new();
        save_token(&cache.cache_path);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "models": [
                    { "slug": "hidden", "visibility": "hide", "supported_in_api": true, "priority": 0 },
                    { "slug": "gpt-6-astra", "supported_in_api": true, "priority": 1,
                      "default_reasoning_level": "high",
                      "supported_reasoning_levels": [
                        { "effort": "high", "description": "More reasoning" },
                        { "effort": "xhigh", "description": "Extra reasoning" },
                        { "effort": "max", "description": "Most reasoning" },
                        { "effort": "ultra", "description": "Ultra reasoning" }
                      ] },
                    { "slug": "gpt-6-terra", "supported_in_api": true, "priority": 2,
                      "default_reasoning_level": "medium",
                      "supported_reasoning_levels": [
                        { "effort": "low" }, { "effort": "medium" }
                      ] },
                    { "slug": "gpt-6-omega", "supported_in_api": true, "priority": 3,
                      "default_reasoning_level": "mega",
                      "supported_reasoning_levels": [
                        { "effort": "ultra" }, { "effort": "mega" }
                      ] },
                    { "slug": "gpt-6-plain", "supported_in_api": true, "priority": 4,
                      "supported_reasoning_levels": [] }
                ]
            })))
            .mount(&server)
            .await;
        let provider = provider_with_endpoint(&server.uri(), cache.cache_path.clone());
        let mut catalog_updates = provider.subscribe_thinking_effort_support().unwrap();
        assert_eq!(
            provider.fetch_supported_models().await.unwrap(),
            vec!["gpt-6-astra", "gpt-6-terra", "gpt-6-omega", "gpt-6-plain"]
        );
        assert!(catalog_updates.has_changed().unwrap());
        catalog_updates.changed().await.unwrap();
        assert_eq!(default_model().as_deref(), Some("gpt-6-astra"));
        assert_eq!(
            reasoning_levels_for_model("gpt-6-astra"),
            vec!["high", "xhigh", "max", "ultra"]
        );
        assert_eq!(get_reasoning_effort("gpt-6-astra"), "high");
        let model = ModelConfig::new("gpt-6-astra");
        let ThinkingEffortSupport::Options(capability) =
            thinking_effort_support_for_model(&provider, &model)
        else {
            panic!("expected catalog reasoning choices");
        };
        assert_eq!(capability.current.as_deref(), Some("high"));
        assert_eq!(
            capability
                .values
                .iter()
                .map(|option| option.value.as_str())
                .collect::<Vec<_>>(),
            vec!["high", "xhigh", "max", "ultra"]
        );
        assert!(
            set_thinking_effort_for_model(&provider, "session", &model, "low")
                .await
                .is_err()
        );
        let max_model = model.with_merged_request_params(std::collections::HashMap::from([(
            "thinking_effort".to_string(),
            json!("max"),
        )]));
        assert!(
            set_thinking_effort_for_model(&provider, "session", &max_model, "max")
                .await
                .unwrap()
        );
        assert_eq!(
            create_codex_request(&max_model, "", &[], &[]).unwrap()["reasoning"]["effort"],
            "max"
        );
        for level in ["xhigh", "ultra"] {
            let selected = ModelConfig::new("gpt-6-astra").with_merged_request_params(
                std::collections::HashMap::from([("thinking_effort".to_string(), json!(level))]),
            );
            assert!(
                set_thinking_effort_for_model(&provider, "session", &selected, level)
                    .await
                    .unwrap()
            );
            assert_eq!(
                create_codex_request(&selected, "", &[], &[]).unwrap()["reasoning"]["effort"],
                level
            );
        }
        let switched = ModelConfig::new("gpt-6-terra").with_merged_request_params(
            std::collections::HashMap::from([("thinking_effort".to_string(), json!("ultra"))]),
        );
        let ThinkingEffortSupport::Options(switched_capability) =
            thinking_effort_support_for_model(&provider, &switched)
        else {
            panic!("expected model-specific reasoning choices");
        };
        assert_eq!(switched_capability.current.as_deref(), Some("medium"));
        assert_eq!(
            switched_capability
                .values
                .iter()
                .map(|option| option.value.as_str())
                .collect::<Vec<_>>(),
            vec!["low", "medium"]
        );
        assert_eq!(
            create_codex_request(&switched, "", &[], &[]).unwrap()["reasoning"]["effort"],
            "medium"
        );
        assert_eq!(
            reasoning_effort_for_config(
                &ModelConfig::new("gpt-6-astra")
                    .with_thinking_effort(goose_providers::thinking::ThinkingEffort::Max)
            ),
            Some("max".to_string())
        );
        let omega = ModelConfig::new("gpt-6-omega")
            .with_thinking_effort(goose_providers::thinking::ThinkingEffort::High);
        assert_eq!(reasoning_effort_for_config(&omega).as_deref(), Some("mega"));
        assert_eq!(
            create_codex_request(&omega, "", &[], &[]).unwrap()["reasoning"]["effort"],
            "mega"
        );
        let plain = ModelConfig::new("gpt-6-plain");
        assert!(matches!(
            thinking_effort_support_for_model(&provider, &plain),
            ThinkingEffortSupport::Unsupported
        ));
        assert!(
            set_thinking_effort_for_model(&provider, "session", &plain, "off")
                .await
                .unwrap()
        );
        assert!(
            set_thinking_effort_for_model(&provider, "session", &plain, "low")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn catalog_changes_from_another_process_notify_open_sessions() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_string_lossy().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path.as_str()))]);
        let cache = TokenCache::new();
        save_token(&cache.cache_path);
        let provider = provider_with_endpoint("http://localhost", cache.cache_path.clone());
        let mut updates = provider.subscribe_thinking_effort_support().unwrap();

        write_private_file(
            &cache.catalog_path(),
            "{\"account_id\":\"account-1\",\"models\":[]}",
        )
        .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), updates.changed())
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn inventory_refresh_rekeys_when_token_discovers_account_id() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_string_lossy().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path.as_str()))]);
        let cache = TokenCache::new();
        let planned = crate::providers::inventory::InventoryIdentityInput::new(
            "chatgpt_codex",
            "chatgpt_codex",
        )
        .with_secret("oauth_account", UNKNOWN_ACCOUNT_IDENTITY)
        .into_identity()
        .unwrap();
        save_token(&cache.cache_path);
        write_private_file(
            &cache.catalog_path(),
            &serde_json::to_string(&CodexCatalogSnapshot {
                account_id: Some("account-1".to_string()),
                models: Vec::new(),
            })
            .unwrap(),
        )
        .unwrap();

        let resolved = crate::providers::inventory::refresh_identity_after_fetch(
            "chatgpt_codex",
            &planned,
            &[],
        )
        .await
        .unwrap();
        assert_ne!(resolved, planned);
        assert_eq!(
            resolved,
            crate::providers::inventory::InventoryIdentityInput::new(
                "chatgpt_codex",
                "chatgpt_codex",
            )
            .with_secret("oauth_account", "account-1")
            .into_identity()
            .unwrap()
        );

        let mut switched = cache.load().unwrap();
        switched.account_id = Some("account-2".to_string());
        cache.save(&switched).unwrap();
        assert!(crate::providers::inventory::refresh_identity_after_fetch(
            "chatgpt_codex",
            &planned,
            &[],
        )
        .await
        .is_err());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn failed_refresh_preserves_catalog_after_account_discovery() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_string_lossy().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path.as_str()))]);
        let cache = TokenCache::new();
        let mut token = TokenData {
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            id_token: None,
            expires_at: Utc::now() + chrono::Duration::hours(1),
            account_id: None,
        };
        cache.save(&token).unwrap();
        let planned = crate::providers::inventory::InventoryIdentityInput::new(
            "chatgpt_codex",
            "chatgpt_codex",
        )
        .with_secret("oauth_account", UNKNOWN_ACCOUNT_IDENTITY)
        .into_identity()
        .unwrap();
        let service = crate::providers::inventory::ProviderInventoryService::new(Arc::new(
            crate::session::session_manager::SessionStorage::new(root.path().to_path_buf()),
        ));
        service
            .store_refreshed_models_for_identity(&planned, &["gpt-6-astra".to_string()])
            .await
            .unwrap();
        let catalog = CodexCatalogSnapshot {
            account_id: None,
            models: vec![serde_json::from_value(json!({
                "slug": "gpt-6-astra", "supported_in_api": true,
                "supported_reasoning_levels": [{"effort": "high"}]
            }))
            .unwrap()],
        };
        write_private_file(
            &cache.catalog_path(),
            &serde_json::to_string(&catalog).unwrap(),
        )
        .unwrap();

        cache.bind_catalog_to_account("account-1").unwrap();
        token.account_id = Some("account-1".to_string());
        cache.save(&token).unwrap();
        service
            .store_refresh_error_after_fetch("chatgpt_codex", &planned, "catalog unavailable")
            .await
            .unwrap();

        let current = crate::providers::inventory::InventoryIdentityInput::new(
            "chatgpt_codex",
            "chatgpt_codex",
        )
        .with_secret("oauth_account", "account-1")
        .into_identity()
        .unwrap();
        assert_ne!(current, planned);
        let snapshot = service
            .find_entry_for_provider("chatgpt_codex")
            .await
            .unwrap();
        assert_eq!(snapshot.models[0].id, "gpt-6-astra");
        assert_eq!(
            snapshot.last_refresh_error.as_deref(),
            Some("catalog unavailable")
        );
        assert_eq!(reasoning_levels_for_model("gpt-6-astra"), vec!["high"]);
    }

    #[test]
    #[serial_test::serial]
    fn oauth_replacement_preserves_existing_account_until_commit() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_string_lossy().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path.as_str()))]);
        let cache = TokenCache::new();
        let token = TokenData {
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            id_token: None,
            expires_at: Utc::now() + chrono::Duration::hours(1),
            account_id: Some("account-1".to_string()),
        };
        let catalog = serde_json::to_string(&CodexCatalogSnapshot {
            account_id: token.account_id.clone(),
            models: vec![serde_json::from_value(json!({
                "slug": "gpt-6-astra", "supported_in_api": true,
                "supported_reasoning_levels": [{"effort": "high"}]
            }))
            .unwrap()],
        })
        .unwrap();
        cache.save(&token).unwrap();
        write_private_file(&cache.catalog_path(), &catalog).unwrap();
        cache.bump_generation().unwrap();
        assert_eq!(cache.load().unwrap().access_token, "access");
        assert_eq!(default_model().as_deref(), Some("gpt-6-astra"));
        assert_eq!(reasoning_levels_for_model("gpt-6-astra"), vec!["high"]);

        let mut replacement = token.clone();
        replacement.access_token = "new-access".to_string();
        cache.commit_oauth_token(&replacement).unwrap();
        assert_eq!(cache.load().unwrap().access_token, "new-access");
        assert_eq!(reasoning_levels_for_model("gpt-6-astra"), vec!["high"]);

        replacement.account_id = Some("account-2".to_string());
        cache.commit_oauth_token(&replacement).unwrap();
        assert!(cache.catalog_snapshot().is_none());
    }

    #[test]
    #[serial_test::serial]
    fn oauth_replacement_clears_catalog_without_verified_account_id() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_string_lossy().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path.as_str()))]);
        let cache = TokenCache::new();
        let token = TokenData {
            access_token: "first".to_string(),
            refresh_token: "first-refresh".to_string(),
            id_token: None,
            expires_at: Utc::now() + chrono::Duration::hours(1),
            account_id: None,
        };
        cache.save(&token).unwrap();
        write_private_file(
            &cache.catalog_path(),
            &serde_json::to_string(&CodexCatalogSnapshot {
                account_id: None,
                models: vec![serde_json::from_value(json!({
                    "slug": "gpt-6-astra", "supported_in_api": true
                }))
                .unwrap()],
            })
            .unwrap(),
        )
        .unwrap();

        let mut replacement = token;
        replacement.access_token = "second".to_string();
        cache.commit_oauth_token(&replacement).unwrap();
        assert!(cache.catalog_snapshot().is_none());
    }

    #[test]
    fn test_known_model_names_include_gpt_5_6_models() {
        let names = known_model_names();

        assert!(names.contains(&"gpt-5.6-sol"));
        assert!(names.contains(&"gpt-5.6-terra"));
        assert!(names.contains(&"gpt-5.6-luna"));
        assert!(names.contains(&"gpt-5.6"));
    }

    #[test]
    fn test_instructions_passed_through() {
        let model = ModelConfig::new("gpt-5.4");
        let payload = create_codex_request(&model, "system prompt", &[], &[]).unwrap();
        let instructions = payload["instructions"].as_str().unwrap();
        assert_eq!(instructions, "system prompt");
    }

    fn provider_with_endpoint(
        server_uri: &str,
        cache_path: std::path::PathBuf,
    ) -> ChatGptCodexProvider {
        ChatGptCodexProvider {
            auth_provider: Arc::new(ChatGptCodexAuthProvider {
                cache: TokenCache { cache_path },
                state: ChatGptCodexAuthState::instance(),
            }),
            name: CHATGPT_CODEX_PROVIDER_NAME.to_string(),
            models_endpoint: format!("{}/models", server_uri),
            request_builder: crate::session_context::session_id_request_builder(),
        }
    }

    fn save_token(cache_path: &std::path::Path) {
        TokenCache {
            cache_path: cache_path.to_path_buf(),
        }
        .save(&TokenData {
            access_token: "access-token".to_string(),
            refresh_token: "refresh-token".to_string(),
            id_token: None,
            expires_at: Utc::now() + chrono::Duration::hours(1),
            account_id: Some("account-1".to_string()),
        })
        .unwrap();
    }

    #[test]
    fn selectable_model_names_filters_by_api_support_and_orders_by_priority() {
        let catalog: CodexCatalog = serde_json::from_value(json!({
            "models": [
                { "slug": "gpt-6-luna", "visibility": "list", "supported_in_api": true, "priority": 4 },
                { "slug": "codex-auto-review", "visibility": "hide", "supported_in_api": true, "priority": 43 },
                { "slug": "gpt-6-astra", "visibility": "list", "supported_in_api": true, "priority": 2 },
                { "slug": "not-served-over-the-api", "visibility": "list", "supported_in_api": false, "priority": 1 },
                { "slug": "flag-omitted", "visibility": "list", "priority": 0 },
                { "slug": "unranked", "supported_in_api": true }
            ]
        }))
        .unwrap();

        assert_eq!(
            selectable_model_names(catalog),
            vec![
                "gpt-6-astra".to_string(),
                "gpt-6-luna".to_string(),
                "unranked".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn fetch_recommended_models_keeps_catalog_priority_order() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "models": [
                    { "slug": "gpt-6-luna", "supported_in_api": true, "priority": 4 },
                    { "slug": "gpt-6-astra", "supported_in_api": true, "priority": 2 }
                ]
            })))
            .mount(&server)
            .await;

        let directory = tempfile::tempdir().unwrap();
        let cache_path = directory.path().join("tokens.json");
        save_token(&cache_path);
        let provider = provider_with_endpoint(&server.uri(), cache_path);

        let models = provider.fetch_recommended_models(false).await.unwrap();

        // Canonical release dates (Luna 2026-09-22, Astra 2026-09-04) would put
        // Luna first, so this pins the endpoint's priority order.
        assert_eq!(
            models,
            vec!["gpt-6-astra".to_string(), "gpt-6-luna".to_string()]
        );
    }

    #[tokio::test]
    async fn fetch_supported_models_rejects_a_catalog_without_models() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "error": { "message": "not found" } })),
            )
            .mount(&server)
            .await;

        let directory = tempfile::tempdir().unwrap();
        let cache_path = directory.path().join("tokens.json");
        save_token(&cache_path);
        let provider = provider_with_endpoint(&server.uri(), cache_path);

        let error = provider.fetch_supported_models().await.unwrap_err();

        assert!(
            matches!(error, ProviderError::RequestFailed(_)),
            "expected RequestFailed, got {error:?}"
        );
    }

    #[tokio::test]
    async fn fetch_supported_models_reads_the_codex_catalog() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .and(wiremock::matchers::query_param(
                "client_version",
                codex_client_version(),
            ))
            .and(wiremock::matchers::header(
                "authorization",
                "Bearer access-token",
            ))
            .and(wiremock::matchers::header(
                "chatgpt-account-id",
                "account-1",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "models": [
                    {
                        "slug": "gpt-6.1-sol",
                        "visibility": "list",
                        "supported_in_api": true,
                        "priority": 1
                    },
                    {
                        "slug": "gpt-5.5",
                        "visibility": "hide",
                        "supported_in_api": true,
                        "priority": 13
                    }
                ]
            })))
            .mount(&server)
            .await;

        let directory = tempfile::tempdir().unwrap();
        let cache_path = directory.path().join("tokens.json");
        save_token(&cache_path);
        let provider = provider_with_endpoint(&server.uri(), cache_path);

        let models = provider.fetch_supported_models().await.unwrap();

        assert_eq!(models, vec!["gpt-6.1-sol".to_string()]);
    }

    #[tokio::test]
    async fn fetch_supported_models_is_not_configured_without_a_cached_token() {
        let server = MockServer::start().await;
        let directory = tempfile::tempdir().unwrap();
        let provider = provider_with_endpoint(&server.uri(), directory.path().join("missing.json"));

        let error = provider.fetch_supported_models().await.unwrap_err();

        assert_eq!(error, ProviderError::NotConfigured);
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn fetch_supported_models_propagates_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let directory = tempfile::tempdir().unwrap();
        let cache_path = directory.path().join("tokens.json");
        save_token(&cache_path);
        let provider = provider_with_endpoint(&server.uri(), cache_path);

        let error = provider.fetch_supported_models().await.unwrap_err();

        assert!(
            matches!(error, ProviderError::ServerError(_)),
            "expected ServerError, got {error:?}"
        );
    }
}
