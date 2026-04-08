use rmcp::transport::auth::{AuthError, CredentialStore, StoredCredentials};

use crate::config::Config;

/// Goose-specific credential store that uses the Config system
///
/// This implementation stores OAuth credentials in the goose configuration
/// system, which handles secure storage (e.g., keychain integration).

#[derive(Clone)]
pub struct GooseCredentialStore {
    name: String,
}

impl GooseCredentialStore {
    pub fn new(name: String) -> Self {
        Self { name }
    }

    fn secret_key(&self) -> String {
        format!("oauth_creds_{}", self.name)
    }
}

#[async_trait::async_trait]
impl CredentialStore for GooseCredentialStore {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        let config = Config::global();
        let key = self.secret_key();

        match config.get_secret::<StoredCredentials>(&key) {
            Ok(credentials) => Ok(Some(credentials)),
            Err(_) => Ok(None), // No credentials found
        }
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        let config = Config::global();
        let key = self.secret_key();

        config
            .set_secret(&key, &credentials)
            .map_err(|e| AuthError::InternalError(format!("Failed to save credentials: {}", e)))
    }

    async fn clear(&self) -> Result<(), AuthError> {
        let config = Config::global();
        let key = self.secret_key();

        config
            .delete_secret(&key)
            .map_err(|e| AuthError::InternalError(format!("Failed to clear credentials: {}", e)))
    }
}
