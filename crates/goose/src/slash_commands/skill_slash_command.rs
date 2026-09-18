use std::path::Path;

use goose_sdk_types::custom_requests::{SourceEntry, SourceType};

use super::types::{SlashCommandEntry, SlashCommandSource};
use super::util::normalize_command_name;

pub fn list_commands(working_dir: Option<&Path>) -> Vec<SlashCommandEntry> {
    commands_from_sources(crate::skills::list_installed_skills(working_dir))
}

pub fn format_installed_skills(working_dir: Option<&Path>) -> String {
    let sources = crate::skills::list_installed_skills(working_dir);
    let skills: Vec<_> = sources
        .iter()
        .filter(|s| matches!(s.source_type, SourceType::Skill | SourceType::BuiltinSkill))
        .collect();

    let mut output = String::new();
    if skills.is_empty() {
        output.push_str("No skills installed.\n\n");
        output.push_str("Skills are loaded from SKILL.md files in:\n");
        output.push_str("  - ~/.agents/skills/ (global)\n");
        output.push_str("  - ~/.agents/plugins/*/skills/ (installed plugins)\n");
        output.push_str("  - .agents/skills/ (in current project)\n");
    } else {
        output.push_str(&format!("**Installed skills ({}):**\n\n", skills.len()));
        for skill in &skills {
            let kind_label = if skill.source_type == SourceType::BuiltinSkill {
                " *(builtin)*"
            } else {
                ""
            };
            output.push_str(&format!(
                "- **{}**{}: {}\n",
                skill.name, kind_label, skill.description
            ));
        }
    }
    output
}

pub fn resolve_command(
    command: &str,
    params_str: &str,
    working_dir: Option<&Path>,
) -> Result<Option<String>, String> {
    let fallback = working_dir
        .is_none()
        .then(|| std::env::current_dir().ok())
        .flatten();
    let Some(skill) =
        crate::skills::discover_skills_with_details(working_dir.or(fallback.as_deref()))
            .into_iter()
            .find(|skill| skill.name.eq_ignore_ascii_case(command))
    else {
        return Ok(None);
    };

    let args = (!params_str.is_empty()).then_some(params_str);
    let prompt = skill
        .loaded_context_with_args(args)
        .map_err(|e| format!("Skill /{}: {}", command, e))?;

    Ok(Some(prompt))
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
    use goose_sdk_types::custom_requests::SourceType;
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

    #[cfg(unix)]
    #[test]
    fn linked_skill_command_renders_physical_supporting_paths() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let installed = root.join(".agents/skills/slash-linked-helper");
        let target = root.join("physical-skill");
        std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
        std::fs::create_dir(&target).unwrap();
        std::fs::write(
            target.join("SKILL.md"),
            "---\nname: slash-linked-helper\ndescription: Helper\n---\nUse guide.md.",
        )
        .unwrap();
        std::fs::write(target.join("guide.md"), "guidance").unwrap();
        std::os::unix::fs::symlink(&target, &installed).unwrap();

        let prompt = resolve_command("slash-linked-helper", "", Some(&root))
            .unwrap()
            .unwrap();
        assert!(prompt.contains(&format!("Skill directory: {}", target.display())));
        assert!(prompt.contains(&format!("guide.md → {}", target.join("guide.md").display())));
        assert!(!prompt.contains(&installed.to_string_lossy().to_string()));
        let listed = crate::skills::list_installed_skills(Some(&root));
        assert_eq!(
            listed
                .iter()
                .find(|s| s.name == "slash-linked-helper")
                .unwrap()
                .path,
            installed.to_string_lossy()
        );
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
