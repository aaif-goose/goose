use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::skills::SkillSource;
use goose_sdk_types::custom_requests::SourceEntry;

use crate::session::Session;

pub(crate) const NO_SKILLS_HELP: &str = "No skills installed.\n\n\
    Skills are loaded from SKILL.md files in:\n  \
    - ~/.agents/skills/ (global)\n  \
    - ~/.agents/plugins/*/skills/ (installed plugins)\n  \
    - .agents/skills/ (in current project)\n";

pub(crate) struct GooseSkillSource;

#[async_trait]
impl SkillSource<Session> for GooseSkillSource {
    async fn skills(&self, session: &Session) -> Result<Vec<SourceEntry>> {
        crate::skills::skills_for_session(&session.working_dir)
    }

    async fn read_supporting_file(
        &self,
        skill: &SourceEntry,
        relative_path: &Path,
    ) -> Result<String> {
        let skill_dir = Path::new(&skill.path).canonicalize()?;
        let name = format!(
            "{}/{}",
            skill.name,
            relative_path.to_string_lossy().replace('\\', "/")
        );
        Ok(crate::skills::load_supporting_file(
            &skill_dir,
            relative_path,
            &name,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose_sdk_types::custom_requests::SourceType;
    use std::collections::HashMap;

    fn skill_at(skill_dir: &Path, file: &Path) -> SourceEntry {
        SourceEntry {
            source_type: SourceType::Skill,
            name: "test-skill".to_string(),
            description: String::new(),
            content: String::new(),
            path: skill_dir.to_string_lossy().into_owned(),
            global: false,
            writable: true,
            supporting_files: vec![file.to_string_lossy().into_owned()],
            properties: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn reads_nested_supporting_file() {
        let root = tempfile::tempdir().unwrap();
        let skill_dir = std::fs::canonicalize(root.path()).unwrap();
        std::fs::create_dir(skill_dir.join("nested")).unwrap();
        let file = skill_dir.join("nested/guide.md");
        std::fs::write(&file, "Nested guidance.").unwrap();

        let content = GooseSkillSource
            .read_supporting_file(&skill_at(&skill_dir, &file), Path::new("nested/guide.md"))
            .await
            .unwrap();

        assert!(content.contains("Nested guidance."));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn reads_supporting_file_under_symlinked_directory() {
        let tempdir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tempdir.path()).unwrap();
        let references = root.join("real-skills/test-skill/references");
        std::fs::create_dir_all(&references).unwrap();
        std::fs::write(references.join("guide.md"), "Symlinked guidance.").unwrap();
        std::os::unix::fs::symlink(root.join("real-skills"), root.join("linked-skills")).unwrap();
        let linked_skill_dir = root.join("linked-skills/test-skill");

        let content = GooseSkillSource
            .read_supporting_file(
                &skill_at(
                    &linked_skill_dir,
                    &linked_skill_dir.join("references/guide.md"),
                ),
                Path::new("references/guide.md"),
            )
            .await
            .unwrap();

        assert!(content.contains("Symlinked guidance."));
    }
}
