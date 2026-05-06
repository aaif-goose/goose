use crate::agents::extension::PLATFORM_EXTENSIONS;
use crate::agents::ExtensionConfig;
use crate::config::extensions::ExtensionEntry;
use serde_yaml::Mapping;

const EXTENSIONS_CONFIG_KEY: &str = "extensions";

pub fn run_migrations(config: &mut Mapping) -> bool {
    let mut changed = false;
    changed |= migrate_platform_extensions(config);
    changed
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
            let def = PLATFORM_EXTENSIONS.get(key).unwrap();
            let entry: ExtensionEntry = serde_yaml::from_value(value.clone()).unwrap();
            assert_eq!(entry.enabled, def.default_enabled);
        }
    }

    #[test]
    fn test_migrate_platform_extensions_refreshes_metadata_without_changing_enabled() {
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
        match todo_entry.config {
            ExtensionConfig::Platform {
                description,
                display_name,
                ..
            } => {
                assert_ne!(description, "old description");
                assert_ne!(display_name.as_deref(), Some("Old Name"));
            }
            other => panic!("expected platform extension, got {other:?}"),
        }
    }

    #[test]
    fn test_migrate_platform_extensions_preserves_existing_enabled_values() {
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
        let custom_developer_entry = ExtensionEntry {
            config: ExtensionConfig::Stdio {
                name: "developer".to_string(),
                description: "Custom user developer tools".to_string(),
                cmd: "custom-developer".to_string(),
                args: Vec::new(),
                envs: Default::default(),
                env_keys: Vec::new(),
                timeout: Some(300),
                bundled: None,
                available_tools: Vec::new(),
            },
            enabled: true,
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
            serde_yaml::Value::String("custom-developer".to_string()),
            serde_yaml::to_value(&custom_developer_entry).unwrap(),
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
        let custom_developer: ExtensionEntry = serde_yaml::from_value(
            extensions
                .get(serde_yaml::Value::String("custom-developer".to_string()))
                .unwrap()
                .clone(),
        )
        .unwrap();

        assert!(analyze.enabled);
        assert!(!developer.enabled);
        assert!(custom_developer.enabled);
    }

    #[test]
    fn test_migrate_platform_extensions_idempotent() {
        let mut config = Mapping::new();
        run_migrations(&mut config);

        let changed = run_migrations(&mut config);
        assert!(!changed);
    }
}
