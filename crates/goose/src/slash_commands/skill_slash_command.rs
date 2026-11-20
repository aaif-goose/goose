use std::path::Path;

use goose_acp_types::custom_requests::SourceEntry;

use super::recipe_slash_command::runnable_recipes;
use super::slash_command::is_builtin_command;
use super::types::{SlashCommandEntry, SlashCommandSource};
use super::util::normalize_command_name;

pub fn list_commands(working_dir: Option<&Path>) -> Vec<SlashCommandEntry> {
    commands_from_sources(crate::skills::list_installed_skills(working_dir))
}

pub fn runnable_skills(working_dir: Option<&Path>) -> Vec<(String, SourceEntry)> {
    let recipes: Vec<String> = runnable_recipes()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    crate::skills::list_installed_skills(working_dir)
        .into_iter()
        .filter_map(|skill| {
            let name = normalize_command_name(&skill.name);
            (!name.is_empty() && !is_builtin_command(&name) && !recipes.contains(&name))
                .then_some((name, skill))
        })
        .collect()
}

pub(super) fn commands_from_sources(sources: Vec<SourceEntry>) -> Vec<SlashCommandEntry> {
    sources
        .into_iter()
        .filter_map(|source| {
            let name = normalize_command_name(&source.name);
            if name.is_empty() {
                return None;
            }
            let input_hint = crate::skills::skill_argument_hint(&source);

            Some(SlashCommandEntry {
                name,
                description: source.description,
                source: SlashCommandSource::Skill,
                source_path: None,
                input_hint,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose_acp_types::custom_requests::SourceType;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn commands_from_sources_marks_entries_as_skill() {
        let commands = commands_from_sources(vec![
            source_entry(SourceType::Skill, "review", "Review code"),
            source_entry(SourceType::Skill, "summarize", "Summarize text"),
        ]);

        let names: Vec<_> = commands.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["review", "summarize"]);
        assert!(commands
            .iter()
            .all(|c| c.source == SlashCommandSource::Skill));
    }

    #[test]
    fn commands_from_sources_normalizes_names() {
        let commands = commands_from_sources(vec![source_entry(
            SourceType::Skill,
            "/Code-Review",
            "Review",
        )]);

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "code-review");
    }

    #[test]
    fn commands_from_sources_skips_empty_names() {
        let commands =
            commands_from_sources(vec![source_entry(SourceType::Skill, "/", "Empty name")]);

        assert!(commands.is_empty());
    }

    #[test]
    fn list_commands_loads_project_skill_from_disk() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp
            .path()
            .join(".agents")
            .join("skills")
            .join("code-review");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: code-review\ndescription: Review changed code\nmetadata:\n  argument-hint: \"[task]\"\n  arguments:\n    - task\n---\nReview the diff.",
        )
        .unwrap();

        let commands = list_commands(Some(tmp.path()));
        let command = commands
            .iter()
            .find(|command| command.name == "code-review")
            .expect("project skill should be listed");

        assert_eq!(command.description, "Review changed code");
        assert_eq!(command.source, SlashCommandSource::Skill);
        assert_eq!(command.input_hint.as_deref(), Some("[task]"));
    }

    fn source_entry(source_type: SourceType, name: &str, description: &str) -> SourceEntry {
        SourceEntry {
            source_type,
            name: name.to_string(),
            description: description.to_string(),
            content: String::new(),
            path: String::new(),
            global: false,
            writable: false,
            supporting_files: Vec::new(),
            properties: HashMap::new(),
        }
    }
}
