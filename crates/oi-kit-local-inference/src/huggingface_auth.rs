use crate::{config_resolver, paths::Paths};
use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const HUGGINGFACE_TOKEN_SECRET_KEY: &str = "HF_TOKEN";
pub const HUGGINGFACE_OAUTH_CACHE_PATH: &str = "huggingface/oauth/tokens.json";

pub type TokenResolver = fn() -> BoxFuture<'static, Result<Option<String>>>;

static TOKEN_RESOLVER: OnceLock<TokenResolver> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HuggingFaceTokenData {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

impl HuggingFaceTokenData {
    pub fn is_expired(&self) -> bool {
        self.expires_at
            .is_some_and(|expires_at| expires_at <= Utc::now())
    }
}

pub fn oauth_cache_path() -> PathBuf {
    Paths::in_config_dir(HUGGINGFACE_OAUTH_CACHE_PATH)
}

fn load_oauth_token_from_path(path: &Path) -> Option<HuggingFaceTokenData> {
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

pub fn usable_oauth_token() -> Option<String> {
    let token = load_oauth_token_from_path(&oauth_cache_path())?;
    (!token.is_expired()).then_some(token.access_token)
}

pub fn hf_token_secret() -> Result<Option<String>> {
    Ok(config_resolver::string_param(HUGGINGFACE_TOKEN_SECRET_KEY)?
        .filter(|token| !token.trim().is_empty()))
}

pub fn set_token_resolver(resolve_token: TokenResolver) {
    let _ = TOKEN_RESOLVER.set(resolve_token);
}

pub async fn resolve_token_async() -> Result<Option<String>> {
    if let Some(resolve_token) = TOKEN_RESOLVER.get() {
        return resolve_token().await;
    }

    if let Some(token) = usable_oauth_token() {
        return Ok(Some(token));
    }
    hf_token_secret()
}
