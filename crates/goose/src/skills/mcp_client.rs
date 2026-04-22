//! MCP-served Agent Skills discovery, per SEP `io.modelcontextprotocol/skills`.
//!
//! Bridges skills served over MCP (via `skill://` or any index-listed URI)
//! into Goose's existing skills pipeline. This module is the discovery layer:
//! it reads a server's `skill://index.json`, parses concrete skill entries,
//! and returns [`McpSkillEntry`] values that the skills platform extension
//! caches and surfaces in the system prompt.
//!
//! Scheme-agnostic: the SEP permits servers to list skills under a
//! domain-native URI scheme (e.g. `github://owner/repo/.../SKILL.md`) so long
//! as the entry appears in `skill://index.json` with `type: "skill-md"`.
//!
//! Security: per the SEP, skill content from MCP servers is UNTRUSTED model
//! input. This module extracts only `name`, `description`, and URI locators
//! from the index — never execution-capable fields.

use rmcp::model::{InitializeResult, ResourceContents};
use serde::Deserialize;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::agents::mcp_client::McpClientTrait;

/// Extension identifier per the SEP.
pub(crate) const SKILLS_EXTENSION_ID: &str = "io.modelcontextprotocol/skills";

/// Well-known index resource URI. Fixed by the SEP — always read from
/// `skill://index.json` regardless of which scheme the listed skills use.
pub(crate) const INDEX_URI: &str = "skill://index.json";

/// How long to wait for a server's index fetch before giving up.
/// Applied at extension-registration time so a misbehaving server cannot
/// stall session startup indefinitely. An empty cache on timeout is
/// acceptable — a future `notifications/resources/list_changed` or
/// explicit UI refresh repopulates.
pub(crate) const INDEX_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// A single indexed skill served over MCP, as surfaced to the skills
/// platform extension. `uri` is stored as-is from the index (any scheme);
/// `base_uri` is `uri` with trailing `SKILL.md` stripped for relative-ref
/// composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSkillEntry {
    pub server: String,
    pub name: String,
    pub description: String,
    pub base_uri: String,
    pub uri: String,
}

/// Returns true if the server's initialize response declares the skills
/// extension capability. Informational only — per the SEP, hosts MUST
/// attempt `skill://index.json` regardless.
pub fn server_declares_skills_capability(info: &InitializeResult) -> bool {
    info.capabilities
        .extensions
        .as_ref()
        .is_some_and(|m| m.contains_key(SKILLS_EXTENSION_ID))
}

/// Minimal index shape matching the SEP / agentskills.io discovery schema.
/// Lenient: unknown fields are ignored; unknown `type` values cause the
/// entry to be skipped (handled by the caller).
#[derive(Debug, Deserialize)]
struct IndexDoc {
    #[serde(default)]
    skills: Vec<IndexEntry>,
}

#[derive(Debug, Deserialize)]
struct IndexEntry {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "type")]
    entry_type: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

/// Fetches and parses `skill://index.json` from a single MCP server via its
/// client handle. Returns an empty vec (with a log) on any failure — this
/// function MUST NOT propagate errors because it runs during extension
/// registration and must not block the agent from starting.
///
/// Caller supplies the server name (extension key) because it's stamped
/// into each returned entry's `server` field for later routing.
pub async fn fetch_server_skills(
    server: &str,
    client: &dyn McpClientTrait,
    session_id: &str,
    cancel: CancellationToken,
) -> Vec<McpSkillEntry> {
    let fetch = async {
        let read = client
            .read_resource(session_id, INDEX_URI, cancel.clone())
            .await?;

        let text = read
            .contents
            .into_iter()
            .find_map(|c| match c {
                ResourceContents::TextResourceContents { text, .. } => Some(text),
                _ => None,
            })
            .ok_or("index resource contained no text contents")?;

        let doc: IndexDoc = serde_json::from_str(&text)
            .map_err(|e| format!("failed to parse {}: {}", INDEX_URI, e))?;

        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(doc)
    };

    let doc = match tokio::time::timeout(INDEX_FETCH_TIMEOUT, fetch).await {
        Ok(Ok(doc)) => doc,
        Ok(Err(e)) => {
            debug!(server, error = %e, "skill index fetch: no usable index");
            return Vec::new();
        }
        Err(_) => {
            warn!(
                server,
                timeout_secs = INDEX_FETCH_TIMEOUT.as_secs(),
                "skill index fetch timed out"
            );
            return Vec::new();
        }
    };

    let mut entries = Vec::new();
    for raw in doc.skills {
        if let Some(entry) = parse_index_entry(server, raw) {
            entries.push(entry);
        }
    }
    entries
}

fn parse_index_entry(server: &str, raw: IndexEntry) -> Option<McpSkillEntry> {
    match raw.entry_type.as_deref() {
        Some("skill-md") => {}
        Some("mcp-resource-template") => {
            // Templates are deferred — the SEP wires them to the MCP
            // completion API, which this implementation does not yet surface.
            return None;
        }
        Some(other) => {
            debug!(server, entry_type = other, "skipping unknown index entry type");
            return None;
        }
        None => {
            debug!(server, "skipping index entry with no type");
            return None;
        }
    }

    let name = raw.name.filter(|s| !s.is_empty()).or_else(|| {
        warn!(server, "skill-md index entry missing required `name`");
        None
    })?;
    let description = raw.description.unwrap_or_default();
    let url = raw.url.filter(|s| !s.is_empty()).or_else(|| {
        warn!(server, name, "skill-md index entry missing required `url`");
        None
    })?;

    let Some(base_uri) = url.strip_suffix("SKILL.md") else {
        warn!(
            server,
            name, url, "skill-md index entry `url` does not end in SKILL.md — skipping"
        );
        return None;
    };

    Some(McpSkillEntry {
        server: server.to_string(),
        name,
        description,
        base_uri: base_uri.to_string(),
        uri: url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rmcp::model::{
        CallToolResult, ExtensionCapabilities, InitializeResult, JsonObject, ListResourcesResult,
        ListToolsResult, ReadResourceResult, ServerCapabilities, ServerNotification,
    };
    use std::collections::HashMap;
    use tokio::sync::mpsc;

    use crate::agents::mcp_client::Error;
    use crate::agents::ToolCallContext;

    /// Test double — exposes a fixed set of resources and no tools.
    pub struct FakeSkillsServer {
        pub info: InitializeResult,
        pub resources: HashMap<String, String>,
        pub delay: Option<Duration>,
    }

    impl FakeSkillsServer {
        fn with_capability() -> InitializeResult {
            let mut caps = ExtensionCapabilities::new();
            caps.insert(SKILLS_EXTENSION_ID.to_string(), JsonObject::new());
            InitializeResult::new(
                ServerCapabilities::builder()
                    .enable_resources()
                    .enable_extensions_with(caps)
                    .build(),
            )
        }

        fn without_capability() -> InitializeResult {
            InitializeResult::new(ServerCapabilities::builder().enable_resources().build())
        }
    }

    #[async_trait]
    impl McpClientTrait for FakeSkillsServer {
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
            unreachable!("FakeSkillsServer has no tools")
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
            if let Some(delay) = self.delay {
                tokio::time::sleep(delay).await;
            }
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

    fn index_with(entries: &str) -> String {
        format!(
            r#"{{"$schema":"https://schemas.agentskills.io/discovery/0.2.0/schema.json","skills":[{}]}}"#,
            entries
        )
    }

    #[test]
    fn test_server_declares_capability() {
        let declared = FakeSkillsServer::with_capability();
        assert!(server_declares_skills_capability(&declared));

        let undeclared = FakeSkillsServer::without_capability();
        assert!(!server_declares_skills_capability(&undeclared));
    }

    #[tokio::test]
    async fn test_discover_via_index_json() {
        let mut resources = HashMap::new();
        resources.insert(
            INDEX_URI.to_string(),
            index_with(
                r#"{"name":"git-workflow","type":"skill-md","description":"Git conventions","url":"skill://git-workflow/SKILL.md"},
                   {"name":"refunds","type":"skill-md","description":"Process refunds","url":"skill://acme/billing/refunds/SKILL.md"}"#,
            ),
        );
        let server = FakeSkillsServer {
            info: FakeSkillsServer::with_capability(),
            resources,
            delay: None,
        };

        let entries =
            fetch_server_skills("gh", &server as &dyn McpClientTrait, "s", CancellationToken::new())
                .await;

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "git-workflow");
        assert_eq!(entries[0].uri, "skill://git-workflow/SKILL.md");
        assert_eq!(entries[0].base_uri, "skill://git-workflow/");
        assert_eq!(entries[0].server, "gh");
        assert_eq!(entries[1].name, "refunds");
        assert_eq!(entries[1].base_uri, "skill://acme/billing/refunds/");
    }

    #[tokio::test]
    async fn test_discover_tolerates_missing_index() {
        let server = FakeSkillsServer {
            info: FakeSkillsServer::with_capability(),
            resources: HashMap::new(),
            delay: None,
        };

        let entries =
            fetch_server_skills("gh", &server as &dyn McpClientTrait, "s", CancellationToken::new())
                .await;
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn test_discover_skips_templates() {
        let mut resources = HashMap::new();
        resources.insert(
            INDEX_URI.to_string(),
            index_with(
                r#"{"name":"real","type":"skill-md","description":"","url":"skill://real/SKILL.md"},
                   {"type":"mcp-resource-template","description":"Per-product docs","url":"skill://docs/{product}/SKILL.md"}"#,
            ),
        );
        let server = FakeSkillsServer {
            info: FakeSkillsServer::with_capability(),
            resources,
            delay: None,
        };

        let entries =
            fetch_server_skills("gh", &server as &dyn McpClientTrait, "s", CancellationToken::new())
                .await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "real");
    }

    #[tokio::test]
    async fn test_discover_accepts_non_skill_scheme() {
        let mut resources = HashMap::new();
        resources.insert(
            INDEX_URI.to_string(),
            index_with(
                r#"{"name":"pull-requests","type":"skill-md","description":"","url":"github://github/repo/skills/pull-requests/SKILL.md"}"#,
            ),
        );
        let server = FakeSkillsServer {
            info: FakeSkillsServer::with_capability(),
            resources,
            delay: None,
        };

        let entries = fetch_server_skills(
            "github",
            &server as &dyn McpClientTrait,
            "s",
            CancellationToken::new(),
        )
        .await;
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].uri,
            "github://github/repo/skills/pull-requests/SKILL.md"
        );
        assert_eq!(
            entries[0].base_uri,
            "github://github/repo/skills/pull-requests/"
        );
    }

    #[tokio::test]
    async fn test_discover_timeout_does_not_block() {
        // Server that sleeps longer than the fetch timeout.
        let server = FakeSkillsServer {
            info: FakeSkillsServer::with_capability(),
            resources: HashMap::new(),
            delay: Some(INDEX_FETCH_TIMEOUT + Duration::from_millis(500)),
        };

        let start = std::time::Instant::now();
        let entries = fetch_server_skills(
            "slow",
            &server as &dyn McpClientTrait,
            "s",
            CancellationToken::new(),
        )
        .await;
        let elapsed = start.elapsed();

        assert!(entries.is_empty());
        // Should have bailed after the timeout, not waited for the server.
        assert!(
            elapsed < INDEX_FETCH_TIMEOUT + Duration::from_millis(500),
            "fetch took {:?}, should have timed out",
            elapsed
        );
    }

}
