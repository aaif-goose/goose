use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::time::Duration;

use fs2::FileExt;
use oauth2::TokenResponse;
use rmcp::transport::auth::{
    AuthError, CredentialRefreshGuard, CredentialStore, OAuthTokenResponse, StoredCredentials,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::time::Instant;
use tracing::warn;

use crate::config::base::SecretUpdate;
use crate::config::paths::Paths;
use crate::config::{Config, ConfigError};

/// A refresh lock is only held across one token exchange.
const REFRESH_LOCK_WAIT: Duration = Duration::from_secs(60);
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Serialize, Deserialize)]
struct PersistedCredentials {
    #[serde(flatten)]
    credentials: StoredCredentials,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    requested_scopes: Option<Vec<String>>,
}

/// OAuth credentials for one MCP extension, persisted through the goose config
/// system (keychain or secrets file) and shared by every goose process.
///
/// Refreshes and authorization flows are coordinated across processes with
/// per-extension file locks, always taken in the order authorize, refresh,
/// secrets, so a rotating refresh token is spent once and a refresh rejected
/// by the server can only clear the credentials it was made with.
#[derive(Clone)]
pub struct GooseCredentialStore {
    name: String,
    config: &'static Config,
    lock_dir: PathBuf,
}

impl GooseCredentialStore {
    pub fn new(name: String) -> Self {
        Self {
            name,
            config: Config::global(),
            lock_dir: Paths::in_config_dir("oauth"),
        }
    }

    fn secret_key(&self) -> String {
        format!("oauth_creds_{}", self.name)
    }

    fn load_persisted(&self) -> Result<Option<PersistedCredentials>, AuthError> {
        match self
            .config
            .get_secret::<PersistedCredentials>(&self.secret_key())
        {
            Ok(credentials) => Ok(Some(credentials)),
            Err(_) => Ok(None),
        }
    }

    fn save_persisted(&self, credentials: PersistedCredentials) -> Result<(), AuthError> {
        self.config
            .set_secret(&self.secret_key(), &credentials)
            .map_err(|e| AuthError::InternalError(format!("Failed to save credentials: {}", e)))
    }

    pub fn load_requested_scopes(&self) -> Result<Option<Vec<String>>, AuthError> {
        Ok(self
            .load_persisted()?
            .and_then(|credentials| credentials.requested_scopes))
    }

    pub fn save_with_requested_scopes(
        &self,
        credentials: StoredCredentials,
        requested_scopes: Option<Vec<String>>,
    ) -> Result<(), AuthError> {
        self.save_persisted(PersistedCredentials {
            credentials,
            requested_scopes,
        })
    }

    /// Waits up to `wait` until no other task or process runs an authorization
    /// flow for this extension. Dropping the returned file releases the lock.
    pub(crate) async fn lock_authorization(&self, wait: Duration) -> Result<File, AuthError> {
        self.lock("authorize", wait).await
    }

    /// Saves a grant obtained by a new authorization. Holding the refresh lock
    /// keeps a refresh that started from the previous grant from overwriting it.
    pub(crate) async fn save_new_grant(
        &self,
        credentials: StoredCredentials,
        requested_scopes: Option<Vec<String>>,
    ) -> Result<(), AuthError> {
        let _refresh_lock = self.lock("refresh", REFRESH_LOCK_WAIT).await?;
        self.save_with_requested_scopes(credentials, requested_scopes)
    }

    /// Replaces the stored token response with `token_response` only while the
    /// store still holds its access token, so a grant another process saved
    /// in the meantime is kept.
    pub(crate) fn replace_token_response(
        &self,
        token_response: OAuthTokenResponse,
    ) -> Result<(), AuthError> {
        let access_token = token_response.access_token().secret().clone();
        self.config
            .update_secret::<Option<PersistedCredentials>, PersistedCredentials, ()>(
                &self.secret_key(),
                |current| match current {
                    Some(mut persisted)
                        if access_token_of(&persisted.credentials)
                            == Some(access_token.as_str()) =>
                    {
                        persisted.credentials.token_response = Some(token_response);
                        SecretUpdate::Write(persisted, ())
                    }
                    _ => SecretUpdate::Unchanged(()),
                },
            )
            .map_err(|e| AuthError::InternalError(format!("Failed to save credentials: {}", e)))
    }

    /// Takes the named per-extension lock, then makes sure the next load sees
    /// what other processes saved while we waited.
    async fn lock(&self, purpose: &str, wait: Duration) -> Result<File, AuthError> {
        let lock = lock_exclusive(
            self.lock_dir
                .join(format!("{}.{purpose}.lock", lock_file_stem(&self.name))),
            wait,
        )
        .await?;
        self.discard_stale_cache();
        Ok(lock)
    }

    /// Drops the in-process secrets cache only when storage holds different
    /// credentials, since dropping it also makes this process rebuild its
    /// providers.
    fn discard_stale_cache(&self) {
        let key = self.secret_key();
        let stored = match self.config.get_secret_uncached::<serde_json::Value>(&key) {
            Ok(value) => Some(value),
            Err(ConfigError::NotFound(_)) => None,
            Err(e) => {
                warn!(
                    "[OAuth:{}] Could not re-read stored credentials: {}",
                    self.name, e
                );
                return;
            }
        };
        if self.config.get_secret::<serde_json::Value>(&key).ok() != stored {
            self.config.invalidate_secrets_cache();
        }
    }

    /// Deletes the stored credentials if they still hold `rejected`'s refresh
    /// token. Returns whether they were deleted.
    fn clear_if_refresh_token_unchanged(
        &self,
        rejected: &StoredCredentials,
    ) -> Result<bool, AuthError> {
        let rejected_refresh_token = refresh_token_of(rejected);
        self.config
            .update_secret::<Option<PersistedCredentials>, PersistedCredentials, bool>(
                &self.secret_key(),
                |current| match current {
                    Some(persisted)
                        if refresh_token_of(&persisted.credentials) == rejected_refresh_token =>
                    {
                        SecretUpdate::Delete(true)
                    }
                    _ => SecretUpdate::Unchanged(false),
                },
            )
            .map_err(|e| {
                AuthError::CredentialStoreError(format!("Failed to clear credentials: {e}"))
            })
    }
}

/// A readable, length-bounded file name that stays unique per extension name.
fn lock_file_stem(name: &str) -> String {
    let readable: String = name
        .chars()
        .take(64)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let digest = Sha256::digest(name.as_bytes());
    let hash: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
    format!("{readable}_{hash}")
}

/// Polls rather than blocking a thread, so a caller that gives up (or is
/// cancelled) never leaves a waiter behind, and a holder that hangs cannot
/// stall every other session indefinitely.
async fn lock_exclusive(path: PathBuf, wait: Duration) -> Result<File, AuthError> {
    let lock_error = |e: std::io::Error| {
        AuthError::CredentialStoreError(format!("Failed to lock {}: {e}", path.display()))
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(lock_error)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(lock_error)?;
    let contended = fs2::lock_contended_error().raw_os_error();
    let deadline = Instant::now() + wait;
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(file),
            Err(e) if e.raw_os_error() == contended => {}
            Err(e) => return Err(lock_error(e)),
        }
        if Instant::now() >= deadline {
            return Err(AuthError::CredentialStoreError(format!(
                "Timed out after {}s waiting for {}",
                wait.as_secs(),
                path.display()
            )));
        }
        tokio::time::sleep(LOCK_POLL_INTERVAL).await;
    }
}

fn access_token_of(credentials: &StoredCredentials) -> Option<&str> {
    credentials
        .token_response
        .as_ref()
        .map(|token| token.access_token().secret().as_str())
}

fn refresh_token_of(credentials: &StoredCredentials) -> Option<&str> {
    credentials
        .token_response
        .as_ref()
        .and_then(|token| token.refresh_token())
        .map(|token| token.secret().as_str())
}

#[async_trait::async_trait]
impl CredentialStore for GooseCredentialStore {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        Ok(self
            .load_persisted()?
            .map(|credentials| credentials.credentials))
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        let requested_scopes = self.load_requested_scopes()?;
        self.save_with_requested_scopes(credentials, requested_scopes)
    }

    async fn clear(&self) -> Result<(), AuthError> {
        self.config
            .delete_secret(&self.secret_key())
            .map_err(|e| AuthError::InternalError(format!("Failed to clear credentials: {}", e)))
    }

    async fn on_refresh_token_rejected(
        &self,
        credentials: &StoredCredentials,
    ) -> Result<(), AuthError> {
        if self.clear_if_refresh_token_unchanged(credentials)? {
            warn!(
                "[OAuth:{}] Refresh token was rejected; cleared stored credentials",
                self.name
            );
        }
        Ok(())
    }

    async fn acquire_refresh_guard(&self) -> Result<Option<CredentialRefreshGuard>, AuthError> {
        Ok(Some(CredentialRefreshGuard::new(
            self.lock("refresh", REFRESH_LOCK_WAIT).await?,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oauth2::basic::BasicTokenType;
    use oauth2::{AccessToken, RefreshToken};
    use rmcp::transport::auth::VendorExtraTokenFields;
    use tempfile::TempDir;

    const TEST_WAIT: Duration = Duration::from_secs(5);

    fn credentials() -> StoredCredentials {
        StoredCredentials::new(
            "client-id".to_string(),
            None,
            vec!["scope.read".to_string()],
            Some(123),
        )
    }

    fn grant(access_token: &str, refresh_token: &str) -> StoredCredentials {
        let mut token_response = OAuthTokenResponse::new(
            AccessToken::new(access_token.to_string()),
            BasicTokenType::Bearer,
            VendorExtraTokenFields::default(),
        );
        token_response.set_refresh_token(Some(RefreshToken::new(refresh_token.to_string())));
        StoredCredentials::new("client-id".to_string(), Some(token_response), vec![], None)
    }

    fn test_store(directory: &TempDir, name: &str) -> GooseCredentialStore {
        let config = Config::new_with_file_secrets(
            directory.path().join("config.yaml"),
            directory.path().join("secrets.yaml"),
        )
        .unwrap();
        GooseCredentialStore {
            name: name.to_string(),
            config: Box::leak(Box::new(config)),
            lock_dir: directory.path().join("oauth"),
        }
    }

    fn stored_access_token(store: &GooseCredentialStore) -> Option<String> {
        store
            .load_persisted()
            .unwrap()
            .and_then(|persisted| access_token_of(&persisted.credentials).map(str::to_string))
    }

    #[test]
    fn persisted_credentials_read_the_legacy_shape() {
        let legacy = serde_json::to_value(credentials()).unwrap();
        let persisted: PersistedCredentials = serde_json::from_value(legacy).unwrap();

        assert_eq!(persisted.credentials.client_id, "client-id");
        assert_eq!(persisted.credentials.granted_scopes, vec!["scope.read"]);
        assert_eq!(persisted.requested_scopes, None);
    }

    #[test]
    fn persisted_credentials_remain_readable_as_stored_credentials() {
        let persisted = PersistedCredentials {
            credentials: credentials(),
            requested_scopes: Some(vec!["scope.read".to_string(), "scope.write".to_string()]),
        };
        let value = serde_json::to_value(persisted).unwrap();
        let credentials: StoredCredentials = serde_json::from_value(value).unwrap();

        assert_eq!(credentials.client_id, "client-id");
        assert_eq!(credentials.granted_scopes, vec!["scope.read"]);
    }

    #[tokio::test]
    async fn rejected_refresh_token_clears_the_grant_it_came_from() {
        let directory = TempDir::new().unwrap();
        let store = test_store(&directory, "ext");
        store.save(grant("access-1", "refresh-1")).await.unwrap();

        store
            .on_refresh_token_rejected(&grant("access-1", "refresh-1"))
            .await
            .unwrap();

        assert!(store.load().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn rejected_refresh_token_keeps_a_successor_grant() {
        let directory = TempDir::new().unwrap();
        let store = test_store(&directory, "ext");
        store.save(grant("access-2", "refresh-2")).await.unwrap();

        store
            .on_refresh_token_rejected(&grant("access-1", "refresh-1"))
            .await
            .unwrap();

        assert_eq!(stored_access_token(&store).as_deref(), Some("access-2"));
    }

    #[tokio::test]
    async fn token_response_is_replaced_only_while_its_access_token_is_stored() {
        let directory = TempDir::new().unwrap();
        let store = test_store(&directory, "ext");
        store.save(grant("access-2", "refresh-2")).await.unwrap();

        let stale = grant("access-1", "refresh-1").token_response.unwrap();
        store.replace_token_response(stale).unwrap();
        assert_eq!(stored_access_token(&store).as_deref(), Some("access-2"));

        let mut current = grant("access-2", "refresh-2").token_response.unwrap();
        current.set_refresh_token(Some(RefreshToken::new("refresh-2b".to_string())));
        store.replace_token_response(current).unwrap();
        let stored = store.load().await.unwrap().unwrap();
        assert_eq!(refresh_token_of(&stored), Some("refresh-2b"));
    }

    #[tokio::test]
    async fn lock_sees_credentials_another_process_saved() {
        let directory = TempDir::new().unwrap();
        let first = test_store(&directory, "ext");
        let second = test_store(&directory, "ext");
        first.save(grant("access-1", "refresh-1")).await.unwrap();
        assert_eq!(stored_access_token(&first).as_deref(), Some("access-1"));

        let generation = first.config.generation();
        drop(first.lock_authorization(TEST_WAIT).await.unwrap());
        assert_eq!(
            first.config.generation(),
            generation,
            "an unchanged store must not make this process rebuild its providers"
        );

        second.save(grant("access-2", "refresh-2")).await.unwrap();
        assert_eq!(stored_access_token(&first).as_deref(), Some("access-1"));
        let _lock = first.lock_authorization(TEST_WAIT).await.unwrap();
        assert_eq!(stored_access_token(&first).as_deref(), Some("access-2"));
    }

    /// A token endpoint that rotates refresh tokens: each one is accepted once,
    /// and any reuse is answered with `invalid_grant`.
    struct RotatingTokenEndpoint {
        state: std::sync::Mutex<(u32, u32)>,
    }

    impl RotatingTokenEndpoint {
        fn issued_and_rejected(&self) -> (u32, u32) {
            *self.state.lock().unwrap()
        }
    }

    impl wiremock::Respond for &'static RotatingTokenEndpoint {
        fn respond(&self, request: &wiremock::Request) -> wiremock::ResponseTemplate {
            let presented = url::form_urlencoded::parse(&request.body)
                .find(|(key, _)| key == "refresh_token")
                .map(|(_, value)| value.into_owned());
            let mut state = self.state.lock().unwrap();
            let (issued, rejected) = &mut *state;
            if presented.as_deref() != Some(format!("refresh-{issued}").as_str()) {
                *rejected += 1;
                return wiremock::ResponseTemplate::new(400)
                    .set_body_json(serde_json::json!({ "error": "invalid_grant" }));
            }
            *issued += 1;
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": format!("access-{issued}"),
                "refresh_token": format!("refresh-{issued}"),
                "token_type": "bearer",
                "expires_in": 3600,
            }))
        }
    }

    #[tokio::test]
    async fn concurrent_refreshes_spend_a_rotating_refresh_token_once() {
        use rmcp::transport::auth::AuthorizationMetadata;
        use rmcp::transport::AuthorizationManager;

        const SESSIONS: u32 = 6;
        let server = wiremock::MockServer::start().await;
        let endpoint: &'static RotatingTokenEndpoint = Box::leak(Box::new(RotatingTokenEndpoint {
            state: std::sync::Mutex::new((0, 0)),
        }));
        wiremock::Mock::given(wiremock::matchers::path("/token"))
            .respond_with(endpoint)
            .mount(&server)
            .await;

        let directory = TempDir::new().unwrap();
        let store = test_store(&directory, "rotating");
        store.save(grant("access-0", "refresh-0")).await.unwrap();

        let mut sessions = Vec::new();
        for _ in 0..SESSIONS {
            let mut manager = AuthorizationManager::new(server.uri()).await.unwrap();
            manager.with_client(reqwest::Client::new()).unwrap();
            let mut metadata = AuthorizationMetadata::default();
            metadata.authorization_endpoint = format!("{}/authorize", server.uri());
            metadata.token_endpoint = format!("{}/token", server.uri());
            manager.set_metadata(metadata);
            manager.configure_client_id("client-id").unwrap();
            manager.set_credential_store(store.clone());
            sessions.push(tokio::spawn(async move { manager.refresh_token().await }));
        }
        for session in sessions {
            session
                .await
                .unwrap()
                .expect("every serialized refresh succeeds");
        }

        assert_eq!(endpoint.issued_and_rejected(), (SESSIONS, 0));
        let stored = store
            .load()
            .await
            .unwrap()
            .expect("the newest grant is kept");
        assert_eq!(
            refresh_token_of(&stored),
            Some(format!("refresh-{SESSIONS}").as_str())
        );
    }

    #[test]
    fn lock_file_stem_is_bounded_and_distinguishes_similar_names() {
        let long = "x".repeat(500);
        assert!(lock_file_stem(&long).len() <= 64 + 1 + 16);
        assert_ne!(
            lock_file_stem("Pi Swisssync"),
            lock_file_stem("Pi/Swisssync")
        );
        assert!(lock_file_stem("Pi Swisssync").starts_with("Pi_Swisssync_"));
    }

    #[tokio::test]
    async fn authorization_lock_is_exclusive_across_handles() {
        let directory = TempDir::new().unwrap();
        let store = test_store(&directory, "ext");
        let held = store.lock_authorization(TEST_WAIT).await.unwrap();

        let contender = OpenOptions::new()
            .read(true)
            .write(true)
            .open(
                directory
                    .path()
                    .join("oauth")
                    .join(format!("{}.authorize.lock", lock_file_stem("ext"))),
            )
            .unwrap();
        assert!(contender.try_lock_exclusive().is_err());

        drop(held);
        assert!(contender.try_lock_exclusive().is_ok());
    }

    #[tokio::test]
    async fn lock_wait_is_bounded_while_another_holder_keeps_it() {
        let directory = TempDir::new().unwrap();
        let store = test_store(&directory, "ext");
        let _held = store.lock_authorization(TEST_WAIT).await.unwrap();

        let error = store
            .lock_authorization(Duration::from_millis(250))
            .await
            .unwrap_err();

        assert!(matches!(error, AuthError::CredentialStoreError(_)));
        assert!(error.to_string().contains("Timed out"));
    }
}
