use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tracing::warn;

use super::types::{SlashCommandEntry, SlashCommandSource};
use super::util::normalize_command_name;
use crate::config::Config;
use crate::recipe::build_recipe::{build_recipe_from_template, RecipeError};
use crate::recipe::{RecipeParameter, RecipeParameterRequirement, Response};

const SLASH_COMMANDS_CONFIG_KEY: &str = "slash_commands";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommandMapping {
    pub command: String,
    pub recipe_path: String,
}

pub fn list_commands() -> Vec<SlashCommandMapping> {
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

fn save_slash_commands(commands: Vec<SlashCommandMapping>) -> Result<()> {
    Config::global()
        .set_param(SLASH_COMMANDS_CONFIG_KEY, &commands)
        .map_err(|e| anyhow::anyhow!("Failed to save slash commands: {}", e))
}

pub fn set_recipe_slash_command(recipe_path: PathBuf, command: Option<String>) -> Result<()> {
    let recipe_path_str = recipe_path.to_string_lossy().to_string();

    let mut commands = list_commands();
    commands.retain(|mapping| mapping.recipe_path != recipe_path_str);

    if let Some(cmd) = command {
        let normalized_cmd = cmd.trim_start_matches('/').to_lowercase();
        if !normalized_cmd.is_empty() {
            commands.push(SlashCommandMapping {
                command: normalized_cmd,
                recipe_path: recipe_path_str,
            });
        }
    }

    save_slash_commands(commands)
}

pub fn get_recipe_for_command(command: &str) -> Option<PathBuf> {
    let normalized = command.trim_start_matches('/').to_lowercase();
    let commands = list_commands();
    commands
        .into_iter()
        .find(|mapping| mapping.command == normalized)
        .map(|mapping| PathBuf::from(mapping.recipe_path))
}

pub(super) fn commands_from_mappings(mappings: Vec<SlashCommandMapping>) -> Vec<SlashCommandEntry> {
    mappings
        .into_iter()
        .filter_map(|mapping| {
            let name = normalize_command_name(&mapping.command);
            if name.is_empty() {
                return None;
            }

            let metadata = recipe_entry(&mapping.recipe_path)?;

            Some(SlashCommandEntry {
                name,
                description: metadata.description,
                source: SlashCommandSource::Recipe,
                input_hint: metadata.input_hint,
            })
        })
        .collect()
}

struct RecipeCommandMetadata {
    description: String,
    input_hint: Option<String>,
}

fn recipe_entry(recipe_path: &str) -> Option<RecipeCommandMetadata> {
    let recipe_path = PathBuf::from(recipe_path);
    if !recipe_path.exists() {
        return None;
    }

    let recipe_content = std::fs::read_to_string(&recipe_path).ok()?;
    let recipe_dir = recipe_path.parent()?;
    let recipe_dir_str = recipe_dir.display().to_string();
    let validation_result = crate::recipe::validate_recipe::validate_recipe_template_from_content(
        &recipe_content,
        Some(recipe_dir_str),
    )
    .ok()?;

    Some(RecipeCommandMetadata {
        description: validation_result.description,
        input_hint: input_hint_for_recipe(validation_result.parameters.as_ref()),
    })
}

fn input_hint_for_recipe(params: Option<&Vec<RecipeParameter>>) -> Option<String> {
    let params = params?;
    if params.is_empty() {
        return None;
    }

    let mut required = Vec::new();
    let mut optional = Vec::new();

    for p in params {
        match p.requirement {
            RecipeParameterRequirement::Required | RecipeParameterRequirement::UserPrompt => {
                required.push(format!("<{}>", p.key));
            }
            RecipeParameterRequirement::Optional => {
                optional.push(format!("[--{} <{}>]", p.key, p.key));
            }
        }
    }

    Some(
        required
            .into_iter()
            .chain(optional)
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn invalid_recipe_msg(command: &str, reason: impl std::fmt::Display) -> String {
    format!("Recipe /{} is not valid: {}", command, reason)
}

pub fn resolve_command(
    command: &str,
    params_str: &str,
) -> Result<Option<(Option<Response>, String)>, String> {
    let full_command = format!("/{}", command);
    let Some(recipe_path) = get_recipe_for_command(&full_command) else {
        return Ok(None);
    };

    if !recipe_path.exists() {
        return Ok(None);
    }

    let recipe_content =
        std::fs::read_to_string(&recipe_path).map_err(|e| invalid_recipe_msg(command, e))?;

    let recipe_dir = recipe_path
        .parent()
        .ok_or_else(|| invalid_recipe_msg(command, "unable to resolve recipe directory"))?;

    let recipe_dir_str = recipe_dir.display().to_string();
    let validation_result = crate::recipe::validate_recipe::validate_recipe_template_from_content(
        &recipe_content,
        Some(recipe_dir_str),
    )
    .map_err(|e| invalid_recipe_msg(command, e))?;

    let empty_params: Vec<RecipeParameter> = Vec::new();
    let all_params = validation_result
        .parameters
        .as_ref()
        .unwrap_or(&empty_params);
    let required: Vec<&RecipeParameter> = all_params
        .iter()
        .filter(|p| {
            matches!(
                p.requirement,
                RecipeParameterRequirement::Required | RecipeParameterRequirement::UserPrompt
            )
        })
        .collect();
    let optional: Vec<&RecipeParameter> = all_params
        .iter()
        .filter(|p| matches!(p.requirement, RecipeParameterRequirement::Optional))
        .collect();

    let param_values: Vec<(String, String)> = if params_str.is_empty() {
        vec![]
    } else if required.len() == 1 && optional.is_empty() {
        vec![(required[0].key.clone(), params_str.to_string())]
    } else {
        parse_recipe_args(params_str, &required, &optional)
            .map_err(|e| format!("Recipe /{}: {}", command, e))?
    };

    let recipe = build_recipe_from_template(
        recipe_content,
        recipe_dir,
        param_values,
        None::<fn(&str, &str) -> Result<String>>,
    )
    .map_err(|e| match e {
        RecipeError::MissingParams { parameters } => invalid_recipe_msg(
            command,
            format!("requires parameter(s): {}.", parameters.join(", "),),
        ),
        other => invalid_recipe_msg(command, other),
    })?;

    let prompt = [recipe.instructions.as_deref(), recipe.prompt.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n\n");

    Ok(Some((recipe.response, prompt)))
}

fn parse_recipe_args(
    params_str: &str,
    required: &[&RecipeParameter],
    optional: &[&RecipeParameter],
) -> Result<Vec<(String, String)>> {
    let tokens = crate::utils::split_command_args(params_str)?;
    let required_keys: HashSet<&str> = required.iter().map(|p| p.key.as_str()).collect();
    let optional_keys: HashSet<&str> = optional.iter().map(|p| p.key.as_str()).collect();

    let mut positionals: Vec<String> = Vec::new();
    let mut flags: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        if let Some(flag) = token.strip_prefix("--") {
            if required_keys.contains(flag) {
                return Err(anyhow!(
                    "Parameter '{}' is required; pass it positionally, not as --{}",
                    flag,
                    flag
                ));
            }
            if !optional_keys.contains(flag) {
                return Err(anyhow!("Unknown parameter: --{}", flag));
            }
            let value = tokens
                .get(i + 1)
                .filter(|v| !v.starts_with("--"))
                .ok_or_else(|| anyhow!("Missing value for --{}", flag))?;
            flags.push((flag.to_string(), value.clone()));
            i += 2;
        } else {
            positionals.push(token.clone());
            i += 1;
        }
    }

    let mut result = Vec::new();
    if required.len() == 1 && !positionals.is_empty() {
        result.push((required[0].key.clone(), positionals.join(" ")));
    } else {
        for (idx, value) in positionals.into_iter().enumerate() {
            if idx >= required.len() {
                return Err(anyhow!("Unexpected positional argument: {}", value));
            }
            result.push((required[idx].key.clone(), value));
        }
    }
    result.extend(flags);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::RecipeParameterInputType;
    use tempfile::TempDir;

    fn required_param(key: &str) -> RecipeParameter {
        RecipeParameter {
            key: key.to_string(),
            input_type: RecipeParameterInputType::String,
            requirement: RecipeParameterRequirement::Required,
            description: format!("{key} parameter"),
            default: None,
            options: None,
        }
    }

    fn optional_param(key: &str) -> RecipeParameter {
        RecipeParameter {
            key: key.to_string(),
            input_type: RecipeParameterInputType::String,
            requirement: RecipeParameterRequirement::Optional,
            description: format!("{key} parameter"),
            default: Some("default".to_string()),
            options: None,
        }
    }

    #[test]
    fn parse_recipe_args_maps_required_positionals_and_optional_flags() {
        let component = required_param("component");
        let from = required_param("from");
        let to = optional_param("to");
        let scope = optional_param("scope");
        let required = vec![&component, &from];
        let optional = vec![&to, &scope];

        let parsed = parse_recipe_args(
            r#""Button Group" old-lib --to new-lib"#,
            &required,
            &optional,
        )
        .unwrap();

        assert_eq!(
            parsed,
            vec![
                ("component".to_string(), "Button Group".to_string()),
                ("from".to_string(), "old-lib".to_string()),
                ("to".to_string(), "new-lib".to_string()),
            ]
        );
    }

    #[test]
    fn parse_recipe_args_greedy_captures_multi_word_required_with_optional_flag() {
        let location = required_param("location");
        let theme = optional_param("theme");
        let required = vec![&location];
        let optional = vec![&theme];

        let parsed =
            parse_recipe_args("Melbourne weather --theme culture", &required, &optional).unwrap();

        assert_eq!(
            parsed,
            vec![
                ("location".to_string(), "Melbourne weather".to_string()),
                ("theme".to_string(), "culture".to_string()),
            ]
        );
    }

    #[test]
    fn parse_recipe_args_keeps_apostrophes_in_unquoted_values() {
        let topic = required_param("topic");
        let theme = optional_param("theme");
        let required = vec![&topic];
        let optional = vec![&theme];

        let parsed = parse_recipe_args(
            "O'Reilly's guide --theme author's-pick",
            &required,
            &optional,
        )
        .unwrap();

        assert_eq!(
            parsed,
            vec![
                ("topic".to_string(), "O'Reilly's guide".to_string()),
                ("theme".to_string(), "author's-pick".to_string()),
            ]
        );
    }

    #[test]
    fn parse_recipe_args_greedy_captures_multi_word_required_without_flags() {
        let location = required_param("location");
        let theme = optional_param("theme");
        let required = vec![&location];
        let optional = vec![&theme];

        let parsed = parse_recipe_args("Melbourne weather", &required, &optional).unwrap();

        assert_eq!(
            parsed,
            vec![("location".to_string(), "Melbourne weather".to_string())]
        );
    }

    #[test]
    fn parse_recipe_args_greedy_still_accepts_quoted_required_value() {
        let location = required_param("location");
        let theme = optional_param("theme");
        let required = vec![&location];
        let optional = vec![&theme];

        let parsed = parse_recipe_args(
            r#""Melbourne weather" --theme culture"#,
            &required,
            &optional,
        )
        .unwrap();

        assert_eq!(
            parsed,
            vec![
                ("location".to_string(), "Melbourne weather".to_string()),
                ("theme".to_string(), "culture".to_string()),
            ]
        );
    }

    #[test]
    fn parse_recipe_args_greedy_handles_flag_before_required_positional() {
        let location = required_param("location");
        let theme = optional_param("theme");
        let required = vec![&location];
        let optional = vec![&theme];

        let parsed =
            parse_recipe_args("--theme culture Melbourne weather", &required, &optional).unwrap();

        assert_eq!(
            parsed,
            vec![
                ("location".to_string(), "Melbourne weather".to_string()),
                ("theme".to_string(), "culture".to_string()),
            ]
        );
    }

    #[test]
    fn parse_recipe_args_allows_values_containing_equals() {
        let component = required_param("component");
        let note = optional_param("note");
        let required = vec![&component];
        let optional = vec![&note];

        let parsed = parse_recipe_args(r#"Button --note "a=b""#, &required, &optional).unwrap();

        assert_eq!(
            parsed,
            vec![
                ("component".to_string(), "Button".to_string()),
                ("note".to_string(), "a=b".to_string()),
            ]
        );
    }

    #[test]
    fn parse_recipe_args_errors_when_flag_value_is_another_flag() {
        let component = required_param("component");
        let from = required_param("from");
        let to = optional_param("to");
        let scope = optional_param("scope");
        let required = vec![&component, &from];
        let optional = vec![&to, &scope];

        let err =
            parse_recipe_args("Button old-lib --to --scope all", &required, &optional).unwrap_err();

        assert!(err.to_string().contains("Missing value for --to"));
    }

    #[test]
    fn parse_recipe_args_errors_on_extra_positionals() {
        let component = required_param("component");
        let from = required_param("from");
        let required = vec![&component, &from];

        let err = parse_recipe_args("Button old-lib extra", &required, &[]).unwrap_err();

        assert!(err
            .to_string()
            .contains("Unexpected positional argument: extra"));
    }

    #[test]
    fn parse_recipe_args_rejects_required_param_passed_as_flag() {
        let component = required_param("component");
        let from = required_param("from");
        let required = vec![&component, &from];

        let err = parse_recipe_args("--component Button old-lib", &required, &[]).unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("'component' is required") && msg.contains("--component"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn parse_recipe_args_errors_on_unknown_flag() {
        let component = required_param("component");
        let required = vec![&component];

        let err = parse_recipe_args("Button --unknown value", &required, &[]).unwrap_err();

        assert!(err.to_string().contains("Unknown parameter: --unknown"));
    }

    #[test]
    fn commands_from_mappings_use_recipe_description() {
        let tmp = TempDir::new().unwrap();
        let recipe_path = tmp.path().join("review.yaml");
        std::fs::write(
            &recipe_path,
            "version: 1.0.0\ntitle: Review Recipe\ndescription: Review with a recipe\ninstructions: Review the change\n",
        )
        .unwrap();

        let commands = commands_from_mappings(vec![SlashCommandMapping {
            command: "/review".to_string(),
            recipe_path: recipe_path.to_string_lossy().to_string(),
        }]);

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "review");
        assert_eq!(commands[0].description, "Review with a recipe");
        assert_eq!(commands[0].source, SlashCommandSource::Recipe);
    }

    #[test]
    fn commands_from_mappings_omit_hint_for_no_param_recipe() {
        let tmp = TempDir::new().unwrap();
        let recipe_path = tmp.path().join("status.yaml");
        std::fs::write(
            &recipe_path,
            "version: 1.0.0\ntitle: Status\ndescription: Check status\ninstructions: Check status\n",
        )
        .unwrap();

        let commands = commands_from_mappings(vec![SlashCommandMapping {
            command: "status".to_string(),
            recipe_path: recipe_path.to_string_lossy().to_string(),
        }]);

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].input_hint, None);
    }

    #[test]
    fn commands_from_mappings_render_one_required_param_hint() {
        let tmp = TempDir::new().unwrap();
        let recipe_path = tmp.path().join("review.yaml");
        std::fs::write(
            &recipe_path,
            "version: 1.0.0\ntitle: Review\ndescription: Review target\ninstructions: \"Review {{ target }}\"\nparameters:\n  - key: target\n    input_type: string\n    requirement: required\n    description: Target\n",
        )
        .unwrap();

        let commands = commands_from_mappings(vec![SlashCommandMapping {
            command: "review".to_string(),
            recipe_path: recipe_path.to_string_lossy().to_string(),
        }]);

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].input_hint.as_deref(), Some("<target>"));
    }

    #[test]
    fn commands_from_mappings_do_not_special_case_args_hint() {
        let tmp = TempDir::new().unwrap();
        let recipe_path = tmp.path().join("deploy.yaml");
        std::fs::write(
            &recipe_path,
            "version: 1.0.0\ntitle: Deploy\ndescription: Deploy\ninstructions: \"Deploy {{ component }} with {{ args }}\"\nparameters:\n  - key: component\n    input_type: string\n    requirement: required\n    description: Component\n  - key: args\n    input_type: string\n    requirement: optional\n    default: default args\n    description: Args\n",
        )
        .unwrap();

        let commands = commands_from_mappings(vec![SlashCommandMapping {
            command: "deploy".to_string(),
            recipe_path: recipe_path.to_string_lossy().to_string(),
        }]);

        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].input_hint.as_deref(),
            Some("<component> [--args <args>]")
        );
    }

    #[test]
    fn commands_from_mappings_skip_missing_and_invalid_recipes() {
        let tmp = TempDir::new().unwrap();
        let invalid_recipe_path = tmp.path().join("invalid.yaml");
        std::fs::write(&invalid_recipe_path, "not: a recipe").unwrap();

        let commands = commands_from_mappings(vec![
            SlashCommandMapping {
                command: "missing".to_string(),
                recipe_path: tmp
                    .path()
                    .join("missing.yaml")
                    .to_string_lossy()
                    .to_string(),
            },
            SlashCommandMapping {
                command: "invalid".to_string(),
                recipe_path: invalid_recipe_path.to_string_lossy().to_string(),
            },
        ]);

        assert!(commands.is_empty());
    }

    #[test]
    fn commands_from_mappings_render_multi_param_hint() {
        let tmp = TempDir::new().unwrap();
        let recipe_path = tmp.path().join("deploy.yaml");
        std::fs::write(
            &recipe_path,
            "version: 1.0.0\ntitle: Deploy\ndescription: Deploy a service\ninstructions: \"Deploy {{ component }} from {{ from }} to {{ to }} scope {{ scope }}\"\nparameters:\n  - key: component\n    input_type: string\n    requirement: required\n    description: Component\n  - key: from\n    input_type: string\n    requirement: required\n    description: From\n  - key: to\n    input_type: string\n    requirement: optional\n    default: prod\n    description: To\n  - key: scope\n    input_type: string\n    requirement: optional\n    default: all\n    description: Scope\n",
        )
        .unwrap();

        let commands = commands_from_mappings(vec![SlashCommandMapping {
            command: "deploy".to_string(),
            recipe_path: recipe_path.to_string_lossy().to_string(),
        }]);

        assert_eq!(commands.len(), 1);
        assert_eq!(
            commands[0].input_hint.as_deref(),
            Some("<component> <from> [--to <to>] [--scope <scope>]")
        );
    }
}
