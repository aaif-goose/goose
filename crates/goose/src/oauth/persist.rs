use rmcp::transport::auth::{
    AuthError, CredentialRefreshGuard, CredentialStore, StoredCredentials,
};
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Serialize, Deserialize)]
struct PersistedCredentials {
    #[serde(flatten)]
    credentials: StoredCredentials,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    requested_scopes: Option<Vec<String>>,
}

/// Goose-specific credential store that uses the Config system
///
/// This implementation stores OAuth credentials in the goose configuration
/// system, which handles secure storage (e.g., keychain integration).

#[derive(Clone)]
pub struct GooseCredentialStore {
    name: String,
    skip_refresh_guard: bool,
}

impl GooseCredentialStore {
    pub fn new(name: String) -> Self {
        Self {
            name,
            skip_refresh_guard: false,
        }
    }

    pub fn without_refresh_guard(self) -> Self {
        Self {
            skip_refresh_guard: true,
            ..self
        }
    }

    fn secret_key(&self) -> String {
        format!("oauth_creds_{}", self.name)
    }

    fn load_persisted(&self) -> Result<Option<PersistedCredentials>, AuthError> {
        let config = Config::global();
        let key = self.secret_key();

        match config.get_secret::<PersistedCredentials>(&key) {
            Ok(credentials) => Ok(Some(credentials)),
            Err(_) => Ok(None),
        }
    }

    pub fn invalidate_cache(&self) {
        Config::global().invalidate_secrets_cache();
    }

    fn save_persisted(&self, credentials: PersistedCredentials) -> Result<(), AuthError> {
        let config = Config::global();
        let key = self.secret_key();

        config
            .set_secret(&key, &credentials)
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
        let config = Config::global();
        let key = self.secret_key();

        config
            .delete_secret(&key)
            .map_err(|e| AuthError::InternalError(format!("Failed to clear credentials: {}", e)))
    }

    async fn acquire_refresh_guard(&self) -> Result<Option<CredentialRefreshGuard>, AuthError> {
        if self.skip_refresh_guard {
            return Ok(None);
        }
        let lock = super::acquire_oauth_flow_lock(&self.name)
            .await
            .map_err(|e| AuthError::CredentialStoreError(e.to_string()))?;
        self.invalidate_cache();
        Ok(Some(CredentialRefreshGuard::new(lock)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials() -> StoredCredentials {
        StoredCredentials::new(
            "client-id".to_string(),
            None,
            vec!["scope.read".to_string()],
            Some(123),
        )
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
    async fn skipped_refresh_guard_does_not_take_the_oauth_lock() {
        let store = GooseCredentialStore::new("Pi Swisssync".to_string()).without_refresh_guard();
        assert!(store.acquire_refresh_guard().await.unwrap().is_none());
    }
}
