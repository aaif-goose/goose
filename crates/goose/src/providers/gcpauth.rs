use async_trait::async_trait;
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{env, fmt, io};
use tokio::sync::RwLock;

/// Represents errors that can occur during GCP authentication.
///
/// This enum encompasses various error conditions that might arise during
/// the authentication process, including credential loading, token creation,
/// and token exchange operations.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// Error when loading credentials from the filesystem or environment
    #[error("Failed to load credentials: {0}")]
    Credentials(String),

    /// Error during JWT token creation
    #[error("Token creation failed: {0}")]
    TokenCreation(String),

    /// Error during OAuth token exchange
    #[error("Token exchange failed: {0}")]
    TokenExchange(String),
}

/// Represents an authentication token with its type and value.
///
/// This structure holds both the token type (e.g., "Bearer") and its
/// actual value, typically used for authentication with GCP services.
/// The token is obtained either through service account or user credentials.
#[derive(Debug, Clone)]
pub struct AuthToken {
    /// The type of the token (e.g., "Bearer")
    pub token_type: String,
    /// The actual token value
    pub token_value: String,
}

impl fmt::Display for AuthToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.token_type, self.token_value)
    }
}

/// Represents the types of Application Default Credentials (ADC) supported.
///
/// GCP supports multiple credential types for authentication. This enum
/// represents the two main types: authorized user and service account.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AdcCredentials {
    /// Credentials for an authorized user (typically from gcloud auth)
    AuthorizedUser(AuthorizedUserCredentials),
    /// Credentials for a service account
    ServiceAccount(ServiceAccountCredentials),
    /// Credentials for the GCP native default account
    #[serde(skip)]
    DefaultAccount(String),
}

/// Credentials for an authorized user account.
///
/// These credentials are typically obtained through interactive login
/// with the gcloud CLI tool.
#[derive(Debug, Deserialize)]
struct AuthorizedUserCredentials {
    /// OAuth 2.0 client ID
    client_id: String,
    /// OAuth 2.0 client secret
    client_secret: String,
    /// OAuth 2.0 refresh token
    refresh_token: String,
    /// URI for token refresh requests
    #[serde(default = "default_token_uri")]
    token_uri: String,
}

/// Credentials for a service account.
///
/// These credentials are typically obtained from a JSON key file
/// downloaded from the Google Cloud Console.
#[derive(Debug, Deserialize)]
struct ServiceAccountCredentials {
    /// Service account email address
    client_email: String,
    /// The private key from JSON credential for signing JWT tokens
    private_key: String,
    /// URI for token exchange requests
    token_uri: String,
}

/// Returns the default OAuth 2.0 token endpoint.
fn default_token_uri() -> String {
    "https://oauth2.googleapis.com/token".to_string()
}

/// A trait that defines operations for interacting with the filesystem.
///
/// This trait provides an abstraction over filesystem operations, primarily
/// for reading credential files. It enables testing through mock implementations.
#[async_trait]
pub trait FilesystemOps {
    /// Reads the contents of a file into a string.
    ///
    /// # Arguments
    /// * `path` - The path to the file to read
    ///
    /// # Returns
    /// * `Result<String, io::Error>` - The contents of the file or an error
    async fn read_to_string(&self, path: String) -> Result<String, io::Error>;
}

/// A trait that defines operations for accessing environment variables.
///
/// This trait provides an abstraction over environment variable access,
/// enabling testing through mock implementations.
pub trait EnvOps {
    /// Retrieves the value of an environment variable.
    ///
    /// # Arguments
    /// * `key` - The name of the environment variable
    ///
    /// # Returns
    /// * `Result<String, env::VarError>` - The value of the variable or an error if not found
    fn get_var(&self, key: &str) -> Result<String, env::VarError>;
}

/// A concrete implementation of FilesystemOps using the actual filesystem.
///
/// This implementation uses tokio's async filesystem operations for
/// reading files in an asynchronous manner.
pub struct RealFilesystemOps;

/// A concrete implementation of EnvOps using the actual environment.
///
/// This implementation directly accesses system environment variables
/// through the standard library.
pub struct RealEnvOps;

#[async_trait]
impl FilesystemOps for RealFilesystemOps {
    async fn read_to_string(&self, path: String) -> Result<String, io::Error> {
        tokio::fs::read_to_string(path).await
    }
}

impl EnvOps for RealEnvOps {
    fn get_var(&self, key: &str) -> Result<String, env::VarError> {
        env::var(key)
    }
}

impl AdcCredentials {
    /// Loads credentials from the default locations.
    /// https://cloud.google.com/docs/authentication/application-default-credentials#personal
    ///
    /// Attempts to load credentials in the following order:
    /// 1. GOOGLE_APPLICATION_CREDENTIALS environment variable
    /// 2. Default gcloud credentials path (~/.config/gcloud/application_default_credentials.json)
    /// 3. Metadata server if running in GCP
    async fn load() -> Result<Self, AuthError> {
        Self::load_impl(
            &RealFilesystemOps,
            &RealEnvOps,
            "http://metadata.google.internal",
        )
        .await
    }

    async fn load_impl(
        fs_ops: &impl FilesystemOps,
        env_ops: &impl EnvOps,
        metadata_base_url: &str,
    ) -> Result<Self, AuthError> {
        // Try GOOGLE_APPLICATION_CREDENTIALS first
        if let Ok(cred_path) = Self::get_env_credentials_path(env_ops) {
            if let Ok(creds) = Self::load_from_file(fs_ops, &cred_path).await {
                return Ok(creds);
            }
        }

        // Try default gcloud credentials path
        if let Ok(cred_path) = Self::get_default_credentials_path(env_ops) {
            if let Ok(creds) = Self::load_from_file(fs_ops, &cred_path).await {
                return Ok(creds);
            }
        }

        // Try metadata server if running on GCP
        if let Ok(creds) = Self::load_from_metadata_server(metadata_base_url).await {
            return Ok(creds);
        }

        Err(AuthError::Credentials(
            "No valid credentials found in any location".to_string(),
        ))
    }

    async fn load_from_file(fs_ops: &impl FilesystemOps, path: &str) -> Result<Self, AuthError> {
        let content = fs_ops.read_to_string(path.to_string()).await.map_err(|e| {
            AuthError::Credentials(format!("Failed to read credentials from {}: {}", path, e))
        })?;

        serde_json::from_str(&content)
            .map_err(|e| AuthError::Credentials(format!("Invalid credentials format: {}", e)))
    }

    fn get_env_credentials_path(env_ops: &impl EnvOps) -> Result<String, AuthError> {
        env_ops
            .get_var("GOOGLE_APPLICATION_CREDENTIALS")
            .map_err(|_| {
                AuthError::Credentials("GOOGLE_APPLICATION_CREDENTIALS not set".to_string())
            })
    }

    fn get_default_credentials_path(env_ops: &impl EnvOps) -> Result<String, AuthError> {
        let (env_var, subpath) = if cfg!(windows) {
            ("APPDATA", "gcloud\\application_default_credentials.json")
        } else {
            (
                "HOME",
                ".config/gcloud/application_default_credentials.json",
            )
        };

        env_ops
            .get_var(env_var)
            .map(|dir| {
                PathBuf::from(dir)
                    .join(subpath)
                    .to_string_lossy()
                    .into_owned()
            })
            .map_err(|_| {
                AuthError::Credentials("Could not determine user home directory".to_string())
            })
    }

    async fn load_from_metadata_server(base_url: &str) -> Result<Self, AuthError> {
        let client = reqwest::Client::new();
        let metadata_path = "/computeMetadata/v1/instance/service-accounts/default/token";

        let response = client
            .get(format!("{}{}", base_url, metadata_path))
            .header("Metadata-Flavor", "Google")
            .send()
            .await
            .map_err(|e| {
                AuthError::Credentials(format!("Metadata server request failed: {}", e))
            })?;

        if !response.status().is_success() {
            return Err(AuthError::Credentials(
                "Not running on GCP or metadata server unavailable".to_string(),
            ));
        }

        // Note: When using metadata server, we have access to the OAuth2 access token
        // that can be used to authenticate applications.
        // However, this token expires. Better to keep the base_url, and fetch a new token
        // when needed
        Ok(AdcCredentials::DefaultAccount(base_url.to_string()))
    }
}

/// Claims structure for JWT tokens.
///
/// These claims are included in the JWT token used for service account
/// authentication.
#[derive(Debug, Serialize)]
struct JwtClaims {
    /// Token issuer (service account email)
    iss: String,
    /// Token subject (service account email)
    sub: String,
    /// Service account scope within role
    scope: String,
    /// Token audience (OAuth endpoint)
    aud: String,
    /// Token issued at timestamp
    iat: u64,
    /// Token expiration timestamp
    exp: u64,
}

/// Holds a cached token and its expiration time.
///
/// Used internally to implement token caching and automatic refresh.
#[derive(Debug, Clone)]
struct CachedToken {
    /// The cached authentication token
    token: AuthToken,
    /// When the token will expire
    expires_at: Instant,
}

/// Response structure for token exchange requests.
#[derive(Debug, Deserialize, Clone)]
struct TokenResponse {
    /// The access token string
    access_token: String,
    /// Token lifetime in seconds
    expires_in: u64,
    /// Token type (e.g., "Bearer")
    #[serde(default)]
    token_type: String,
}

/// Handles authentication with Google Cloud Platform services.
///
/// This struct manages the complete authentication lifecycle including:
/// - Loading and validating credentials
/// - Creating and refreshing tokens
/// - Caching tokens for efficient reuse
/// - Managing concurrent access through atomic operations
///
/// It supports both service account and authorized user authentication methods,
/// automatically selecting the appropriate method based on available credentials.
/// ```
#[derive(Debug)]
pub struct GcpAuth {
    /// The loaded credentials (service account or authorized user)
    credentials: AdcCredentials,
    /// HTTP client for making token exchange requests
    client: reqwest::Client,
    /// Thread-safe cache for the current token
    cached_token: Arc<RwLock<Option<CachedToken>>>,
}

impl GcpAuth {
    /// Creates a new GCP authentication handler.
    ///
    /// Initializes the authentication handler by:
    /// 1. Loading credentials from default locations
    /// 2. Setting up an HTTP client for token requests
    /// 3. Initializing the token cache
    ///
    /// The credentials are loaded in the following order:
    /// 1. GOOGLE_APPLICATION_CREDENTIALS environment variable
    /// 2. Default gcloud credentials path
    /// 3. GCP metadata server (when running on GCP)
    ///
    /// # Returns
    /// * `Result<Self, AuthError>` - A new GcpAuth instance or an error if initialization fails
    pub async fn new() -> Result<Self, AuthError> {
        Ok(Self {
            credentials: AdcCredentials::load().await?,
            client: reqwest::Client::new(),
            cached_token: Arc::new(RwLock::new(None)),
        })
    }

    /// Retrieves a valid authentication token.
    ///
    /// This method implements an efficient token management strategy:
    /// 1. Checks the cache for a valid token
    /// 2. Returns the cached token if not expired
    /// 3. Obtains a new token if needed or expired
    /// 4. Uses double-checked locking for thread safety
    ///
    /// The returned token includes a type (usually "Bearer") and the actual
    /// token value used for authentication with GCP services.
    ///
    /// # Returns
    /// * `Result<AuthToken, AuthError>` - A valid authentication token or an error
    pub async fn get_token(&self) -> Result<AuthToken, AuthError> {
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

        // Get new token
        let token_response = match &self.credentials {
            AdcCredentials::ServiceAccount(creds) => self.get_service_account_token(creds).await?,
            AdcCredentials::AuthorizedUser(creds) => self.get_authorized_user_token(creds).await?,
            AdcCredentials::DefaultAccount(creds) => self.get_default_access_token(creds).await?,
        };

        let auth_token = AuthToken {
            token_type: if token_response.token_type.is_empty() {
                "Bearer".to_string()
            } else {
                token_response.token_type
            },
            token_value: token_response.access_token,
        };

        let expires_at = Instant::now()
            + Duration::from_secs(
                token_response.expires_in.saturating_sub(30), // 30 second buffer
            );

        *token_guard = Some(CachedToken {
            token: auth_token.clone(),
            expires_at,
        });

        Ok(auth_token)
    }

    /// Creates a JWT token for service account authentication.
    ///
    /// # Arguments
    /// * `creds` - Service account credentials for signing the token
    ///
    /// # Returns
    /// * `Result<String>` - A signed JWT token
    fn create_jwt_token(&self, creds: &ServiceAccountCredentials) -> Result<String, AuthError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| AuthError::TokenCreation(e.to_string()))?
            .as_secs();

        let claims = JwtClaims {
            iss: creds.client_email.clone(),
            sub: creds.client_email.clone(),
            scope: "https://www.googleapis.com/auth/cloud-platform".to_string(),
            aud: creds.token_uri.clone(),
            iat: now,
            exp: now + 3600, // 1 hours validity
        };

        let encoding_key = EncodingKey::from_rsa_pem(creds.private_key.as_bytes())
            .map_err(|e| AuthError::TokenCreation(format!("Invalid private key: {}", e)))?;

        encode(
            &Header::new(jsonwebtoken::Algorithm::RS256),
            &claims,
            &encoding_key,
        )
        .map_err(|e| AuthError::TokenCreation(format!("Failed to create JWT: {}", e)))
    }

    /// Exchanges a token or assertion for an access token.
    ///
    /// # Arguments
    /// * `token_uri` - The token exchange endpoint
    /// * `params` - Parameters for the token exchange request
    ///
    /// # Returns
    /// * `Result<TokenResponse>` - The token exchange response
    async fn exchange_token(
        &self,
        token_uri: &str,
        params: &[(&str, &str)],
    ) -> Result<TokenResponse, AuthError> {
        let response = self
            .client
            .post(token_uri)
            .form(params)
            .send()
            .await
            .map_err(|e| AuthError::TokenExchange(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AuthError::TokenExchange(format!(
                "Status {}: {}",
                status, error_text
            )));
        }

        response
            .json::<TokenResponse>()
            .await
            .map_err(|e| AuthError::TokenExchange(format!("Invalid response: {}", e)))
    }

    /// Gets a token using service account credentials.
    ///
    /// # Arguments
    /// * `creds` - Service account credentials
    ///
    /// # Returns
    /// * `Result<TokenResponse>` - The token response
    async fn get_service_account_token(
        &self,
        creds: &ServiceAccountCredentials,
    ) -> Result<TokenResponse, AuthError> {
        let jwt = self.create_jwt_token(creds)?;
        let params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &jwt),
            ("scope", "https://www.googleapis.com/auth/cloud-platform"),
        ];

        self.exchange_token(&creds.token_uri, &params).await
    }

    /// Gets a token using authorized user credentials.
    ///
    /// # Arguments
    /// * `creds` - Authorized user credentials
    ///
    /// # Returns
    /// * `Result<TokenResponse>` - The token response
    async fn get_authorized_user_token(
        &self,
        creds: &AuthorizedUserCredentials,
    ) -> Result<TokenResponse, AuthError> {
        let params = [
            ("client_id", creds.client_id.as_str()),
            ("client_secret", creds.client_secret.as_str()),
            ("refresh_token", creds.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
            ("scope", "https://www.googleapis.com/auth/cloud-platform"),
        ];

        self.exchange_token(&creds.token_uri, &params).await
    }

    /// Gets a token directly from the GCP metadata endpoint.
    ///
    /// # Arguments
    /// * `base_url` - Default Access Token base url
    ///
    /// # Returns
    /// * `Result<TokenResponse>` - The token response
    async fn get_default_access_token(&self, base_url: &str) -> Result<TokenResponse, AuthError> {
        let metadata_path = "/computeMetadata/v1/instance/service-accounts/default/token";
        let response = self
            .client
            .get(format!("{}{}", base_url, metadata_path))
            .header("Metadata-Flavor", "Google")
            .send()
            .await
            .map_err(|e| AuthError::TokenExchange(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AuthError::TokenExchange(format!(
                "Status {}: {}",
                status, error_text
            )));
        }

        response
            .json::<TokenResponse>()
            .await
            .map_err(|e| AuthError::TokenExchange(format!("Invalid response: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate::eq;
    use tokio::time::sleep;
    use wiremock::matchers::{header, method, path};
    // Only import what we need
    use wiremock::{Mock, MockServer, ResponseTemplate};

    mockall::mock! {
        #[derive(Debug)]
        FilesystemOpsMock {}

        #[async_trait]
        impl FilesystemOps for FilesystemOpsMock {
            async fn read_to_string(&self, path: String) -> Result<String, std::io::Error>;
        }
    }

    mockall::mock! {
        #[derive(Debug)]
        EnvOpsMock {}

        impl EnvOps for EnvOpsMock {
            fn get_var(&self, key: &str) -> Result<String, env::VarError>;
        }
    }

    struct TestContext {
        fs_mock: MockFilesystemOpsMock,
        env_mock: MockEnvOpsMock,
        mock_server: Option<MockServer>,
    }

    impl TestContext {
        fn new() -> Self {
            Self {
                fs_mock: MockFilesystemOpsMock::new(),
                env_mock: MockEnvOpsMock::new(),
                mock_server: None,
            }
        }

        async fn with_metadata_server(mut self) -> Self {
            self.mock_server = Some(MockServer::start().await);
            self
        }
    }

    // Test fixtures for credentials
    fn mock_service_account() -> ServiceAccountCredentials {
        ServiceAccountCredentials {
            client_email: "test@test.com".to_string(),
            // This is a generated test credential
            private_key: "-----BEGIN RSA PRIVATE KEY-----
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
-----END RSA PRIVATE KEY-----"
                .to_string(),
            token_uri: "https://oauth2.googleapis.com/token".to_string(),
        }
    }

    fn mock_authorized_user() -> AuthorizedUserCredentials {
        AuthorizedUserCredentials {
            client_id: "test_client".to_string(),
            client_secret: "test_secret".to_string(),
            refresh_token: "test_refresh".to_string(),
            token_uri: "https://oauth2.googleapis.com/token".to_string(),
        }
    }

    // Helper function to create a test GcpAuth instance with credentials
    async fn create_test_auth_with_creds(creds: AdcCredentials) -> GcpAuth {
        GcpAuth {
            credentials: creds,
            client: reqwest::Client::new(),
            cached_token: Arc::new(RwLock::new(None)),
        }
    }

    #[tokio::test]
    async fn test_token_caching() {
        let auth = GcpAuth {
            credentials: AdcCredentials::ServiceAccount(mock_service_account()),
            client: reqwest::Client::new(),
            cached_token: Arc::new(RwLock::new(Some(CachedToken {
                token: AuthToken {
                    token_type: "Bearer".to_string(),
                    token_value: "cached_token".to_string(),
                },
                expires_at: Instant::now() + Duration::from_secs(3600),
            }))),
        };

        // First call should return cached token
        let token1 = auth.get_token().await.unwrap();
        assert_eq!(token1.token_value, "cached_token");

        // Second call should return same cached token
        let token2 = auth.get_token().await.unwrap();
        assert_eq!(token2.token_value, "cached_token");
    }

    #[tokio::test]
    async fn test_token_expiration() {
        let auth = GcpAuth {
            credentials: AdcCredentials::ServiceAccount(mock_service_account()),
            client: reqwest::Client::new(),
            cached_token: Arc::new(RwLock::new(Some(CachedToken {
                token: AuthToken {
                    token_type: "Bearer".to_string(),
                    token_value: "expired_token".to_string(),
                },
                expires_at: Instant::now() - Duration::from_secs(1),
            }))),
        };

        // Should fail as token is expired and real credentials aren't available
        let result = auth.get_token().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invalid_credentials() {
        let auth = create_test_auth_with_creds(AdcCredentials::ServiceAccount(
            ServiceAccountCredentials {
                client_email: "".to_string(),
                private_key: "invalid".to_string(),
                token_uri: "https://invalid.example.com".to_string(),
            },
        ))
        .await;

        let result = auth.get_token().await;
        assert!(result.is_err());
        match result {
            Err(AuthError::TokenCreation(_)) => (),
            _ => panic!("Expected TokenCreationError"),
        }
    }

    #[tokio::test]
    async fn test_concurrent_token_access() {
        let auth = Arc::new(GcpAuth {
            credentials: AdcCredentials::ServiceAccount(mock_service_account()),
            client: reqwest::Client::new(),
            cached_token: Arc::new(RwLock::new(Some(CachedToken {
                token: AuthToken {
                    token_type: "Bearer".to_string(),
                    token_value: "concurrent_token".to_string(),
                },
                expires_at: Instant::now() + Duration::from_secs(3600),
            }))),
        });

        let mut handles = vec![];

        // Spawn multiple concurrent token requests
        for _ in 0..10 {
            let auth_clone = Arc::clone(&auth);
            handles.push(tokio::spawn(async move {
                auth_clone.get_token().await.unwrap()
            }));
        }

        // All requests should return the same cached token
        for handle in handles {
            let token = handle.await.unwrap();
            assert_eq!(token.token_value, "concurrent_token");
        }
    }

    #[tokio::test]
    async fn test_token_refresh_race_condition() {
        let auth = Arc::new(GcpAuth {
            credentials: AdcCredentials::ServiceAccount(mock_service_account()),
            client: reqwest::Client::new(),
            cached_token: Arc::new(RwLock::new(Some(CachedToken {
                token: AuthToken {
                    token_type: "Bearer".to_string(),
                    token_value: "about_to_expire".to_string(),
                },
                expires_at: Instant::now() + Duration::from_millis(100),
            }))),
        });

        let mut handles = vec![];

        for i in 0..5 {
            let auth_clone = Arc::clone(&auth);
            handles.push(tokio::spawn(async move {
                sleep(Duration::from_millis(i * 50)).await;
                let result = auth_clone.get_token().await;
                match result {
                    Ok(token) => {
                        // Should be the cached token since we can't actually exchange tokens in tests
                        assert_eq!(
                            token.token_value, "about_to_expire",
                            "Expected cached token, got: {}",
                            token.token_value
                        );
                    }
                    Err(e) => {
                        match e {
                            AuthError::TokenExchange(err) => {
                                // This is expected - we can't actually exchange tokens in tests
                                assert!(
                                    err.contains("invalid_scope") || err.contains("400"),
                                    "Unexpected error message: {}",
                                    err
                                );
                            }
                            other => panic!("Unexpected error type: {:?}", other),
                        }
                    }
                }
            }));
        }

        // Wait for all handles
        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_authorized_user_token() {
        let auth = GcpAuth {
            credentials: AdcCredentials::AuthorizedUser(mock_authorized_user()),
            client: reqwest::Client::new(),
            cached_token: Arc::new(RwLock::new(None)),
        };

        // This should fail since we can't actually make the token exchange request
        let result = auth.get_token().await;
        assert!(result.is_err());
        match result {
            Err(AuthError::TokenExchange(_)) => (),
            _ => panic!("Expected TokenExchangeError"),
        }
    }

    #[tokio::test]
    async fn test_service_account_jwt_creation() {
        let auth = GcpAuth {
            credentials: AdcCredentials::ServiceAccount(mock_service_account()),
            client: reqwest::Client::new(),
            cached_token: Arc::new(RwLock::new(None)),
        };

        let jwt = auth.create_jwt_token(&mock_service_account());
        assert!(jwt.is_ok(), "JWT creation failed: {:?}", jwt.err());
        let jwt_str = jwt.unwrap();
        assert!(jwt_str.starts_with("ey"), "JWT should start with 'ey'");
        assert_eq!(
            jwt_str.matches('.').count(),
            2,
            "JWT should have exactly 2 dots"
        );
    }

    #[tokio::test]
    async fn test_load_from_env_credentials() {
        let mut context = TestContext::new();

        // Mock environment variable
        context
            .env_mock
            .expect_get_var()
            .with(eq("GOOGLE_APPLICATION_CREDENTIALS"))
            .times(1)
            .return_once(|_| Ok("/path/to/credentials.json".to_string()));

        // Mock file content - convert &str to String for comparison
        let creds_content = r#"{
            "type": "service_account",
            "client_email": "test@example.com",
            "private_key": "-----BEGIN PRIVATE KEY-----\nMIIE...test...key\n-----END PRIVATE KEY-----\n",
            "token_uri": "https://oauth2.googleapis.com/token"
        }"#;

        context
            .fs_mock
            .expect_read_to_string()
            .with(eq("/path/to/credentials.json".to_string())) // Convert to String
            .times(1)
            .return_once(move |_| Ok(creds_content.to_string()));

        let result = AdcCredentials::load_impl(
            &context.fs_mock,
            &context.env_mock,
            "http://metadata.example.com",
        )
        .await;

        assert!(result.is_ok());
        if let Ok(AdcCredentials::ServiceAccount(sa)) = result {
            assert_eq!(sa.client_email, "test@example.com");
            assert!(sa.private_key.contains("test...key"));
        } else {
            panic!("Expected ServiceAccount credentials");
        }
    }

    #[tokio::test]
    async fn test_load_from_default_path() {
        let mut context = TestContext::new();

        // Mock environment variables
        context
            .env_mock
            .expect_get_var()
            .with(eq("GOOGLE_APPLICATION_CREDENTIALS"))
            .times(1)
            .return_once(|_| Err(env::VarError::NotPresent));

        let home_var = if cfg!(windows) { "APPDATA" } else { "HOME" };
        context
            .env_mock
            .expect_get_var()
            .with(eq(home_var))
            .times(1)
            .return_once(|_| Ok("/home/testuser".to_string()));

        // Mock file content
        let creds_content = r#"{
        "type": "authorized_user",
        "client_id": "test_client",
        "client_secret": "test_secret",
        "refresh_token": "test_refresh"
    }"#;

        let expected_path = if cfg!(windows) {
            "/home/testuser/gcloud/application_default_credentials.json".to_string()
        } else {
            "/home/testuser/.config/gcloud/application_default_credentials.json".to_string()
        };

        context
            .fs_mock
            .expect_read_to_string()
            .with(eq(expected_path.clone())) // Use clone() to avoid borrowing issues
            .times(1)
            .return_once(move |_| Ok(creds_content.to_string()));

        let result = AdcCredentials::load_impl(
            &context.fs_mock,
            &context.env_mock,
            "http://metadata.example.com",
        )
        .await;

        assert!(result.is_ok());
        if let Ok(AdcCredentials::AuthorizedUser(au)) = result {
            assert_eq!(au.client_id, "test_client");
            assert_eq!(au.client_secret, "test_secret");
            assert_eq!(au.refresh_token, "test_refresh");
        } else {
            panic!("Expected AuthorizedUser credentials");
        }
    }

    #[tokio::test]
    async fn test_load_from_metadata_server() {
        let mut context = TestContext::new();

        // Mock environment variable lookups to fail
        context
            .env_mock
            .expect_get_var()
            .with(eq("GOOGLE_APPLICATION_CREDENTIALS"))
            .times(1)
            .return_once(|_| Err(env::VarError::NotPresent));

        let home_var = if cfg!(windows) { "APPDATA" } else { "HOME" };
        context
            .env_mock
            .expect_get_var()
            .with(eq(home_var))
            .times(1)
            .return_once(|_| Err(env::VarError::NotPresent));

        // Initialize mock server
        let context = context.with_metadata_server().await;
        let mock_server = context
            .mock_server
            .as_ref()
            .expect("Mock server should be initialized");

        // Define expected token values
        let expected_token = "test_token";
        let expected_type = "Bearer";
        let expected_expires = 3600;

        // Configure mock response
        Mock::given(method("GET"))
            .and(path(
                "/computeMetadata/v1/instance/service-accounts/default/token",
            ))
            .and(header("Metadata-Flavor", "Google"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": expected_token,
                "expires_in": expected_expires,
                "token_type": expected_type,
            })))
            .mount(mock_server)
            .await;

        // Execute the code under test
        let result =
            AdcCredentials::load_impl(&context.fs_mock, &context.env_mock, &mock_server.uri())
                .await;

        // Assertions
        assert!(
            result.is_ok(),
            "Expected successful result, got {:?}",
            result
        );

        if let Ok(AdcCredentials::DefaultAccount(base_url)) = result {
            assert_eq!(base_url, mock_server.uri());
        } else {
            panic!("Expected DefaultAccount credentials, got {:?}", result);
        }
    }

    #[tokio::test]
    async fn test_invalid_credentials_file() {
        let mut context = TestContext::new();

        // Mock GOOGLE_APPLICATION_CREDENTIALS environment variable
        context
            .env_mock
            .expect_get_var()
            .with(eq("GOOGLE_APPLICATION_CREDENTIALS"))
            .times(1)
            .return_once(|_| Ok("/path/to/credentials.json".to_string()));

        // Mock filesystem read for the invalid credentials file
        context
            .fs_mock
            .expect_read_to_string()
            .with(eq("/path/to/credentials.json".to_string()))
            .times(1)
            .return_once(|_| Ok("invalid json".to_string()));

        // Mock HOME/APPDATA environment variable
        let home_var = if cfg!(windows) { "APPDATA" } else { "HOME" };
        context
            .env_mock
            .expect_get_var()
            .with(eq(home_var))
            .times(1)
            .return_once(|_| Ok("/home/user".to_string()));

        // Mock filesystem read for the default credentials path
        let default_creds_path = if cfg!(windows) {
            "/home/user/gcloud/application_default_credentials.json"
        } else {
            "/home/user/.config/gcloud/application_default_credentials.json"
        };
        context
            .fs_mock
            .expect_read_to_string()
            .with(eq(default_creds_path.to_string()))
            .times(1)
            .return_once(|_| {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "File not found",
                ))
            });

        let result = AdcCredentials::load_impl(
            &context.fs_mock,
            &context.env_mock,
            "http://metadata.example.com",
        )
        .await;

        assert!(matches!(result, Err(AuthError::Credentials(_))));
    }

    #[tokio::test]
    async fn test_no_credentials_found() {
        let mut context = TestContext::new();

        // Mock all credential sources to fail
        context
            .env_mock
            .expect_get_var()
            .with(eq("GOOGLE_APPLICATION_CREDENTIALS"))
            .times(1)
            .return_once(|_| Err(env::VarError::NotPresent));

        context
            .env_mock
            .expect_get_var()
            .with(eq(if cfg!(windows) { "APPDATA" } else { "HOME" }))
            .times(1)
            .return_once(|_| Err(env::VarError::NotPresent));

        let result = AdcCredentials::load_impl(
            &context.fs_mock,
            &context.env_mock,
            "http://metadata.example.com",
        )
        .await;
        assert!(matches!(result, Err(AuthError::Credentials(_))));
    }
}
