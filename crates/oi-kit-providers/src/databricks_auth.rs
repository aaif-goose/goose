use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use crate::api_client::AuthProvider;

const DEFAULT_CLIENT_ID: &str = "databricks-cli";
const DEFAULT_REDIRECT_URL: &str = "http://localhost";
const DEFAULT_SCOPES: &[&str] = &["all-apis", "offline_access"];

pub type DatabricksOauthTokenFuture = Pin<Box<dyn Future<Output = Result<String>> + Send>>;
pub type DatabricksOauthTokenProvider =
    Arc<dyn Fn(String, String, String, Vec<String>) -> DatabricksOauthTokenFuture + Send + Sync>;
pub type DatabricksTokenResolver = Arc<dyn Fn() -> Option<String> + Send + Sync>;
pub type DatabricksRefreshHook = Arc<dyn Fn() + Send + Sync>;
pub type DatabricksSessionIdProvider = Arc<dyn Fn() -> Option<String> + Send + Sync>;

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

pub struct DatabricksAuthProvider {
    pub auth: DatabricksAuth,
    pub token_cache: Arc<Mutex<Option<String>>>,
    pub oauth_token_provider: Option<DatabricksOauthTokenProvider>,
    pub token_resolver: Option<DatabricksTokenResolver>,
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
                        let fresh = self
                            .token_resolver
                            .as_ref()
                            .and_then(|resolve| resolve())
                            .unwrap_or_else(|| original.clone());
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
            } => {
                let Some(provider) = &self.oauth_token_provider else {
                    anyhow::bail!("Databricks OAuth token provider is not configured")
                };
                provider(
                    host.clone(),
                    client_id.clone(),
                    redirect_url.clone(),
                    scopes.clone(),
                )
                .await?
            }
        };
        Ok(("Authorization".to_string(), format!("Bearer {token}")))
    }
}
