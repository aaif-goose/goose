use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::paths::Paths;
use crate::config::{Config, ConfigError};
use crate::providers::base::{ProviderMetadata, ProviderType};
use crate::providers::huggingface_auth;

pub const SECRET_STORE_ID_PREFIX: &str = "secret_store:";
pub const PROVIDER_CACHE_ID_PREFIX: &str = "provider_cache:";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSecretStorage {
    SecretStore,
    ProviderCache,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSecretStatus {
    Valid,
    Expired,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSecret {
    pub id: String,
    pub provider: String,
    pub provider_display_name: String,
    pub name: String,
    pub storage: ProviderSecretStorage,
    pub expires_at: Option<DateTime<Utc>>,
    pub status: ProviderSecretStatus,
    pub configured: bool,
    pub has_secret: bool,
    pub can_delete: bool,
    pub can_configure: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configure_provider: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DeleteProviderSecretError {
    #[error("Invalid provider secret id: '{0}'")]
    InvalidId(String),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

fn provider_secret_status(expires_at: Option<DateTime<Utc>>) -> ProviderSecretStatus {
    match expires_at {
        Some(expires_at) if expires_at <= Utc::now() => ProviderSecretStatus::Expired,
        Some(_) => ProviderSecretStatus::Valid,
        None => ProviderSecretStatus::Unknown,
    }
}

fn parse_expiry_value(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::String(value) => DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|dt| dt.with_timezone(&Utc)),
        Value::Number(value) => value
            .as_i64()
            .and_then(|timestamp| Utc.timestamp_opt(timestamp, 0).single()),
        _ => None,
    }
}

fn find_expires_at(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::Object(map) => {
            if map
                .get("refresh_token")
                .and_then(Value::as_str)
                .is_some_and(|token| !token.is_empty())
            {
                return None;
            }
            if let Some(expires_at) = map.get("expires_at").and_then(parse_expiry_value) {
                return Some(expires_at);
            }
            if let Some(expires_at) = map.get("expires_on").and_then(parse_expiry_value) {
                return Some(expires_at);
            }
            map.values().find_map(find_expires_at)
        }
        Value::Array(values) => values.iter().find_map(find_expires_at),
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct ProviderCacheSecretDefinition {
    provider: &'static str,
    name: &'static str,
    path: &'static str,
    is_directory: bool,
}

const PROVIDER_CACHE_SECRET_DEFINITIONS: &[ProviderCacheSecretDefinition] = &[
    ProviderCacheSecretDefinition {
        provider: "gemini_oauth",
        name: "OAuth token",
        path: "gemini_oauth/tokens.json",
        is_directory: false,
    },
    ProviderCacheSecretDefinition {
        provider: "chatgpt_codex",
        name: "OAuth token",
        path: "chatgpt_codex/tokens.json",
        is_directory: false,
    },
    ProviderCacheSecretDefinition {
        provider: "kimi_code",
        name: "OAuth token",
        path: "kimicode/token.json",
        is_directory: false,
    },
    ProviderCacheSecretDefinition {
        provider: "github_copilot",
        name: "OAuth token",
        path: "githubcopilot",
        is_directory: true,
    },
    ProviderCacheSecretDefinition {
        provider: "xai_oauth",
        name: "OAuth token",
        path: "xai_oauth/tokens.json",
        is_directory: false,
    },
    ProviderCacheSecretDefinition {
        provider: "databricks",
        name: "OAuth token",
        path: "databricks/oauth",
        is_directory: true,
    },
    ProviderCacheSecretDefinition {
        provider: "databricks_v2",
        name: "OAuth token",
        path: "databricks/oauth",
        is_directory: true,
    },
];

fn provider_cache_definitions_for_display() -> Vec<ProviderCacheSecretDefinition> {
    let mut seen_paths = HashSet::new();
    PROVIDER_CACHE_SECRET_DEFINITIONS
        .iter()
        .copied()
        .filter(|definition| seen_paths.insert(definition.path))
        .collect()
}

fn provider_cache_definition(provider: &str) -> Option<ProviderCacheSecretDefinition> {
    PROVIDER_CACHE_SECRET_DEFINITIONS
        .iter()
        .copied()
        .find(|definition| definition.provider == provider)
}

fn provider_cache_providers_sharing_cache(provider: &str) -> Vec<&'static str> {
    let Some(definition) = provider_cache_definition(provider) else {
        return Vec::new();
    };

    PROVIDER_CACHE_SECRET_DEFINITIONS
        .iter()
        .filter(|other| other.path == definition.path)
        .map(|definition| definition.provider)
        .collect()
}

fn read_json_file(path: &Path) -> Option<Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
}

fn collect_json_expiries(path: &Path, is_directory: bool) -> Vec<DateTime<Utc>> {
    if !is_directory {
        return read_json_file(path)
            .and_then(|value| find_expires_at(&value))
            .into_iter()
            .collect();
    }

    let mut expiries = Vec::new();
    let mut stack = vec![path.to_path_buf()];

    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(current) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            if let Some(expires_at) =
                read_json_file(&path).and_then(|value| find_expires_at(&value))
            {
                expiries.push(expires_at);
            }
        }
    }

    expiries
}

fn provider_cache_exists(path: &Path, is_directory: bool) -> bool {
    if !is_directory {
        return path.is_file();
    }

    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };

    entries.flatten().any(|entry| {
        let path = entry.path();
        path.is_file() || provider_cache_exists(&path, true)
    })
}

fn provider_cache_expiry(definition: ProviderCacheSecretDefinition) -> Option<DateTime<Utc>> {
    let path = Paths::in_config_dir(definition.path);
    collect_json_expiries(&path, definition.is_directory)
        .into_iter()
        .min()
}

fn build_provider_cache_secret(
    definition: ProviderCacheSecretDefinition,
    display_names: &HashMap<String, String>,
) -> Option<ProviderSecret> {
    let path = Paths::in_config_dir(definition.path);
    if !provider_cache_exists(&path, definition.is_directory) {
        return None;
    }

    let expires_at = provider_cache_expiry(definition);
    Some(ProviderSecret {
        id: format!("{}{}", PROVIDER_CACHE_ID_PREFIX, definition.provider),
        provider: definition.provider.to_string(),
        provider_display_name: display_names
            .get(definition.provider)
            .cloned()
            .unwrap_or_else(|| definition.provider.to_string()),
        name: definition.name.to_string(),
        storage: ProviderSecretStorage::ProviderCache,
        expires_at,
        status: provider_secret_status(expires_at),
        configured: true,
        has_secret: true,
        can_delete: true,
        can_configure: false,
        configure_provider: None,
    })
}

fn build_huggingface_oauth_secret(
    token: Option<huggingface_auth::HuggingFaceTokenData>,
) -> ProviderSecret {
    let expires_at = token.as_ref().and_then(|token| token.expires_at);
    let has_secret = token.is_some();

    ProviderSecret {
        id: format!(
            "{}{}",
            PROVIDER_CACHE_ID_PREFIX,
            huggingface_auth::HUGGINGFACE_PROVIDER_NAME
        ),
        provider: huggingface_auth::HUGGINGFACE_PROVIDER_NAME.to_string(),
        provider_display_name: huggingface_auth::HUGGINGFACE_DISPLAY_NAME.to_string(),
        name: huggingface_auth::HUGGINGFACE_OAUTH_TOKEN_NAME.to_string(),
        storage: ProviderSecretStorage::ProviderCache,
        expires_at,
        status: provider_secret_status(expires_at),
        configured: has_secret,
        has_secret,
        can_delete: has_secret,
        can_configure: true,
        configure_provider: Some(huggingface_auth::HUGGINGFACE_PROVIDER_NAME.to_string()),
    }
}

fn build_secret_store_secrets(
    stored_secrets: &HashMap<String, Value>,
    providers: &[(ProviderMetadata, ProviderType)],
) -> Vec<ProviderSecret> {
    let mut secrets = Vec::new();

    for (metadata, _) in providers {
        for config_key in metadata.config_keys.iter().filter(|key| key.secret) {
            if !stored_secrets.contains_key(&config_key.name) {
                continue;
            }
            secrets.push(ProviderSecret {
                id: format!(
                    "{}{}:{}",
                    SECRET_STORE_ID_PREFIX, metadata.name, config_key.name
                ),
                provider: metadata.name.clone(),
                provider_display_name: metadata.display_name.clone(),
                name: config_key.name.clone(),
                storage: ProviderSecretStorage::SecretStore,
                expires_at: None,
                status: ProviderSecretStatus::Unknown,
                configured: true,
                has_secret: true,
                can_delete: true,
                can_configure: false,
                configure_provider: None,
            });
        }
    }

    secrets
}

fn is_known_provider_secret(
    providers: &[(ProviderMetadata, ProviderType)],
    provider: &str,
    key: &str,
) -> bool {
    providers
        .iter()
        .filter(|(metadata, _)| metadata.name == provider)
        .flat_map(|(metadata, _)| metadata.config_keys.iter())
        .any(|config_key| config_key.secret && config_key.name == key)
}

fn unconfigure_provider(config: &Config, provider_name: &str) -> Result<(), ConfigError> {
    if let Some(mut entry) = crate::config::get_provider_entry(config, provider_name) {
        entry.configured = false;
        crate::config::set_provider_entry(config, provider_name, &entry)?;
    }

    let configured_marker = format!("{}_configured", provider_name);
    match config.delete(&configured_marker) {
        Ok(()) | Err(ConfigError::NotFound(_)) => Ok(()),
        Err(e) => Err(e),
    }
}

fn parse_secret_store_id(id: &str) -> Option<(&str, &str)> {
    let rest = id.strip_prefix(SECRET_STORE_ID_PREFIX)?;
    rest.split_once(':')
}

fn parse_provider_cache_id(id: &str) -> Option<&str> {
    id.strip_prefix(PROVIDER_CACHE_ID_PREFIX)
}

fn is_valid_provider_name(provider_name: &str) -> bool {
    !provider_name.is_empty()
        && provider_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn should_unconfigure_after_secret_delete(
    provider: &str,
    key: &str,
    has_usable_huggingface_oauth_token: impl FnOnce() -> bool,
) -> bool {
    provider == huggingface_auth::HUGGINGFACE_PROVIDER_NAME
        && key == huggingface_auth::HUGGINGFACE_TOKEN_SECRET_KEY
        && !has_usable_huggingface_oauth_token()
}

pub async fn list_provider_secrets() -> Result<Vec<ProviderSecret>, ConfigError> {
    let config = Config::global();
    let stored_secrets = config.all_secrets()?;
    let providers = crate::providers::providers().await;
    let display_names: HashMap<String, String> = providers
        .iter()
        .map(|(metadata, _)| (metadata.name.clone(), metadata.display_name.clone()))
        .collect();

    let mut secrets = build_secret_store_secrets(&stored_secrets, &providers);

    for definition in provider_cache_definitions_for_display() {
        if let Some(secret) = build_provider_cache_secret(definition, &display_names) {
            if !secrets.iter().any(|existing| existing.id == secret.id) {
                secrets.push(secret);
            }
        }
    }

    let huggingface_secret = build_huggingface_oauth_secret(huggingface_auth::load_oauth_token());
    if let Some(existing) = secrets
        .iter_mut()
        .find(|existing| existing.id == huggingface_secret.id)
    {
        *existing = huggingface_secret;
    } else {
        secrets.push(huggingface_secret);
    }

    secrets.sort_by(|a, b| {
        a.provider_display_name
            .cmp(&b.provider_display_name)
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(secrets)
}

pub async fn delete_provider_secret(id: &str) -> Result<(), DeleteProviderSecretError> {
    let config = Config::global();

    if let Some((provider, key)) = parse_secret_store_id(id) {
        let providers = crate::providers::providers().await;
        if !is_known_provider_secret(&providers, provider, key) {
            return Err(DeleteProviderSecretError::InvalidId(id.to_string()));
        }

        config.delete_secret(key)?;
        if should_unconfigure_after_secret_delete(provider, key, || {
            huggingface_auth::has_configured_token().unwrap_or(false)
        }) {
            unconfigure_provider(config, provider)?;
        }
        return Ok(());
    }

    if let Some(provider) = parse_provider_cache_id(id) {
        if provider == huggingface_auth::HUGGINGFACE_PROVIDER_NAME {
            huggingface_auth::clear_oauth_token()?;
            unconfigure_provider(config, provider)?;
            return Ok(());
        }

        if !is_valid_provider_name(provider) || provider_cache_definition(provider).is_none() {
            return Err(DeleteProviderSecretError::InvalidId(id.to_string()));
        }
        crate::providers::cleanup_provider(provider).await?;
        for shared_provider in provider_cache_providers_sharing_cache(provider) {
            unconfigure_provider(config, shared_provider)?;
        }
        return Ok(());
    }

    Err(DeleteProviderSecretError::InvalidId(id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderEntry;
    use crate::providers::base::ConfigKey;
    use serde_json::json;

    fn new_test_config() -> Config {
        let unique = format!(
            "goose-provider-secrets-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let config_path = std::env::temp_dir().join(format!("{unique}-config.yaml"));
        let secrets_path = std::env::temp_dir().join(format!("{unique}-secrets.yaml"));
        Config::new_with_file_secrets(config_path, secrets_path).unwrap()
    }

    #[test]
    fn secret_store_listing_only_includes_provider_secret_keys() {
        let metadata = ProviderMetadata::new(
            "openai",
            "OpenAI",
            "OpenAI provider",
            "gpt-4o",
            vec![],
            "https://example.com",
            vec![
                ConfigKey::new("OPENAI_API_KEY", true, true, None, true),
                ConfigKey::new("OPENAI_HOST", false, false, None, false),
            ],
        );
        let providers = vec![(metadata, ProviderType::Builtin)];
        let stored_secrets = HashMap::from([
            (
                "OPENAI_API_KEY".to_string(),
                Value::String("secret-value".to_string()),
            ),
            (
                "UNRELATED_SECRET".to_string(),
                Value::String("other-secret".to_string()),
            ),
            (
                "OPENAI_HOST".to_string(),
                Value::String("https://api.openai.com".to_string()),
            ),
        ]);

        let secrets = build_secret_store_secrets(&stored_secrets, &providers);

        assert_eq!(secrets.len(), 1);
        assert_eq!(secrets[0].id, "secret_store:openai:OPENAI_API_KEY");
        assert_eq!(secrets[0].provider_display_name, "OpenAI");
        assert_eq!(secrets[0].name, "OPENAI_API_KEY");
        assert_eq!(secrets[0].storage, ProviderSecretStorage::SecretStore);
        assert_eq!(secrets[0].status, ProviderSecretStatus::Unknown);
    }

    #[test]
    fn provider_secret_delete_validation_requires_provider_secret_key() {
        let metadata = ProviderMetadata::new(
            "openai",
            "OpenAI",
            "OpenAI provider",
            "gpt-4o",
            vec![],
            "https://example.com",
            vec![
                ConfigKey::new("OPENAI_API_KEY", true, true, None, true),
                ConfigKey::new("OPENAI_HOST", false, false, None, false),
            ],
        );
        let providers = vec![(metadata, ProviderType::Builtin)];

        assert!(is_known_provider_secret(
            &providers,
            "openai",
            "OPENAI_API_KEY"
        ));
        assert!(!is_known_provider_secret(
            &providers,
            "openai",
            "OPENAI_HOST"
        ));
        assert!(!is_known_provider_secret(
            &providers,
            "openai",
            "UNRELATED_SECRET"
        ));
        assert!(!is_known_provider_secret(
            &providers,
            "anthropic",
            "OPENAI_API_KEY"
        ));
    }

    #[test]
    fn expiry_extraction_handles_nested_rfc3339_values() {
        let expires_at = Utc::now() + chrono::Duration::hours(1);
        let value = json!({
            "project_id": "project",
            "token": {
                "access_token": "secret",
                "expires_at": expires_at.to_rfc3339(),
            }
        });

        let parsed = find_expires_at(&value).expect("expected expiry");

        assert_eq!(parsed.timestamp(), expires_at.timestamp());
        assert_eq!(
            provider_secret_status(Some(parsed)),
            ProviderSecretStatus::Valid
        );
    }

    #[test]
    fn expiry_extraction_ignores_refreshable_access_tokens() {
        let expires_at = Utc::now() - chrono::Duration::hours(1);
        let value = json!({
            "access_token": "access",
            "refresh_token": "refresh",
            "expires_at": expires_at.to_rfc3339(),
        });

        assert_eq!(find_expires_at(&value), None);
    }

    #[test]
    fn expiry_extraction_handles_expired_unix_timestamps() {
        let value = json!({
            "info": {
                "expires_at": 1
            }
        });

        let parsed = find_expires_at(&value).expect("expected expiry");

        assert_eq!(parsed.timestamp(), 1);
        assert_eq!(
            provider_secret_status(Some(parsed)),
            ProviderSecretStatus::Expired
        );
    }

    #[test]
    fn unconfigure_provider_clears_structured_entry() {
        let config = new_test_config();
        crate::config::set_provider_entry(
            &config,
            "huggingface",
            &ProviderEntry {
                enabled: true,
                model: "Qwen/Qwen3-Coder-480B-A35B-Instruct".to_string(),
                configured: true,
            },
        )
        .unwrap();

        unconfigure_provider(&config, "huggingface").unwrap();

        let entry = crate::config::get_provider_entry(&config, "huggingface").unwrap();
        assert!(entry.enabled);
        assert_eq!(entry.model, "Qwen/Qwen3-Coder-480B-A35B-Instruct");
        assert!(!entry.configured);
    }

    #[test]
    fn unconfigure_provider_deletes_legacy_configured_marker() {
        let config = new_test_config();
        config.set_param("huggingface_configured", true).unwrap();

        unconfigure_provider(&config, "huggingface").unwrap();

        assert!(config.get_param::<bool>("huggingface_configured").is_err());
    }

    #[test]
    fn deleting_huggingface_token_unconfigures_without_oauth() {
        assert!(should_unconfigure_after_secret_delete(
            "huggingface",
            "HF_TOKEN",
            || false
        ));
    }
}
