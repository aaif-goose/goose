use crate::agents::extension::PLATFORM_EXTENSIONS;
use crate::agents::ExtensionConfig;
use crate::config::extensions::ExtensionEntry;
use crate::config::providers::ProviderEntry;
use serde_yaml::Mapping;

const EXTENSIONS_CONFIG_KEY: &str = "extensions";
const PROVIDERS_CONFIG_KEY: &str = "providers";
const ACTIVE_PROVIDER_KEY: &str = "active_provider";

pub fn run_migrations(config: &mut Mapping) -> bool {
    let mut changed = false;
    changed |= migrate_platform_extensions(config);
    changed |= migrate_provider_config(config);
    changed
}

/// Run only non-destructive migrations suitable for in-memory read paths.
/// Provider migration is excluded because it removes flat keys that
/// `get_param()` callers may still look up directly.
pub fn run_read_migrations(config: &mut Mapping) {
    migrate_platform_extensions(config);
}

fn migrate_platform_extensions(config: &mut Mapping) -> bool {
    let extensions_key = serde_yaml::Value::String(EXTENSIONS_CONFIG_KEY.to_string());

    let extensions_value = config
        .get(&extensions_key)
        .cloned()
        .unwrap_or(serde_yaml::Value::Mapping(Mapping::new()));

    let mut extensions_map: Mapping = match extensions_value {
        serde_yaml::Value::Mapping(m) => m,
        _ => Mapping::new(),
    };

    let mut needs_save = false;

    for (name, def) in PLATFORM_EXTENSIONS.iter() {
        let ext_key = serde_yaml::Value::String(name.to_string());
        let existing = extensions_map.get(&ext_key);

        let needs_migration = match existing {
            None => true,
            Some(value) => match serde_yaml::from_value::<ExtensionEntry>(value.clone()) {
                Ok(entry) => match &entry.config {
                    ExtensionConfig::Platform {
                        description,
                        display_name,
                        ..
                    }
                    | ExtensionConfig::Builtin {
                        description,
                        display_name,
                        ..
                    } => {
                        description != def.description
                            || display_name.as_deref() != Some(def.display_name)
                    }
                    _ => true,
                },
                Err(_) => true,
            },
        };

        if needs_migration {
            let existing_entry =
                existing.and_then(|v| serde_yaml::from_value::<ExtensionEntry>(v.clone()).ok());

            let enabled = existing_entry
                .as_ref()
                .map(|e| e.enabled)
                .unwrap_or(def.default_enabled);

            // If the extension already exists as type 'builtin', preserve that type
            let is_existing_builtin = existing_entry
                .as_ref()
                .is_some_and(|e| matches!(e.config, ExtensionConfig::Builtin { .. }));

            let config = if is_existing_builtin {
                ExtensionConfig::Builtin {
                    name: def.name.to_string(),
                    description: def.description.to_string(),
                    display_name: Some(def.display_name.to_string()),
                    timeout: None,
                    bundled: Some(true),
                    available_tools: Vec::new(),
                }
            } else {
                ExtensionConfig::Platform {
                    name: def.name.to_string(),
                    description: def.description.to_string(),
                    display_name: Some(def.display_name.to_string()),
                    bundled: Some(true),
                    available_tools: Vec::new(),
                }
            };

            let new_entry = ExtensionEntry { config, enabled };

            if let Ok(value) = serde_yaml::to_value(&new_entry) {
                extensions_map.insert(ext_key, value);
                needs_save = true;
            }
        }
    }

    if needs_save {
        config.insert(extensions_key, serde_yaml::Value::Mapping(extensions_map));
    }

    needs_save
}

/// Remove leftover legacy flat keys when `providers:` block already exists.
fn cleanup_legacy_provider_keys(config: &mut Mapping) -> bool {
    let configured_suffix = "_configured";
    let mut changed = false;

    let stale_keys: Vec<serde_yaml::Value> = config
        .keys()
        .filter(|k| {
            k.as_str()
                .map(|s| {
                    s == "GOOSE_PROVIDER" || s == "GOOSE_MODEL" || s.ends_with(configured_suffix)
                })
                .unwrap_or(false)
        })
        .cloned()
        .collect();

    for key in stale_keys {
        config.shift_remove(&key);
        changed = true;
    }

    changed
}

/// Migrate flat provider keys to the structured `providers:` block.
///
/// Old layout (flat keys):
/// ```yaml
/// GOOSE_PROVIDER: claude-acp
/// GOOSE_MODEL: current
/// claude-acp_configured: true
/// lmstudio_configured: true
/// ```
///
/// New layout:
/// ```yaml
/// active_provider: claude-acp
/// providers:
///   claude-acp:
///     enabled: true
///     model: current
///     configured: true
///   lmstudio:
///     enabled: true
///     model: ""
///     configured: true
/// ```
///
fn migrate_provider_config(config: &mut Mapping) -> bool {
    let providers_key = serde_yaml::Value::String(PROVIDERS_CONFIG_KEY.to_string());

    // If providers block already exists, backfill active_provider from the
    // legacy flat key when missing, then clean up leftover flat keys.
    if config.contains_key(&providers_key) {
        let ap_key = serde_yaml::Value::String(ACTIVE_PROVIDER_KEY.to_string());
        if !config.contains_key(&ap_key) {
            if let Some(legacy) = config
                .get(serde_yaml::Value::String("GOOSE_PROVIDER".to_string()))
                .and_then(|v| v.as_str())
            {
                config.insert(ap_key, serde_yaml::Value::String(legacy.to_string()));
            }
        }
        return cleanup_legacy_provider_keys(config);
    }

    // Read the old flat keys, if present.
    let active_provider = config
        .get(serde_yaml::Value::String("GOOSE_PROVIDER".to_string()))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let active_model = config
        .get(serde_yaml::Value::String("GOOSE_MODEL".to_string()))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    // Scan for `*_configured` keys to discover all previously-used providers.
    let configured_suffix = "_configured";
    let mut discovered_providers: Vec<String> = config
        .keys()
        .filter_map(|k| {
            k.as_str().and_then(|s| {
                if s.ends_with(configured_suffix) {
                    Some(s.trim_end_matches(configured_suffix).to_string())
                } else {
                    None
                }
            })
        })
        .collect();

    // Ensure the active provider is in the list even if no `*_configured`
    // marker exists for it yet.
    if let Some(ref ap) = active_provider {
        if !discovered_providers.contains(ap) {
            discovered_providers.push(ap.clone());
        }
    }

    // If there is nothing to migrate, bail out.
    if discovered_providers.is_empty() && active_provider.is_none() {
        return false;
    }

    // Build the providers mapping.
    let mut providers_map = Mapping::new();
    for name in &discovered_providers {
        let is_active = active_provider.as_deref() == Some(name.as_str());
        let model = if is_active {
            active_model.clone()
        } else {
            String::new()
        };
        let entry = ProviderEntry {
            enabled: true,
            model,
            configured: true,
        };
        if let Ok(value) = serde_yaml::to_value(&entry) {
            providers_map.insert(serde_yaml::Value::String(name.clone()), value);
        }
    }

    config.insert(providers_key, serde_yaml::Value::Mapping(providers_map));

    // Write `active_provider` top-level key.
    if let Some(ref ap) = active_provider {
        config.insert(
            serde_yaml::Value::String(ACTIVE_PROVIDER_KEY.to_string()),
            serde_yaml::Value::String(ap.clone()),
        );
    }

    // Remove old flat keys.
    config.shift_remove(serde_yaml::Value::String("GOOSE_PROVIDER".to_string()));
    config.shift_remove(serde_yaml::Value::String("GOOSE_MODEL".to_string()));
    for name in &discovered_providers {
        let marker_key = serde_yaml::Value::String(format!("{}{}", name, configured_suffix));
        config.shift_remove(&marker_key);
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrate_platform_extensions_empty_config() {
        let mut config = Mapping::new();
        let changed = run_migrations(&mut config);

        assert!(changed);
        let extensions_key = serde_yaml::Value::String(EXTENSIONS_CONFIG_KEY.to_string());
        assert!(config.contains_key(&extensions_key));
    }

    #[test]
    fn test_migrate_platform_extensions_preserves_enabled_state() {
        let mut config = Mapping::new();
        let mut extensions = Mapping::new();
        let todo_entry = ExtensionEntry {
            config: ExtensionConfig::Platform {
                name: "todo".to_string(),
                description: "old description".to_string(),
                display_name: Some("Old Name".to_string()),
                bundled: Some(true),
                available_tools: Vec::new(),
            },
            enabled: false,
        };
        extensions.insert(
            serde_yaml::Value::String("todo".to_string()),
            serde_yaml::to_value(&todo_entry).unwrap(),
        );
        config.insert(
            serde_yaml::Value::String(EXTENSIONS_CONFIG_KEY.to_string()),
            serde_yaml::Value::Mapping(extensions),
        );

        let changed = run_migrations(&mut config);
        assert!(changed);

        let extensions_key = serde_yaml::Value::String(EXTENSIONS_CONFIG_KEY.to_string());
        let extensions = config.get(&extensions_key).unwrap().as_mapping().unwrap();
        let todo_key = serde_yaml::Value::String("todo".to_string());
        let todo_value = extensions.get(&todo_key).unwrap();
        let todo_entry: ExtensionEntry = serde_yaml::from_value(todo_value.clone()).unwrap();

        assert!(!todo_entry.enabled);
    }

    #[test]
    fn test_migrate_platform_extensions_idempotent() {
        let mut config = Mapping::new();
        run_migrations(&mut config);

        let changed = run_migrations(&mut config);
        assert!(!changed);
    }

    // -----------------------------------------------------------------------
    // Provider migration tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_migrate_provider_config_basic() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("claude-acp".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("current".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("claude-acp_configured".to_string()),
            serde_yaml::Value::Bool(true),
        );

        let changed = migrate_provider_config(&mut config);
        assert!(changed);

        // active_provider should be set
        let active = config
            .get(serde_yaml::Value::String("active_provider".to_string()))
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(active, "claude-acp");

        // providers block should exist with the entry
        let providers = config
            .get(serde_yaml::Value::String("providers".to_string()))
            .unwrap()
            .as_mapping()
            .unwrap();
        let entry: ProviderEntry = serde_yaml::from_value(
            providers
                .get(serde_yaml::Value::String("claude-acp".to_string()))
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert!(entry.enabled);
        assert!(entry.configured);
        assert_eq!(entry.model, "current");

        // Old flat keys should be removed
        assert!(!config.contains_key(serde_yaml::Value::String("GOOSE_PROVIDER".to_string())));
        assert!(!config.contains_key(serde_yaml::Value::String("GOOSE_MODEL".to_string())));
        assert!(!config.contains_key(serde_yaml::Value::String(
            "claude-acp_configured".to_string()
        )));
    }

    #[test]
    fn test_migrate_provider_config_multiple_configured() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("claude-acp".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("current".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("claude-acp_configured".to_string()),
            serde_yaml::Value::Bool(true),
        );
        config.insert(
            serde_yaml::Value::String("lmstudio_configured".to_string()),
            serde_yaml::Value::Bool(true),
        );

        let changed = migrate_provider_config(&mut config);
        assert!(changed);

        let providers = config
            .get(serde_yaml::Value::String("providers".to_string()))
            .unwrap()
            .as_mapping()
            .unwrap();

        // Both providers should exist
        let claude: ProviderEntry = serde_yaml::from_value(
            providers
                .get(serde_yaml::Value::String("claude-acp".to_string()))
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert_eq!(claude.model, "current");
        assert!(claude.configured);

        let lmstudio: ProviderEntry = serde_yaml::from_value(
            providers
                .get(serde_yaml::Value::String("lmstudio".to_string()))
                .unwrap()
                .clone(),
        )
        .unwrap();
        // lmstudio was not the active provider, so model should be empty
        assert_eq!(lmstudio.model, "");
        assert!(lmstudio.configured);

        // Old markers removed
        assert!(!config.contains_key(serde_yaml::Value::String(
            "claude-acp_configured".to_string()
        )));
        assert!(!config.contains_key(serde_yaml::Value::String("lmstudio_configured".to_string())));
    }

    #[test]
    fn test_migrate_provider_config_idempotent() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("openai".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("gpt-4o".to_string()),
        );

        let changed_first = migrate_provider_config(&mut config);
        assert!(changed_first);

        let changed_second = migrate_provider_config(&mut config);
        assert!(!changed_second, "Second migration run should be a no-op");
    }

    #[test]
    fn test_migrate_provider_config_empty_config() {
        let mut config = Mapping::new();

        let changed = migrate_provider_config(&mut config);
        assert!(!changed, "Empty config should not trigger migration");
    }

    #[test]
    fn test_migrate_provider_config_no_model() {
        let mut config = Mapping::new();
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("anthropic".to_string()),
        );
        // No GOOSE_MODEL key

        let changed = migrate_provider_config(&mut config);
        assert!(changed);

        let providers = config
            .get(serde_yaml::Value::String("providers".to_string()))
            .unwrap()
            .as_mapping()
            .unwrap();
        let entry: ProviderEntry = serde_yaml::from_value(
            providers
                .get(serde_yaml::Value::String("anthropic".to_string()))
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert_eq!(entry.model, "");
    }

    #[test]
    fn test_cleanup_legacy_keys_when_providers_exists() {
        let mut config = Mapping::new();
        // Simulate state: providers block exists but stale flat keys remain
        let mut providers_map = Mapping::new();
        if let Ok(value) = serde_yaml::to_value(&ProviderEntry {
            enabled: true,
            model: "current".to_string(),
            configured: true,
        }) {
            providers_map.insert(serde_yaml::Value::String("claude-acp".to_string()), value);
        }
        config.insert(
            serde_yaml::Value::String("providers".to_string()),
            serde_yaml::Value::Mapping(providers_map),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_PROVIDER".to_string()),
            serde_yaml::Value::String("lmstudio".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("GOOSE_MODEL".to_string()),
            serde_yaml::Value::String("some-model".to_string()),
        );
        config.insert(
            serde_yaml::Value::String("claude-acp_configured".to_string()),
            serde_yaml::Value::Bool(true),
        );

        let changed = migrate_provider_config(&mut config);
        assert!(changed);

        // Legacy keys should be gone
        assert!(!config.contains_key(serde_yaml::Value::String("GOOSE_PROVIDER".to_string())));
        assert!(!config.contains_key(serde_yaml::Value::String("GOOSE_MODEL".to_string())));
        assert!(!config.contains_key(serde_yaml::Value::String(
            "claude-acp_configured".to_string()
        )));

        // Providers block should be untouched
        assert!(config.contains_key(serde_yaml::Value::String("providers".to_string())));

        // active_provider should be backfilled from legacy GOOSE_PROVIDER
        assert_eq!(
            config
                .get(serde_yaml::Value::String("active_provider".to_string()))
                .and_then(|v| v.as_str()),
            Some("lmstudio")
        );
    }
}
