use super::base::{Config, ConfigError};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yaml::Mapping;
use std::env;
use tracing::warn;

const PROVIDERS_CONFIG_KEY: &str = "providers";
const ACTIVE_PROVIDER_KEY: &str = "active_provider";

/// A single provider's persisted configuration within the `providers:` block.
///
/// The `providers` block in config.yaml is the authoritative source for
/// per-provider settings, replacing the old flat-key scheme where switching
/// providers destructively overwrote `GOOSE_PROVIDER` / `GOOSE_MODEL`.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProviderEntry {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub configured: bool,
}

// ---------------------------------------------------------------------------
// Read helpers
// ---------------------------------------------------------------------------

fn parse_providers_map(raw: Mapping) -> IndexMap<String, ProviderEntry> {
    let mut map = IndexMap::with_capacity(raw.len());
    for (k, v) in raw {
        match (k, serde_yaml::from_value::<ProviderEntry>(v)) {
            (serde_yaml::Value::String(key), Ok(entry)) => {
                map.insert(key, entry);
            }
            (k, v) => {
                warn!(
                    key = ?k,
                    value = ?v,
                    "Skipping malformed provider config entry"
                );
            }
        }
    }
    map
}

fn get_providers_map(config: &Config) -> IndexMap<String, ProviderEntry> {
    let raw: Mapping = config
        .get_param(PROVIDERS_CONFIG_KEY)
        .unwrap_or_else(|_| Default::default());
    parse_providers_map(raw)
}

/// Retrieve the [`ProviderEntry`] for a named provider, if it exists.
pub fn get_provider_entry(config: &Config, name: &str) -> Option<ProviderEntry> {
    get_providers_map(config).get(name).cloned()
}

// ---------------------------------------------------------------------------
// Write helpers
// ---------------------------------------------------------------------------

/// Persist a [`ProviderEntry`] under `providers.{name}`.
pub fn set_provider_entry(
    config: &Config,
    name: &str,
    entry: &ProviderEntry,
) -> Result<(), ConfigError> {
    let name = name.to_string();
    let entry = entry.clone();
    config.update_param::<Mapping, _, _>(PROVIDERS_CONFIG_KEY, |raw| {
        let mut map = parse_providers_map(raw);
        map.insert(name, entry);
        map
    })
}

// ---------------------------------------------------------------------------
// Active-provider accessors
// ---------------------------------------------------------------------------

/// Return the currently active provider name.
///
/// Resolution order:
/// 1. `GOOSE_PROVIDER` environment variable (uppercase check performed by
///    `get_param`)
/// 2. `active_provider` key in config.yaml
/// 3. Legacy flat `GOOSE_PROVIDER` key in config.yaml (backward compat)
pub fn get_active_provider(config: &Config) -> Option<String> {
    // Env var takes precedence (get_param checks env automatically)
    if let Ok(val) = env::var("GOOSE_PROVIDER") {
        return Some(val);
    }

    // New structured key
    if let Ok(val) = config.get_param::<String>(ACTIVE_PROVIDER_KEY) {
        return Some(val);
    }

    // Legacy flat key fallback
    config.get_param::<String>("GOOSE_PROVIDER").ok()
}

/// Return the model for the currently active provider.
///
/// Resolution order:
/// 1. `GOOSE_MODEL` environment variable
/// 2. Model recorded in the active provider's entry (`providers.{name}.model`)
/// 3. Legacy flat `GOOSE_MODEL` key in config.yaml
pub fn get_active_model(config: &Config) -> Option<String> {
    // Env var takes precedence
    if let Ok(val) = env::var("GOOSE_MODEL") {
        return Some(val);
    }

    // Try provider entry model
    if let Some(provider_name) = get_active_provider(config) {
        if let Some(entry) = get_provider_entry(config, &provider_name) {
            if !entry.model.is_empty() {
                return Some(entry.model);
            }
        }
    }

    // Legacy flat key fallback
    config.get_param::<String>("GOOSE_MODEL").ok()
}

/// Set the active provider and update its entry in the `providers` block.
///
/// This writes:
/// - `active_provider: {name}` at the top level
/// - `providers.{name}` with `configured: true`, `enabled: true`, and the
///   supplied model.
pub fn set_active_provider(config: &Config, name: &str, model: &str) -> Result<(), ConfigError> {
    config.set_param(ACTIVE_PROVIDER_KEY, name)?;
    let entry = ProviderEntry {
        enabled: true,
        model: model.to_string(),
        configured: true,
    };
    set_provider_entry(config, name, &entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn new_test_config() -> Config {
        let config_file = NamedTempFile::new().unwrap();
        let secrets_file = NamedTempFile::new().unwrap();
        Config::new_with_file_secrets(config_file.path(), secrets_file.path()).unwrap()
    }

    #[test]
    fn test_set_and_get_provider_entry() {
        let config = new_test_config();
        let entry = ProviderEntry {
            enabled: true,
            model: "gpt-4o".to_string(),
            configured: true,
        };
        set_provider_entry(&config, "openai", &entry).unwrap();

        let loaded = get_provider_entry(&config, "openai").unwrap();
        assert!(loaded.enabled);
        assert_eq!(loaded.model, "gpt-4o");
        assert!(loaded.configured);
    }

    #[test]
    fn test_get_provider_entry_missing() {
        let config = new_test_config();
        assert!(get_provider_entry(&config, "nonexistent").is_none());
    }

    #[test]
    fn test_set_active_provider_writes_structured_keys() {
        let config = new_test_config();
        set_active_provider(&config, "claude-acp", "current").unwrap();

        let active: String = config.get_param(ACTIVE_PROVIDER_KEY).unwrap();
        assert_eq!(active, "claude-acp");

        let entry = get_provider_entry(&config, "claude-acp").unwrap();
        assert!(entry.enabled);
        assert!(entry.configured);
        assert_eq!(entry.model, "current");
    }

    #[test]
    fn test_get_active_provider_from_new_key() {
        let config = new_test_config();
        config.set_param(ACTIVE_PROVIDER_KEY, "openai").unwrap();

        let result = get_active_provider(&config);
        assert_eq!(result, Some("openai".to_string()));
    }

    #[test]
    fn test_get_active_provider_falls_back_to_legacy() {
        let config = new_test_config();
        config.set_param("GOOSE_PROVIDER", "anthropic").unwrap();

        let result = get_active_provider(&config);
        assert_eq!(result, Some("anthropic".to_string()));
    }

    #[test]
    fn test_get_active_provider_none_when_empty() {
        let config = new_test_config();
        let result = get_active_provider(&config);
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_active_model_from_provider_entry() {
        let config = new_test_config();
        set_active_provider(&config, "openai", "gpt-4o").unwrap();

        let result = get_active_model(&config);
        assert_eq!(result, Some("gpt-4o".to_string()));
    }

    #[test]
    fn test_get_active_model_falls_back_to_legacy() {
        let config = new_test_config();
        // Only set the legacy key, no providers block
        config.set_param("GOOSE_MODEL", "gpt-3.5-turbo").unwrap();

        let result = get_active_model(&config);
        assert_eq!(result, Some("gpt-3.5-turbo".to_string()));
    }

    #[test]
    fn test_multiple_providers_preserved() {
        let config = new_test_config();

        // Set up two providers
        set_active_provider(&config, "openai", "gpt-4o").unwrap();
        set_active_provider(&config, "anthropic", "claude-3-opus").unwrap();

        // Both entries should exist
        let openai = get_provider_entry(&config, "openai").unwrap();
        assert_eq!(openai.model, "gpt-4o");
        assert!(openai.configured);

        let anthropic = get_provider_entry(&config, "anthropic").unwrap();
        assert_eq!(anthropic.model, "claude-3-opus");
        assert!(anthropic.configured);

        // Active provider should be the last one set
        let active = get_active_provider(&config);
        assert_eq!(active, Some("anthropic".to_string()));
    }
}
