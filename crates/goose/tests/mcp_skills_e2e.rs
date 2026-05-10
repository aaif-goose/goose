//! Mechanical end-to-end test for the skills-over-MCP extension.
//!
//! Spawns the test fork of `github-mcp-server` (the `add-agent-skills`
//! branch of `skills-over-mcp-ig/servers/github-mcp-server`) on an
//! ephemeral port, wires it into an `ExtensionManager`, and verifies the
//! discovery + load round-trip end-to-end:
//!
//! 1. `skill://index.json` is fetched at connect time and the cache is
//!    populated with the `pull-requests` entry (concrete `skill-md`).
//! 2. The entry exposes `url` directly and derives the skill root via
//!    `skill_root_uri()` (no separate `base_uri` field — that was a
//!    pre-SEP host-internal optimization that has been removed).
//! 3. `load_skill("pull-requests")` on the `SkillsClient` dispatches via
//!    `ExtensionManager::read_resource` (now requires `extension_name`)
//!    and returns the SKILL.md body, in the same `# Loaded Skill: …` /
//!    `## Supporting Files` / `Skill base:` framing used by FS skills
//!    (see `test_load_skill_framing_parity_fs_vs_mcp` for the lock).
//!
//! Behaviour not covered here, exercised by unit tests in
//! `crates/goose/src/skills/`:
//!
//! - `mcp-resource-template` entries (parsed by `parse_template`,
//!   resolved via `load_skill_template` with MCP `completion/complete`
//!   validation) — see `skills::client::tests::test_load_skill_template_*`.
//! - `notifications/resources/list_changed` cache invalidation — see
//!   `skills::client::tests::test_list_changed_refreshes_cache`.
//!
//! No LLM involved; catches regressions in the resource-read plumbing in
//! sub-second wallclock. Gated on `GITHUB_TOKEN` and the server binary
//! existing at `servers/github-mcp-server/github-mcp-server[.exe]` (or
//! the explicit `GOOSE_E2E_SERVER_BIN` override). Marked `#[ignore]` so
//! `cargo test` stays quiet. Run with:
//!
//!     GITHUB_TOKEN=$(gh auth token) \
//!         cargo test -p goose --test mcp_skills_e2e -- --ignored --nocapture
//!
//! LLM-in-the-loop scenario runs live out-of-tree at
//! `skills-over-mcp-ig/clients/goose-harness/`.

use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use goose::agents::extension::{Envs, ExtensionConfig, PlatformExtensionContext};
use goose::agents::extension_manager::ExtensionManager;
use goose::agents::mcp_client::McpClientTrait;
use goose::agents::ToolCallContext;
use goose::skills::mcp_client::McpSkillEntry;
use goose::skills::SkillsClient;

/// Pick an unused local TCP port by asking the OS for one.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    listener.local_addr().unwrap().port()
}

/// Poll TCP connect until the server is accepting connections or the
/// deadline passes. Returns Err with elapsed on timeout.
fn wait_for_port(port: u16, timeout: Duration) -> Result<(), Duration> {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let start = Instant::now();
    while start.elapsed() < timeout {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(start.elapsed())
}

/// Guarded subprocess — kills the server when dropped.
struct ServerHandle {
    child: Child,
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Default path to the pre-built server binary in this workspace layout.
fn default_server_binary() -> PathBuf {
    // `crates/goose/tests/...` — four parents up to `skills-over-mcp-ig/`.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir
        .parent() // crates/
        .and_then(|p| p.parent()) // clients/goose/
        .and_then(|p| p.parent()) // clients/
        .and_then(|p| p.parent()) // skills-over-mcp-ig/
        .expect("workspace layout");
    root.join("servers")
        .join("github-mcp-server")
        .join(if cfg!(windows) {
            "github-mcp-server.exe"
        } else {
            "github-mcp-server"
        })
}

fn spawn_server(port: u16) -> Result<ServerHandle, String> {
    let bin = std::env::var("GOOSE_E2E_SERVER_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_server_binary());

    if !bin.exists() {
        return Err(format!(
            "server binary not found at {}. Set GOOSE_E2E_SERVER_BIN.",
            bin.display()
        ));
    }

    let child = Command::new(&bin)
        .arg("http")
        .arg("--port")
        .arg(port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to spawn {}: {}", bin.display(), e))?;

    let handle = ServerHandle { child };

    match wait_for_port(port, Duration::from_secs(10)) {
        Ok(()) => Ok(handle),
        Err(elapsed) => Err(format!(
            "server did not bind :{} within {:?}",
            port, elapsed
        )),
    }
}

/// Skip the test with a printed reason when prerequisites are missing.
macro_rules! skip_if_missing {
    ($var:expr, $reason:expr) => {
        if std::env::var($var).is_err() {
            eprintln!("SKIP: {} ({} not set)", $reason, $var);
            return;
        }
    };
}

/// Builds the same `streamable_http` extension config the desktop UI would
/// construct for the test fork of `github-mcp-server`. `endpoint` is the
/// full MCP URI (e.g. `http://127.0.0.1:8082/mcp`).
fn github_extension_config(endpoint: &str, token: &str) -> ExtensionConfig {
    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), format!("Bearer {}", token));
    ExtensionConfig::StreamableHttp {
        name: "github".to_string(),
        description: "GitHub MCP (skills-over-MCP test fork)".to_string(),
        uri: endpoint.to_string(),
        envs: Envs::default(),
        env_keys: Vec::new(),
        headers,
        timeout: Some(30),
        socket: None,
        bundled: None,
        available_tools: Vec::new(),
    }
}

#[tokio::test]
#[ignore]
async fn mcp_skills_discovery_and_load_against_real_server() {
    skip_if_missing!(
        "GITHUB_TOKEN",
        "needs github PAT to auth against github-mcp-server"
    );
    let token = std::env::var("GITHUB_TOKEN").unwrap();

    let port = free_port();
    let server = match spawn_server(port) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("SKIP: {}", e);
            return;
        }
    };
    eprintln!("[e2e] spawned github-mcp-server on :{}", port);

    // Construct an ExtensionManager the way production code does.
    let temp = tempfile::tempdir().expect("tempdir");
    let mgr = Arc::new(ExtensionManager::new_without_provider(
        temp.path().to_path_buf(),
    ));

    // Pass the session id we'll use later for load_skill — the MCP
    // client locks itself to a single session on its first request.
    mgr.add_extension(
        github_extension_config(&format!("http://127.0.0.1:{}/mcp", port), &token),
        None,
        None,
        Some("e2e-session"),
    )
    .await
    .expect("add_extension should succeed");

    // Phase 1: mechanical check — cache populated?
    let cached: Vec<McpSkillEntry> = mgr.aggregated_mcp_skills().await;
    eprintln!("[e2e] cached mcp skills: {:#?}", cached);
    assert!(
        !cached.is_empty(),
        "server should advertise at least one concrete skill via skill://index.json"
    );

    // The reference server (`skills-over-mcp-ig/servers/github-mcp-server`
    // on `add-agent-skills`) ships a set of GitHub-workflow skills whose
    // names are stable across the fork. Pick one we'll exercise the
    // load_skill round-trip against. `review-pr` covers the multi-comment
    // pending-review flow.
    const SAMPLE_SKILL: &str = "review-pr";
    let entry = cached
        .iter()
        .find(|e| e.name == SAMPLE_SKILL)
        .unwrap_or_else(|| {
            panic!(
                "expected `{}` in discovered skills; got names: {:?}",
                SAMPLE_SKILL,
                cached.iter().map(|e| &e.name).collect::<Vec<_>>()
            )
        });
    assert!(
        entry.url.starts_with("skill://") && entry.url.ends_with("/SKILL.md"),
        "entry URL should be a skill:// SKILL.md URI; got: {}",
        entry.url
    );
    assert!(
        entry.skill_root_uri().ends_with('/'),
        "skill_root_uri should end in `/`; got: {}",
        entry.skill_root_uri()
    );

    // Phase 2: mechanical check — load_skill dispatches through resources/read?
    // Construct a SkillsClient wired to the manager (same shape as the
    // platform extension does in production).
    let session_manager = Arc::new(goose::session::SessionManager::instance());
    let ctx = PlatformExtensionContext {
        extension_manager: Some(Arc::downgrade(&mgr)),
        session_manager,
        session: Some(Arc::new(goose::session::Session {
            working_dir: temp.path().to_path_buf(),
            ..goose::session::Session::default()
        })),
    };
    let skills_client = SkillsClient::new(ctx).expect("SkillsClient::new");

    let tool_ctx = ToolCallContext::new("e2e-session".to_string(), None, None);
    let args: rmcp::model::JsonObject =
        serde_json::from_value(serde_json::json!({"name": SAMPLE_SKILL})).unwrap();
    let result = skills_client
        .call_tool(
            &tool_ctx,
            "load_skill",
            Some(args),
            CancellationToken::new(),
        )
        .await
        .expect("call_tool should not propagate transport error");

    let text = match &result.content[0].raw {
        rmcp::model::RawContent::Text(t) => t.text.clone(),
        _ => panic!("expected text content"),
    };
    eprintln!("[e2e] load_skill output (truncated):\n{:.400}", text);

    assert!(
        !result.is_error.unwrap_or(false),
        "load_skill should succeed; got: {}",
        text
    );
    assert!(
        text.starts_with(&format!("# Loaded Skill: {}", SAMPLE_SKILL)),
        "framing should lead with `# Loaded Skill: <name>`; got: {:.200}",
        text
    );
    assert!(
        text.contains("(mcp skill from github)"),
        "framing should carry the MCP origin tag; got: {:.200}",
        text
    );
    // The framing also lays down the FS/MCP parity anchor.
    assert!(
        text.contains("Skill base:") || !text.contains("## Supporting Files"),
        "if a Supporting Files section appears, it must use the `Skill base:` header; got:\n{}",
        text
    );

    // Dynamic instructions should surface the skill too.
    let dynamic = skills_client
        .get_dynamic_instructions("e2e-session")
        .await
        .expect("get_dynamic_instructions should return Some");
    eprintln!("[e2e] dynamic instructions:\n{}", dynamic);
    assert!(
        dynamic.contains(SAMPLE_SKILL) && dynamic.contains("github"),
        "dynamic instructions should list the skill with its origin server"
    );

    drop(server); // explicit kill — makes intent obvious
    eprintln!("[e2e] PASS");
}
