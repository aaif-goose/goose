use crate::config::paths::Paths;
use anyhow::Result;
use axum::{extract::Query, response::Html, routing::get, Router};
use base64::Engine;
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;
use std::{collections::HashMap, fs, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::sync::{oneshot, Mutex as TokioMutex};
use url::Url;

static OAUTH_MUTEX: Lazy<TokioMutex<()>> = Lazy::new(|| TokioMutex::new(()));

#[derive(Debug, Clone)]
struct OidcEndpoints {
    authorization_endpoint: String,
    token_endpoint: String,
}

#[derive(Serialize, Deserialize)]
struct TokenData {
    /// The access token used to authenticate API requests
    access_token: String,

    /// Optional refresh token that can be used to obtain a new access token
    /// when the current one expires, enabling offline access without user interaction
    refresh_token: Option<String>,

    /// When the access token expires (if known)
    /// Used to determine when a token needs to be refreshed
    expires_at: Option<DateTime<Utc>>,
}

struct TokenCache {
    cache_path: PathBuf,
}

fn get_base_path() -> PathBuf {
    Paths::in_config_dir("databricks/oauth")
}

pub fn cleanup_oauth_cache() -> Result<()> {
    let base_path = get_base_path();
    match fs::remove_dir_all(&base_path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

impl TokenCache {
    fn new(host: &str, client_id: &str, scopes: &[String]) -> Self {
        let mut hasher = sha2::Sha256::new();
        hasher.update(host.as_bytes());
        hasher.update(client_id.as_bytes());
        hasher.update(scopes.join(",").as_bytes());
        let hash = format!("{:x}", hasher.finalize());

        fs::create_dir_all(get_base_path()).unwrap();
        let cache_path = get_base_path().join(format!("{}.json", hash));

        Self { cache_path }
    }

    fn load_token(&self) -> Option<TokenData> {
        if let Ok(contents) = fs::read_to_string(&self.cache_path) {
            if let Ok(token_data) = serde_json::from_str::<TokenData>(&contents) {
                // Only return tokens that have a refresh token
                if token_data.refresh_token.is_some() {
                    // If token is not expired, return it for immediate use
                    if let Some(expires_at) = token_data.expires_at {
                        if expires_at > Utc::now() {
                            return Some(token_data);
                        }
                        // If token is expired but has refresh token, return it so we can refresh
                        return Some(token_data);
                    }
                    // No expiration time but has refresh token, return it
                    return Some(token_data);
                }
                // Token doesn't have a refresh token, ignore it to force a new OAuth flow
            }
        }
        None
    }

    fn save_token(&self, token_data: &TokenData) -> Result<()> {
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string(token_data)?;
        fs::write(&self.cache_path, contents)?;
        Ok(())
    }
}

async fn get_workspace_endpoints(host: &str) -> Result<OidcEndpoints> {
    let base_url = Url::parse(host).expect("Invalid host URL");
    let oidc_url = base_url
        .join("oidc/.well-known/oauth-authorization-server")
        .expect("Invalid OIDC URL");

    let client = reqwest::Client::new();
    let resp = client.get(oidc_url.clone()).send().await?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!(
            "Failed to get OIDC configuration from {}",
            oidc_url
        ));
    }

    let oidc_config: Value = resp.json().await?;

    let authorization_endpoint = oidc_config
        .get("authorization_endpoint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("authorization_endpoint not found in OIDC configuration"))?
        .to_string();

    let token_endpoint = oidc_config
        .get("token_endpoint")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("token_endpoint not found in OIDC configuration"))?
        .to_string();

    Ok(OidcEndpoints {
        authorization_endpoint,
        token_endpoint,
    })
}

struct OAuthFlow {
    endpoints: OidcEndpoints,
    client_id: String,
    redirect_url: String,
    scopes: Vec<String>,
    state: String,
    verifier: String,
}

impl OAuthFlow {
    fn new(
        endpoints: OidcEndpoints,
        client_id: String,
        redirect_url: String,
        scopes: Vec<String>,
    ) -> Self {
        Self {
            endpoints,
            client_id,
            redirect_url,
            scopes,
            state: nanoid::nanoid!(16),
            verifier: nanoid::nanoid!(64),
        }
    }

    /// Extracts token data from an OAuth 2.0 token response.
    ///
    /// This helper method consolidates the common logic for processing token responses
    /// from both initial token requests and refresh token requests.
    ///
    /// # Parameters
    /// * `token_response` - The JSON response from the OAuth server's token endpoint
    /// * `old_refresh_token` - Optional previous refresh token to use as fallback if the
    ///   response doesn't contain a new refresh token. This handles token rotation where
    ///   some providers don't return a new refresh token with every refresh operation.
    ///
    /// # Returns
    /// A Result containing the TokenData with access_token, refresh_token (if available)
    ///
    /// # Error
    /// Returns an error if the required access_token is missing from the response.
    fn extract_token_data(
        &self,
        token_response: &Value,
        old_refresh_token: Option<&str>,
    ) -> Result<TokenData> {
        // Extract access token (required)
        let access_token = token_response
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("access_token not found in token response"))?
            .to_string();

        // Extract refresh token if available
        let refresh_token = token_response
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| old_refresh_token.map(|s| s.to_string()));

        // Handle token expiration
        let expires_at =
            if let Some(expires_in) = token_response.get("expires_in").and_then(|v| v.as_u64()) {
                // Traditional OAuth flow with expires_in seconds
                Some(Utc::now() + chrono::Duration::seconds(expires_in as i64))
            } else {
                // If the server doesn't provide any expiration info, log it but don't set an expiration
                // This will make us rely on the refresh token for renewal rather than expiration time
                tracing::debug!(
                    "No expiration information provided by server, token expiration unknown."
                );
                None
            };

        Ok(TokenData {
            access_token,
            refresh_token,
            expires_at,
        })
    }

    fn get_authorization_url_with_redirect(&self, redirect_url: &str) -> String {
        let challenge = {
            let digest = sha2::Sha256::digest(self.verifier.as_bytes());
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
        };

        let params = [
            ("response_type", "code"),
            ("client_id", &self.client_id),
            ("redirect_uri", redirect_url),
            ("scope", &self.scopes.join(" ")),
            ("state", &self.state),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
        ];

        format!(
            "{}?{}",
            self.endpoints.authorization_endpoint,
            serde_urlencoded::to_string(params).unwrap()
        )
    }

    async fn exchange_code_for_token_with_redirect(
        &self,
        code: &str,
        redirect_url: &str,
    ) -> Result<TokenData> {
        let params = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_url),
            ("code_verifier", &self.verifier),
            ("client_id", &self.client_id),
        ];

        let client = reqwest::Client::new();
        let resp = client
            .post(&self.endpoints.token_endpoint)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&params)
            .send()
            .await?;

        if !resp.status().is_success() {
            let err_text = resp.text().await?;
            return Err(anyhow::anyhow!(
                "Failed to exchange code for token: {}",
                err_text
            ));
        }

        let token_response: Value = resp.json().await?;
        self.extract_token_data(&token_response, None)
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenData> {
        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.client_id),
        ];

        tracing::debug!("Refreshing token using refresh_token");

        let client = reqwest::Client::new();
        let resp = client
            .post(&self.endpoints.token_endpoint)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&params)
            .send()
            .await?;

        if !resp.status().is_success() {
            let err_text = resp.text().await?;
            return Err(anyhow::anyhow!("Failed to refresh token: {}", err_text));
        }

        let token_response: Value = resp.json().await?;
        self.extract_token_data(&token_response, Some(refresh_token))
    }

    async fn execute(&self) -> Result<TokenData> {
        // Create a channel that will send the auth code from the app process
        let (tx, rx) = oneshot::channel();
        let state = self.state.clone();
        // Axum can theoretically spawn multiple threads, so we need this to be in an Arc even
        // though it will ultimately only get used once
        let tx = Arc::new(tokio::sync::Mutex::new(Some(tx)));

        // Set up a server that will receive the redirect, capture the code, and display success/failure
        let app = Router::new().route(
            "/",
            get(move |Query(params): Query<HashMap<String, String>>| {
                let tx = Arc::clone(&tx);
                let state = state.clone();
                async move {
                    let code = params.get("code").cloned();
                    let received_state = params.get("state").cloned();

                    if let (Some(code), Some(received_state)) = (code, received_state) {
                        if received_state == state {
                            if let Some(sender) = tx.lock().await.take() {
                                if sender.send(code).is_ok() {
                                    // Use the improved HTML response
                                    return Html(
                                        "<h2>Login Success</h2><p>You can close this window</p>",
                                    );
                                }
                            }
                            Html("<h2>Error</h2><p>Authentication already completed.</p>")
                        } else {
                            Html("<h2>Error</h2><p>State mismatch.</p>")
                        }
                    } else {
                        Html("<h2>Error</h2><p>Authentication failed.</p>")
                    }
                }
            }),
        );

        // Start the server to accept the oauth code
        let redirect_url_parsed = Url::parse(&self.redirect_url)?;
        let requested_port = redirect_url_parsed.port();

        // If no port is specified (or port is explicitly 0), let the OS assign one
        // Otherwise, use the requested port
        let env_port: Option<u16> = std::env::var("GOOSE_OAUTH_CALLBACK_PORT")
            .ok()
            .and_then(|p| p.parse().ok());
        let bind_port = requested_port.or(env_port).unwrap_or(0);
        let addr = SocketAddr::from(([127, 0, 0, 1], bind_port));
        let listener = tokio::net::TcpListener::bind(addr).await?;

        let actual_port = listener.local_addr()?.port();

        let server_handle = tokio::spawn(async move {
            let server = axum::serve(listener, app);
            server.await.unwrap();
        });

        let actual_redirect_url = format!("http://localhost:{}", actual_port);

        // Open the browser which will redirect with the code to the server
        let authorization_url = self.get_authorization_url_with_redirect(&actual_redirect_url);
        if webbrowser::open(&authorization_url).is_err() {
            println!(
                "Please open this URL in your browser:\n{}",
                authorization_url
            );
        }

        // Wait for the authorization code with a timeout
        let code = tokio::time::timeout(
            std::time::Duration::from_secs(60), // 1 minute timeout
            rx,
        )
        .await
        .map_err(|_| anyhow::anyhow!("Authentication timed out"))??;

        // Stop the server
        server_handle.abort();

        // Exchange the code for a token using the actual redirect URL
        self.exchange_code_for_token_with_redirect(&code, &actual_redirect_url)
            .await
    }
}

pub(crate) async fn get_oauth_token_async(
    host: &str,
    client_id: &str,
    redirect_url: &str,
    scopes: &[String],
) -> Result<String> {
    // Acquire the global mutex to ensure only one OAuth flow runs at a time
    let _guard = OAUTH_MUTEX.lock().await;

    let token_cache = TokenCache::new(host, client_id, scopes);

    // Try cache first
    if let Some(token) = token_cache.load_token() {
        // If token has an expiration time, check if it's expired
        if let Some(expires_at) = token.expires_at {
            if expires_at > Utc::now() {
                return Ok(token.access_token);
            }
            // Token is expired, will try to refresh below
            tracing::debug!("Token is expired, attempting to refresh");
        } else {
            // No expiration time was provided by the server
            // We'll use the token without checking expiration
            // This is safe because we'll fall back to refresh token if the server rejects it
            tracing::debug!("Token has no expiration time, using it without expiration check");
            return Ok(token.access_token);
        }

        // Token is expired or has no expiration, try to refresh if we have a refresh token
        if let Some(refresh_token) = token.refresh_token {
            // Get endpoints for token refresh
            match get_workspace_endpoints(host).await {
                Ok(endpoints) => {
                    let flow = OAuthFlow::new(
                        endpoints,
                        client_id.to_string(),
                        redirect_url.to_string(),
                        scopes.to_vec(),
                    );

                    // Try to refresh the token
                    match flow.refresh_token(&refresh_token).await {
                        Ok(new_token) => {
                            // NOTE: Per OAuth 2.0 RFC 6749, the authorization server MAY issue
                            // a new refresh_token. We save the entire token response so that we
                            // capture all updated token data, even if no new refresh_token is returned.
                            if let Err(e) = token_cache.save_token(&new_token) {
                                tracing::warn!("Failed to save refreshed token: {}", e);
                            }
                            tracing::info!("Successfully refreshed token");
                            return Ok(new_token.access_token);
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to refresh token, will try new auth flow: {}",
                                e
                            );
                            // Continue to new auth flow
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to get endpoints for token refresh: {}", e);
                    // Continue to new auth flow
                }
            }
        }
    }

    // Get endpoints and execute flow for a new token
    let endpoints = get_workspace_endpoints(host).await?;
    let flow = OAuthFlow::new(
        endpoints,
        client_id.to_string(),
        redirect_url.to_string(),
        scopes.to_vec(),
    );

    // Execute the OAuth flow and get token
    let token = flow.execute().await?;

    // Cache and return
    token_cache.save_token(&token)?;
    Ok(token.access_token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn test_get_workspace_endpoints() -> Result<()> {
        let mock_server = MockServer::start().await;

        let mock_response = serde_json::json!({
            "authorization_endpoint": "https://example.com/oauth2/authorize",
            "token_endpoint": "https://example.com/oauth2/token"
        });

        Mock::given(method("GET"))
            .and(path("/oidc/.well-known/oauth-authorization-server"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
            .mount(&mock_server)
            .await;

        let endpoints = get_workspace_endpoints(&mock_server.uri()).await?;

        assert_eq!(
            endpoints.authorization_endpoint,
            "https://example.com/oauth2/authorize"
        );
        assert_eq!(endpoints.token_endpoint, "https://example.com/oauth2/token");

        Ok(())
    }

    #[test]
    fn test_token_cache() -> Result<()> {
        let cache = TokenCache::new(
            "https://example.com",
            "test-client",
            &["scope1".to_string()],
        );

        // Test with expiration time
        let token_data = TokenData {
            access_token: "test-token".to_string(),
            refresh_token: Some("test-refresh-token".to_string()),
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
        };

        cache.save_token(&token_data)?;

        let loaded_token = cache.load_token().unwrap();
        assert_eq!(loaded_token.access_token, token_data.access_token);
        assert_eq!(loaded_token.refresh_token, token_data.refresh_token);
        assert!(loaded_token.expires_at.is_some());

        // Test without expiration time
        let token_data_no_expiry = TokenData {
            access_token: "test-token-2".to_string(),
            refresh_token: Some("test-refresh-token-2".to_string()),
            expires_at: None,
        };

        cache.save_token(&token_data_no_expiry)?;

        let loaded_token = cache.load_token().unwrap();
        assert_eq!(loaded_token.access_token, token_data_no_expiry.access_token);
        assert_eq!(
            loaded_token.refresh_token,
            token_data_no_expiry.refresh_token
        );
        assert!(loaded_token.expires_at.is_none());

        Ok(())
    }

    #[test]
    fn test_extract_token_data() -> Result<()> {
        let endpoints = OidcEndpoints {
            authorization_endpoint: "https://example.com/oauth2/authorize".to_string(),
            token_endpoint: "https://example.com/oauth2/token".to_string(),
        };

        let flow = OAuthFlow::new(
            endpoints,
            "test-client".to_string(),
            "http://localhost".to_string(),
            vec!["all-apis".to_string()],
        );

        // Test with expires_in (traditional OAuth)
        let token_response = serde_json::json!({
            "access_token": "test-access-token",
            "refresh_token": "test-refresh-token",
            "expires_in": 3600
        });

        let token_data = flow.extract_token_data(&token_response, None)?;
        assert_eq!(token_data.access_token, "test-access-token");
        assert_eq!(
            token_data.refresh_token,
            Some("test-refresh-token".to_string())
        );
        assert!(token_data.expires_at.is_some());

        // Test with invalid expires_at format
        let token_response = serde_json::json!({
            "access_token": "invalid-format-token",
            "refresh_token": "invalid-format-refresh",
            "expires_at": "invalid-date-format"
        });

        let token_data = flow.extract_token_data(&token_response, None)?;
        assert_eq!(token_data.access_token, "invalid-format-token");
        assert_eq!(
            token_data.refresh_token,
            Some("invalid-format-refresh".to_string())
        );
        assert!(token_data.expires_at.is_none()); // Should be None due to parse error

        Ok(())
    }
}
