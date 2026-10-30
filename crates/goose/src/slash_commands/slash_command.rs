use std::collections::HashSet;
use std::path::Path;

use super::types::{SlashCommandEntry, SlashCommandSource};
use super::util::normalize_command_name;
use crate::agents::execute_commands;

pub fn list_builtin_commands() -> Vec<SlashCommandEntry> {
    crate::agents::execute_commands::list_commands()
        .iter()
        .map(|command| SlashCommandEntry {
            name: command.name.to_string(),
            description: command.description.to_string(),
            source: SlashCommandSource::Builtin,
            source_path: None,
            input_hint: None,
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

pub fn is_builtin_command(command: &str) -> bool {
    let command = normalize_command_name(command);
    execute_commands::list_commands()
        .iter()
        .any(|builtin| builtin.name == command)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slash_commands::recipe_slash_command::{self, SlashCommandMapping};
    use crate::slash_commands::skill_slash_command;
    use tempfile::TempDir;

    #[test]
    fn lists_acp_safe_builtin_commands() {
        let commands = list_builtin_commands();
        let names: Vec<_> = commands
            .iter()
            .map(|command| command.name.as_str())
            .collect();

        assert_eq!(
            names,
            vec![
                "prompts", "prompt", "compact", "clear", "skills", "doctor", "goal", "grind",
                "status"
            ]
        );
        assert!(commands
            .iter()
            .all(|command| command.source == SlashCommandSource::Builtin));
    }

    fn entry(name: &str, source: SlashCommandSource) -> SlashCommandEntry {
        SlashCommandEntry {
            name: name.to_string(),
            description: format!("{name} description"),
            source,
            source_path: None,
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

    fn write_skill(dir: &Path, name: &str) {
        let skill_dir = dir.join(".agents").join("skills").join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name} skill\n---\n{name} body"),
        )
        .unwrap();
    }

    fn recipe_mappings(dir: &Path, commands: &[&str]) -> String {
        let mappings: Vec<_> = commands
            .iter()
            .map(|command| {
                let recipe_path = dir.join(format!("{command}.yaml"));
                std::fs::write(
                    &recipe_path,
                    format!(
                        "version: 1.0.0\ntitle: {command}\ndescription: {command} recipe\nprompt: {command} prompt\n"
                    ),
                )
                .unwrap();
                SlashCommandMapping {
                    command: command.to_string(),
                    recipe_path: recipe_path.display().to_string(),
                }
            })
            .collect();
        serde_json::to_string(&mappings).unwrap()
    }

    fn runs_as_skill(command: &str, working_dir: &Path) -> bool {
        skill_slash_command::resolve_command(command, "", Some(working_dir))
            .unwrap()
            .is_some()
    }

    fn runs_as_recipe(command: &str) -> bool {
        recipe_slash_command::resolve_command(command, "")
            .unwrap()
            .is_some()
    }

    #[test]
    fn builtin_names_never_run_as_recipes_or_skills() {
        let tmp = TempDir::new().unwrap();
        write_skill(tmp.path(), "compact");
        let mappings = recipe_mappings(tmp.path(), &["compact"]);
        let _guard = env_lock::lock_env([("SLASH_COMMANDS", Some(mappings.as_str()))]);

        for command in ["compact", "Compact", "/COMPACT"] {
            assert!(is_builtin_command(command), "{command}");
            assert!(!runs_as_recipe(command), "{command}");
            assert!(!runs_as_skill(command, tmp.path()), "{command}");
        }
        assert!(!is_builtin_command("review"));
    }

    #[test]
    fn recipe_wins_over_skill_with_the_same_name() {
        let tmp = TempDir::new().unwrap();
        write_skill(tmp.path(), "precedence-shared");
        let mappings = recipe_mappings(tmp.path(), &["precedence-shared"]);
        let _guard = env_lock::lock_env([("SLASH_COMMANDS", Some(mappings.as_str()))]);

        for command in ["precedence-shared", "Precedence-Shared"] {
            assert!(runs_as_recipe(command), "{command}");
            assert!(!runs_as_skill(command, tmp.path()), "{command}");
        }
    }

    #[test]
    fn skill_runs_regardless_of_case() {
        let tmp = TempDir::new().unwrap();
        write_skill(tmp.path(), "precedence-skill");
        write_skill(tmp.path(), "Résumé-Écrire");
        let _guard = env_lock::lock_env([("SLASH_COMMANDS", Some("[]"))]);

        for command in [
            "precedence-skill",
            "PRECEDENCE-SKILL",
            "Résumé-Écrire",
            "résumé-écrire",
            "RÉSUMÉ-ÉCRIRE",
        ] {
            assert!(runs_as_skill(command, tmp.path()), "{command}");
        }
        assert!(!runs_as_skill("precedence-missing", tmp.path()));
    }

    #[test]
    fn recipe_without_file_does_not_block_skill() {
        let tmp = TempDir::new().unwrap();
        write_skill(tmp.path(), "precedence-stale");
        let mappings = serde_json::to_string(&[SlashCommandMapping {
            command: "precedence-stale".to_string(),
            recipe_path: tmp.path().join("missing.yaml").display().to_string(),
        }])
        .unwrap();
        let _guard = env_lock::lock_env([("SLASH_COMMANDS", Some(mappings.as_str()))]);

        assert!(!runs_as_recipe("precedence-stale"));
        assert!(runs_as_skill("precedence-stale", tmp.path()));
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
