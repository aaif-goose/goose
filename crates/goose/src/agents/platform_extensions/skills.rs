use super::{parse_frontmatter, Source, SourceKind};
use crate::agents::builtin_skills;
use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::tool_execution::ToolCallContext;
use crate::config::paths::Paths;
use async_trait::async_trait;
use rmcp::model::{
    CallToolResult, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ServerCapabilities, ServerNotification,
};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::warn;

pub static EXTENSION_NAME: &str = "skills";

#[derive(Debug, Deserialize)]
struct SkillMetadata {
    name: String,
    description: String,
}

pub fn parse_skill_content(content: &str, path: PathBuf) -> Option<Source> {
    let (metadata, body): (SkillMetadata, String) = parse_frontmatter(content)?;

    if metadata.name.contains('/') {
        warn!(
            "Skill name '{}' contains '/' which is not allowed, skipping",
            metadata.name
        );
        return None;
    }

    Some(Source {
        name: metadata.name,
        kind: SourceKind::Skill,
        description: metadata.description,
        path,
        content: body,
        supporting_files: Vec::new(),
    })
}

pub fn scan_skills_from_dir(dir: &Path, seen: &mut HashSet<String>) -> Vec<Source> {
    let mut sources = Vec::new();
    let mut visited_dirs = HashSet::new();
    for skill_file in collect_skill_files(dir, &mut visited_dirs) {
        let Some(skill_dir) = skill_file.parent() else {
            continue;
        };
        let content = match std::fs::read_to_string(&skill_file) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read skill file {}: {}", skill_file.display(), e);
                continue;
            }
        };

        if let Some(mut source) = parse_skill_content(&content, skill_dir.to_path_buf()) {
            if !seen.contains(&source.name) {
                let mut visited_support_dirs = HashSet::new();
                source.supporting_files =
                    find_supporting_files(skill_dir, &mut visited_support_dirs);
                seen.insert(source.name.clone());
                sources.push(source);
            }
        }
    }
    sources
}

fn collect_skill_files(dir: &Path, visited_dirs: &mut HashSet<PathBuf>) -> Vec<PathBuf> {
    let mut skill_files = Vec::new();

    walk_files_recursively(
        dir,
        visited_dirs,
        &mut |path| !should_skip_dir(path),
        &mut |path| {
            if path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md") {
                skill_files.push(path.to_path_buf());
            }
        },
    );

    skill_files
}

fn should_skip_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git") | Some(".hg") | Some(".svn")
    )
}

fn walk_files_recursively<F, G>(
    dir: &Path,
    visited_dirs: &mut HashSet<PathBuf>,
    should_descend: &mut G,
    visit_file: &mut F,
) where
    F: FnMut(&Path),
    G: FnMut(&Path) -> bool,
{
    let canonical_dir = match std::fs::canonicalize(dir) {
        Ok(path) => path,
        Err(_) => return,
    };

    if !visited_dirs.insert(canonical_dir) {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            if should_descend(&path) {
                walk_files_recursively(&path, visited_dirs, should_descend, visit_file);
            }
        } else if path.is_file() {
            visit_file(&path);
        }
    }
}

pub fn find_supporting_files(
    directory: &Path,
    visited_dirs: &mut HashSet<PathBuf>,
) -> Vec<PathBuf> {
    let mut files = Vec::new();

    walk_files_recursively(
        directory,
        visited_dirs,
        &mut |path| !should_skip_dir(path) && !path.join("SKILL.md").is_file(),
        &mut |path| {
            let is_skill_md = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n == "SKILL.md")
                .unwrap_or(false);
            if !is_skill_md {
                files.push(path.to_path_buf());
            }
        },
    );

    files
}

fn skill_dirs(working_dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let home = dirs::home_dir();
    let config = Paths::config_dir();

    let local = vec![
        working_dir.join(".goose/skills"),
        working_dir.join(".claude/skills"),
        working_dir.join(".agents/skills"),
    ];

    let global = [
        home.as_ref().map(|h| h.join(".agents/skills")),
        Some(config.join("skills")),
        home.as_ref().map(|h| h.join(".claude/skills")),
        home.as_ref().map(|h| h.join(".config/agents/skills")),
    ]
    .into_iter()
    .flatten()
    .collect();

    (local, global)
}

pub fn discover_skills(working_dir: &Path) -> Vec<Source> {
    let mut sources = Vec::new();
    let mut seen = HashSet::new();

    let (local_dirs, global_dirs) = skill_dirs(working_dir);

    for dir in local_dirs {
        sources.extend(scan_skills_from_dir(&dir, &mut seen));
    }

    for dir in global_dirs {
        sources.extend(scan_skills_from_dir(&dir, &mut seen));
    }

    for content in builtin_skills::get_all() {
        if let Some(source) = parse_skill_content(content, PathBuf::new()) {
            if !seen.contains(&source.name) {
                seen.insert(source.name.clone());
                sources.push(Source {
                    kind: SourceKind::BuiltinSkill,
                    ..source
                });
            }
        }
    }

    sources
}

pub fn list_installed_skills(working_dir: Option<&Path>) -> Vec<Source> {
    let dir = working_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    discover_skills(&dir)
}

fn build_skill_instructions(skills: &[&Source]) -> String {
    let mut instructions = String::new();
    if !skills.is_empty() {
        instructions.push_str(
            "\n\nYou have these skills at your disposal, when it is clear they can help you solve a problem or you are asked to use them:",
        );
        for skill in skills {
            instructions.push_str(&format!("\n• {} - {}", skill.name, skill.description));
        }
    }
    instructions
}

pub struct SkillsClient {
    info: InitializeResult,
}

impl SkillsClient {
    pub fn new(context: PlatformExtensionContext) -> anyhow::Result<Self> {
        let instructions = if let Some(session) = &context.session {
            let sources = discover_skills(&session.working_dir);
            let mut skills: Vec<&Source> = sources
                .iter()
                .filter(|s| s.kind == SourceKind::Skill || s.kind == SourceKind::BuiltinSkill)
                .collect();
            skills.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));
            build_skill_instructions(&skills)
        } else {
            String::new()
        };

        let info = InitializeResult::new(ServerCapabilities::builder().build())
            .with_server_info(Implementation::new(EXTENSION_NAME, "1.0.0").with_title("Skills"))
            .with_instructions(instructions);

        Ok(Self { info })
    }
}

#[async_trait]
impl McpClientTrait for SkillsClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult {
            tools: vec![],
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        _ctx: &ToolCallContext,
        name: &str,
        _arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        Ok(CallToolResult::error(vec![rmcp::model::Content::text(
            format!("Error: Unknown tool: {}", name),
        )]))
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }

    async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
        let (_tx, rx) = mpsc::channel(1);
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[test]
    fn test_parse_skill_content() {
        let skill = "---\nname: test-skill\ndescription: A test skill\n---\nSkill body here.";
        let source = parse_skill_content(skill, PathBuf::new()).unwrap();
        assert_eq!(source.name, "test-skill");
        assert_eq!(source.kind, SourceKind::Skill);
        assert!(source.content.contains("Skill body"));
    }

    #[test]
    fn test_parse_skill_rejects_slash_in_name() {
        let skill = "---\nname: bad/skill\ndescription: A skill\n---\nContent.";
        assert!(parse_skill_content(skill, PathBuf::new()).is_none());
    }

    #[test]
    fn test_parse_skill_rejects_invalid_frontmatter() {
        assert!(parse_skill_content("no frontmatter", PathBuf::new()).is_none());
        assert!(parse_skill_content("---\nunclosed", PathBuf::new()).is_none());
    }

    #[test]
    fn test_discover_skills_from_filesystem() {
        let temp_dir = TempDir::new().unwrap();

        let goose_skill = temp_dir.path().join(".goose/skills/my-skill");
        fs::create_dir_all(&goose_skill).unwrap();
        fs::write(
            goose_skill.join("SKILL.md"),
            "---\nname: my-skill\ndescription: goose version\n---\nContent",
        )
        .unwrap();

        let claude_skill = temp_dir.path().join(".claude/skills/my-skill");
        fs::create_dir_all(&claude_skill).unwrap();
        fs::write(
            claude_skill.join("SKILL.md"),
            "---\nname: my-skill\ndescription: claude version\n---\nContent",
        )
        .unwrap();

        let sources = discover_skills(temp_dir.path());
        let skill = sources.iter().find(|s| s.name == "my-skill").unwrap();
        assert_eq!(skill.description, "goose version");
    }

    #[test]
    fn test_discover_skills_includes_builtins() {
        let temp_dir = TempDir::new().unwrap();
        let sources = discover_skills(temp_dir.path());
        assert!(sources.iter().any(|s| s.kind == SourceKind::BuiltinSkill));
    }

    #[test]
    fn test_skill_supporting_files() {
        let temp_dir = TempDir::new().unwrap();

        let skill_dir = temp_dir.path().join(".goose/skills/my-skill");
        fs::create_dir_all(skill_dir.join("templates/nested")).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: A skill\n---\nContent",
        )
        .unwrap();
        fs::write(skill_dir.join("myscript.sh"), "#!/bin/bash\necho ok").unwrap();
        fs::write(skill_dir.join("templates/report.txt"), "template").unwrap();
        fs::write(skill_dir.join("templates/nested/checklist.txt"), "nested").unwrap();

        let sources = discover_skills(temp_dir.path());
        let skill = sources.iter().find(|s| s.name == "my-skill").unwrap();
        assert_eq!(skill.supporting_files.len(), 3);
    }

    #[test]
    fn test_build_skill_instructions_empty() {
        let empty: &[&Source] = &[];
        assert_eq!(build_skill_instructions(empty), "");
    }

    #[test]
    fn test_build_skill_instructions_with_skills() {
        let skill = Source {
            name: "test".to_string(),
            kind: SourceKind::Skill,
            description: "A test skill".to_string(),
            path: PathBuf::new(),
            content: String::new(),
            supporting_files: vec![],
        };
        let instructions = build_skill_instructions(&[&skill]);
        assert!(instructions.contains("test - A test skill"));
    }

    #[tokio::test]
    async fn test_skills_client_no_tools() {
        let context = PlatformExtensionContext {
            extension_manager: None,
            session_manager: Arc::new(crate::session::SessionManager::instance()),
            session: None,
        };
        let client = SkillsClient::new(context).unwrap();
        let result = client
            .list_tools("test", None, CancellationToken::new())
            .await
            .unwrap();
        assert!(result.tools.is_empty());
    }
}
