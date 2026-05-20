use std::path::PathBuf;

use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::warn;
use utoipa::ToSchema;

use crate::config::Config;
use crate::recipe::Recipe;

const SLASH_COMMANDS_CONFIG_KEY: &str = "slash_commands";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
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

pub fn resolve_slash_command(command: &str) -> Option<Recipe> {
    let recipe_path = get_recipe_for_command(command)?;

    if !recipe_path.exists() {
        return None;
    }
    let recipe_content = std::fs::read_to_string(&recipe_path).ok()?;
    let recipe = Recipe::from_content(&recipe_content).ok()?;

    Some(recipe)
}
