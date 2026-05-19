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

/// The Agent Skills discovery schema URI this host has been tested
/// against. Per the SEP ("Clients SHOULD match against known $schema
/// URIs before processing"), we log at `debug!` when the server's
/// declared `$schema` doesn't match — but still attempt to process the
/// index leniently. Newer schemas typically remain wire-compatible for
/// the small subset of fields we read.
pub(crate) const KNOWN_INDEX_SCHEMA: &str =
    "https://schemas.agentskills.io/discovery/0.2.0/schema.json";

/// How long to wait for a server's index fetch before giving up.
/// Applied at extension-registration time so a misbehaving server cannot
/// stall session startup indefinitely. An empty cache on timeout is
/// acceptable — a future `notifications/resources/list_changed` or
/// explicit UI refresh repopulates.
pub(crate) const INDEX_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// A single indexed skill served over MCP, as surfaced to the skills
/// platform extension. `url` is stored as-is from the index (any scheme).
/// The skill root (used for composing relative refs) is derived from `url`
/// at use time via [`McpSkillEntry::skill_root_uri`] rather than cached as
/// a separate field — per the SEP, hosts resolve relative refs against
/// the skill's directory URI, which is the entry URI minus its final
/// path segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSkillEntry {
    pub server: String,
    pub name: String,
    pub description: String,
    pub url: String,
}

impl McpSkillEntry {
    /// Skill root URI: `url` truncated at (and including) the final `/`.
    /// If `url` already ends with `/`, returns it unchanged. This matches
    /// the SEP's relative-resolution rule — the entry URI's directory is
    /// the base for relative refs, regardless of whether the trailing
    /// segment is the literal `SKILL.md` or something else.
    ///
    /// Returns a borrowed slice — called on the per-turn prompt-render
    /// path, so avoid the allocation.
    pub fn skill_root_uri(&self) -> &str {
        match self.url.rfind('/') {
            Some(idx) => &self.url[..=idx],
            None => &self.url,
        }
    }
}

/// A templated skill catalog entry — `type: "mcp-resource-template"` in
/// the SEP. `url_template` is an RFC 6570 level-1 template (e.g.
/// `github://{owner}/{repo}/.../SKILL.md`); placeholders are resolved at
/// `load_skill_template` time via the MCP `completion/complete` endpoint.
///
/// Stored separately from [`McpSkillEntry`] because templates need
/// completion plumbing concrete entries don't, and the rendering path
/// for the system prompt is distinct (a sibling bullet list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSkillTemplate {
    pub server: String,
    pub name: String,
    pub description: String,
    pub url_template: String,
}

impl McpSkillTemplate {
    /// Returns the placeholder names (`{name}`) appearing in
    /// `url_template`, in left-to-right order, de-duplicated. Used to
    /// build the `[placeholders: ...]` hint in the system prompt and to
    /// drive completion validation. Hand-rolled scanner; no regex
    /// dependency added.
    pub fn placeholders(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let bytes = self.url_template.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'{' {
                let start = i + 1;
                let mut j = start;
                while j < bytes.len() && bytes[j] != b'}' && bytes[j] != b'/' {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'}' && j > start {
                    if let Ok(name) = std::str::from_utf8(&bytes[start..j]) {
                        if !out.iter().any(|n| n == name) {
                            out.push(name.to_string());
                        }
                    }
                    i = j + 1;
                    continue;
                }
            }
            i += 1;
        }
        out
    }
}

/// All MCP-served skills discovered from a single server's index. The
/// split mirrors the two SEP entry types — concrete `skill-md` entries
/// addressable by name, and `mcp-resource-template` catalogs that the
/// model addresses via `load_skill_template` after the host validates
/// placeholder values against the server's completion endpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerSkills {
    pub concrete: Vec<McpSkillEntry>,
    pub templates: Vec<McpSkillTemplate>,
}

impl ServerSkills {
    pub fn is_empty(&self) -> bool {
        self.concrete.is_empty() && self.templates.is_empty()
    }
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
/// entry to be skipped (handled by the caller). `$schema` is captured
/// so [`fetch_server_skills`] can log when it diverges from the schema
/// the host was built against — per the SEP, clients SHOULD match
/// against known `$schema` URIs before processing.
#[derive(Debug, Deserialize)]
struct IndexDoc {
    #[serde(default, rename = "$schema")]
    schema: Option<String>,
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

/// Fetches and parses `skill://index.json` from a single MCP server via
/// its client handle. Returns an empty [`ServerSkills`] (with a log) on
/// any failure — this function MUST NOT propagate errors because it runs
/// during extension registration and must not block the agent from
/// starting.
///
/// Caller supplies the server name (extension key) because it's stamped
/// into each returned entry's `server` field for later routing.
pub async fn fetch_server_skills(
    server: &str,
    client: &dyn McpClientTrait,
    session_id: &str,
    cancel: CancellationToken,
) -> ServerSkills {
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
            return ServerSkills::default();
        }
        Err(_) => {
            warn!(
                server,
                timeout_secs = INDEX_FETCH_TIMEOUT.as_secs(),
                "skill index fetch timed out"
            );
            return ServerSkills::default();
        }
    };

    // SEP SHOULD: match against known $schema URIs before processing.
    // Lenient — we still process. A server publishing a newer schema
    // typically stays wire-compatible for our subset (name, type,
    // description, url), and a server omitting `$schema` is common
    // enough to not be worth blocking on.
    match doc.schema.as_deref() {
        Some(KNOWN_INDEX_SCHEMA) => {}
        Some(other) => debug!(
            server,
            declared = other,
            expected = KNOWN_INDEX_SCHEMA,
            "skill index `$schema` does not match the host-known URI; processing leniently"
        ),
        None => debug!(
            server,
            "skill index has no `$schema` field; processing leniently"
        ),
    }

    let mut out = ServerSkills::default();
    let mut template_counter = 0usize;
    for raw in doc.skills {
        match parse_index_entry(server, raw, &mut template_counter) {
            Parsed::Concrete(entry) => out.concrete.push(entry),
            Parsed::Template(tpl) => out.templates.push(tpl),
            Parsed::Skip => {}
        }
    }
    out
}

enum Parsed {
    Concrete(McpSkillEntry),
    Template(McpSkillTemplate),
    Skip,
}

fn parse_index_entry(server: &str, raw: IndexEntry, template_counter: &mut usize) -> Parsed {
    match raw.entry_type.as_deref() {
        Some("skill-md") => parse_concrete(server, raw),
        Some("mcp-resource-template") => parse_template(server, raw, template_counter),
        Some(other) => {
            debug!(
                server,
                entry_type = other,
                "skipping unknown index entry type"
            );
            Parsed::Skip
        }
        None => {
            debug!(server, "skipping index entry with no type");
            Parsed::Skip
        }
    }
}

fn parse_concrete(server: &str, raw: IndexEntry) -> Parsed {
    let Some(name) = raw.name.filter(|s| !s.is_empty()) else {
        warn!(server, "skill-md index entry missing required `name`");
        return Parsed::Skip;
    };
    let description = raw.description.unwrap_or_default();
    let Some(url) = raw.url.filter(|s| !s.is_empty()) else {
        warn!(server, name, "skill-md index entry missing required `url`");
        return Parsed::Skip;
    };

    if !url.ends_with("SKILL.md") {
        debug!(
            server,
            name, url, "skill-md index entry `url` does not end in `SKILL.md`"
        );
    }

    Parsed::Concrete(McpSkillEntry {
        server: server.to_string(),
        name,
        description,
        url,
    })
}

fn parse_template(server: &str, raw: IndexEntry, counter: &mut usize) -> Parsed {
    let Some(url_template) = raw.url.filter(|s| !s.is_empty()) else {
        warn!(server, "mcp-resource-template entry missing required `url`");
        return Parsed::Skip;
    };
    // Per the SEP, the SHOULD-level template entry name is optional —
    // synthesize a stable ordinal when absent so the model has a handle
    // to address it.
    let name = raw.name.filter(|s| !s.is_empty()).unwrap_or_else(|| {
        *counter += 1;
        format!("template-{}", counter)
    });
    let description = raw.description.unwrap_or_default();

    Parsed::Template(McpSkillTemplate {
        server: server.to_string(),
        name,
        description,
        url_template,
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

        let skills = fetch_server_skills(
            "gh",
            &server as &dyn McpClientTrait,
            "s",
            CancellationToken::new(),
        )
        .await;

        let entries = &skills.concrete;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "git-workflow");
        assert_eq!(entries[0].url, "skill://git-workflow/SKILL.md");
        assert_eq!(entries[0].skill_root_uri(), "skill://git-workflow/");
        assert_eq!(entries[0].server, "gh");
        assert_eq!(entries[1].name, "refunds");
        assert_eq!(entries[1].skill_root_uri(), "skill://acme/billing/refunds/");
        assert!(skills.templates.is_empty());
    }

    #[tokio::test]
    async fn test_discover_tolerates_missing_index() {
        let server = FakeSkillsServer {
            info: FakeSkillsServer::with_capability(),
            resources: HashMap::new(),
            delay: None,
        };

        let skills = fetch_server_skills(
            "gh",
            &server as &dyn McpClientTrait,
            "s",
            CancellationToken::new(),
        )
        .await;
        assert!(skills.is_empty());
    }

    #[tokio::test]
    async fn test_discover_returns_templates() {
        let mut resources = HashMap::new();
        resources.insert(
            INDEX_URI.to_string(),
            index_with(
                r#"{"name":"real","type":"skill-md","description":"","url":"skill://real/SKILL.md"},
                   {"name":"product-docs","type":"mcp-resource-template","description":"Per-product docs","url":"skill://docs/{product}/SKILL.md"},
                   {"type":"mcp-resource-template","description":"Workflow runs","url":"github://{owner}/{repo}/.../SKILL.md"}"#,
            ),
        );
        let server = FakeSkillsServer {
            info: FakeSkillsServer::with_capability(),
            resources,
            delay: None,
        };

        let skills = fetch_server_skills(
            "gh",
            &server as &dyn McpClientTrait,
            "s",
            CancellationToken::new(),
        )
        .await;

        // Concrete bin: one entry.
        assert_eq!(skills.concrete.len(), 1);
        assert_eq!(skills.concrete[0].name, "real");

        // Template bin: two entries. First keeps its declared name; second
        // is unnamed, gets a synthesized `template-N` handle.
        assert_eq!(skills.templates.len(), 2);
        assert_eq!(skills.templates[0].name, "product-docs");
        assert_eq!(skills.templates[0].placeholders(), vec!["product"]);
        assert_eq!(skills.templates[1].name, "template-1");
        assert_eq!(skills.templates[1].placeholders(), vec!["owner", "repo"]);
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

        let skills = fetch_server_skills(
            "github",
            &server as &dyn McpClientTrait,
            "s",
            CancellationToken::new(),
        )
        .await;
        let entries = &skills.concrete;
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].url,
            "github://github/repo/skills/pull-requests/SKILL.md"
        );
        assert_eq!(
            entries[0].skill_root_uri(),
            "github://github/repo/skills/pull-requests/"
        );
    }

    #[tokio::test]
    async fn test_discover_tolerates_unknown_schema_uri() {
        // SEP says clients SHOULD match against known $schema URIs before
        // processing; our policy is to log at debug! and still process,
        // since the subset of fields we read (name, type, description,
        // url) is stable across the schema revisions we know about.
        let mut resources = HashMap::new();
        resources.insert(
            INDEX_URI.to_string(),
            // Hand-rolled to override the `$schema` value (the `index_with`
            // helper hardcodes the canonical one).
            r#"{
              "$schema": "https://schemas.agentskills.io/discovery/9.9.9/schema.json",
              "skills": [
                {"name":"alpha","type":"skill-md","description":"a","url":"skill://alpha/SKILL.md"}
              ]
            }"#
            .to_string(),
        );
        let server = FakeSkillsServer {
            info: FakeSkillsServer::with_capability(),
            resources,
            delay: None,
        };

        let skills = fetch_server_skills(
            "gh",
            &server as &dyn McpClientTrait,
            "s",
            CancellationToken::new(),
        )
        .await;
        assert_eq!(skills.concrete.len(), 1);
        assert_eq!(skills.concrete[0].name, "alpha");
    }

    #[tokio::test]
    async fn test_discover_tolerates_missing_schema_field() {
        // Same lenient posture for an index with no `$schema` at all —
        // common in early servers — should not block discovery.
        let mut resources = HashMap::new();
        resources.insert(
            INDEX_URI.to_string(),
            r#"{
              "skills": [
                {"name":"alpha","type":"skill-md","description":"a","url":"skill://alpha/SKILL.md"}
              ]
            }"#
            .to_string(),
        );
        let server = FakeSkillsServer {
            info: FakeSkillsServer::with_capability(),
            resources,
            delay: None,
        };
        let skills = fetch_server_skills(
            "gh",
            &server as &dyn McpClientTrait,
            "s",
            CancellationToken::new(),
        )
        .await;
        assert_eq!(skills.concrete.len(), 1);
    }

    #[tokio::test]
    async fn test_discover_directory_form_url() {
        // Per the SEP, hosts MUST tolerate `url` entries that do not end in
        // the literal `SKILL.md` — e.g. servers that publish a skill's
        // directory URI. The skill_root_uri derivation should yield the
        // same directory.
        let mut resources = HashMap::new();
        resources.insert(
            INDEX_URI.to_string(),
            index_with(
                r#"{"name":"refunds","type":"skill-md","description":"","url":"skill://acme/billing/refunds/"}"#,
            ),
        );
        let server = FakeSkillsServer {
            info: FakeSkillsServer::with_capability(),
            resources,
            delay: None,
        };

        let skills = fetch_server_skills(
            "acme",
            &server as &dyn McpClientTrait,
            "s",
            CancellationToken::new(),
        )
        .await;
        let entries = &skills.concrete;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].url, "skill://acme/billing/refunds/");
        assert_eq!(entries[0].skill_root_uri(), "skill://acme/billing/refunds/");
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
        let skills = fetch_server_skills(
            "slow",
            &server as &dyn McpClientTrait,
            "s",
            CancellationToken::new(),
        )
        .await;
        let elapsed = start.elapsed();

        assert!(skills.is_empty());
        // Should have bailed after the timeout, not waited for the server.
        assert!(
            elapsed < INDEX_FETCH_TIMEOUT + Duration::from_millis(500),
            "fetch took {:?}, should have timed out",
            elapsed
        );
    }
}
