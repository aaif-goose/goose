use crate::subprocess::SubprocessExt;
use chrono;
use serde::Deserialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Represents errors that can occur during Azure authentication.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// Error when loading credentials from the filesystem or environment
    #[error("Failed to load credentials: {0}")]
    Credentials(String),

    /// Error during token exchange
    #[error("Token exchange failed: {0}")]
    TokenExchange(String),
}

/// Represents an authentication token with its type and value.
#[derive(Debug, Clone)]
pub struct AuthToken {
    /// The type of the token (e.g., "Bearer")
    pub token_type: String,
    /// The actual token value
    pub token_value: String,
}

/// Represents the types of Azure credentials supported.
#[derive(Debug, Clone)]
pub enum AzureCredentials {
    /// API key based authentication
    ApiKey(String),
    /// Pre-acquired Microsoft Entra ID access token (e.g. AZURE_OPENAI_AD_TOKEN)
    BearerToken(String),
    /// Azure credential chain based authentication
    DefaultCredential,
}

/// Holds a cached token and its expiration time.
#[derive(Debug, Clone)]
struct CachedToken {
    token: AuthToken,
    expires_at: Instant,
}

/// Response from Azure token endpoint
#[derive(Debug, Clone, Deserialize)]
struct TokenResponse {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "tokenType")]
    token_type: String,
    #[serde(rename = "expires_on")]
    expires_on: u64,
}

/// Azure authentication handler that manages credentials and token caching.
#[derive(Debug)]
pub struct AzureAuth {
    credentials: AzureCredentials,
    resource: String,
    cached_token: Arc<RwLock<Option<CachedToken>>>,
}

impl AzureAuth {
    /// Creates a new Azure authentication handler.
    ///
    /// Initializes the authentication handler by:
    /// 1. Loading credentials from environment
    /// 2. Setting up an HTTP client for token requests
    /// 3. Initializing the token cache
    ///
    /// # Returns
    /// * `Result<Self, AuthError>` - A new AzureAuth instance or an error if initialization fails
    pub fn new(api_key: Option<String>, ad_token: Option<String>) -> Result<Self, AuthError> {
        Self::new_with_resource(
            api_key,
            ad_token,
            "https://cognitiveservices.azure.com".to_string(),
        )
    }

    pub fn new_with_resource(
        api_key: Option<String>,
        ad_token: Option<String>,
        resource: String,
    ) -> Result<Self, AuthError> {
        let credentials = match (ad_token, api_key) {
            (Some(token), _) => AzureCredentials::BearerToken(token),
            (None, Some(key)) => AzureCredentials::ApiKey(key),
            (None, None) => AzureCredentials::DefaultCredential,
        };

        Ok(Self {
            credentials,
            resource,
            cached_token: Arc::new(RwLock::new(None)),
        })
    }

    /// Returns the type of credentials being used.
    pub fn credential_type(&self) -> &AzureCredentials {
        &self.credentials
    }

    pub async fn invalidate_token(&self) {
        *self.cached_token.write().await = None;
    }

    /// Retrieves a valid authentication token.
    ///
    /// This method implements an efficient token management strategy:
    /// 1. For API key auth, returns the API key directly
    /// 2. For bearer token auth, returns the pre-acquired token directly
    /// 3. For Azure credential chain:
    ///    a. Checks the cache for a valid token
    ///    b. Returns the cached token if not expired
    ///    c. Obtains a new token if needed or expired
    ///    d. Uses double-checked locking for thread safety
    ///
    /// # Returns
    /// * `Result<AuthToken, AuthError>` - A valid authentication token or an error
    pub async fn get_token(&self) -> Result<AuthToken, AuthError> {
        match &self.credentials {
            AzureCredentials::ApiKey(key) => Ok(AuthToken {
                token_type: "Bearer".to_string(),
                token_value: key.clone(),
            }),
            AzureCredentials::BearerToken(token) => Ok(AuthToken {
                token_type: "Bearer".to_string(),
                token_value: token.clone(),
            }),
            AzureCredentials::DefaultCredential => self.get_default_credential_token().await,
        }
    }

    async fn get_default_credential_token(&self) -> Result<AuthToken, AuthError> {
        // Try read lock first for better concurrency
        if let Some(cached) = self.cached_token.read().await.as_ref() {
            if cached.expires_at > Instant::now() {
                return Ok(cached.token.clone());
            }
        }

        // Take write lock only if needed
        let mut token_guard = self.cached_token.write().await;

        // Double-check expiration after acquiring write lock
        if let Some(cached) = token_guard.as_ref() {
            if cached.expires_at > Instant::now() {
                return Ok(cached.token.clone());
            }
        }

        let az = if cfg!(windows) { "az.cmd" } else { "az" };
        let output = tokio::process::Command::new(az)
            .args(["account", "get-access-token", "--resource", &self.resource])
            .set_no_window()
            .output()
            .await
            .map_err(|e| AuthError::TokenExchange(format!("Failed to execute Azure CLI: {}", e)))?;

        if !output.status.success() {
            return Err(AuthError::TokenExchange(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let token_response: TokenResponse = serde_json::from_slice(&output.stdout)
            .map_err(|e| AuthError::TokenExchange(format!("Invalid token response: {}", e)))?;

        let auth_token = AuthToken {
            token_type: token_response.token_type,
            token_value: token_response.access_token,
        };

        let expires_at = Instant::now()
            + Duration::from_secs(
                token_response
                    .expires_on
                    .saturating_sub(chrono::Utc::now().timestamp() as u64)
                    .saturating_sub(30),
            );

        *token_guard = Some(CachedToken {
            token: auth_token.clone(),
            expires_at,
        });

        Ok(auth_token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ad_token_takes_precedence_over_api_key() {
        let auth = AzureAuth::new(Some("key".to_string()), Some("token".to_string())).unwrap();
        assert!(matches!(
            auth.credential_type(),
            AzureCredentials::BearerToken(_)
        ));
    }

    #[test]
    fn test_api_key_when_no_ad_token() {
        let auth = AzureAuth::new(Some("key".to_string()), None).unwrap();
        assert!(matches!(
            auth.credential_type(),
            AzureCredentials::ApiKey(_)
        ));
    }

    #[test]
    fn test_default_credential_when_neither() {
        let auth = AzureAuth::new(None, None).unwrap();
        assert!(matches!(
            auth.credential_type(),
            AzureCredentials::DefaultCredential
        ));
    }

    #[tokio::test]
    async fn test_bearer_token_get_token() {
        let auth = AzureAuth::new(None, Some("my-token".to_string())).unwrap();
        let token = auth.get_token().await.unwrap();
        assert_eq!(token.token_type, "Bearer");
        assert_eq!(token.token_value, "my-token");
    }
}
