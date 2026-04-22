use super::discover_skills;
use super::mcp_client::McpSkillEntry;
use crate::agents::extension::PlatformExtensionContext;
use crate::agents::extension_manager::ExtensionManager;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::ToolCallContext;
use async_trait::async_trait;
use goose_sdk::custom_requests::{SourceEntry, SourceType};
use rmcp::model::{
    CallToolResult, Content, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ResourceContents, ServerCapabilities, ServerNotification, Tool,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Weak};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::warn;

/// How long a cached snapshot of installed FS skill names stays valid. The
/// cache backs FS-vs-MCP collision detection in `get_dynamic_instructions`,
/// which runs every reply; without the cache we'd walk up to seven skill
/// directories per turn once any MCP skill is cached. Ten seconds is plenty
/// short for newly-added local skills to show up on the next collision
/// check.
const FS_NAMES_TTL: Duration = Duration::from_secs(10);

pub static EXTENSION_NAME: &str = "skills";

pub struct SkillsClient {
    info: InitializeResult,
    working_dir: PathBuf,
    /// Weak reference to the extension manager so we can, per turn, read
    /// the MCP-served skills cache populated at server connect time and
    /// dispatch `resources/read` when `load_skill` hits an MCP entry.
    /// `None` in session-less contexts (tests, bootstrap).
    extension_manager: Option<Weak<ExtensionManager>>,
    /// TTL-cached snapshot of installed FS skill names. Read on every reply
    /// to drive FS-vs-MCP collision prefixing; we recompute at most once
    /// per `FS_NAMES_TTL` so the per-turn cost is amortized.
    fs_names_cache: Mutex<FsNamesCache>,
}

#[derive(Default)]
struct FsNamesCache {
    refreshed_at: Option<Instant>,
    names: HashSet<String>,
}

impl SkillsClient {
    pub fn new(context: PlatformExtensionContext) -> anyhow::Result<Self> {
        let working_dir = context
            .session
            .as_ref()
            .map(|s| s.working_dir.clone())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        let mut instructions = String::new();
        if context.session.is_some() {
            let sources = discover_skills(Some(&working_dir));
            let mut skills: Vec<&SourceEntry> = sources
                .iter()
                .filter(|s| {
                    s.source_type == SourceType::Skill || s.source_type == SourceType::BuiltinSkill
                })
                .collect();
            skills.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));

            if !skills.is_empty() {
                instructions.push_str(
                    "\n\nYou have these skills at your disposal, when it is clear they can help you solve a problem or you are asked to use them:",
                );
                for skill in &skills {
                    instructions.push_str(&format!("\n• {} - {}", skill.name, skill.description));
                }
            }
        }

        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(EXTENSION_NAME, "1.0.0").with_title("Skills"))
            .with_instructions(instructions);

        Ok(Self {
            info,
            working_dir,
            extension_manager: context.extension_manager,
            fs_names_cache: Mutex::new(FsNamesCache::default()),
        })
    }

    /// Returns the current set of MCP skills visible via the extension
    /// manager's cache, or empty if no manager is attached.
    async fn mcp_skills(&self) -> Vec<McpSkillEntry> {
        match self.extension_manager.as_ref().and_then(|w| w.upgrade()) {
            Some(mgr) => mgr.aggregated_mcp_skills().await,
            None => Vec::new(),
        }
    }

    /// Cached wrapper around `fs_skill_names`. Rescans the FS when the
    /// previous snapshot is older than `FS_NAMES_TTL`, or on first call.
    /// Never holds the mutex across the FS walk — we drop the guard, do
    /// the blocking scan, then re-acquire to write. The scan can race
    /// with a concurrent call but the result is equivalent (both computes
    /// produce the same set for identical FS state, and the last write
    /// wins with a fresh timestamp).
    fn fs_skill_names_cached(&self) -> HashSet<String> {
        {
            let cache = self.fs_names_cache.lock().expect("fs_names_cache poisoned");
            if let Some(ts) = cache.refreshed_at {
                if ts.elapsed() < FS_NAMES_TTL {
                    return cache.names.clone();
                }
            }
        }

        let fresh = fs_skill_names(&self.working_dir);

        let mut cache = self.fs_names_cache.lock().expect("fs_names_cache poisoned");
        cache.refreshed_at = Some(Instant::now());
        cache.names = fresh.clone();
        fresh
    }
}

/// Rebuilds the list of FS skill names currently installed. Used to detect
/// FS-vs-MCP name collisions (FS wins — the MCP entry is rendered with a
/// `<server>__<name>` prefix).
fn fs_skill_names(working_dir: &Path) -> HashSet<String> {
    discover_skills(Some(working_dir))
        .into_iter()
        .filter(|s| matches!(s.source_type, SourceType::Skill | SourceType::BuiltinSkill))
        .map(|s| s.name)
        .collect()
}

/// Renders the MCP skills section of the system prompt. Collisions with
/// FS skill names use the `<server>__<name>` form so the model can still
/// address the MCP entry unambiguously via `load_skill`. Empty output
/// when no MCP skills are available — caller drops the section entirely.
fn format_mcp_skills_section(fs_names: &HashSet<String>, mcp: &[McpSkillEntry]) -> String {
    if mcp.is_empty() {
        return String::new();
    }

    // Bucket by bare name to detect MCP-vs-MCP collisions separately from
    // FS collisions.
    let mut counts: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    for entry in mcp {
        *counts.entry(entry.name.as_str()).or_insert(0) += 1;
    }

    let mut sorted: Vec<&McpSkillEntry> = mcp.iter().collect();
    sorted.sort_by(|a, b| (a.name.as_str(), a.server.as_str()).cmp(&(b.name.as_str(), b.server.as_str())));

    let mut out = String::from(
        "\n\nYou also have these skills from connected MCP servers. Load them via load_skill by name; if a collision is shown in <server>__<name> form, use that exact form:",
    );
    for entry in sorted {
        let needs_prefix =
            fs_names.contains(&entry.name) || counts[entry.name.as_str()] > 1;
        let display_name = if needs_prefix {
            format!("{}__{}", entry.server, entry.name)
        } else {
            entry.name.clone()
        };
        // URI intentionally omitted: the model addresses MCP skills by
        // name via `load_skill`, and including full URIs for every entry
        // bloats every turn's system prompt on servers with many skills.
        out.push_str(&format!(
            "\n• {} ({}) - {}",
            display_name, entry.server, entry.description
        ));
    }
    out
}

/// Extracts the first text content from a `ReadResourceResult`. Returns
/// `None` if the result contains only blob contents (binary). Logs a
/// warning if the server returned more than one text entry — the SEP
/// expects SKILL.md to arrive as a single document, and a multi-entry
/// response likely means the server is splitting content in a way the
/// host won't reassemble.
fn first_text_content(
    result: rmcp::model::ReadResourceResult,
    server: &str,
    uri: &str,
) -> Option<String> {
    let mut text_count = 0usize;
    let mut first: Option<String> = None;
    for c in result.contents {
        if let ResourceContents::TextResourceContents { text, .. } = c {
            text_count += 1;
            if first.is_none() {
                first = Some(text);
            }
        }
    }
    if text_count > 1 {
        warn!(
            server,
            uri,
            text_count,
            "read_resource returned multiple text contents; only the first was used"
        );
    }
    first
}

/// Normalizes a supporting-file relative reference before composing it with
/// a server's `base_uri`. Rejects inputs that could escape the skill
/// directory — `..` segments or a leading `/`. Backslashes are folded to
/// forward slashes so Windows-style paths from the model don't slip past
/// the `..` check. Returns `None` if the input is unsafe to compose.
fn sanitize_relative_ref(raw: &str) -> Option<String> {
    let normalized = raw.replace('\\', "/");
    if normalized.starts_with('/') {
        return None;
    }
    if normalized.split('/').any(|segment| segment == "..") {
        return None;
    }
    Some(normalized)
}

/// Finds an MCP skill entry by name, accepting either the bare name or the
/// `<server>__<name>` collision form. Literal match wins so a server can
/// legitimately publish a skill whose name contains `__` without being
/// hijacked by a coincidental server/skill pair on the other side of the
/// split.
fn find_mcp_by_name<'a>(mcp: &'a [McpSkillEntry], query: &str) -> Option<&'a McpSkillEntry> {
    if let Some(hit) = mcp.iter().find(|e| e.name == query) {
        return Some(hit);
    }
    if let Some((server_prefix, bare_name)) = query.split_once("__") {
        return mcp
            .iter()
            .find(|e| e.server == server_prefix && e.name == bare_name);
    }
    None
}

/// Reads a resource from the owning MCP server and wraps it in the same
/// "Loaded Skill" framing used for filesystem skills, so the model sees a
/// consistent shape regardless of source.
async fn read_mcp_and_frame(
    mgr: &ExtensionManager,
    session_id: &str,
    entry: &McpSkillEntry,
    uri: &str,
    cancel: CancellationToken,
) -> CallToolResult {
    match mgr.read_resource(session_id, uri, &entry.server, cancel).await {
        Ok(result) => match first_text_content(result, &entry.server, uri) {
            Some(body) => CallToolResult::success(vec![Content::text(format!(
                "# Loaded Skill: {} (mcp skill from {})\n\n{}\n\n---\nThis knowledge is now available in your context.",
                entry.name, entry.server, body
            ))]),
            None => CallToolResult::error(vec![Content::text(format!(
                "Resource '{}' from '{}' had no text content.",
                uri, entry.server
            ))]),
        },
        Err(e) => CallToolResult::error(vec![Content::text(format!(
            "Failed to read '{}' from '{}': {}",
            uri, entry.server, e.message
        ))]),
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
        let load_skill_schema = serde_json::json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the skill to load. Use \"skill-name/path\" to load a supporting file. For MCP skills with a name collision, use the \"<server>__<name>\" form shown in your system instructions. Do NOT pass a URI here — use the read_resource tool (on the extensionmanager) if you only have a URI."
                }
            }
        });

        let load_skill = Tool::new(
            "load_skill",
            "Load a skill's full content into your context so you can follow its instructions.\n\n\
             Skills are listed in your system instructions (both local skills and skills from connected MCP servers). When you need to use one, load it first to get the detailed instructions.\n\n\
             Examples:\n\
             - load_skill(name: \"gdrive\") → Loads the gdrive skill instructions\n\
             - load_skill(name: \"my-skill/template.md\") → Loads a supporting file\n\
             - load_skill(name: \"github__pull-requests\") → Disambiguates a collision between two servers\n\n\
             Use read_resource (from the extensionmanager) if you only have a raw URI. Do NOT use read_text_file, text_editor, or shell on skill URIs — those operate on filesystem paths."
                .to_string(),
            load_skill_schema.as_object().unwrap().clone(),
        );

        Ok(ListToolsResult {
            tools: vec![load_skill],
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        if name != "load_skill" {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Unknown tool: {}",
                name
            ))]));
        }

        let skill_name = arguments
            .as_ref()
            .and_then(|args| args.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if skill_name.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text(
                "Missing required parameter: name",
            )]));
        }

        // Reject raw URIs — they go through `read_resource` (a separate
        // tool) rather than `load_skill`. Shares `looks_like_uri` with
        // `developer::edit::reject_uri_path` so the two guardrails can't
        // drift apart on scheme shape.
        if crate::agents::platform_extensions::looks_like_uri(skill_name) {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "'{}' looks like a URI. Use the read_resource tool instead (it takes a server name and a uri). load_skill takes a skill name or <skill>/<relative/path>.",
                skill_name
            ))]));
        }

        let skills = discover_skills(Some(&self.working_dir));

        if let Some(skill) = skills.iter().find(|s| s.name == skill_name) {
            let mut output = format!(
                "# Loaded Skill: {} ({})\n\n{}\n",
                skill.name,
                skill.source_type,
                skill.to_load_text()
            );

            if !skill.supporting_files.is_empty() {
                let skill_dir = Path::new(&skill.path);
                output.push_str(&format!(
                    "\n## Supporting Files\n\nSkill directory: {}\n\n",
                    skill.path
                ));
                for file in &skill.supporting_files {
                    if let Ok(relative) = Path::new(file).strip_prefix(skill_dir) {
                        let rel_str = relative.to_string_lossy().replace('\\', "/");
                        output.push_str(&format!(
                            "- {} → load_skill(name: \"{}/{}\")\n",
                            rel_str, skill.name, rel_str
                        ));
                    }
                }
            }

            output.push_str("\n---\nThis knowledge is now available in your context.");
            return Ok(CallToolResult::success(vec![Content::text(output)]));
        }

        if let Some((parent_skill_name, raw_relative_path)) = skill_name.split_once('/') {
            let relative_path = raw_relative_path.replace('\\', "/");
            if let Some(skill) = skills.iter().find(|s| {
                s.name == parent_skill_name
                    && matches!(s.source_type, SourceType::Skill | SourceType::BuiltinSkill)
            }) {
                let skill_dir = PathBuf::from(&skill.path);
                let canonical_skill_dir = skill_dir
                    .canonicalize()
                    .unwrap_or_else(|_| skill_dir.clone());

                for file_path in &skill.supporting_files {
                    let file_path_buf = Path::new(file_path);
                    let Ok(rel) = file_path_buf.strip_prefix(&skill_dir) else {
                        continue;
                    };
                    if rel.to_string_lossy().replace('\\', "/") != relative_path {
                        continue;
                    }

                    return Ok(match file_path_buf.canonicalize() {
                        Ok(canonical) if canonical.starts_with(&canonical_skill_dir) => {
                            match std::fs::read_to_string(&canonical) {
                                Ok(content) => {
                                    CallToolResult::success(vec![Content::text(format!(
                                        "# Loaded: {}\n\n{}\n\n---\nFile loaded into context.",
                                        skill_name, content
                                    ))])
                                }
                                Err(e) => CallToolResult::error(vec![Content::text(format!(
                                    "Failed to read '{}': {}",
                                    skill_name, e
                                ))]),
                            }
                        }
                        Ok(_) => CallToolResult::error(vec![Content::text(format!(
                            "Refusing to load '{}': resolves outside the skill directory",
                            skill_name
                        ))]),
                        Err(e) => CallToolResult::error(vec![Content::text(format!(
                            "Failed to resolve '{}': {}",
                            skill_name, e
                        ))]),
                    });
                }

                let available: Vec<String> = skill
                    .supporting_files
                    .iter()
                    .filter_map(|f| {
                        Path::new(f)
                            .strip_prefix(&skill_dir)
                            .ok()
                            .map(|r| r.to_string_lossy().replace('\\', "/"))
                    })
                    .take(10)
                    .collect();

                return Ok(if available.is_empty() {
                    CallToolResult::error(vec![Content::text(format!(
                        "Skill '{}' has no supporting files.",
                        skill.name
                    ))])
                } else {
                    CallToolResult::error(vec![Content::text(format!(
                        "File '{}' not found. Available: {}",
                        skill_name,
                        available.join(", ")
                    ))])
                });
            }
        }

        // MCP skill routing. Read the cache populated at extension-connect
        // time. `<server>__<name>` disambiguation is supported alongside
        // bare names.
        let mcp_skills = self.mcp_skills().await;
        let mgr = self.extension_manager.as_ref().and_then(|w| w.upgrade());

        if let Some(entry) = find_mcp_by_name(&mcp_skills, skill_name) {
            if let Some(ref mgr) = mgr {
                return Ok(read_mcp_and_frame(
                    mgr.as_ref(),
                    &ctx.session_id,
                    entry,
                    &entry.uri,
                    cancellation_token.clone(),
                )
                .await);
            }
        }

        if let Some((parent, raw_rel)) = skill_name.split_once('/') {
            if let Some(entry) = find_mcp_by_name(&mcp_skills, parent) {
                if let Some(ref mgr) = mgr {
                    let Some(rel) = sanitize_relative_ref(raw_rel) else {
                        return Ok(CallToolResult::error(vec![Content::text(format!(
                            "Refusing to load '{}': relative path must not contain '..' or start with '/'.",
                            skill_name
                        ))]));
                    };
                    let composed = format!("{}{}", entry.base_uri, rel);
                    return Ok(read_mcp_and_frame(
                        mgr.as_ref(),
                        &ctx.session_id,
                        entry,
                        &composed,
                        cancellation_token.clone(),
                    )
                    .await);
                }
            }
        }

        let mut candidates: Vec<&str> = skills
            .iter()
            .filter(|s| {
                s.name.to_lowercase().contains(&skill_name.to_lowercase())
                    || skill_name.to_lowercase().contains(&s.name.to_lowercase())
            })
            .map(|s| s.name.as_str())
            .collect();
        candidates.extend(mcp_skills.iter().filter_map(|e| {
            if e.name.to_lowercase().contains(&skill_name.to_lowercase())
                || skill_name.to_lowercase().contains(&e.name.to_lowercase())
            {
                Some(e.name.as_str())
            } else {
                None
            }
        }));
        candidates.sort();
        candidates.dedup();
        candidates.truncate(3);

        Ok(if candidates.is_empty() {
            CallToolResult::error(vec![Content::text(format!(
                "Skill '{}' not found.",
                skill_name
            ))])
        } else {
            CallToolResult::error(vec![Content::text(format!(
                "Skill '{}' not found. Did you mean: {}?",
                skill_name,
                candidates.join(", ")
            ))])
        })
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }

    async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
        let (_tx, rx) = mpsc::channel(1);
        rx
    }

    async fn get_dynamic_instructions(&self, _session_id: &str) -> Option<String> {
        let mcp = self.mcp_skills().await;
        if mcp.is_empty() {
            return None;
        }
        let fs_names = self.fs_skill_names_cached();
        let section = format_mcp_skills_section(&fs_names, &mcp);
        if section.is_empty() {
            None
        } else {
            Some(section)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_load_skill_from_filesystem() {
        let temp_dir = TempDir::new().unwrap();
        let skill_dir = temp_dir.path().join(".goose/skills/my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: A test skill\n---\nDo the thing.",
        )
        .unwrap();

        let session = std::sync::Arc::new(crate::session::Session {
            working_dir: temp_dir.path().to_path_buf(),
            ..crate::session::Session::default()
        });
        let client = SkillsClient::new(PlatformExtensionContext {
            extension_manager: None,
            session_manager: Arc::new(crate::session::SessionManager::instance()),
            session: Some(session),
        })
        .unwrap();

        let ctx = ToolCallContext::new("test".to_string(), None, None);
        let args: JsonObject =
            serde_json::from_value(serde_json::json!({"name": "my-skill"})).unwrap();
        let result = client
            .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
            .await
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));
        let text = match &result.content[0].raw {
            rmcp::model::RawContent::Text(t) => &t.text,
            _ => panic!("expected text"),
        };
        assert!(text.contains("my-skill"));
        assert!(text.contains("Do the thing"));
    }

    #[tokio::test]
    async fn test_load_skill_not_found_returns_error() {
        let client = SkillsClient::new(PlatformExtensionContext {
            extension_manager: None,
            session_manager: Arc::new(crate::session::SessionManager::instance()),
            session: None,
        })
        .unwrap();

        let ctx = ToolCallContext::new("test".to_string(), None, None);
        let args: JsonObject =
            serde_json::from_value(serde_json::json!({"name": "nonexistent"})).unwrap();
        let result = client
            .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
            .await
            .unwrap();

        assert!(result.is_error.unwrap_or(false));
    }

    // ---------- MCP skill routing tests ----------
    //
    // These exercise the end-to-end path: register a FakeMcp server in an
    // ExtensionManager (which populates the mcp_skills cache at connect
    // time), wire a SkillsClient to a Weak of that manager, and call
    // load_skill.

    use crate::agents::extension::ExtensionConfig;
    use crate::agents::extension_manager::ExtensionManager;
    use async_trait::async_trait;
    use rmcp::model::{
        ExtensionCapabilities, ListResourcesResult, ReadResourceResult, ServerNotification,
    };
    use std::collections::HashMap;

    struct FakeMcp {
        info: InitializeResult,
        resources: HashMap<String, String>,
    }

    impl FakeMcp {
        fn new(resources: HashMap<String, String>) -> Self {
            let mut caps = ExtensionCapabilities::new();
            caps.insert(
                super::super::mcp_client::SKILLS_EXTENSION_ID.to_string(),
                JsonObject::new(),
            );
            let info = InitializeResult::new(
                ServerCapabilities::builder()
                    .enable_resources()
                    .enable_extensions_with(caps)
                    .build(),
            );
            Self { info, resources }
        }
    }

    #[async_trait]
    impl McpClientTrait for FakeMcp {
        async fn list_tools(
            &self,
            _session_id: &str,
            _next_cursor: Option<String>,
            _cancel_token: CancellationToken,
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
            _name: &str,
            _arguments: Option<JsonObject>,
            _cancel_token: CancellationToken,
        ) -> Result<CallToolResult, Error> {
            unreachable!("FakeMcp has no tools")
        }

        fn get_info(&self) -> Option<&InitializeResult> {
            Some(&self.info)
        }

        async fn list_resources(
            &self,
            _session_id: &str,
            _next_cursor: Option<String>,
            _cancel_token: CancellationToken,
        ) -> Result<ListResourcesResult, Error> {
            Ok(ListResourcesResult {
                resources: vec![],
                next_cursor: None,
                meta: None,
            })
        }

        async fn read_resource(
            &self,
            _session_id: &str,
            uri: &str,
            _cancel_token: CancellationToken,
        ) -> Result<ReadResourceResult, Error> {
            match self.resources.get(uri) {
                Some(text) => Ok(ReadResourceResult::new(vec![
                    ResourceContents::TextResourceContents {
                        uri: uri.to_string(),
                        mime_type: None,
                        text: text.clone(),
                        meta: None,
                    },
                ])),
                None => Err(Error::TransportClosed),
            }
        }

        async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
            mpsc::channel(1).1
        }
    }

    async fn setup_client_with_fake(
        server_name: &str,
        resources: HashMap<String, String>,
        working_dir: PathBuf,
    ) -> (SkillsClient, Arc<ExtensionManager>, TempDir) {
        let tmp = TempDir::new().unwrap();
        let mgr = Arc::new(ExtensionManager::new_without_provider(
            tmp.path().to_path_buf(),
        ));

        let fake: std::sync::Arc<dyn McpClientTrait> = std::sync::Arc::new(FakeMcp::new(resources));
        mgr.add_client(
            server_name.to_string(),
            ExtensionConfig::Builtin {
                name: server_name.to_string(),
                display_name: Some(server_name.to_string()),
                description: "fake mcp".to_string(),
                timeout: None,
                bundled: None,
                available_tools: vec![],
            },
            fake,
            None,
            None,
            Some("s"),
        )
        .await;

        let session = Arc::new(crate::session::Session {
            working_dir: working_dir.clone(),
            ..crate::session::Session::default()
        });

        let client = SkillsClient::new(PlatformExtensionContext {
            extension_manager: Some(Arc::downgrade(&mgr)),
            session_manager: Arc::new(crate::session::SessionManager::instance()),
            session: Some(session),
        })
        .unwrap();

        (client, mgr, tmp)
    }

    fn index_json(entries: &str) -> String {
        format!(
            r#"{{"$schema":"https://schemas.agentskills.io/discovery/0.2.0/schema.json","skills":[{}]}}"#,
            entries
        )
    }

    fn text_of(r: &CallToolResult) -> String {
        match &r.content[0].raw {
            rmcp::model::RawContent::Text(t) => t.text.clone(),
            _ => panic!("expected text content"),
        }
    }

    #[tokio::test]
    async fn test_load_mcp_skill_basic() {
        let tmp = TempDir::new().unwrap();
        let mut resources = HashMap::new();
        resources.insert(
            "skill://index.json".to_string(),
            index_json(
                r#"{"name":"git-workflow","type":"skill-md","description":"Git","url":"skill://git-workflow/SKILL.md"}"#,
            ),
        );
        resources.insert(
            "skill://git-workflow/SKILL.md".to_string(),
            "---\nname: git-workflow\ndescription: Git\n---\nGit body text.".to_string(),
        );

        let (client, _mgr, _tmp_guard) =
            setup_client_with_fake("gh", resources, tmp.path().to_path_buf()).await;

        let ctx = ToolCallContext::new("s".to_string(), None, None);
        let args: JsonObject =
            serde_json::from_value(serde_json::json!({"name": "git-workflow"})).unwrap();
        let result = client
            .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
            .await
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));
        let body = text_of(&result);
        assert!(body.contains("Git body text"), "got: {}", body);
        assert!(body.contains("mcp skill from gh"), "got: {}", body);
    }

    #[tokio::test]
    async fn test_load_mcp_skill_non_skill_scheme() {
        let tmp = TempDir::new().unwrap();
        let mut resources = HashMap::new();
        resources.insert(
            "skill://index.json".to_string(),
            index_json(
                r#"{"name":"pull-requests","type":"skill-md","description":"PRs","url":"github://o/r/skills/pull-requests/SKILL.md"}"#,
            ),
        );
        resources.insert(
            "github://o/r/skills/pull-requests/SKILL.md".to_string(),
            "PR review workflow body.".to_string(),
        );

        let (client, _mgr, _tmp_guard) =
            setup_client_with_fake("github", resources, tmp.path().to_path_buf()).await;

        let ctx = ToolCallContext::new("s".to_string(), None, None);
        let args: JsonObject =
            serde_json::from_value(serde_json::json!({"name": "pull-requests"})).unwrap();
        let result = client
            .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
            .await
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));
        assert!(text_of(&result).contains("PR review workflow body"));
    }

    #[tokio::test]
    async fn test_load_mcp_supporting_file() {
        let tmp = TempDir::new().unwrap();
        let mut resources = HashMap::new();
        resources.insert(
            "skill://index.json".to_string(),
            index_json(
                r#"{"name":"docs","type":"skill-md","description":"","url":"skill://docs/SKILL.md"}"#,
            ),
        );
        resources.insert(
            "skill://docs/references/GUIDE.md".to_string(),
            "Guide body.".to_string(),
        );

        let (client, _mgr, _tmp_guard) =
            setup_client_with_fake("srv", resources, tmp.path().to_path_buf()).await;

        let ctx = ToolCallContext::new("s".to_string(), None, None);
        let args: JsonObject =
            serde_json::from_value(serde_json::json!({"name": "docs/references/GUIDE.md"}))
                .unwrap();
        let result = client
            .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
            .await
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));
        assert!(text_of(&result).contains("Guide body"));
    }

    #[tokio::test]
    async fn test_load_mcp_supporting_file_rejects_parent_traversal() {
        // A model (or a hijacked server index) that asks for `docs/../other`
        // or `docs//absolute` must not be composed into the server URI — it
        // could escape the skill directory on a filesystem-backed resolver.
        let tmp = TempDir::new().unwrap();
        let mut resources = HashMap::new();
        resources.insert(
            "skill://index.json".to_string(),
            index_json(
                r#"{"name":"docs","type":"skill-md","description":"","url":"skill://docs/SKILL.md"}"#,
            ),
        );
        let (client, _mgr, _tmp_guard) =
            setup_client_with_fake("srv", resources, tmp.path().to_path_buf()).await;

        let ctx = ToolCallContext::new("s".to_string(), None, None);
        for bad in ["docs/../secrets/SKILL.md", "docs//etc/passwd"] {
            let args: JsonObject =
                serde_json::from_value(serde_json::json!({"name": bad})).unwrap();
            let result = client
                .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
                .await
                .unwrap();
            assert!(
                result.is_error.unwrap_or(false),
                "expected rejection for {bad}, got: {:?}",
                text_of(&result)
            );
            let body = text_of(&result);
            assert!(
                body.contains("Refusing to load") || body.contains(".."),
                "unexpected rejection message for {bad}: {body}"
            );
        }
    }

    #[tokio::test]
    async fn test_load_skill_uri_input_redirects_to_read_resource() {
        // load_skill is name-only; passing a URI returns an instructive
        // error pointing the model at read_resource.
        let tmp = TempDir::new().unwrap();
        let (client, _mgr, _tmp_guard) = setup_client_with_fake(
            "srv",
            HashMap::new(),
            tmp.path().to_path_buf(),
        )
        .await;

        let ctx = ToolCallContext::new("s".to_string(), None, None);
        let args: JsonObject = serde_json::from_value(
            serde_json::json!({"name": "skill://unknown/SKILL.md"}),
        )
        .unwrap();
        let result = client
            .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
            .await
            .unwrap();

        assert!(result.is_error.unwrap_or(false));
        let body = text_of(&result);
        assert!(body.contains("read_resource"), "got: {}", body);
    }

    #[tokio::test]
    async fn test_get_extensions_info_roundtrip_does_not_deadlock() {
        // Regression guard: `ExtensionManager::get_extensions_info` iterates
        // registered extensions and invokes `get_dynamic_instructions` on
        // each. `SkillsClient::get_dynamic_instructions` in turn calls
        // `mgr.aggregated_mcp_skills()`, which re-acquires the same
        // `extensions` mutex. If any future edit inlines that call inside
        // the iteration lock scope, this test will hang (tokio::time::timeout
        // turns that into a clean failure).
        let tmp = TempDir::new().unwrap();
        let working_dir = tmp.path().to_path_buf();
        let mut resources = HashMap::new();
        resources.insert(
            "skill://index.json".to_string(),
            index_json(
                r#"{"name":"alpha","type":"skill-md","description":"A","url":"skill://alpha/SKILL.md"}"#,
            ),
        );

        let mgr = Arc::new(ExtensionManager::new_without_provider(working_dir.clone()));
        let fake: Arc<dyn McpClientTrait> = Arc::new(FakeMcp::new(resources));
        mgr.add_client(
            "srv".to_string(),
            ExtensionConfig::Builtin {
                name: "srv".to_string(),
                display_name: Some("srv".to_string()),
                description: "fake mcp".to_string(),
                timeout: None,
                bundled: None,
                available_tools: vec![],
            },
            fake,
            None,
            None,
            Some("s"),
        )
        .await;

        // Register a SkillsClient into the manager so `get_extensions_info`
        // will call its `get_dynamic_instructions` during iteration.
        let session = Arc::new(crate::session::Session {
            working_dir: working_dir.clone(),
            ..crate::session::Session::default()
        });
        let skills_client: Arc<dyn McpClientTrait> = Arc::new(
            SkillsClient::new(PlatformExtensionContext {
                extension_manager: Some(Arc::downgrade(&mgr)),
                session_manager: Arc::new(crate::session::SessionManager::instance()),
                session: Some(session),
            })
            .unwrap(),
        );
        mgr.add_client(
            EXTENSION_NAME.to_string(),
            ExtensionConfig::Builtin {
                name: EXTENSION_NAME.to_string(),
                display_name: Some("Skills".to_string()),
                description: "skills".to_string(),
                timeout: None,
                bundled: None,
                available_tools: vec![],
            },
            skills_client,
            None,
            None,
            Some("s"),
        )
        .await;

        // Bounded wait — a self-deadlock would hang forever.
        let infos = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            mgr.get_extensions_info("s", &working_dir),
        )
        .await
        .expect("get_extensions_info must not deadlock on the extensions lock");

        let skills_info = infos
            .iter()
            .find(|i| i.name == EXTENSION_NAME)
            .expect("skills extension should appear in get_extensions_info output");
        assert!(
            skills_info.instructions.contains("alpha"),
            "dynamic instructions should include the MCP skill; got: {}",
            skills_info.instructions
        );
    }

    #[tokio::test]
    async fn test_dynamic_instructions_include_mcp_skills() {
        let tmp = TempDir::new().unwrap();
        let mut resources = HashMap::new();
        resources.insert(
            "skill://index.json".to_string(),
            index_json(
                r#"{"name":"alpha","type":"skill-md","description":"A","url":"skill://alpha/SKILL.md"}"#,
            ),
        );
        let (client, _mgr, _tmp_guard) =
            setup_client_with_fake("srv", resources, tmp.path().to_path_buf()).await;

        let out = client
            .get_dynamic_instructions("s")
            .await
            .expect("should have dynamic output");
        assert!(out.contains("alpha"), "got: {}", out);
        assert!(out.contains("srv"), "got: {}", out);
    }

    /// Registers a second MCP server on an existing manager. Mirrors the
    /// shape of `setup_client_with_fake` for the second call.
    async fn register_fake(
        mgr: &Arc<ExtensionManager>,
        server_name: &str,
        resources: HashMap<String, String>,
    ) {
        let fake: Arc<dyn McpClientTrait> = Arc::new(FakeMcp::new(resources));
        mgr.add_client(
            server_name.to_string(),
            ExtensionConfig::Builtin {
                name: server_name.to_string(),
                display_name: Some(server_name.to_string()),
                description: "fake mcp".to_string(),
                timeout: None,
                bundled: None,
                available_tools: vec![],
            },
            fake,
            None,
            None,
            Some("s"),
        )
        .await;
    }

    #[tokio::test]
    async fn test_mcp_vs_mcp_collision_renders_prefixed_names() {
        // Two servers publish the same skill name. Dynamic instructions
        // must render BOTH with `<server>__<name>` so the model can
        // address them unambiguously.
        let tmp = TempDir::new().unwrap();
        let mut r1 = HashMap::new();
        r1.insert(
            "skill://index.json".to_string(),
            index_json(
                r#"{"name":"shared","type":"skill-md","description":"from one","url":"skill://shared/SKILL.md"}"#,
            ),
        );
        let (client, mgr, _tmp) =
            setup_client_with_fake("one", r1, tmp.path().to_path_buf()).await;

        let mut r2 = HashMap::new();
        r2.insert(
            "skill://index.json".to_string(),
            index_json(
                r#"{"name":"shared","type":"skill-md","description":"from two","url":"skill://shared/SKILL.md"}"#,
            ),
        );
        register_fake(&mgr, "two", r2).await;

        let out = client
            .get_dynamic_instructions("s")
            .await
            .expect("dynamic output");
        assert!(out.contains("one__shared"), "missing one__shared; got:\n{}", out);
        assert!(out.contains("two__shared"), "missing two__shared; got:\n{}", out);
        // Bare "shared" alone (followed by a space or paren) must NOT appear
        // as a display name — only the prefixed forms.
        assert!(
            !out.contains("• shared "),
            "bare 'shared' should not be rendered when collision exists; got:\n{}",
            out
        );
    }

    #[tokio::test]
    async fn test_fs_vs_mcp_collision_renders_prefixed() {
        // A filesystem skill named "shared" coexists with an MCP skill
        // of the same name. The MCP entry must be rendered prefixed so
        // the model can still reach it.
        let tmp = TempDir::new().unwrap();
        let fs_skill_dir = tmp.path().join(".goose/skills/shared");
        fs::create_dir_all(&fs_skill_dir).unwrap();
        fs::write(
            fs_skill_dir.join("SKILL.md"),
            "---\nname: shared\ndescription: local\n---\nbody",
        )
        .unwrap();

        let mut r = HashMap::new();
        r.insert(
            "skill://index.json".to_string(),
            index_json(
                r#"{"name":"shared","type":"skill-md","description":"from mcp","url":"skill://shared/SKILL.md"}"#,
            ),
        );
        let (client, _mgr, _tmp) =
            setup_client_with_fake("srv", r, tmp.path().to_path_buf()).await;

        let out = client
            .get_dynamic_instructions("s")
            .await
            .expect("dynamic output");
        assert!(
            out.contains("srv__shared"),
            "MCP entry should be prefixed against FS collision; got:\n{}",
            out
        );
    }

    #[tokio::test]
    async fn test_load_skill_resolves_server_prefix() {
        // With two MCP servers publishing the same skill name, the model
        // can disambiguate by using `<server>__<name>` as the load_skill
        // argument.
        let tmp = TempDir::new().unwrap();
        let mut r1 = HashMap::new();
        r1.insert(
            "skill://index.json".to_string(),
            index_json(
                r#"{"name":"shared","type":"skill-md","description":"","url":"skill://shared/SKILL.md"}"#,
            ),
        );
        r1.insert(
            "skill://shared/SKILL.md".to_string(),
            "body from server one".to_string(),
        );
        let (client, mgr, _tmp) =
            setup_client_with_fake("one", r1, tmp.path().to_path_buf()).await;

        let mut r2 = HashMap::new();
        r2.insert(
            "skill://index.json".to_string(),
            index_json(
                r#"{"name":"shared","type":"skill-md","description":"","url":"skill://shared/SKILL.md"}"#,
            ),
        );
        r2.insert(
            "skill://shared/SKILL.md".to_string(),
            "body from server two".to_string(),
        );
        register_fake(&mgr, "two", r2).await;

        let ctx = ToolCallContext::new("s".to_string(), None, None);
        let args: JsonObject =
            serde_json::from_value(serde_json::json!({"name": "two__shared"})).unwrap();
        let result = client
            .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
            .await
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));
        let body = text_of(&result);
        assert!(
            body.contains("body from server two"),
            "prefix should route to server two; got:\n{}",
            body
        );
        assert!(!body.contains("body from server one"));
    }

    #[tokio::test]
    async fn test_load_skill_literal_name_wins_over_prefix_split() {
        // A server publishes a skill whose name contains `__`. A second
        // server coincidentally matches the left half of the split, with a
        // skill matching the right half. `load_skill` called with the
        // literal name must route to the literal entry, not the pair.
        let tmp = TempDir::new().unwrap();

        // Server "srv" hosts a skill literally named "foo__bar".
        let mut r1 = HashMap::new();
        r1.insert(
            "skill://index.json".to_string(),
            index_json(
                r#"{"name":"foo__bar","type":"skill-md","description":"","url":"skill://foo__bar/SKILL.md"}"#,
            ),
        );
        r1.insert(
            "skill://foo__bar/SKILL.md".to_string(),
            "literal foo__bar body".to_string(),
        );
        let (client, mgr, _tmp) =
            setup_client_with_fake("srv", r1, tmp.path().to_path_buf()).await;

        // Server "foo" hosts skill "bar" — creates the ambiguity.
        let mut r2 = HashMap::new();
        r2.insert(
            "skill://index.json".to_string(),
            index_json(
                r#"{"name":"bar","type":"skill-md","description":"","url":"skill://bar/SKILL.md"}"#,
            ),
        );
        r2.insert(
            "skill://bar/SKILL.md".to_string(),
            "foo's bar body".to_string(),
        );
        register_fake(&mgr, "foo", r2).await;

        let ctx = ToolCallContext::new("s".to_string(), None, None);
        let args: JsonObject =
            serde_json::from_value(serde_json::json!({"name": "foo__bar"})).unwrap();
        let result = client
            .call_tool(&ctx, "load_skill", Some(args), CancellationToken::new())
            .await
            .unwrap();

        assert!(!result.is_error.unwrap_or(false));
        let body = text_of(&result);
        assert!(
            body.contains("literal foo__bar body"),
            "literal skill name must win over server/skill prefix split; got:\n{}",
            body
        );
        assert!(!body.contains("foo's bar body"));
    }
}
