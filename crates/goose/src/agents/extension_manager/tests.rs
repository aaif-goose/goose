use super::*;
use rmcp::model::CallToolResult;
use rmcp::model::{CustomNotification, InitializeResult, JsonObject};
use rmcp::{object, ServiceError as Error};

use rmcp::model::ListPromptsResult;
use rmcp::model::ListResourcesResult;
use rmcp::model::ListToolsResult;
use rmcp::model::ReadResourceResult;
use rmcp::model::ServerNotification;

use super::super::tool_execution::{ToolCallContext, ToolCallNotificationEmitter};
use futures::StreamExt;
use goose_test_support::mcp::{McpFixtureServer, FAKE_CODE};
use rmcp::model::{CallToolRequestParams, GetPromptResult, Resource};
use rmcp::ServiceExt;
use std::sync::atomic::AtomicUsize;
use tokio::sync::{mpsc, Semaphore};

impl ExtensionManager {
    pub(crate) async fn add_test_client(
        self: &Arc<Self>,
        session_id: &str,
        config: ExtensionConfig,
        client: McpClientBox,
        info: Option<ServerConfig>,
    ) {
        self.install_test_client(session_id, config.clone(), client, info)
            .await;
        self.select(session_id, config).await.unwrap();
    }

    pub(crate) async fn install_test_client(
        self: &Arc<Self>,
        scope_id: &str,
        config: ExtensionConfig,
        client: McpClientBox,
        info: Option<ServerConfig>,
    ) {
        let mut slot = self.new_slot(scope_id, &config, None, None, self.client(scope_id));
        slot.placement = None;
        slot.runtime = OnceCell::new_with(Some(Runtime::Running(Arc::new(Extension {
            key: config.key(),
            config: config.clone(),
            resolved_config: config.clone(),
            client,
            server_info: info,
            tools_version: Arc::new(AtomicU64::new(0)),
            tools: Mutex::new(None),
        }))));
        self.scopes
            .lock()
            .await
            .entry(scope_id.to_string())
            .or_default()
            .insert(config.key(), Arc::new(slot));
    }

    async fn add_mock_extension(self: &Arc<Self>, name: String, client: McpClientBox) {
        self.install_test_client("session", builtin_config(&name, vec![]), client, None)
            .await;
    }
}

struct MockClient {}

#[async_trait::async_trait]
impl McpClientTrait for MockClient {
    fn get_info(&self) -> Option<&InitializeResult> {
        None
    }

    async fn list_resources(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListResourcesResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn read_resource(
        &self,
        _session_id: &str,
        _uri: &str,
        _cancellation_token: CancellationToken,
    ) -> Result<ReadResourceResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        use serde_json::json;
        use std::sync::Arc;
        Ok(ListToolsResult {
            tools: vec![
                Tool::new(
                    "tool".to_string(),
                    "A basic tool".to_string(),
                    Arc::new(json!({}).as_object().unwrap().clone()),
                ),
                Tool::new(
                    "available_tool".to_string(),
                    "An available tool".to_string(),
                    Arc::new(json!({}).as_object().unwrap().clone()),
                ),
                Tool::new(
                    "hidden_tool".to_string(),
                    "hidden tool".to_string(),
                    Arc::new(json!({}).as_object().unwrap().clone()),
                ),
                {
                    let mut t = Tool::new(
                        "render_chart".to_string(),
                        "Render a chart".to_string(),
                        Arc::new(json!({}).as_object().unwrap().clone()),
                    );
                    t.meta = Some(MetaObject(
                        json!({ "ui": { "resourceUri": "ui://autovisualiser/chart" } })
                            .as_object()
                            .unwrap()
                            .clone(),
                    ));
                    t
                },
            ],
            next_cursor: None,
            meta: None,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        _ctx: &ToolCallContext,
        name: &str,
        _arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        match name {
            "tool" | "test__tool" | "available_tool" | "hidden_tool" | "render_chart"
            | "unadvertised_tool" => Ok(CallToolResult::success(vec![])),
            _ => Err(Error::TransportClosed),
        }
    }

    async fn list_prompts(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListPromptsResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn get_prompt(
        &self,
        _session_id: &str,
        _name: &str,
        _arguments: Value,
        _cancellation_token: CancellationToken,
    ) -> Result<GetPromptResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
        mpsc::channel(1).1
    }
}

struct ResourceClient {
    label: &'static str,
}

#[async_trait::async_trait]
impl McpClientTrait for ResourceClient {
    async fn list_resources(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListResourcesResult, Error> {
        Ok(ListResourcesResult {
            resources: vec![Resource::new(
                "resource://snapshot".to_string(),
                format!("{} resource", self.label),
            )],
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        _session_id: &str,
        uri: &str,
        _cancellation_token: CancellationToken,
    ) -> Result<ReadResourceResult, Error> {
        Ok(ReadResourceResult::new(vec![
            rmcp::model::ResourceContents::text(self.label, uri),
        ]))
    }

    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult::default())
    }

    async fn call_tool(
        &self,
        _ctx: &ToolCallContext,
        _name: &str,
        _arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        Err(Error::TransportClosed)
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        None
    }
}

struct ContextNotificationClient;

#[async_trait::async_trait]
impl McpClientTrait for ContextNotificationClient {
    fn get_info(&self) -> Option<&InitializeResult> {
        None
    }

    async fn list_tools(
        &self,
        session_id: &str,
        next_cursor: Option<String>,
        cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        MockClient {}
            .list_tools(session_id, next_cursor, cancellation_token)
            .await
    }

    async fn call_tool(
        &self,
        ctx: &ToolCallContext,
        _name: &str,
        _arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        if let Some(emitter) = ctx.notification_emitter() {
            let request_id = ctx
                .tool_call_request_id
                .as_deref()
                .expect("an emitter requires a request ID");
            emitter.emit_best_effort(ServerNotification::CustomNotification(
                CustomNotification::new(format!("scoped/{request_id}"), None),
            ));
        }
        Ok(CallToolResult::success(vec![]))
    }

    async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
        let (sender, receiver) = mpsc::channel(1);
        sender
            .try_send(ServerNotification::CustomNotification(
                CustomNotification::new("client/subscription", None),
            ))
            .expect("test notification should fit");
        receiver
    }
}

async fn dispatch_notification_methods(ctx: ToolCallContext) -> Vec<String> {
    let temp_dir = tempfile::tempdir().unwrap();
    let extension_manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    extension_manager
        .add_mock_extension(
            "notifications".to_string(),
            Arc::new(ContextNotificationClient),
        )
        .await;

    let tool_call =
        CallToolRequestParams::new("notifications__tool".to_string()).with_arguments(object!({}));
    let dispatched = extension_manager
        .scope_lease(&ctx.session_id)
        .await
        .call(
            tool_call,
            CallRequest::from(&ctx),
            CancellationToken::default(),
        )
        .await
        .expect("tool call should dispatch");

    assert!(dispatched.result.await.is_ok());

    let mut methods = dispatched
        .notification_stream
        .expect("notification stream should exist")
        .filter_map(|notification| async move {
            match notification {
                ServerNotification::CustomNotification(notification) => Some(notification.method),
                _ => None,
            }
        })
        .collect::<Vec<_>>()
        .await;
    methods.sort();
    methods
}

#[tokio::test]
async fn dispatch_merges_request_scoped_and_client_notifications() {
    let methods = dispatch_notification_methods(ToolCallContext::new(
        "session".to_string(),
        None,
        Some("request".to_string()),
    ))
    .await;

    assert_eq!(methods, vec!["client/subscription", "scoped/request"]);
}

#[tokio::test]
async fn dispatch_reuses_existing_notification_emitter() {
    let temp_dir = tempfile::tempdir().unwrap();
    let extension_manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    extension_manager
        .add_mock_extension(
            "notifications".to_string(),
            Arc::new(ContextNotificationClient),
        )
        .await;
    let (sender, mut receiver) = mpsc::channel(1);
    let ctx = ToolCallContext::new(
        "session".to_string(),
        None,
        Some("nested-request".to_string()),
    )
    .with_notification_emitter(ToolCallNotificationEmitter::new(sender));
    let tool_call =
        CallToolRequestParams::new("notifications__tool".to_string()).with_arguments(object!({}));

    let dispatched = extension_manager
        .scope_lease(&ctx.session_id)
        .await
        .call(
            tool_call,
            CallRequest::from(&ctx),
            CancellationToken::default(),
        )
        .await
        .expect("tool call should dispatch");
    assert!(dispatched.result.await.is_ok());

    let notification = receiver
        .try_recv()
        .expect("parent emitter should receive nested notification");
    let ServerNotification::CustomNotification(notification) = notification else {
        panic!("expected a custom notification");
    };
    assert_eq!(notification.method, "scoped/nested-request");

    let methods = dispatched
        .notification_stream
        .expect("client notification stream should exist")
        .filter_map(|notification| async move {
            match notification {
                ServerNotification::CustomNotification(notification) => Some(notification.method),
                _ => None,
            }
        })
        .collect::<Vec<_>>()
        .await;
    assert_eq!(methods, vec!["client/subscription"]);
}

#[tokio::test]
async fn dispatch_without_request_id_uses_only_client_notifications() {
    let methods =
        dispatch_notification_methods(ToolCallContext::new("session".to_string(), None, None))
            .await;

    assert_eq!(methods, vec!["client/subscription"]);
}

#[test]
fn test_tool_owner_binding_uses_metadata_not_flattened_name() {
    let tool = |name: &str, owner: &str| {
        let mut tool = Tool::new(
            name.to_string(),
            "test tool".to_string(),
            Arc::new(serde_json::Map::new()),
        );
        tool.meta = Some(MetaObject(
            serde_json::json!({ TOOL_EXTENSION_META_KEY: owner })
                .as_object()
                .unwrap()
                .clone(),
        ));
        tool
    };

    let own_tool = tool("ext_a__own", "ext_a");
    let sibling_tool = tool("ext_a__ext_b__secret", "ext_a__ext_b");

    assert!(is_tool_owned_by_extension(&own_tool, "ext_a"));
    assert!(!is_tool_owned_by_extension(&sibling_tool, "ext_a"));
}

struct NamedToolsClient(Vec<Tool>);

#[async_trait::async_trait]
impl McpClientTrait for NamedToolsClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult {
            tools: self.0.clone(),
            next_cursor: None,
            meta: None,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        _ctx: &ToolCallContext,
        _name: &str,
        _arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        Ok(CallToolResult::success(vec![]))
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        None
    }
}

struct SubagentMetaClient;

#[async_trait::async_trait]
impl McpClientTrait for SubagentMetaClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult {
            tools: vec![Tool::new(
                "delegate".to_string(),
                "test tool".to_string(),
                Arc::new(serde_json::Map::new()),
            )],
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        _ctx: &ToolCallContext,
        _name: &str,
        _arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let meta = serde_json::json!({
            "subagent_session_id": "other-session",
            "unrelated": "kept",
        });
        Ok(CallToolResult::success(vec![])
            .with_meta(Some(MetaObject(meta.as_object().unwrap().clone()))))
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        None
    }
}

async fn dispatch_subagent_meta_tool(extension: &str, tool: &str) -> Option<MetaObject> {
    let temp_dir = tempfile::tempdir().unwrap();
    let extension_manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    extension_manager
        .add_mock_extension(extension.to_string(), Arc::new(SubagentMetaClient))
        .await;

    let dispatched = extension_manager
        .scope_lease("session")
        .await
        .call(
            CallToolRequestParams::new(tool.to_string()).with_arguments(object!({})),
            CallRequest::from(&ToolCallContext::new("session".to_string(), None, None)),
            CancellationToken::default(),
        )
        .await
        .expect("tool call should dispatch");

    dispatched.result.await.unwrap().meta
}

#[tokio::test]
async fn dispatch_strips_subagent_session_id_from_external_extensions() {
    let meta = dispatch_subagent_meta_tool("external", "external__delegate").await;

    assert_eq!(
        meta.map(|meta| serde_json::Value::Object(meta.0)),
        Some(serde_json::json!({"unrelated": "kept"})),
    );
}

#[tokio::test]
async fn dispatch_keeps_subagent_session_id_from_platform_extensions() {
    let meta = dispatch_subagent_meta_tool("summon", "delegate").await;

    assert_eq!(
        meta.and_then(|meta| meta.0.get("subagent_session_id").cloned()),
        Some(serde_json::json!("other-session")),
    );
}

fn app_tool(name: &str) -> Tool {
    let mut tool = Tool::new(
        name.to_string(),
        "test tool".to_string(),
        Arc::new(serde_json::Map::new()),
    );
    tool.meta = Some(MetaObject(
        serde_json::json!({ "ui": { "resourceUri": "ui://test/app" } })
            .as_object()
            .unwrap()
            .clone(),
    ));
    tool
}

/// `ext_a` publishing `ext_b__secret` and `ext_a__ext_b` publishing `secret`
/// flatten to the same public name. Whoever the catalog keeps, an app
/// dispatch scoped to the other extension must be refused.
#[tokio::test]
async fn app_dispatch_rejects_colliding_flattened_name_from_sibling_owner() {
    let temp_dir = tempfile::tempdir().unwrap();
    let extension_manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    extension_manager
        .add_mock_extension(
            "ext_a__ext_b".to_string(),
            Arc::new(NamedToolsClient(vec![app_tool("secret")])),
        )
        .await;
    extension_manager
        .add_mock_extension(
            "ext_a".to_string(),
            Arc::new(NamedToolsClient(vec![app_tool("ext_b__secret")])),
        )
        .await;

    let lease = extension_manager.scope_lease("session").await;
    assert_eq!(
        lease.tools().await.len(),
        1,
        "colliding names collapse to one entry"
    );
    let owner = get_tool_owner(&lease.tools().await[0]).unwrap();
    let other = if owner == "ext_a" {
        "ext_a__ext_b"
    } else {
        "ext_a"
    };

    let ctx = ToolCallContext::new("session".to_string(), None, None);
    let result = extension_manager
        .scope_lease(&ctx.session_id)
        .await
        .call_for_app(
            CallToolRequestParams::new("ext_a__ext_b__secret".to_string()),
            other,
            CallRequest::from(&ctx),
            CancellationToken::default(),
        )
        .await;
    let Err(error) = result else {
        panic!("app dispatch accepted a sibling owner's colliding tool name");
    };
    assert_eq!(error.code, ErrorCode::RESOURCE_NOT_FOUND);
}

struct BlockingToolsClient {
    calls: AtomicUsize,
    first_fetch_started: Semaphore,
    release_first_fetch: Semaphore,
}

#[async_trait::async_trait]
impl McpClientTrait for BlockingToolsClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let name = if call == 0 { "old" } else { "new" };

        if call == 0 {
            self.first_fetch_started.add_permits(1);
            let _permit = self.release_first_fetch.acquire().await.unwrap();
        }

        Ok(ListToolsResult {
            tools: vec![Tool::new(
                name,
                format!("{name} tool list"),
                Arc::new(JsonObject::new()),
            )],
            next_cursor: None,
            meta: None,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        _ctx: &ToolCallContext,
        _name: &str,
        _arguments: Option<JsonObject>,
        _cancel_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        Ok(CallToolResult::success(vec![]))
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        None
    }
}

fn builtin_config(name: &str, available_tools: Vec<String>) -> ExtensionConfig {
    ExtensionConfig::Builtin {
        name: name.to_string(),
        display_name: Some(name.to_string()),
        description: "built-in".to_string(),
        timeout: None,
        bundled: None,
        available_tools,
    }
}

#[tokio::test]
async fn resolve_replaces_a_slot_whose_config_changed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let extension_manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    extension_manager
        .add_mock_extension("ext_a".to_string(), Arc::new(MockClient {}))
        .await;

    let same = ExtensionSet::new("session", None, vec![builtin_config("ext_a", vec![])]).unwrap();
    assert!(!extension_manager
        .resolve(&same)
        .await
        .tools()
        .await
        .is_empty());

    let narrower = ExtensionSet::new(
        "session",
        None,
        vec![builtin_config("ext_a", vec!["tool".to_string()])],
    )
    .unwrap();
    let lease = extension_manager.resolve(&narrower).await;
    assert!(lease.tools().await.is_empty());
    let results = lease.start().await;
    assert!(!results[0].success);
    assert!(results[0]
        .error
        .as_deref()
        .is_some_and(|error| error.contains("Unknown extension")));
}

fn serve_fixture(read: tokio::io::DuplexStream, write: tokio::io::DuplexStream) {
    tokio::spawn(async move {
        let running = McpFixtureServer::new().serve((read, write)).await.unwrap();
        let _ = running.waiting().await;
    });
}

static COUNTED_FIXTURE_STARTS: AtomicUsize = AtomicUsize::new(0);

fn serve_counted_fixture(read: tokio::io::DuplexStream, write: tokio::io::DuplexStream) {
    COUNTED_FIXTURE_STARTS.fetch_add(1, Ordering::SeqCst);
    serve_fixture(read, write);
}

static ENABLED_FIXTURE_STARTS: AtomicUsize = AtomicUsize::new(0);

fn serve_enabled_fixture(read: tokio::io::DuplexStream, write: tokio::io::DuplexStream) {
    ENABLED_FIXTURE_STARTS.fetch_add(1, Ordering::SeqCst);
    serve_fixture(read, write);
}

static GATED_FIXTURE_STARTS: AtomicUsize = AtomicUsize::new(0);
static GATED_FIXTURE_GATE: Semaphore = Semaphore::const_new(0);

fn serve_gated_fixture(read: tokio::io::DuplexStream, write: tokio::io::DuplexStream) {
    GATED_FIXTURE_STARTS.fetch_add(1, Ordering::SeqCst);
    tokio::spawn(async move {
        GATED_FIXTURE_GATE.acquire().await.unwrap().forget();
        let running = McpFixtureServer::new().serve((read, write)).await.unwrap();
        let _ = running.waiting().await;
    });
}

/// A real MCP server that runs in-process, so starting it reads no
/// secrets; an HTTP server would look up OAuth credentials in the keyring.
fn fixture_config(
    name: &'static str,
    spawn: crate::builtin_extension::SpawnServerFn,
) -> ExtensionConfig {
    crate::builtin_extension::register_builtin_extension(name, spawn);
    builtin_config(name, vec![])
}

fn missing_stdio(name: &str, temp_dir: &Path) -> ExtensionConfig {
    ExtensionConfig::Stdio {
        name: name.to_string(),
        description: String::new(),
        cmd: temp_dir.join("missing-binary").display().to_string(),
        args: vec![],
        envs: Default::default(),
        env_keys: vec![],
        timeout: Some(5),
        cwd: None,
        bundled: None,
        available_tools: vec![],
    }
}

async fn session_selecting(
    manager: &ExtensionManager,
    working_dir: &Path,
    extensions: Vec<ExtensionConfig>,
) -> Session {
    let session_manager = &manager.get_context().session_manager;
    let session = session_manager
        .create_session(
            working_dir.to_path_buf(),
            "selection".to_string(),
            crate::session::SessionType::Hidden,
            crate::config::GooseMode::default(),
        )
        .await
        .unwrap();
    session_manager
        .update(&session.id)
        .provider_name("openai")
        .apply()
        .await
        .unwrap();
    session_manager
        .update_enabled_extensions(&session.id, |selected| *selected = extensions)
        .await
        .unwrap();
    session_manager
        .get_session(&session.id, false)
        .await
        .unwrap()
}

#[tokio::test]
async fn concurrent_selection_changes_from_separate_managers_are_all_kept() {
    let temp_dir = tempfile::tempdir().unwrap();
    let first = ExtensionManager::with_data_dir(temp_dir.path().to_path_buf());
    let session = session_selecting(&first, temp_dir.path(), vec![]).await;
    let managers = (0..8)
        .map(|_| ExtensionManager::with_data_dir(temp_dir.path().to_path_buf()))
        .collect::<Vec<_>>();

    let results = futures::future::join_all(managers.iter().enumerate().map(|(i, manager)| {
        manager.select(&session.id, builtin_config(&format!("ext_{i}"), vec![]))
    }))
    .await;
    for result in results {
        result.unwrap();
    }

    let mut selected = first.list_extensions(&session.id).await.unwrap();
    selected.sort();
    assert_eq!(
        selected,
        (0..8).map(|i| format!("ext_{i}")).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn a_selection_with_colliding_keys_is_refused() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = ExtensionManager::with_data_dir(temp_dir.path().to_path_buf());
    let session =
        session_selecting(&manager, temp_dir.path(), vec![builtin_config("a", vec![])]).await;

    let result = manager
        .get_context()
        .session_manager
        .update_enabled_extensions(&session.id, |selected| {
            *selected = vec![builtin_config("a.b", vec![]), builtin_config("a/b", vec![])]
        })
        .await;

    assert!(result.unwrap_err().to_string().contains("appears twice"));
    assert_eq!(
        manager.list_extensions(&session.id).await.unwrap(),
        vec!["a"]
    );
}

#[tokio::test]
async fn first_lease_starts_the_selection_in_the_session_record() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    let session = session_selecting(
        &manager,
        temp_dir.path(),
        vec![fixture_config("fixture", serve_fixture)],
    )
    .await;

    let tools = manager
        .current_lease(&session.id)
        .await
        .unwrap()
        .tools()
        .await;

    assert!(tools.iter().any(|tool| tool.name == "fixture__app_card"));
}

#[tokio::test]
async fn concurrent_first_leases_start_an_extension_once() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    let session = session_selecting(
        &manager,
        temp_dir.path(),
        vec![fixture_config("counted_fixture", serve_counted_fixture)],
    )
    .await;

    let callers = 20;
    let barrier = Arc::new(tokio::sync::Barrier::new(callers));
    let starts = (0..callers).map(|_| {
        let manager = Arc::clone(&manager);
        let session_id = session.id.clone();
        let barrier = Arc::clone(&barrier);
        tokio::spawn(async move {
            barrier.wait().await;
            manager
                .current_lease(&session_id)
                .await
                .unwrap()
                .start()
                .await
        })
    });
    for results in futures::future::join_all(starts).await {
        assert!(results.unwrap()[0].success);
    }

    assert_eq!(COUNTED_FIXTURE_STARTS.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn concurrent_enables_start_an_extension_once() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    let session = session_selecting(&manager, temp_dir.path(), vec![]).await;
    let config = fixture_config("enabled_fixture", serve_enabled_fixture);

    let enables =
        futures::future::join_all((0..10).map(|_| manager.enable(&session.id, config.clone())))
            .await;

    assert!(enables.iter().all(Result::is_ok));
    assert_eq!(ENABLED_FIXTURE_STARTS.load(Ordering::SeqCst), 1);
    assert_eq!(
        manager.list_extensions(&session.id).await.unwrap(),
        vec!["enabled_fixture"]
    );
}

#[tokio::test]
async fn a_lease_resolved_while_enabling_keeps_the_started_process() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    let session = session_selecting(&manager, temp_dir.path(), vec![]).await;
    let config = fixture_config("gated_fixture", serve_gated_fixture);

    let enable = tokio::spawn({
        let manager = Arc::clone(&manager);
        let session_id = session.id.clone();
        async move { manager.enable(&session_id, config).await }
    });
    while GATED_FIXTURE_STARTS.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    let during = manager.current_lease(&session.id).await.unwrap();
    assert!(!during.is_enabled("gated_fixture"));
    GATED_FIXTURE_GATE.add_permits(2);
    enable.await.unwrap().unwrap();

    let after = manager.current_lease(&session.id).await.unwrap();
    assert!(after.start().await.iter().all(|result| result.success));
    assert_eq!(GATED_FIXTURE_STARTS.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn selection_changes_show_up_in_the_next_lease_only() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    let session = session_selecting(
        &manager,
        temp_dir.path(),
        vec![fixture_config("fixture", serve_fixture)],
    )
    .await;
    let before = manager.current_lease(&session.id).await.unwrap();

    assert!(manager.disable(&session.id, "fixture").await.unwrap());
    let after = manager.current_lease(&session.id).await.unwrap();

    assert!(after.tools().await.is_empty());
    assert!(before
        .tools()
        .await
        .iter()
        .any(|tool| tool.name == "fixture__app_card"));
}

#[tokio::test]
async fn a_provider_running_its_own_tool_loop_keeps_mcp_servers_out_of_the_lease() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    let todo = ExtensionConfig::Platform {
        name: "todo".to_string(),
        display_name: None,
        description: String::new(),
        bundled: None,
        available_tools: vec![],
    };
    let session = session_selecting(
        &manager,
        temp_dir.path(),
        vec![todo, missing_stdio("stdio", temp_dir.path())],
    )
    .await;
    manager
        .get_context()
        .session_manager
        .update(&session.id)
        .provider_name("claude-code")
        .apply()
        .await
        .unwrap();

    let lease = manager.current_lease(&session.id).await.unwrap();

    assert_eq!(
        lease
            .configs()
            .iter()
            .map(ExtensionConfig::key)
            .collect::<Vec<_>>(),
        vec!["todo"]
    );
    assert!(lease.start().await.iter().all(|result| result.success));
}

#[tokio::test]
async fn a_failing_extension_is_reported_and_left_out() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    let session = session_selecting(
        &manager,
        temp_dir.path(),
        vec![
            missing_stdio("broken", temp_dir.path()),
            fixture_config("fixture", serve_fixture),
        ],
    )
    .await;

    let lease = manager.current_lease(&session.id).await.unwrap();
    let results = lease.start().await;

    assert_eq!(
        results
            .iter()
            .map(|result| (result.name.as_str(), result.success))
            .collect::<Vec<_>>(),
        vec![("broken", false), ("fixture", true)]
    );
    assert!(lease
        .tools()
        .await
        .iter()
        .all(|tool| get_tool_owner(tool).as_deref() == Some("fixture")));
}

#[tokio::test]
async fn a_new_working_dir_restarts_only_extensions_that_run_in_it() {
    let temp_dir = tempfile::tempdir().unwrap();
    let new_working_dir = tempfile::tempdir().unwrap();
    let manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    let developer = ExtensionConfig::Platform {
        name: "developer".to_string(),
        display_name: None,
        description: "developer".to_string(),
        bundled: None,
        available_tools: vec![],
    };
    let session = session_selecting(
        &manager,
        temp_dir.path(),
        vec![developer, fixture_config("fixture", serve_fixture)],
    )
    .await;
    manager
        .current_lease(&session.id)
        .await
        .unwrap()
        .start()
        .await;
    let before = manager.scopes.lock().await[&session.id].clone();

    manager
        .get_context()
        .session_manager
        .update(&session.id)
        .working_dir(new_working_dir.path().to_path_buf())
        .apply()
        .await
        .unwrap();
    let lease = manager.current_lease(&session.id).await.unwrap();

    let after = manager.scopes.lock().await[&session.id].clone();
    assert!(Arc::ptr_eq(&before["developer"], &after["developer"]));
    assert!(!Arc::ptr_eq(&before["fixture"], &after["fixture"]));
    assert_eq!(lease.working_dir(), Some(new_working_dir.path()));
    assert!(lease.start().await.iter().all(|result| result.success));
}

#[tokio::test]
async fn client_changes_restart_only_extensions_that_negotiate_capabilities() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    let set = ExtensionSet::new(
        "client-snapshot",
        Some(temp_dir.path().to_path_buf()),
        vec![
            builtin_config("developer", vec![]),
            fixture_config("client_fixture", serve_fixture),
        ],
    )
    .unwrap();
    let slots = || async { manager.scopes.lock().await[set.scope_id()].clone() };
    let mut client = ClientContext::new(&GoosePlatform::GooseDesktop, None);
    manager.attach_client(set.scope_id(), client.clone());
    let held = manager.resolve(&set).await;
    assert!(held.start().await.iter().all(|result| result.success));
    let first = slots().await;

    manager.attach_client(set.scope_id(), client.clone());
    // Change capabilities after resolve captures the client, while it waits for slots.
    let scopes = manager.scopes.lock().await;
    let mut resolving = std::pin::pin!(manager.resolve(&set));
    assert!(futures::poll!(resolving.as_mut()).is_pending());
    client.capabilities.mcpui = false;
    manager.attach_client(set.scope_id(), client);
    drop(scopes);
    let reattached_lease = resolving.await;
    let reattached = slots().await;
    for key in ["developer", "client_fixture"] {
        assert!(
            Arc::ptr_eq(&first[key], &reattached[key]),
            "{key} was restarted"
        );
    }

    let current = manager.resolve(&set).await;
    let changed = slots().await;
    assert!(Arc::ptr_eq(&reattached["developer"], &changed["developer"]));
    assert!(!Arc::ptr_eq(
        &reattached["client_fixture"],
        &changed["client_fixture"]
    ));
    assert!(current.start().await.iter().all(|result| result.success));

    for (lease, supports_apps) in [(&held, true), (&reattached_lease, true), (&current, false)] {
        let result = lease
            .call(
                CallToolRequestParams::new("client_fixture__app_card"),
                CallRequest::default(),
                CancellationToken::default(),
            )
            .await
            .unwrap()
            .result
            .await
            .unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let hydrated = result
            .meta
            .as_ref()
            .is_some_and(|meta| meta.0.contains_key(TRUSTED_TOOL_UPDATE_META_KEY));
        assert_eq!(hydrated, supports_apps);
    }
}

#[tokio::test]
async fn a_held_lease_survives_session_release() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    let session = session_selecting(
        &manager,
        temp_dir.path(),
        vec![fixture_config("release_fixture", serve_fixture)],
    )
    .await;
    let held = manager.current_lease(&session.id).await.unwrap();
    assert!(held.start().await.iter().all(|result| result.success));
    let before = Arc::downgrade(&manager.scopes.lock().await[&session.id]["release_fixture"]);

    manager.release(&session.id).await;
    assert!(!manager.scopes.lock().await.contains_key(&session.id));

    let result = held
        .call(
            CallToolRequestParams::new("release_fixture__get_code"),
            CallRequest::default(),
            CancellationToken::default(),
        )
        .await
        .unwrap()
        .result
        .await
        .unwrap();
    assert!(!result.is_error.unwrap_or(false));
    assert!(result
        .content
        .iter()
        .any(|content| content.as_text().is_some_and(|text| text.text == FAKE_CODE)));

    let current = manager.current_lease(&session.id).await.unwrap();
    assert!(!Weak::ptr_eq(
        &before,
        &Arc::downgrade(&manager.scopes.lock().await[&session.id]["release_fixture"]),
    ));
    assert!(current.start().await.iter().all(|result| result.success));
    drop(held);
    assert!(before.upgrade().is_none());
}

#[test]
fn set_rejects_the_same_extension_twice() {
    let error = ExtensionSet::new(
        "s",
        None,
        vec![
            builtin_config("Ext-A", vec![]),
            builtin_config("ext-a", vec![]),
        ],
    )
    .unwrap_err();
    assert!(error.to_string().contains("appears twice"));
}

#[tokio::test]
async fn extension_manager_tools_follow_resource_support() {
    let temp_dir = tempfile::tempdir().unwrap();
    let extension_manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    let session = session_selecting(&extension_manager, temp_dir.path(), vec![]).await;
    extension_manager
        .enable(
            &session.id,
            ExtensionConfig::Platform {
                name: "extensionmanager".to_string(),
                display_name: None,
                description: String::new(),
                bundled: None,
                available_tools: vec![],
            },
        )
        .await
        .unwrap();

    let tools = extension_manager
        .current_lease(&session.id)
        .await
        .unwrap()
        .tools_for("extensionmanager")
        .await;
    assert!(tools
        .iter()
        .all(|tool| tool.name != "extensionmanager__list_resources"));

    let resource_info = InitializeResult::new(
        rmcp::model::ServerCapabilities::builder()
            .enable_resources()
            .build(),
    );
    extension_manager
        .add_test_client(
            &session.id,
            builtin_config("resources", vec![]),
            Arc::new(MockClient {}),
            Some(resource_info),
        )
        .await;

    let tools = extension_manager
        .current_lease(&session.id)
        .await
        .unwrap()
        .tools_for("extensionmanager")
        .await;
    assert!(tools
        .iter()
        .any(|tool| tool.name == "extensionmanager__list_resources"));

    extension_manager
        .disable(&session.id, "resources")
        .await
        .unwrap();
    let tools = extension_manager
        .current_lease(&session.id)
        .await
        .unwrap()
        .tools_for("extensionmanager")
        .await;
    assert!(tools
        .iter()
        .all(|tool| tool.name != "extensionmanager__list_resources"));
}

#[tokio::test]
async fn extension_manager_resource_tools_use_the_calling_lease() {
    let temp_dir = tempfile::tempdir().unwrap();
    let extension_manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    let session = session_selecting(&extension_manager, temp_dir.path(), vec![]).await;
    extension_manager
        .enable(
            &session.id,
            ExtensionConfig::Platform {
                name: "extensionmanager".to_string(),
                display_name: None,
                description: String::new(),
                bundled: None,
                available_tools: vec![],
            },
        )
        .await
        .unwrap();
    let resource_info = InitializeResult::new(
        rmcp::model::ServerCapabilities::builder()
            .enable_resources()
            .build(),
    );
    extension_manager
        .add_test_client(
            &session.id,
            builtin_config("resources", vec![]),
            Arc::new(ResourceClient { label: "old" }),
            Some(resource_info.clone()),
        )
        .await;
    let lease = extension_manager.current_lease(&session.id).await.unwrap();
    extension_manager
        .disable(&session.id, "resources")
        .await
        .unwrap();
    assert!(lease
        .tools()
        .await
        .iter()
        .any(|tool| tool.name == "extensionmanager__read_resource"));

    extension_manager
        .add_test_client(
            &session.id,
            builtin_config("resources", vec![]),
            Arc::new(ResourceClient { label: "new" }),
            Some(resource_info),
        )
        .await;

    let listed = lease
        .call(
            CallToolRequestParams::new("extensionmanager__list_resources")
                .with_arguments(object!({"extension_name": "resources"})),
            CallRequest::default(),
            CancellationToken::default(),
        )
        .await
        .unwrap()
        .result
        .await
        .unwrap();
    assert!(listed.content.iter().any(|content| content
        .as_text()
        .is_some_and(|text| text.text.contains("old resource"))));

    let read = lease
        .call(
            CallToolRequestParams::new("extensionmanager__read_resource").with_arguments(object!({
                "extension_name": "resources",
                "uri": "resource://snapshot"
            })),
            CallRequest::default(),
            CancellationToken::default(),
        )
        .await
        .unwrap()
        .result
        .await
        .unwrap();
    assert!(read.content.iter().any(|content| content
        .as_text()
        .is_some_and(|text| text.text.ends_with("\n\nold"))));
}

#[tokio::test]
async fn tool_list_changed_during_fetch_prevents_stale_cache() {
    let temp_dir = tempfile::tempdir().unwrap();
    let extension_manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    let tools_client = Arc::new(BlockingToolsClient {
        calls: AtomicUsize::new(0),
        first_fetch_started: Semaphore::new(0),
        release_first_fetch: Semaphore::new(0),
    });
    extension_manager
        .add_mock_extension("dynamic".to_string(), tools_client.clone())
        .await;
    let slot = extension_manager.scopes.lock().await["session"]["dynamic"].clone();
    let tools_version = slot.start().await.unwrap().tools_version.clone();

    let manager = extension_manager;
    let first_fetch = {
        let manager = manager.clone();
        tokio::spawn(async move { manager.scope_lease("session").await.tools().await })
    };

    let _started = tools_client.first_fetch_started.acquire().await.unwrap();
    tools_version.fetch_add(1, Ordering::SeqCst);
    tools_client.release_first_fetch.add_permits(1);

    let stale_result = first_fetch.await.unwrap();
    assert!(stale_result.iter().any(|tool| tool.name == "dynamic__old"));

    let refreshed = manager.scope_lease("session").await.tools().await;
    assert!(refreshed.iter().any(|tool| tool.name == "dynamic__new"));
    assert_eq!(tools_client.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn successful_mutation_is_persisted_when_another_mutation_fails() {
    let temp_dir = tempfile::tempdir().unwrap();
    let manager = Arc::new(ExtensionManager::with_data_dir(
        temp_dir.path().to_path_buf(),
    ));
    let session = manager
        .get_context()
        .session_manager
        .create_session(
            temp_dir.path().to_path_buf(),
            "mixed-mutations".to_string(),
            crate::session::SessionType::Hidden,
            crate::config::GooseMode::default(),
        )
        .await
        .unwrap();

    let successful = manager.apply(
        ExtensionMutation::Enable {
            name: "analyze".to_string(),
        },
        &session.id,
    );
    let failed = manager.apply(
        ExtensionMutation::Enable {
            name: "missing-extension".to_string(),
        },
        &session.id,
    );
    let (successful, failed) = tokio::join!(successful, failed);

    assert!(successful.is_ok());
    assert!(failed.is_err());
    let stored_session = manager
        .get_context()
        .session_manager
        .get_session(&session.id, false)
        .await
        .unwrap();
    let stored_extensions =
        EnabledExtensionsState::from_extension_data(&stored_session.extension_data).unwrap();
    assert!(stored_extensions
        .extensions
        .iter()
        .any(|config| config.key() == "analyze"));
}

#[test]
fn test_recover_mangled_tool_name() {
    let tools = [("developer__shell", None), ("platform__search", None)];
    assert_eq!(
        recover_mangled_tool_name("developer.shell", tools.iter().copied()).as_deref(),
        Some("developer__shell")
    );
    assert_eq!(
        recover_mangled_tool_name("functions.developer__shell", tools.iter().copied()).as_deref(),
        Some("developer__shell")
    );
    assert_eq!(
        recover_mangled_tool_name("functions.developer.shell", tools.iter().copied()).as_deref(),
        Some("developer__shell")
    );
    assert_eq!(
        recover_mangled_tool_name("developer shell", tools.iter().copied()),
        None
    );
    assert_eq!(
        recover_mangled_tool_name("developer__shell!", tools.iter().copied()),
        None
    );
    assert_eq!(
        recover_mangled_tool_name("nonexistent.tool", tools.iter().copied()),
        None
    );

    let dotted_tool = [("dotted__db.query", None)];
    assert_eq!(
        recover_mangled_tool_name("dotted.db.query", dotted_tool.iter().copied()).as_deref(),
        Some("dotted__db.query")
    );
}

#[test]
fn test_recover_mangled_tool_name_unprefixed_extension() {
    // Platform extensions with unprefixed_tools=true (e.g. "developer")
    // advertise tools with no "__" prefix at all; the owner lives only in
    // metadata. GLM's documented "developer.shell" reproduction (#9486)
    // and emulated "developer__shell" calls must recover via the owner,
    // not the tool's own (absent) prefix.
    let tools = [("shell", Some("developer")), ("write", Some("developer"))];
    assert_eq!(
        recover_mangled_tool_name("developer.shell", tools.iter().copied()).as_deref(),
        Some("shell")
    );
    assert_eq!(
        recover_mangled_tool_name("developer__shell", tools.iter().copied()).as_deref(),
        Some("shell")
    );
    assert_eq!(
        recover_mangled_tool_name("functions.developer.shell", tools.iter().copied()).as_deref(),
        Some("shell")
    );

    // Wrong owner must not match.
    assert_eq!(
        recover_mangled_tool_name("other_extension.shell", tools.iter().copied()),
        None
    );

    // Ambiguity across two different unprefixed extensions that both own
    // a tool matching the same mangled input must refuse, not guess.
    let ambiguous = [("shell", Some("dev_a")), ("shell", Some("dev_b"))];
    assert_eq!(
        recover_mangled_tool_name("dev_a.shell", ambiguous.iter().copied()).as_deref(),
        Some("shell")
    );
}

#[test]
fn test_recover_mangled_tool_name_non_extension_manager_tools() {
    // recipe__final_output and platform__manage_schedule are appended by
    // Agent::list_tools outside the extension manager (see #9486); they
    // use the same "__" convention, so no owner metadata is needed.
    let tools = [
        ("recipe__final_output", None),
        ("platform__manage_schedule", None),
    ];
    assert_eq!(
        recover_mangled_tool_name("recipe.final_output", tools.iter().copied()).as_deref(),
        Some("recipe__final_output")
    );
    assert_eq!(
        recover_mangled_tool_name("platform.manage_schedule", tools.iter().copied()).as_deref(),
        Some("platform__manage_schedule")
    );
}
