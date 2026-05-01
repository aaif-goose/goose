use crate::agents::extension::PLATFORM_EXTENSIONS;
use crate::agents::ExtensionConfig;
use crate::config::extensions::ExtensionEntry;
use serde_yaml::Mapping;

const EXTENSIONS_CONFIG_KEY: &str = "extensions";
const EXTENSIONS_ON_DEMAND_MIGRATION_KEY: &str = "extensions_on_demand_migration";
const EXTENSION_MANAGER_KEY: &str = "extensionmanager";
const CORE_DEFAULT_EXTENSION_KEYS: &[&str] =
    &[EXTENSION_MANAGER_KEY, "developer", "skills", "todo"];

pub fn run_migrations(config: &mut Mapping) -> bool {
    let mut changed = false;
    changed |= migrate_platform_extensions(config);
    changed |= migrate_extensions_to_on_demand_defaults(config);
    changed
}

fn migration_done(config: &Mapping, key: &str) -> bool {
    config
        .get(serde_yaml::Value::String(key.to_string()))
        .and_then(serde_yaml::Value::as_bool)
        .unwrap_or(false)
}

fn mark_migration_done(config: &mut Mapping, key: &str) {
    config.insert(
        serde_yaml::Value::String(key.to_string()),
        serde_yaml::Value::Bool(true),
    );
}

fn should_enable_by_default(key: &str) -> bool {
    CORE_DEFAULT_EXTENSION_KEYS.contains(&key)
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

fn migrate_extensions_to_on_demand_defaults(config: &mut Mapping) -> bool {
    if migration_done(config, EXTENSIONS_ON_DEMAND_MIGRATION_KEY) {
        return false;
    }

    let extensions_key = serde_yaml::Value::String(EXTENSIONS_CONFIG_KEY.to_string());
    let Some(serde_yaml::Value::Mapping(extensions_map)) = config.get_mut(&extensions_key) else {
        mark_migration_done(config, EXTENSIONS_ON_DEMAND_MIGRATION_KEY);
        return true;
    };

    for value in extensions_map.values_mut() {
        let Ok(mut entry) = serde_yaml::from_value::<ExtensionEntry>(value.clone()) else {
            continue;
        };

        let should_enable = should_enable_by_default(&entry.config.key());
        if entry.enabled != should_enable {
            entry.enabled = should_enable;
            if let Ok(next_value) = serde_yaml::to_value(&entry) {
                *value = next_value;
            }
        }
    }

    mark_migration_done(config, EXTENSIONS_ON_DEMAND_MIGRATION_KEY);
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

        let extensions = config.get(&extensions_key).unwrap().as_mapping().unwrap();
        for (key, value) in extensions {
            let key = key.as_str().unwrap();
            let entry: ExtensionEntry = serde_yaml::from_value(value.clone()).unwrap();
            assert_eq!(entry.enabled, should_enable_by_default(key));
        }
    }

    #[test]
    fn test_migrate_platform_extensions_applies_core_defaults() {
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

        assert!(todo_entry.enabled);
    }

    #[test]
    fn test_migrate_extensions_to_on_demand_defaults() {
        let mut config = Mapping::new();
        let mut extensions = Mapping::new();

        let analyze_entry = ExtensionEntry {
            config: ExtensionConfig::Platform {
                name: "analyze".to_string(),
                description: "Analyze code structure".to_string(),
                display_name: Some("Analyze".to_string()),
                bundled: Some(true),
                available_tools: Vec::new(),
            },
            enabled: true,
        };
        let developer_entry = ExtensionEntry {
            config: ExtensionConfig::Platform {
                name: "developer".to_string(),
                description: "Write and edit files, and execute shell commands".to_string(),
                display_name: Some("Developer".to_string()),
                bundled: Some(true),
                available_tools: Vec::new(),
            },
            enabled: false,
        };
        let extension_manager_entry = ExtensionEntry {
            config: ExtensionConfig::Platform {
                name: "Extension Manager".to_string(),
                description: "Enable extension management tools".to_string(),
                display_name: Some("Extension Manager".to_string()),
                bundled: Some(true),
                available_tools: Vec::new(),
            },
            enabled: false,
        };

        extensions.insert(
            serde_yaml::Value::String("analyze".to_string()),
            serde_yaml::to_value(&analyze_entry).unwrap(),
        );
        extensions.insert(
            serde_yaml::Value::String("developer".to_string()),
            serde_yaml::to_value(&developer_entry).unwrap(),
        );
        extensions.insert(
            serde_yaml::Value::String(EXTENSION_MANAGER_KEY.to_string()),
            serde_yaml::to_value(&extension_manager_entry).unwrap(),
        );
        config.insert(
            serde_yaml::Value::String(EXTENSIONS_CONFIG_KEY.to_string()),
            serde_yaml::Value::Mapping(extensions),
        );

        assert!(run_migrations(&mut config));

        let extensions_key = serde_yaml::Value::String(EXTENSIONS_CONFIG_KEY.to_string());
        let extensions = config.get(&extensions_key).unwrap().as_mapping().unwrap();
        let analyze: ExtensionEntry = serde_yaml::from_value(
            extensions
                .get(serde_yaml::Value::String("analyze".to_string()))
                .unwrap()
                .clone(),
        )
        .unwrap();
        let developer: ExtensionEntry = serde_yaml::from_value(
            extensions
                .get(serde_yaml::Value::String("developer".to_string()))
                .unwrap()
                .clone(),
        )
        .unwrap();
        let extension_manager: ExtensionEntry = serde_yaml::from_value(
            extensions
                .get(serde_yaml::Value::String(EXTENSION_MANAGER_KEY.to_string()))
                .unwrap()
                .clone(),
        )
        .unwrap();

        assert!(!analyze.enabled);
        assert!(developer.enabled);
        assert!(extension_manager.enabled);
    }

    #[test]
    fn test_migrate_platform_extensions_idempotent() {
        let mut config = Mapping::new();
        run_migrations(&mut config);

        let changed = run_migrations(&mut config);
        assert!(!changed);
    }
}
