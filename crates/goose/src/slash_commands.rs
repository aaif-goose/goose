use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::config::Config;
use crate::recipe::Recipe;
use crate::recipe::RECIPE_FILE_EXTENSIONS;

const SLASH_COMMANDS_CONFIG_KEY: &str = "slash_commands";
const AUTO_SLASH_COMMANDS_CONFIG_KEY: &str = "auto_slash_commands";
const GOOSE_RECIPE_PATH_ENV_VAR: &str = "GOOSE_RECIPE_PATH";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommandMapping {
    pub command: String,
    pub recipe_path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutoSlashCommandsConfig {
    #[serde(default)]
    pub from_goose_recipe_path: bool,
}

fn save_slash_commands(commands: Vec<SlashCommandMapping>) -> Result<()> {
    Config::global()
        .set_param(SLASH_COMMANDS_CONFIG_KEY, &commands)
        .map_err(|e| anyhow::anyhow!("Failed to save slash commands: {}", e))
}

fn list_explicit_slash_commands() -> Vec<SlashCommandMapping> {
    Config::global()
        .get_param(SLASH_COMMANDS_CONFIG_KEY)
        .unwrap_or_else(|err| {
            warn!(
                "Failed to load {}: {}. Falling back to empty list.",
                SLASH_COMMANDS_CONFIG_KEY, err
            );
            Vec::new()
        })
}

fn is_reserved_command_name(command: &str) -> bool {
    let normalized = command.trim_start_matches('/');

    crate::agents::execute_commands::list_commands()
        .iter()
        .any(|cmd| cmd.name == normalized)
        || crate::agents::execute_commands::COMPACT_TRIGGERS
            .iter()
            .filter_map(|trigger| trigger.strip_prefix('/'))
            .any(|trigger| trigger == normalized)
}

pub fn list_slash_commands() -> Vec<SlashCommandMapping> {
    let commands = list_explicit_slash_commands();

    let auto_config: AutoSlashCommandsConfig = Config::global()
        .get_param(AUTO_SLASH_COMMANDS_CONFIG_KEY)
        .unwrap_or_default();

    let recipe_dirs = goose_recipe_path_dirs();

    build_slash_commands(commands, &auto_config, &recipe_dirs)
}

fn build_slash_commands(
    mut commands: Vec<SlashCommandMapping>,
    auto_config: &AutoSlashCommandsConfig,
    recipe_dirs: &[PathBuf],
) -> Vec<SlashCommandMapping> {
    if auto_config.from_goose_recipe_path {
        extend_from_recipe_dirs(&mut commands, recipe_dirs);
    }

    commands.retain(|mapping| !is_reserved_command_name(&mapping.command));
    commands
}

pub fn set_recipe_slash_command(recipe_path: PathBuf, command: Option<String>) -> Result<()> {
    // Does not allow registering reserved command names as slash commands.
    let recipe_path_str = recipe_path.to_string_lossy().to_string();

    let mut commands = list_explicit_slash_commands();
    commands.retain(|mapping| mapping.recipe_path != recipe_path_str);

    if let Some(cmd) = command {
        let normalized_cmd = normalize_command(&cmd);

        if !normalized_cmd.is_empty() {
            if is_reserved_command_name(&normalized_cmd) {
                anyhow::bail!(
                    "Slash command '{}' conflicts with a built-in command",
                    normalized_cmd
                );
            }

            commands.push(SlashCommandMapping {
                command: normalized_cmd,
                recipe_path: recipe_path_str,
            });
        }
    }

    save_slash_commands(commands)
}

fn extend_from_recipe_dirs(commands: &mut Vec<SlashCommandMapping>, dirs: &[PathBuf]) {
    let mut seen: HashSet<String> = commands
        .iter()
        .map(|mapping| normalize_command(&mapping.command))
        .collect();

    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };

        let mut paths = entries
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();

        paths.sort();

        for path in paths {
            if !is_supported_slash_command_recipe(&path) {
                continue;
            }

            let Some(command) = command_from_recipe_path(&path) else {
                continue;
            };

            let normalized = normalize_command(&command);

            if !seen.insert(normalized.clone()) {
                continue;
            }

            commands.push(SlashCommandMapping {
                command: normalized,
                recipe_path: path.to_string_lossy().to_string(),
            });
        }
    }
}

fn goose_recipe_path_dirs() -> Vec<PathBuf> {
    let Ok(recipe_path_env) = env::var(GOOSE_RECIPE_PATH_ENV_VAR) else {
        return Vec::new();
    };

    let path_separator = if cfg!(windows) { ';' } else { ':' };

    recipe_path_env
        .split(path_separator)
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .collect()
}

fn is_supported_slash_command_recipe(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };

    if !RECIPE_FILE_EXTENSIONS.contains(&extension) {
        return false;
    }

    let Ok(recipe_content) = std::fs::read_to_string(path) else {
        return false;
    };

    let Some(recipe_dir) = path.parent() else {
        return false;
    };

    let recipe_dir_str = recipe_dir.display().to_string();

    let Ok(validation_result) =
        crate::recipe::validate_recipe::validate_recipe_template_from_content(
            &recipe_content,
            Some(recipe_dir_str),
        )
    else {
        return false;
    };

    let required_param_count = validation_result
        .parameters
        .as_ref()
        .map(|params| params.iter().filter(|p| p.default.is_none()).count())
        .unwrap_or(0);

    required_param_count <= 1
}

fn command_from_recipe_path(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(normalize_command)
}

fn normalize_command(command: &str) -> String {
    command.trim_start_matches('/').to_lowercase()
}

pub fn get_recipe_for_command(command: &str) -> Option<PathBuf> {
    let normalized = normalize_command(command);
    let commands = list_slash_commands();

    commands
        .into_iter()
        .find(|mapping| mapping.command == normalized)
        .map(|mapping| PathBuf::from(mapping.recipe_path))
}

pub fn resolve_slash_command(command: &str) -> Option<Recipe> {
    let recipe_path = get_recipe_for_command(command)?;

    if !recipe_path.exists() {
        return None;
    }
    let recipe_content = std::fs::read_to_string(&recipe_path).ok()?;
    let recipe = Recipe::from_content(&recipe_content).ok()?;

    Some(recipe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn mapping(command: &str, recipe_path: &str) -> SlashCommandMapping {
        SlashCommandMapping {
            command: command.to_string(),
            recipe_path: recipe_path.to_string(),
        }
    }

    fn valid_recipe(title: &str) -> String {
        format!(
            r#"
version: "1.0.0"
title: "{title}"
description: "Test recipe"
parameters:
  - key: args
    input_type: string
    requirement: optional
    description: "User input"
    default: ""
prompt: "Run test recipe with {{{{ args }}}}"
"#
        )
    }

    fn recipe_with_two_required_params() -> String {
        r#"
version: "1.0.0"
title: "Unsupported"
description: "Unsupported recipe"
prompt: "Run unsupported recipe"
parameters:
  - key: first
    input_type: string
    requirement: required
    description: "First input"
  - key: second
    input_type: string
    requirement: required
    description: "Second input"
"#
        .to_string()
    }

    #[test]
    fn auto_slash_commands_disabled_does_not_discover_recipes() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("speckit.plan.yaml"), valid_recipe("Plan")).unwrap();

        let commands = build_slash_commands(
            Vec::new(),
            &AutoSlashCommandsConfig {
                from_goose_recipe_path: false,
            },
            &[dir.path().to_path_buf()],
        );

        assert!(commands.is_empty());
    }

    #[test]
    fn auto_slash_commands_discovers_supported_recipe_files() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("speckit.plan.yaml"), valid_recipe("Plan")).unwrap();
        fs::write(
            dir.path().join("daily-report.json"),
            valid_recipe("Daily Report"),
        )
        .unwrap();
        fs::write(dir.path().join("notes.txt"), "not a recipe").unwrap();

        let commands = build_slash_commands(
            Vec::new(),
            &AutoSlashCommandsConfig {
                from_goose_recipe_path: true,
            },
            &[dir.path().to_path_buf()],
        );

        let names = commands
            .iter()
            .map(|m| m.command.as_str())
            .collect::<Vec<_>>();

        assert!(names.contains(&"speckit.plan"));
        assert!(names.contains(&"daily-report"));
        assert!(!names.contains(&"notes"));
    }

    #[test]
    fn auto_slash_commands_excludes_recipes_with_multiple_required_parameters() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("unsupported.yaml"),
            recipe_with_two_required_params(),
        )
        .unwrap();

        let commands = build_slash_commands(
            Vec::new(),
            &AutoSlashCommandsConfig {
                from_goose_recipe_path: true,
            },
            &[dir.path().to_path_buf()],
        );

        assert!(commands.is_empty());
    }

    #[test]
    fn explicit_slash_commands_take_precedence_over_auto_discovered_commands() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("speckit.plan.yaml"),
            valid_recipe("Auto Plan"),
        )
        .unwrap();

        let explicit_path = "/explicit/speckit.plan.yaml";
        let commands = build_slash_commands(
            vec![mapping("speckit.plan", explicit_path)],
            &AutoSlashCommandsConfig {
                from_goose_recipe_path: true,
            },
            &[dir.path().to_path_buf()],
        );

        let matches = commands
            .iter()
            .filter(|m| m.command == "speckit.plan")
            .collect::<Vec<_>>();

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].recipe_path, explicit_path);
    }

    #[test]
    fn auto_slash_commands_normalizes_file_stems_to_lowercase() {
        let dir = tempdir().unwrap();

        // ###########################################################
        let path = dir.path().join("speckit.plan.yaml");
        fs::write(&path, valid_recipe("Plan")).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let result = crate::recipe::validate_recipe::validate_recipe_template_from_content(
            &content,
            Some(dir.path().display().to_string()),
        );

        assert!(result.is_ok(), "{result:?}");
        // ###########################################################
        fs::write(dir.path().join("Speckit.Plan.yaml"), valid_recipe("Plan")).unwrap();

        let commands = build_slash_commands(
            Vec::new(),
            &AutoSlashCommandsConfig {
                from_goose_recipe_path: true,
            },
            &[dir.path().to_path_buf()],
        );

        assert_eq!(commands[0].command, "speckit.plan");
    }

    #[test]
    fn auto_slash_commands_uses_deterministic_order_before_deduping() {
        let dir = tempdir().unwrap();
        let json_path = dir.path().join("build.json");
        let yaml_path = dir.path().join("build.yaml");

        fs::write(&yaml_path, valid_recipe("Build YAML")).unwrap();
        fs::write(&json_path, valid_recipe("Build JSON")).unwrap();

        let commands = build_slash_commands(
            Vec::new(),
            &AutoSlashCommandsConfig {
                from_goose_recipe_path: true,
            },
            &[dir.path().to_path_buf()],
        );

        let build = commands.iter().find(|m| m.command == "build").unwrap();

        // Paths are sorted before first-seen-wins dedupe, so build.json wins
        // lexicographically over build.yaml.
        assert_eq!(PathBuf::from(&build.recipe_path), json_path);
    }

    #[test]
    fn reserved_command_names_are_filtered_from_effective_slash_commands() {
        let commands = build_slash_commands(
            vec![mapping("compact", "/explicit/compact.yaml")],
            &AutoSlashCommandsConfig {
                from_goose_recipe_path: false,
            },
            &[],
        );

        assert!(commands.is_empty());
    }
}
