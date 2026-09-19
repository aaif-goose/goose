use serde::Deserialize;

use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use std::{env, fs};

use futures::StreamExt;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ErrorCode, ErrorData, ProtocolVersion, Tool,
};
use rmcp::object;
use tokio_util::sync::CancellationToken;

use goose::action_required_manager::ElicitationOutcome;
use goose::agents::extension::{Envs, ExtensionConfig};
use goose::agents::extension_manager::{
    CallRequest, ExtensionLease, ExtensionManager, ExtensionManagerCapabilities, ExtensionSet,
};
use goose::agents::GoosePlatform;
use goose::config::GooseMode;
use goose::conversation::message::ActionRequiredData;
use goose::session::SessionType;
use goose_providers::model::ModelConfig;
use goose_test_support::mcp::{ContextReport, APP_CARD_HTML, APP_CARD_RESOURCE_URI};
use goose_test_support::{McpFixture, FAKE_CODE};

use test_case::test_case;

use async_trait::async_trait;
use goose::conversation::message::Message;
use goose::providers::base::{
    stream_from_single_message, MessageStream, Provider, ProviderDef, ProviderMetadata,
};
use goose_providers::conversation::token_usage::{ProviderUsage, Usage};
use goose_providers::errors::ProviderError;
use once_cell::sync::Lazy;
use std::process::Command;

#[derive(Deserialize)]
struct CargoBuildMessage {
    reason: String,
    target: Target,
    executable: String,
}

#[derive(Deserialize)]
struct Target {
    name: String,
    kind: Vec<String>,
}

#[derive(Clone, Default)]
pub struct MockProvider;

impl MockProvider {
    pub fn new() -> Self {
        Self
    }
}

impl goose::providers::base::ProviderDescriptor for MockProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::empty()
    }
}

impl ProviderDef for MockProvider {
    type Provider = Self;

    fn from_env(
        _extensions: Vec<goose::config::ExtensionConfig>,
        _tls_config: Option<goose::providers::api_client::TlsConfig>,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<Self>> {
        Box::pin(async move { Ok(Self::new()) })
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn get_name(&self) -> &str {
        "mock"
    }

    async fn stream(
        &self,
        _model_config: &ModelConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let message = Message::assistant().with_text("\"So we beat on, boats against the current, borne back ceaselessly into the past.\" — F. Scott Fitzgerald, The Great Gatsby (1925)");
        let usage = ProviderUsage::new("mock".to_string(), Usage::default());
        Ok(stream_from_single_message(message, usage))
    }
}

fn build_bin(package: &str, bin: &str) -> PathBuf {
    let output = Command::new("cargo")
        .args([
            "build",
            "--frozen",
            "-p",
            package,
            "--bin",
            bin,
            "--message-format=json",
        ])
        .output()
        .expect("failed to build binary");

    if !output.status.success() {
        panic!("build failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(serde_json::from_str::<CargoBuildMessage>)
        .filter_map(Result::ok)
        .filter(|message| message.reason == "compiler-artifact")
        .filter_map(|message| {
            if message.target.name == bin && message.target.kind.contains(&String::from("bin")) {
                Some(PathBuf::from(message.executable))
            } else {
                None
            }
        })
        .next()
        .expect("failed to parse binary path")
}

static REPLAY_BINARY_PATH: Lazy<PathBuf> = Lazy::new(|| build_bin("goose-test", "capture"));
static FIXTURE_BINARY_PATH: Lazy<PathBuf> =
    Lazy::new(|| build_bin("goose-test-support", "mcp_fixture_server"));

struct Fixture {
    manager: Arc<ExtensionManager>,
    session_manager: Arc<goose::session::SessionManager>,
    _temp_dir: tempfile::TempDir,
}

/// `protocol_version` pins the client to the legacy `initialize` handshake;
/// `None` lets it probe `server/discover` first.
async fn fixture(mcpui: bool, protocol_version: Option<ProtocolVersion>) -> Fixture {
    let temp_dir = tempfile::tempdir().unwrap();
    let session_manager = Arc::new(goose::session::SessionManager::new(
        temp_dir.path().to_path_buf(),
    ));
    let manager = Arc::new(ExtensionManager::new(
        Arc::new(tokio::sync::Mutex::new(None)),
        session_manager.clone(),
        None,
        "extension-contract".to_string(),
        ExtensionManagerCapabilities {
            mcpui,
            host_info: None,
            elicitation_handler: None,
            protocol_version,
        },
        false,
    ));
    Fixture {
        manager,
        session_manager,
        _temp_dir: temp_dir,
    }
}

impl Fixture {
    async fn session(&self, session_type: SessionType) -> goose::session::Session {
        self.session_manager
            .create_session(
                self._temp_dir.path().to_path_buf(),
                format!("{session_type:?}"),
                session_type,
                GooseMode::Auto,
            )
            .await
            .unwrap()
    }

    async fn add(&self, session: &goose::session::Session, config: &ExtensionConfig) {
        self.manager
            .add_extension(
                config.clone(),
                Some(session.working_dir.clone()),
                None,
                Some(&session.id),
            )
            .await
            .unwrap();
    }

    async fn resolve(
        &self,
        session: &goose::session::Session,
        configs: &[ExtensionConfig],
    ) -> ExtensionLease {
        self.manager
            .resolve(
                &ExtensionSet::new(
                    &session.id,
                    Some(session.working_dir.clone()),
                    configs.to_vec(),
                )
                .unwrap(),
            )
            .await
    }
}

fn stdio_fixture(name: &str, mode: Option<&str>, available_tools: &[&str]) -> ExtensionConfig {
    let mut args = vec!["stdio".to_string()];
    args.extend(mode.map(str::to_string));
    ExtensionConfig::Stdio {
        name: name.to_string(),
        description: "stdio fixture".to_string(),
        cmd: FIXTURE_BINARY_PATH.to_string_lossy().to_string(),
        args,
        envs: Envs::default(),
        env_keys: vec![],
        timeout: Some(30),
        cwd: None,
        bundled: Some(false),
        available_tools: available_tools.iter().map(|s| s.to_string()).collect(),
    }
}

fn http_fixture(name: &str, uri: &str) -> ExtensionConfig {
    ExtensionConfig::StreamableHttp {
        name: name.to_string(),
        description: "HTTP fixture".to_string(),
        uri: uri.to_string(),
        envs: Envs::default(),
        env_keys: vec![],
        headers: HashMap::new(),
        timeout: Some(30),
        socket: None,
        client_id: None,
        client_secret_key: None,
        scopes: vec![],
        bundled: Some(false),
        available_tools: vec![],
    }
}

fn platform(name: &str) -> ExtensionConfig {
    ExtensionConfig::Platform {
        name: name.to_string(),
        description: name.to_string(),
        display_name: Some(name.to_string()),
        bundled: Some(true),
        available_tools: vec![],
    }
}

fn tool_names(tools: &[Tool]) -> Vec<String> {
    tools.iter().map(|tool| tool.name.to_string()).collect()
}

fn request(name: &str, arguments: Option<rmcp::model::JsonObject>) -> CallToolRequestParams {
    let mut request = CallToolRequestParams::new(name.to_string());
    if let Some(arguments) = arguments {
        request = request.with_arguments(arguments);
    }
    request
}

async fn call(
    lease: &ExtensionLease,
    name: &str,
    arguments: Option<rmcp::model::JsonObject>,
) -> Result<CallToolResult, ErrorData> {
    lease
        .call(
            request(name, arguments),
            CallRequest::new(format!("call-{name}")),
            CancellationToken::default(),
        )
        .await?
        .result
        .await
}

fn text_of(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content| content.as_text())
        .map(|content| content.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

async fn call_text(lease: &ExtensionLease, name: &str) -> String {
    text_of(&call(lease, name, None).await.unwrap())
}

async fn inspect_context(lease: &ExtensionLease, name: &str) -> ContextReport {
    serde_json::from_str(&call_text(lease, name).await).unwrap()
}

/// One manager, three transports, one lease: the catalog, mangled-name
/// recovery, filtering, app scoping, the restart-on-config-change rule, and
/// what a held lease does and does not see when the registry changes.
///
/// The client is pinned to the legacy `initialize` handshake, so this is
/// also where legacy HTTP is stateful — one server instance per session, a
/// GET stream for server-initiated traffic — and where the stdio server
/// speaks each version the handshake still has to negotiate down to.
#[test_case(ProtocolVersion::V_2025_11_25; "stdio_2025_11_25")]
#[test_case(ProtocolVersion::V_2025_06_18; "stdio_2025_06_18")]
#[test_case(ProtocolVersion::V_2024_11_05; "stdio_2024_11_05")]
#[tokio::test]
async fn extension_lifecycle_across_real_transports(stdio_version: ProtocolVersion) {
    let http_server = McpFixture::with_max_protocol_version(ProtocolVersion::V_2025_11_25).await;
    let fx = fixture(false, Some(ProtocolVersion::V_2025_11_25)).await;
    let session = fx.session(SessionType::Hidden).await;

    let http = http_fixture("fixture_http", &http_server.url);
    let stdio = stdio_fixture(
        "fixture_stdio",
        Some(stdio_version.as_str()),
        &["get_code", "inspect_context"],
    );
    let todo = platform("todo");
    for config in [&http, &stdio, &todo] {
        fx.add(&session, config).await;
    }
    let configs = vec![http.clone(), stdio.clone(), todo.clone()];
    let lease = fx.resolve(&session, &configs).await;

    // The catalog: prefixed per extension, filtered by available_tools.
    let names = tool_names(&lease.tools().await);
    for expected in [
        "fixture_http__get_code",
        "fixture_http__db.query",
        "fixture_stdio__get_code",
        "fixture_stdio__inspect_context",
        "todo__todo_write",
    ] {
        assert!(names.contains(&expected.to_string()), "{names:?}");
    }
    assert!(!names.contains(&"fixture_stdio__get_image".to_string()));
    let mut stdio_names = tool_names(&lease.tools_for("fixture_stdio").await);
    stdio_names.sort();
    assert_eq!(
        stdio_names,
        ["fixture_stdio__get_code", "fixture_stdio__inspect_context"]
    );
    assert!(tool_names(&lease.tools_excluding("fixture_stdio").await)
        .iter()
        .all(|name| !name.starts_with("fixture_stdio__")));

    // Calls reach each transport.
    assert_eq!(call_text(&lease, "fixture_http__get_code").await, FAKE_CODE);
    assert_eq!(
        call_text(&lease, "fixture_stdio__get_code").await,
        FAKE_CODE
    );
    let http_context = inspect_context(&lease, "fixture_http__inspect_context").await;
    let stdio_context = inspect_context(&lease, "fixture_stdio__inspect_context").await;
    let working_dir = session.working_dir.to_string_lossy().into_owned();
    assert_eq!(
        http_context.protocol_version,
        ProtocolVersion::V_2025_11_25.as_str()
    );
    assert_eq!(
        http_context.request_session_id.as_deref(),
        Some(session.id.as_str())
    );
    assert_eq!(
        http_context.request_working_dir.as_deref(),
        Some(working_dir.as_str())
    );
    let working_dir_root = url::Url::from_file_path(&session.working_dir)
        .unwrap()
        .to_string();
    assert_eq!(http_context.roots, std::slice::from_ref(&working_dir_root));
    assert_eq!(
        inspect_context(&lease, "fixture_http__inspect_context")
            .await
            .instance_id,
        http_context.instance_id
    );
    // The client proposes 2025-11-25; the handshake lands on whatever the
    // server can do.
    assert_eq!(stdio_context.protocol_version, stdio_version.as_str());
    assert_eq!(
        fs::canonicalize(&stdio_context.process_cwd).unwrap(),
        fs::canonicalize(&session.working_dir).unwrap()
    );
    assert_eq!(
        stdio_context.process_session_id.as_deref(),
        Some(session.id.as_str())
    );
    assert_eq!(
        stdio_context.request_session_id.as_deref(),
        Some(session.id.as_str())
    );
    assert_eq!(
        stdio_context.request_working_dir.as_deref(),
        Some(working_dir.as_str())
    );
    assert_eq!(stdio_context.roots, [working_dir_root]);
    assert!(text_of(
        &call(
            &lease,
            "todo__todo_write",
            Some(object!({ "content": "- [ ] contract" }))
        )
        .await
        .unwrap()
    )
    .starts_with("Updated ("));

    // Spellings a model mangles resolve to the same tools; an exact dotted
    // name is never rewritten.
    assert_eq!(call_text(&lease, "fixture_stdio.get_code").await, FAKE_CODE);
    assert_eq!(
        call_text(&lease, "functions.fixture_stdio__get_code").await,
        FAKE_CODE
    );
    assert_eq!(call_text(&lease, "fixture_http__db.query").await, "rows");
    assert_eq!(call_text(&lease, "fixture_http.db.query").await, "rows");
    let unknown = call(&lease, "no_such_tool", None).await.unwrap_err();
    assert_eq!(unknown.code, ErrorCode::RESOURCE_NOT_FOUND);
    assert!(
        unknown.message.contains("no_such_tool")
            && unknown.message.contains("fixture_http__get_code")
    );

    // Filtered out by available_tools, and an app scoped to one extension
    // cannot reach another's tool.
    assert_eq!(
        call(&lease, "fixture_stdio__get_image", None)
            .await
            .unwrap_err()
            .code,
        ErrorCode::RESOURCE_NOT_FOUND
    );
    let Err(scoped) = lease
        .call_for_app(
            request("fixture_http__get_code", None),
            "fixture_stdio",
            CallRequest::default(),
            CancellationToken::default(),
        )
        .await
    else {
        panic!("an app scoped to fixture_stdio reached fixture_http's tool");
    };
    assert_eq!(scoped.code, ErrorCode::RESOURCE_NOT_FOUND);

    // Re-adding an identical config keeps the process; a changed one restarts
    // it. A lease resolved before the restart keeps the old process.
    let first_instance = stdio_context.instance_id;
    fx.add(&session, &stdio).await;
    assert_eq!(
        inspect_context(
            &fx.resolve(&session, &configs).await,
            "fixture_stdio__inspect_context",
        )
        .await
        .instance_id,
        first_instance
    );
    let wider_stdio = stdio_fixture(
        "fixture_stdio",
        Some(stdio_version.as_str()),
        &["get_code", "inspect_context", "db.query"],
    );
    fx.add(&session, &wider_stdio).await;
    let after_restart = fx
        .resolve(&session, &[http.clone(), wider_stdio.clone(), todo.clone()])
        .await;
    assert_ne!(
        inspect_context(&after_restart, "fixture_stdio__inspect_context")
            .await
            .instance_id,
        first_instance
    );
    assert_eq!(
        inspect_context(&lease, "fixture_stdio__inspect_context")
            .await
            .instance_id,
        first_instance
    );
    // The old config no longer matches what is running, so a set that still
    // names it gets nothing for that extension.
    assert!(
        tool_names(&fx.resolve(&session, &configs).await.tools().await)
            .iter()
            .all(|name| !name.starts_with("fixture_stdio__"))
    );

    // tools/list_changed arrives on the GET stream: the next resolve sees the
    // new tool while the held lease stays as it was.
    let late = "fixture_http__late_tool".to_string();
    let current = vec![http.clone(), wider_stdio.clone(), todo.clone()];
    assert!(!tool_names(&lease.tools().await).contains(&late));
    assert_eq!(
        call_text(&lease, "fixture_http__change_tools").await,
        "changed"
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !tool_names(&fx.resolve(&session, &current).await.tools().await).contains(&late) {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("tools/list_changed never invalidated the cache");
    assert!(!tool_names(&lease.tools().await).contains(&late));
    assert_eq!(
        call_text(&fx.resolve(&session, &current).await, &late).await,
        "late"
    );

    // Removal is invisible to a held lease and visible to a fresh one.
    fx.manager.remove_extension("todo").await.unwrap();
    assert!(tool_names(&lease.tools().await).contains(&"todo__todo_write".to_string()));
    assert!(!tool_names(
        &fx.resolve(&session, &[http.clone(), wider_stdio.clone(), todo.clone()])
            .await
            .tools()
            .await
    )
    .contains(&"todo__todo_write".to_string()));

    assert!(ExtensionSet::new("s", None, vec![todo.clone(), platform("Todo")]).is_err());
}

/// Everything that crosses the wire besides a plain call: per-session tool
/// lists, progress notifications, MCP-app hydration, forged result meta, and
/// elicitation routed back to the originating call with and without the
/// server echoing `_meta`.
#[tokio::test]
async fn extension_protocol_traffic_through_a_lease() {
    let http_server = McpFixture::new().await;
    let fx = fixture(true, None).await;
    let user = fx.session(SessionType::User).await;
    let subagent = fx.session(SessionType::SubAgent).await;
    let stdio = stdio_fixture("fixture", None, &[]);
    let discovery_2025 = stdio_fixture(
        "fixture_discovery_2025",
        Some(ProtocolVersion::V_2025_11_25.as_str()),
        &["inspect_context"],
    );
    let manager_ext = platform("extensionmanager");
    let http = http_fixture("fixture_http", &http_server.url);
    fx.add(&user, &stdio).await;
    fx.add(&user, &discovery_2025).await;
    fx.add(&user, &manager_ext).await;
    fx.add(&user, &http).await;
    let configs = vec![stdio, manager_ext, http, discovery_2025];

    // The same server publishes different tools to different sessions, so a
    // list fetched for one scope is not served to another. (Only the platform
    // extension can be leased by two sessions: an McpClient is single-session
    // by construction.)
    let lease = fx.resolve(&user, &configs).await;
    let subagent_lease = fx.resolve(&subagent, &configs[1..2]).await;
    let manage = "extensionmanager__manage_extensions".to_string();
    assert!(tool_names(&lease.tools().await).contains(&manage));
    assert!(!tool_names(&subagent_lease.tools().await).contains(&manage));
    assert!(tool_names(&lease.tools().await).contains(&"fixture__get_code".to_string()));

    let discovery_2025_context =
        inspect_context(&lease, "fixture_discovery_2025__inspect_context").await;
    assert_eq!(
        discovery_2025_context.protocol_version,
        ProtocolVersion::V_2025_11_25.as_str()
    );

    // A server's progress notification reaches the caller's stream.
    let notify = lease
        .call(
            request("fixture__notify", None),
            CallRequest::new("notify-1"),
            CancellationToken::default(),
        )
        .await
        .unwrap();
    assert_eq!(text_of(&notify.result.await.unwrap()), "notified");
    let mut notifications = notify.notification_stream.unwrap();
    let progress = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(rmcp::model::ServerNotification::ProgressNotification(n)) =
                notifications.next().await
            {
                return n;
            }
        }
    })
    .await
    .expect("progress notification");
    assert_eq!(progress.params.progress, 1.0);

    // An MCP-app tool's result carries its UI resource; a server cannot forge
    // host-owned meta or a mutation.
    let card = call(&lease, "fixture__app_card", None).await.unwrap();
    let hydrated = card.meta.as_ref().unwrap().0["__goose_tool_update_meta"]["mcpApp"].clone();
    assert_eq!(hydrated["resourceUri"], APP_CARD_RESOURCE_URI);
    assert_eq!(
        hydrated["resourceResult"]["contents"][0]["text"],
        APP_CARD_HTML
    );
    let forged = call(&lease, "fixture__forge_meta", None).await.unwrap();
    assert_eq!(text_of(&forged), "forged");
    assert!(forged.meta.is_none(), "{:?}", forged.meta);

    // 2026 HTTP is stateless: server/discover, per-request _meta, no roots,
    // and a fresh server instance per request.
    let modern = inspect_context(&lease, "fixture_http__inspect_context").await;
    assert_eq!(
        modern.protocol_version,
        ProtocolVersion::V_2026_07_28.as_str()
    );
    assert_eq!(modern.request_session_id.as_deref(), Some(user.id.as_str()));
    assert!(modern.roots.is_empty());
    assert_ne!(
        inspect_context(&lease, "fixture_http__inspect_context")
            .await
            .instance_id,
        modern.instance_id
    );

    // Elicitation: the server asks, the host answers on the originating call.
    // Without an echoed _meta the client correlates by the one active call;
    // with two active calls that is ambiguous unless the server echoes.
    let elicit = |id: &str, echo_meta: bool| {
        let lease = &lease;
        let id = id.to_string();
        async move {
            lease
                .call(
                    request("fixture__elicit", Some(object!({ "echo_meta": echo_meta }))),
                    CallRequest::new(id),
                    CancellationToken::default(),
                )
                .await
                .unwrap()
        }
    };
    let answer = |call: goose::agents::tool_execution::ToolCallResult, name: &'static str| {
        let session_manager = fx.session_manager.clone();
        let session_id = user.id.clone();
        async move {
            let mut action_required = call.action_required_stream.unwrap();
            let result = tokio::spawn(call.result);
            let message =
                tokio::time::timeout(std::time::Duration::from_secs(5), action_required.next())
                    .await
                    .expect("elicitation request")
                    .unwrap();
            let elicitation_id = message
                .content
                .iter()
                .find_map(
                    |content| match content.as_action_required().map(|a| &a.data) {
                        Some(ActionRequiredData::Elicitation { id, .. }) => Some(id.clone()),
                        _ => None,
                    },
                )
                .expect("elicitation message");
            goose::elicitation::complete_elicitation_with_generated_message(
                &session_manager,
                &session_id,
                &elicitation_id,
                ElicitationOutcome::Accept(serde_json::json!({ "name": name })),
            )
            .await
            .unwrap();
            result.await.unwrap()
        }
    };

    let alone = elicit("e-alone", false).await;
    assert_eq!(text_of(&answer(alone, "Ada").await.unwrap()), "Ada");
    let first = elicit("e-first", false).await;
    let mut first_requests = first.action_required_stream.unwrap();
    let first_result = tokio::spawn(first.result);
    let first_message =
        tokio::time::timeout(std::time::Duration::from_secs(5), first_requests.next())
            .await
            .unwrap()
            .unwrap();

    let ambiguous = elicit("e-ambiguous", false).await.result.await.unwrap_err();
    assert!(
        ambiguous.message.contains("multiple tool calls are active"),
        "{ambiguous:?}"
    );

    let echoed = elicit("e-echoed", true).await;
    assert_eq!(text_of(&answer(echoed, "Grace").await.unwrap()), "Grace");

    let first_id = first_message
        .content
        .iter()
        .find_map(
            |content| match content.as_action_required().map(|a| &a.data) {
                Some(ActionRequiredData::Elicitation { id, .. }) => Some(id.clone()),
                _ => None,
            },
        )
        .unwrap();
    goose::elicitation::complete_elicitation_with_generated_message(
        &fx.session_manager,
        &user.id,
        &first_id,
        ElicitationOutcome::Accept(serde_json::json!({ "name": "Linus" })),
    )
    .await
    .unwrap();
    assert_eq!(text_of(&first_result.await.unwrap().unwrap()), "Linus");
}

enum TestMode {
    Record,
    Playback,
}

#[test_case(
    vec!["npx", "-y", "@modelcontextprotocol/server-everything@2026.1.14"],
    vec![
        CallToolRequestParams::new("echo").with_arguments(object!({"message": "Hello, world!" })),
        CallToolRequestParams::new("get-sum").with_arguments(object!({"a": 1, "b": 2 })),
        CallToolRequestParams::new("trigger-long-running-operation").with_arguments(object!({"duration": 1, "steps": 5 })),
        CallToolRequestParams::new("get-structured-content").with_arguments(object!({"location": "New York"})),
        CallToolRequestParams::new("trigger-sampling-request").with_arguments(object!({"prompt": "Please provide a quote from The Great Gatsby", "maxTokens": 100 }))
    ],
    vec![]
)]
#[test_case(
    vec!["github-mcp-server", "stdio"],
    vec![
        CallToolRequestParams::new("get_file_contents").with_arguments(object!({
            "owner": "block",
            "repo": "goose",
            "path": "README.md",
            "sha": "ab62b863c1666232a67048b6c4e10007a2a5b83c"
        })),
    ],
    vec!["GITHUB_PERSONAL_ACCESS_TOKEN"]
)]
#[test_case(
    vec!["uv", "run", "--with", "fastmcp==2.14.4", "fastmcp", "run", "tests/fastmcp_test_server.py"],
    vec![
        CallToolRequestParams::new("divide").with_arguments(object!({
            "dividend": 10,
            "divisor": 2
        }))
    ],
    vec![]
)]
#[tokio::test]
async fn test_replayed_session(
    command: Vec<&str>,
    tool_calls: Vec<CallToolRequestParams>,
    required_envs: Vec<&str>,
) {
    // The working directory is sent to the server verbatim in our `roots/list`
    // response, so it is part of the recorded protocol traffic. It must be an
    // absolute path (relative paths are not convertible to a `file://` URL) and
    // it must be stable across machines, otherwise playback compares a recorded
    // path against whatever cwd the test happens to run in.
    const TEST_WORKING_DIR: &str = "/tmp/goose_test";
    fs::create_dir_all(TEST_WORKING_DIR).ok();

    let _env = env_lock::lock_env([
        ("GOOSE_MCP_CLIENT_VERSION", Some("0.0.0")),
        ("GOOSE_PROVIDER", Some("openai")),
        ("GOOSE_MODEL", Some("gpt-4o")),
        ("GOOSE_WORKING_DIR", Some(TEST_WORKING_DIR)),
    ]);

    // Setup test file for developer extension tests
    let test_file_path = "/tmp/goose_test/goose.txt";
    fs::write(test_file_path, "# goose\n").ok();
    let replay_file_name = command
        .iter()
        .map(|s| s.replace("/", "_"))
        .collect::<Vec<String>>()
        .join("");
    let mut replay_file_path =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("should find the project root"));
    replay_file_path.push("tests");
    replay_file_path.push("mcp_replays");
    replay_file_path.push(&replay_file_name);

    let mode = if env::var("GOOSE_RECORD_MCP").is_ok() {
        TestMode::Record
    } else {
        assert!(replay_file_path.exists(), "replay file doesn't exist");
        TestMode::Playback
    };

    let mode_arg = match mode {
        TestMode::Record => "record",
        TestMode::Playback => "playback",
    };
    let cmd = REPLAY_BINARY_PATH.to_string_lossy().to_string();
    let mut args = vec!["stdio", mode_arg]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<String>>();

    args.push(replay_file_path.to_string_lossy().to_string());

    let mut env = HashMap::new();

    if matches!(mode, TestMode::Record) {
        args.extend(command.into_iter().map(|arg| {
            if arg == "tests/fastmcp_test_server.py" {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join(arg)
                    .to_string_lossy()
                    .into_owned()
            } else {
                arg.to_string()
            }
        }));

        for key in required_envs {
            match env::var(key) {
                Ok(v) => {
                    env.insert(key.to_string(), v);
                }
                Err(_) => {
                    eprintln!("skipping due to missing required env variable: {}", key);
                    return;
                }
            }
        }
    }

    let envs = Envs::new(env);
    let extension_config = ExtensionConfig::Stdio {
        name: "test".to_string(),
        description: "Test".to_string(),
        cmd,
        args,
        envs,
        env_keys: vec![],
        timeout: Some(30),
        cwd: None,
        bundled: Some(false),
        available_tools: vec![],
    };

    let provider = Arc::new(tokio::sync::Mutex::new(Some(
        Arc::new(MockProvider::new()) as Arc<dyn Provider>
    )));
    let temp_dir = tempfile::tempdir().unwrap();
    let session_manager = Arc::new(goose::session::SessionManager::new(
        temp_dir.path().to_path_buf(),
    ));
    let extension_manager = Arc::new(ExtensionManager::new(
        provider,
        session_manager,
        None,
        GoosePlatform::GooseDesktop.to_string(),
        ExtensionManagerCapabilities {
            mcpui: true,
            host_info: None,
            elicitation_handler: None,
            protocol_version: None,
        },
        true,
    ));

    #[allow(clippy::redundant_closure_call)]
    let result = (async || -> Result<(), Box<dyn std::error::Error>> {
        extension_manager
            .add_extension(extension_config, None, None, None)
            .await?;
        let mut results = Vec::new();
        for tool_call in tool_calls {
            let mut new_call = CallToolRequestParams::new(format!("test__{}", tool_call.name));
            if let Some(args) = tool_call.arguments {
                new_call = new_call.with_arguments(args);
            }
            let tool_call = new_call;
            // One dispatch per call, as the recordings were made: a server that
            // sends tools/list_changed between calls gets re-listed. A held
            // lease would freeze the catalog and diverge from the transcript.
            let ctx = goose::agents::ToolCallContext::new(
                "test-session-id".to_string(),
                None,
                Some("test-id".to_string()),
            );
            let result = extension_manager
                .dispatch_tool_call(&ctx, tool_call, CancellationToken::default())
                .await;

            let tool_result = result?;
            results.push(tool_result.result.await?);
        }

        let mut results_path = replay_file_path.clone();
        results_path.pop();
        results_path.push(format!("{}.results.json", replay_file_name));

        match mode {
            TestMode::Record => {
                serde_json::to_writer_pretty(File::create(results_path)?, &results)?
            }
            TestMode::Playback => assert_eq!(
                serde_json::from_reader::<_, Vec<CallToolResult>>(File::open(results_path)?)?,
                results
            ),
        };

        Ok(())
    })()
    .await;

    if let Err(err) = result {
        if matches!(mode, TestMode::Playback) {
            let errors =
                fs::read_to_string(format!("{}.errors.txt", replay_file_path.to_string_lossy()))
                    .expect("could not read errors");
            eprintln!("errors from {}", replay_file_path.to_string_lossy());
            eprintln!("{}", errors);
            eprintln!();
        }
        panic!("Test failed: {:?}", err);
    }
}
