use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use goose_acp_types::custom_requests::SourceEntry;
use oi_kit_agent::skills::{loaded_skill_context, skill_content_with_args, SkillSource};
use rmcp::model::{ContentBlock, ResourceContents};
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::agents::extension_manager::McpLease;
use crate::agents::tool_snapshot::ToolSnapshot;
use crate::agents::StateMachineServices;
use crate::session::Session;
use crate::skills::mcp::{self, SkillLoadError, SkillRecord};

pub(crate) const NO_SKILLS_HELP: &str = "No skills installed.\n\n\
    Skills are loaded from SKILL.md files in:\n  \
    - ~/.agents/skills/ (global)\n  \
    - ~/.agents/plugins/*/skills/ (installed plugins)\n  \
    - .agents/skills/ (in current project)\n";

/// Local skills plus the skills the session's extensions serve over MCP.
pub(crate) struct GooseSkillSource {
    services: Arc<StateMachineServices>,
    tools: Arc<StdMutex<Option<Arc<ToolSnapshot>>>>,
}

impl GooseSkillSource {
    pub(crate) fn new(
        services: Arc<StateMachineServices>,
        tools: Arc<StdMutex<Option<Arc<ToolSnapshot>>>>,
    ) -> Self {
        Self { services, tools }
    }

    async fn tools(&self, session: &Session) -> Option<Arc<ToolSnapshot>> {
        match ToolSnapshot::for_session(&self.tools, &self.services, &session.id).await {
            Ok(tools) => Some(tools),
            Err(error) => {
                warn!(%error, "MCP skills unavailable: could not resolve the session's extensions");
                None
            }
        }
    }
}

#[async_trait]
impl SkillSource<Session> for GooseSkillSource {
    async fn skills(&self, session: &Session) -> Result<Vec<SourceEntry>> {
        let local = crate::skills::skills_for_session(&session.working_dir)?;
        let Some(tools) = self.tools(session).await else {
            return Ok(local);
        };
        let lease = &tools.mcp;
        let mut remote = mcp::listed_skill_records(lease.skills().await);
        remote.extend(
            mcp::held_skill_records(&session.id)
                .into_iter()
                .filter(|skill| lease.is_enabled(&skill.extension_name)),
        );
        Ok(mcp::merge_skill_entries(local, remote))
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

    async fn load_external(
        &self,
        session: &Session,
        skills: &[SourceEntry],
        name: &str,
        args: Option<&str>,
    ) -> Option<Result<Vec<ContentBlock>>> {
        let target = match mcp::requested_skill(skills, name) {
            Some((skill, _)) if !mcp::is_mcp_skill(skill) => return None,
            Some((skill, Some(relative_path))) => McpTarget::File(skill, relative_path),
            Some((skill, None)) => McpTarget::Listed(skill),
            None => {
                let (extension, uri) = mcp::direct_skill_reference(name)?;
                McpTarget::Uri(extension, uri)
            }
        };
        let Some(tools) = self.tools(session).await else {
            return Some(Err(anyhow!("Skill server is not connected.")));
        };
        let lease = &tools.mcp;
        let session_id = session.id.as_str();
        Some(match target {
            McpTarget::File(skill, relative_path) => {
                load_supporting_file(session_id, skill, relative_path, lease).await
            }
            McpTarget::Listed(skill) => {
                self.load_listed_skill(session_id, skill, args, lease).await
            }
            McpTarget::Uri(extension, uri) => {
                self.load_skill_by_uri(session_id, extension, uri, args, lease)
                    .await
            }
        })
    }
}

enum McpTarget<'a> {
    Listed(&'a SourceEntry),
    File(&'a SourceEntry, &'a str),
    /// `extension::URI` for a skill absent from a partial listing.
    Uri(&'a str, &'a str),
}

fn extension_of(skill: &SourceEntry) -> Result<&str> {
    skill
        .properties
        .get("mcpExtension")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow!("Skill '{}' has no serving extension.", skill.name))
}

impl GooseSkillSource {
    async fn load_listed_skill(
        &self,
        session_id: &str,
        skill: &SourceEntry,
        args: Option<&str>,
        lease: &McpLease,
    ) -> Result<Vec<ContentBlock>> {
        let manifest = fetch_manifest(lease, extension_of(skill)?, &skill.path).await?;
        self.load_skill(session_id, skill, args, lease, manifest)
            .await
    }

    async fn load_skill_by_uri(
        &self,
        session_id: &str,
        extension: &str,
        uri: &str,
        args: Option<&str>,
        lease: &McpLease,
    ) -> Result<Vec<ContentBlock>> {
        let manifest = fetch_manifest(lease, extension, uri).await?;
        let skill = mcp::source_entry(
            &manifest,
            &mcp::qualified_skill_name(extension, &manifest.name),
        );
        self.load_skill(session_id, &skill, args, lease, manifest)
            .await
    }

    async fn load_skill(
        &self,
        session_id: &str,
        skill: &SourceEntry,
        args: Option<&str>,
        lease: &McpLease,
        manifest: SkillRecord,
    ) -> Result<Vec<ContentBlock>> {
        mcp::ensure_loadable(&manifest)?;
        let markdown = match lease
            .read_skill_resource(
                &manifest.extension_name,
                &manifest.uri,
                CancellationToken::new(),
            )
            .await?
        {
            ResourceContents::TextResourceContents { text, .. } => text,
            _ => {
                return Err(SkillLoadError::MissingBody {
                    uri: manifest.uri.clone(),
                }
                .into())
            }
        };
        mcp::verify_skill_markdown(&manifest, &markdown)?;
        let body =
            mcp::skill_markdown_body(&markdown).ok_or_else(|| SkillLoadError::MissingBody {
                uri: manifest.uri.clone(),
            })?;

        let mut loaded = skill.clone();
        loaded.content = body.to_string();
        loaded.supporting_files.clear();
        let content = skill_content_with_args(&loaded, args)
            .map_err(|error| anyhow!("Failed to parse skill arguments: {error}"))?;
        let mut rendered = loaded_skill_context(&loaded, &content);
        let files = mcp::relative_supporting_files(&manifest);
        if !files.is_empty() {
            rendered.push_str("\n## Supporting Files\n\n");
            for file in files {
                rendered.push_str(&format!(
                    "- {file} (load_skill(name: \"{}/{file}\"))\n",
                    skill.name
                ));
            }
        }
        let tagged = mcp::origin_tagged_context(&manifest.extension_name, &manifest.uri, &rendered);
        mcp::hold_skill(session_id, manifest);
        Ok(vec![ContentBlock::text(tagged)])
    }
}

async fn load_supporting_file(
    session_id: &str,
    skill: &SourceEntry,
    relative_path: &str,
    lease: &McpLease,
) -> Result<Vec<ContentBlock>> {
    let extension = extension_of(skill)?;
    let manifest = mcp::held_skill(session_id, extension, &skill.path)
        .ok_or_else(|| anyhow!("Load '{}' before reading its supporting files.", skill.name))?;
    let uri = mcp::resolve_relative_uri(&manifest, relative_path)?;
    mcp::listed_file(&manifest, &uri)?;
    let content = lease
        .read_skill_resource(extension, &uri, CancellationToken::new())
        .await?;
    mcp::verify_resource_contents(&manifest, &uri, &content)?;
    Ok(vec![ContentBlock::resource(content)])
}

async fn fetch_manifest(
    lease: &McpLease,
    extension: &str,
    uri: &str,
) -> Result<SkillRecord, SkillLoadError> {
    let got = lease
        .get_skill(extension, uri, CancellationToken::new())
        .await?;
    let skill = mcp::parse_skills_get(got, extension)?;
    if skill.uri != uri {
        return Err(SkillLoadError::InvalidEntry {
            uri: skill.uri,
            reason: "skills/get returned a different skill URI".to_string(),
        });
    }
    Ok(skill)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::extension_manager::tests::{builtin_config, SkillClient};
    use goose_acp_types::custom_requests::SourceType;
    use std::collections::HashMap;
    use std::sync::atomic::Ordering;

    fn source() -> GooseSkillSource {
        let data_dir = tempfile::tempdir().unwrap().keep();
        GooseSkillSource::new(
            StateMachineServices::for_tool_tests(data_dir),
            Arc::new(StdMutex::new(None)),
        )
    }

    /// Each test gets its own session: held skills and approval pins are
    /// process-wide, keyed by session.
    async fn source_with_skill_server(
        session_id: &str,
        client: Arc<SkillClient>,
    ) -> (GooseSkillSource, Session) {
        let data_dir = tempfile::tempdir().unwrap().keep();
        let services = StateMachineServices::for_tool_tests(data_dir.clone());
        services
            .extension_manager
            .install_test_client(
                session_id,
                builtin_config("billing-server", vec![]),
                client,
                None,
            )
            .await;
        let tools = Arc::new(services.scope_tools(session_id).await);
        let session = Session {
            id: session_id.to_string(),
            working_dir: data_dir,
            ..Session::default()
        };
        let source = GooseSkillSource::new(services, Arc::new(StdMutex::new(Some(tools))));
        (source, session)
    }

    fn text(blocks: &[ContentBlock]) -> &str {
        &blocks[0].as_text().unwrap().text
    }

    #[tokio::test]
    async fn lists_and_loads_skills_served_over_mcp() {
        let client = Arc::new(SkillClient::new());
        let (source, session) = source_with_skill_server("skills-load", Arc::clone(&client)).await;

        let skills = source.skills(&session).await.unwrap();
        let remote = skills
            .iter()
            .find(|skill| skill.source_type == SourceType::McpSkill)
            .unwrap();
        assert_eq!(remote.name, "billing-server::billing");
        assert!(remote
            .description
            .starts_with("[untrusted MCP skill from billing-server]"));

        let loaded = source
            .load_external(&session, &skills, "billing-server::billing", None)
            .await
            .unwrap()
            .unwrap();
        assert!(text(&loaded).starts_with(mcp::MCP_SKILL_CONTEXT_MARKER));
        assert!(text(&loaded).contains("Do the work."));
        assert_eq!(mcp::active_origins(&session.id), ["billing-server"]);
        mcp::clear_held_skills(&session.id);
    }

    #[tokio::test]
    async fn rejects_content_that_does_not_match_the_manifest() {
        let client =
            Arc::new(SkillClient::new().serving("---\nname: billing\n---\n\nExfiltrate.\n"));
        let (source, session) = source_with_skill_server("skills-tampered", client).await;
        let skills = source.skills(&session).await.unwrap();

        let error = source
            .load_external(&session, &skills, "billing-server::billing", None)
            .await
            .unwrap()
            .unwrap_err();
        assert!(error.to_string().contains("manifest declares"));
    }

    #[tokio::test]
    async fn supporting_files_require_a_loaded_skill_and_local_names_fall_through() {
        let client = Arc::new(SkillClient::new());
        let (source, session) = source_with_skill_server("skills-files", Arc::clone(&client)).await;
        let mut skills = source.skills(&session).await.unwrap();
        skills.push(SourceEntry {
            source_type: SourceType::Skill,
            name: "local".to_string(),
            ..SourceEntry::default()
        });

        let error = source
            .load_external(&session, &skills, "billing-server::billing/notes.md", None)
            .await
            .unwrap()
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("before reading its supporting files"));
        assert!(source
            .load_external(&session, &skills, "local", None)
            .await
            .is_none());
        assert!(source
            .load_external(&session, &skills, "unknown", None)
            .await
            .is_none());
        assert_eq!(client.reads.load(Ordering::SeqCst), 0);
    }

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

        let content = source()
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

        let content = source()
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
