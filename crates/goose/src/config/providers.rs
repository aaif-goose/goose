use super::base::{Config, ConfigError};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yaml::Mapping;
use std::env;
use tracing::warn;

const PROVIDERS_CONFIG_KEY: &str = "providers";
const ACTIVE_PROVIDER_KEY: &str = "active_provider";
pub(crate) const ENABLEMENT_VERSION_KEY: &str = "provider_enablement_version";

pub fn provider_enablement_migrated(config: &Config) -> bool {
    config.get_param::<u32>(ENABLEMENT_VERSION_KEY).unwrap_or(0) >= 1
}

pub fn provider_enablement_override(config: &Config, name: &str) -> Option<bool> {
    config
        .get_param::<Mapping>(PROVIDERS_CONFIG_KEY)
        .ok()?
        .get(name)?
        .get("enabled")?
        .as_bool()
}

pub fn provider_enabled(config: &Config, name: &str) -> bool {
    get_provider_entry(config, name).is_some_and(|entry| entry.enabled)
}

pub fn set_provider_enabled(config: &Config, name: &str, enabled: bool) -> Result<(), ConfigError> {
    config.update_param::<Mapping, _, _>(PROVIDERS_CONFIG_KEY, |mut raw| {
        let key = serde_yaml::Value::String(name.to_string());
        let entry = raw
            .entry(key)
            .or_insert_with(|| serde_yaml::Value::Mapping(Mapping::new()));
        if let Some(entry) = entry.as_mapping_mut() {
            entry.insert("enabled".into(), enabled.into());
        }
        raw
    })
}

// Read raw fields so an absent enabled field is not mistaken for an explicit false.
pub(crate) fn migrate_enablement(
    effective: &Mapping,
    writable: &mut Mapping,
    legacy_visible: &[String],
    legacy_install: bool,
) -> bool {
    if effective
        .get(ENABLEMENT_VERSION_KEY)
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        >= 1
    {
        return false;
    }
    let mut providers = effective
        .get(PROVIDERS_CONFIG_KEY)
        .and_then(|v| v.as_mapping())
        .cloned()
        .unwrap_or_default();
    let active = effective
        .get(ACTIVE_PROVIDER_KEY)
        .or_else(|| effective.get("GOOSE_PROVIDER"))
        .and_then(|v| v.as_str());
    let mut names: Vec<String> = providers
        .keys()
        .filter_map(|k| k.as_str().map(str::to_string))
        .collect();
    if legacy_install {
        names.extend_from_slice(legacy_visible);
    }
    names.extend(active.map(str::to_string));
    for name in names {
        let entry = providers
            .entry(name.clone().into())
            .or_insert_with(|| serde_yaml::Value::Mapping(Mapping::new()));
        if let Some(fields) = entry.as_mapping_mut() {
            fields.entry("enabled".into()).or_insert_with(|| {
                (active == Some(name.as_str())
                    || legacy_visible.contains(&name)
                    || fields_configured(effective, &name))
                .into()
            });
        }
    }
    // Only enabled fields are written; inherited model/credential settings stay in their layer.
    let local = writable
        .entry(PROVIDERS_CONFIG_KEY.into())
        .or_insert_with(|| serde_yaml::Value::Mapping(Mapping::new()));
    if let Some(local) = local.as_mapping_mut() {
        for (name, entry) in providers {
            if let Some(enabled) = entry.get("enabled").and_then(|v| v.as_bool()) {
                let fields = local
                    .entry(name)
                    .or_insert_with(|| serde_yaml::Value::Mapping(Mapping::new()));
                if let Some(fields) = fields.as_mapping_mut() {
                    fields.insert("enabled".into(), enabled.into());
                }
            }
        }
    }
    writable.insert(ENABLEMENT_VERSION_KEY.into(), 1.into());
    true
}

fn fields_configured(values: &Mapping, name: &str) -> bool {
    values
        .get(PROVIDERS_CONFIG_KEY)
        .and_then(|v| v.get(name))
        .and_then(|v| v.get("configured"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProviderEntry {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub configured: bool,
}

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

pub fn get_provider_entry(config: &Config, name: &str) -> Option<ProviderEntry> {
    get_providers_map(config).get(name).cloned()
}

pub fn set_provider_entry(
    config: &Config,
    name: &str,
    entry: &ProviderEntry,
) -> Result<(), ConfigError> {
    let name = name.to_string();
    let entry = serde_yaml::to_value(entry)?;
    config.update_param::<Mapping, _, _>(PROVIDERS_CONFIG_KEY, |mut raw| {
        raw.insert(name.into(), entry);
        raw
    })
}

pub fn get_active_provider(config: &Config) -> Option<String> {
    if let Ok(val) = env::var("GOOSE_PROVIDER") {
        return Some(val);
    }
    if let Ok(val) = config.get_param::<String>(ACTIVE_PROVIDER_KEY) {
        return Some(val);
    }
    config.get_param::<String>("GOOSE_PROVIDER").ok()
}

pub fn get_active_model(config: &Config) -> Option<String> {
    if let Ok(val) = env::var("GOOSE_MODEL") {
        return Some(val);
    }
    if let Some(provider_name) = get_active_provider(config) {
        if let Some(entry) = get_provider_entry(config, &provider_name) {
            if !entry.model.is_empty() {
                return Some(entry.model);
            }
        }
    }
    config.get_param::<String>("GOOSE_MODEL").ok()
}

pub fn set_active_provider(config: &Config, name: &str, model: &str) -> Result<(), ConfigError> {
    config.set_param(ACTIVE_PROVIDER_KEY, name)?;
    let entry = ProviderEntry {
        enabled: true,
        model: model.to_string(),
        configured: true,
    };
    set_provider_entry(config, name, &entry)
}

pub fn clear_active_provider(config: &Config) -> Result<(), ConfigError> {
    for key in [ACTIVE_PROVIDER_KEY, "GOOSE_PROVIDER", "GOOSE_MODEL"] {
        match config.delete(key) {
            Ok(()) | Err(ConfigError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
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
    fn test_clear_active_provider_preserves_provider_entries() {
        let config = new_test_config();
        set_active_provider(&config, "openai", "gpt-4o").unwrap();

        clear_active_provider(&config).unwrap();

        assert!(get_active_provider(&config).is_none());
        let entry = get_provider_entry(&config, "openai").unwrap();
        assert_eq!(entry.model, "gpt-4o");
        assert!(entry.configured);
    }

    #[test]
    fn test_clear_active_provider_removes_legacy_keys() {
        let config = new_test_config();
        config.set_param("GOOSE_PROVIDER", "anthropic").unwrap();
        config.set_param("GOOSE_MODEL", "claude").unwrap();

        clear_active_provider(&config).unwrap();

        assert!(get_active_provider(&config).is_none());
        assert!(get_active_model(&config).is_none());
    }

    #[test]
    fn test_get_active_model_from_provider_entry() {
        let config = new_test_config();
        set_active_provider(&config, "openai", "gpt-4o").unwrap();

        let result = get_active_model(&config);
        assert_eq!(result, Some("gpt-4o".to_string()));
    }

    #[test]
    fn test_multiple_providers_preserved() {
        let config = new_test_config();
        set_active_provider(&config, "openai", "gpt-4o").unwrap();
        set_active_provider(&config, "anthropic", "claude-3-opus").unwrap();

        let openai = get_provider_entry(&config, "openai").unwrap();
        assert_eq!(openai.model, "gpt-4o");
        assert!(openai.configured);

        let anthropic = get_provider_entry(&config, "anthropic").unwrap();
        assert_eq!(anthropic.model, "claude-3-opus");
        assert!(anthropic.configured);

        assert_eq!(get_active_provider(&config), Some("anthropic".to_string()));
    }
    #[test]
    fn enablement_migration_preserves_choices_and_models() {
        let mut config: Mapping = serde_yaml::from_str(
            r#"
active_provider: anthropic
providers:
  anthropic:
    model: claude-custom
  aws_bedrock:
    enabled: false
  ollama:
    enabled: true
    configured: false
"#,
        )
        .unwrap();
        let effective = config.clone();
        assert!(migrate_enablement(
            &effective,
            &mut config,
            &["aws_bedrock".into(), "codex".into()],
            true
        ));
        assert_eq!(
            config["providers"]["aws_bedrock"]["enabled"].as_bool(),
            Some(false)
        );
        assert_eq!(
            config["providers"]["anthropic"]["enabled"].as_bool(),
            Some(true)
        );
        assert_eq!(
            config["providers"]["anthropic"]["model"].as_str(),
            Some("claude-custom")
        );
        assert_eq!(
            config["providers"]["ollama"]["enabled"].as_bool(),
            Some(true)
        );
        assert_eq!(
            config["providers"]["codex"]["enabled"].as_bool(),
            Some(true)
        );
        let effective = config.clone();
        assert!(!migrate_enablement(
            &effective,
            &mut config,
            &["new_provider".into()],
            true
        ));
        assert!(config["providers"].get("new_provider").is_none());
    }

    #[test]
    fn fresh_install_does_not_enable_default_only_providers() {
        let mut config = Mapping::new();
        assert!(migrate_enablement(
            &Mapping::new(),
            &mut config,
            &["aws_bedrock".into()],
            false
        ));
        assert!(config["providers"].as_mapping().unwrap().is_empty());
        assert_eq!(config[ENABLEMENT_VERSION_KEY].as_u64(), Some(1));
    }

    #[test]
    fn enablement_migration_keeps_layered_settings_in_their_layer() {
        let effective: Mapping = serde_yaml::from_str(
            r#"
providers:
  custom:
    enabled: true
    model: inherited-model
"#,
        )
        .unwrap();
        let mut writable = Mapping::new();
        migrate_enablement(&effective, &mut writable, &[], true);
        assert_eq!(
            writable["providers"]["custom"]["enabled"].as_bool(),
            Some(true)
        );
        assert!(writable["providers"]["custom"].get("model").is_none());
    }

    #[test]
    fn disable_persists_without_erasing_credentials_or_model() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let secrets = dir.path().join("secrets.yaml");
        let config = Config::new_with_file_secrets(&path, &secrets).unwrap();
        set_active_provider(&config, "aws_bedrock", "my-model").unwrap();
        config
            .set_secret("AWS_BEARER_TOKEN_BEDROCK", &"test-token")
            .unwrap();
        set_provider_enabled(&config, "aws_bedrock", false).unwrap();
        let reloaded = Config::new_with_file_secrets(&path, &secrets).unwrap();
        assert!(!provider_enabled(&reloaded, "aws_bedrock"));
        assert_eq!(
            get_provider_entry(&reloaded, "aws_bedrock").unwrap().model,
            "my-model"
        );
        assert_eq!(
            reloaded
                .get_secret::<String>("AWS_BEARER_TOKEN_BEDROCK")
                .unwrap(),
            "test-token"
        );
        assert_eq!(
            get_active_provider(&reloaded).as_deref(),
            Some("aws_bedrock")
        );
        set_provider_enabled(&reloaded, "aws_bedrock", true).unwrap();
        assert!(provider_enabled(&reloaded, "aws_bedrock"));
    }

    #[test]
    fn updating_one_provider_preserves_missing_enablement_on_others() {
        let config = new_test_config();
        let entries: Mapping = serde_yaml::from_str("legacy:\n  model: saved-model\n").unwrap();
        config.set_param("providers", entries).unwrap();
        set_active_provider(&config, "anthropic", "claude").unwrap();
        assert_eq!(provider_enablement_override(&config, "legacy"), None);
    }
    #[test]
    fn migration_does_not_overwrite_unreadable_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        let config = Config::new_with_file_secrets(&path, dir.path().join("secrets.yaml")).unwrap();
        let invalid = "providers: [unterminated";
        std::fs::write(&path, invalid).unwrap();
        assert!(config
            .migrate_provider_enablement(&["aws_bedrock".into()])
            .is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), invalid);
    }
}
