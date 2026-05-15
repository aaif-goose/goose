use std::collections::HashSet;
use std::path::Path;

use super::types::{SlashCommandEntry, SlashCommandSource};
use super::util::normalize_command_name;

pub fn list_builtin_commands() -> Vec<SlashCommandEntry> {
    crate::agents::execute_commands::list_commands()
        .iter()
        .map(|command| SlashCommandEntry {
            name: command.name.to_string(),
            description: command.description.to_string(),
            source: SlashCommandSource::Builtin,
            input_hint: builtin_input_hint(command.name).map(str::to_string),
        })
        .collect()
}

pub fn list_acp_commands(working_dir: Option<&Path>) -> Vec<SlashCommandEntry> {
    merge_command_sources(
        list_builtin_commands(),
        super::recipe_slash_command::commands_from_mappings(
            super::recipe_slash_command::list_commands(),
        ),
        super::skill_slash_command::list_commands(working_dir),
    )
}

pub(super) fn merge_command_sources(
    builtins: Vec<SlashCommandEntry>,
    recipes: Vec<SlashCommandEntry>,
    skills: Vec<SlashCommandEntry>,
) -> Vec<SlashCommandEntry> {
    let mut commands = builtins;
    let mut reserved_names: HashSet<String> = commands
        .iter()
        .map(|command| normalize_command_name(&command.name))
        .collect();

    for command in recipes {
        if reserved_names.insert(normalize_command_name(&command.name)) {
            commands.push(command);
        }
    }

    commands.extend(
        skills
            .into_iter()
            .filter(|command| !reserved_names.contains(&normalize_command_name(&command.name))),
    );
    commands
}

fn builtin_input_hint(command: &str) -> Option<&'static str> {
    match command {
        "prompt" => Some("<name> [--info] [key=value...]"),
        "prompts" => Some("[--extension <name>]"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_acp_safe_builtin_commands() {
        let commands = list_builtin_commands();
        let names: Vec<_> = commands
            .iter()
            .map(|command| command.name.as_str())
            .collect();

        assert_eq!(
            names,
            vec!["prompts", "prompt", "compact", "clear", "skills", "doctor"]
        );
        assert!(commands
            .iter()
            .all(|command| command.source == SlashCommandSource::Builtin));
    }

    #[test]
    fn includes_input_hints_for_argument_taking_builtins() {
        let commands = list_builtin_commands();
        let prompt = commands
            .iter()
            .find(|command| command.name == "prompt")
            .expect("prompt command should be listed");
        let prompts = commands
            .iter()
            .find(|command| command.name == "prompts")
            .expect("prompts command should be listed");
        let compact = commands
            .iter()
            .find(|command| command.name == "compact")
            .expect("compact command should be listed");

        assert_eq!(
            prompt.input_hint.as_deref(),
            Some("<name> [--info] [key=value...]")
        );
        assert_eq!(prompts.input_hint.as_deref(), Some("[--extension <name>]"));
        assert_eq!(compact.input_hint, None);
    }

    fn entry(name: &str, source: SlashCommandSource) -> SlashCommandEntry {
        SlashCommandEntry {
            name: name.to_string(),
            description: format!("{name} description"),
            source,
            input_hint: None,
        }
    }

    #[test]
    fn merge_recipe_wins_over_skill_on_name_collision() {
        let merged = merge_command_sources(
            vec![entry("compact", SlashCommandSource::Builtin)],
            vec![entry("review", SlashCommandSource::Recipe)],
            vec![entry("review", SlashCommandSource::Skill)],
        );

        let review: Vec<_> = merged.iter().filter(|c| c.name == "review").collect();
        assert_eq!(review.len(), 1);
        assert_eq!(review[0].source, SlashCommandSource::Recipe);
    }

    #[test]
    fn merge_builtin_wins_over_recipe_and_skill() {
        let merged = merge_command_sources(
            vec![entry("compact", SlashCommandSource::Builtin)],
            vec![entry("compact", SlashCommandSource::Recipe)],
            vec![entry("compact", SlashCommandSource::Skill)],
        );

        let compact: Vec<_> = merged.iter().filter(|c| c.name == "compact").collect();
        assert_eq!(compact.len(), 1);
        assert_eq!(compact[0].source, SlashCommandSource::Builtin);
    }

    #[test]
    fn merge_dedupes_by_normalized_name() {
        let merged = merge_command_sources(
            vec![entry("Compact", SlashCommandSource::Builtin)],
            vec![entry("/compact", SlashCommandSource::Recipe)],
            vec![entry("COMPACT", SlashCommandSource::Skill)],
        );

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].source, SlashCommandSource::Builtin);
    }
}
