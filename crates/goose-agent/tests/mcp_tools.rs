#![cfg(not(target_arch = "wasm32"))]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use goose_agent::mcp::{provider_safe_tool_name, McpTool, McpToolCatalog, McpToolClient, McpTools};
use goose_agent::operation::{ConversationEffect, Emitter, Operation};
use goose_agent::tool::{ToolOperation, ToolProvider};
use goose_provider_types::conversation::{message::Message, Conversation};
use rmcp::model::{CallToolRequestParams, CallToolResult, ErrorData, ListToolsResult, Tool};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

fn tool(name: &str) -> Tool {
    Tool::new(
        name.to_string(),
        "test tool",
        Arc::new(json!({"type":"object"}).as_object().unwrap().clone()),
    )
}

#[derive(Default)]
struct Client {
    calls: Mutex<Vec<(String, String, CallToolRequestParams)>>,
    discoveries: Mutex<Vec<Option<String>>>,
    name: Mutex<String>,
}

#[async_trait]
impl McpToolClient<String> for Client {
    async fn list_tools(
        &self,
        _session: &String,
        cursor: Option<String>,
        _cancel: CancellationToken,
    ) -> Result<ListToolsResult, ErrorData> {
        self.discoveries.lock().unwrap().push(cursor.clone());
        let mut page = ListToolsResult::default();
        if cursor.is_none() {
            page.tools = vec![tool(&self.name.lock().unwrap())];
            page.next_cursor = Some("next".into());
        } else {
            page.tools = vec![tool("second")];
        }
        Ok(page)
    }

    async fn call_tool(
        &self,
        session: &String,
        request_id: &str,
        call: CallToolRequestParams,
        cancel: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        if cancel.is_cancelled() {
            return Err(ErrorData::internal_error("cancelled", None));
        }
        self.calls
            .lock()
            .unwrap()
            .push((session.clone(), request_id.into(), call));
        let mut result = CallToolResult::structured(json!({"answer": 42}));
        result.meta = Some(rmcp::model::MetaObject(
            json!({"test": true}).as_object().unwrap().clone(),
        ));
        Ok(result)
    }
}

fn client() -> Arc<Client> {
    Arc::new(Client {
        name: Mutex::new("db.query".into()),
        ..Default::default()
    })
}

async fn discover(client: Arc<Client>) -> McpTools<String> {
    McpTools::discover(
        &"session".into(),
        vec![("db".into(), client as Arc<dyn McpToolClient<String>>)],
        CancellationToken::new(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn discovery_paginates_and_dispatch_preserves_requests_and_results() {
    let client = client();
    let tools = discover(client.clone()).await;
    assert_eq!(
        *client.discoveries.lock().unwrap(),
        vec![None, Some("next".into())]
    );
    assert_eq!(tools.catalog().entries().len(), 2);
    let name = &tools.catalog().entries()[0].tool.name;
    let mut call = CallToolRequestParams::new(name.to_string())
        .with_arguments(json!({"query": "select 1"}).as_object().unwrap().clone());
    call.meta
        .get_or_insert_default()
        .0
        .insert("trace".into(), json!("abc"));
    let result = tools
        .call(
            &"session".into(),
            "request",
            call.clone(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result.structured_content, Some(json!({"answer":42})));
    assert_eq!(result.meta.unwrap().0.get("test"), Some(&json!(true)));
    let calls = client.calls.lock().unwrap();
    assert_eq!(calls[0].0, "session");
    assert_eq!(calls[0].1, "request");
    assert_eq!(calls[0].2.name, "db.query");
    assert_eq!(calls[0].2.arguments, call.arguments);
    assert_eq!(calls[0].2.meta, call.meta);
}

#[tokio::test]
async fn snapshots_remain_stable_after_refresh_and_keep_the_client_alive() {
    let client = client();
    let old = discover(client.clone()).await;
    *client.name.lock().unwrap() = "new".into();
    let new = discover(client.clone()).await;
    assert_eq!(old.catalog().entries()[0].server_name, "db.query");
    assert_eq!(new.catalog().entries()[0].server_name, "new");
    let weak = Arc::downgrade(&client);
    drop(client);
    drop(new);
    assert!(weak.upgrade().is_some());
    let call = CallToolRequestParams::new(old.catalog().entries()[0].tool.name.to_string());
    old.call(&"session".into(), "old", call, CancellationToken::new())
        .await
        .unwrap();
    drop(old);
    assert!(weak.upgrade().is_none());
}

#[tokio::test]
async fn duplicate_routes_are_rejected_and_unknown_tools_are_not_dispatched() {
    let entry = || McpTool {
        tool: tool("duplicate"),
        server_name: "original".into(),
        target: (),
    };
    assert!(McpToolCatalog::new(vec![entry(), entry()]).is_err());
    let client = client();
    let tools = discover(client.clone()).await;
    assert!(tools
        .call(
            &"session".into(),
            "id",
            CallToolRequestParams::new("missing"),
            CancellationToken::new()
        )
        .await
        .is_err());
    assert!(client.calls.lock().unwrap().is_empty());
    let duplicates = vec![
        (
            "db".into(),
            client.clone() as Arc<dyn McpToolClient<String>>,
        ),
        ("db".into(), client as Arc<dyn McpToolClient<String>>),
    ];
    assert!(
        McpTools::discover(&"session".into(), duplicates, CancellationToken::new())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn cancellation_reaches_client() {
    let client = client();
    let tools = discover(client.clone()).await;
    let cancel = CancellationToken::new();
    cancel.cancel();
    let call = CallToolRequestParams::new(tools.catalog().entries()[0].tool.name.to_string());
    assert!(tools
        .call(&"session".into(), "cancel", call, cancel)
        .await
        .is_err());
    assert!(client.calls.lock().unwrap().is_empty());
}

#[test]
fn provider_names_preserve_safe_names_and_distinguish_lossy_rewrites() {
    assert_eq!(provider_safe_tool_name("safe__name-1"), "safe__name-1");
    assert_ne!(
        provider_safe_tool_name("db.query"),
        provider_safe_tool_name("db_query")
    );
    assert_ne!(
        provider_safe_tool_name(&"a".repeat(65)),
        provider_safe_tool_name(&"a".repeat(66))
    );
    for input in ["", "日本語", &"a".repeat(100)] {
        let name = provider_safe_tool_name(input);
        assert!(!name.is_empty() && name.len() <= 64);
        assert!(name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
    }
}

#[tokio::test]
async fn tool_operation_advertises_and_executes_the_snapshot() {
    let client = client();
    let provider = Arc::new(discover(client.clone()).await);
    let session = "session".to_string();
    let advertised = provider.tools(&session).await.unwrap();
    let operation = ToolOperation::new().with_provider(provider);
    let definitions =
        <ToolOperation<String> as Operation<String, ConversationEffect>>::inference_tools(
            &operation, &session,
        )
        .await
        .unwrap();
    assert_eq!(definitions, advertised);
    let conversation = Conversation::new_unvalidated(vec![
        Message::user().with_text("query"),
        Message::assistant().with_tool_request(
            "call",
            Ok(CallToolRequestParams::new(advertised[0].name.to_string())),
        ),
    ]);
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    <ToolOperation<String> as Operation<String, ConversationEffect>>::run(
        &operation,
        &session,
        &conversation,
        &emit,
    )
    .await
    .unwrap();
    assert_eq!(client.calls.lock().unwrap().len(), 1);
}

struct Server;

impl rmcp::ServerHandler for Server {
    fn get_info(&self) -> rmcp::model::ServerConfig {
        rmcp::model::InitializeResult::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
    }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(vec![tool("echo")]))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, ErrorData> {
        Ok(CallToolResult::structured(
            json!({"name": request.name, "arguments": request.arguments}),
        )
        .into())
    }
}

#[tokio::test]
async fn connected_rmcp_peer_can_be_attached_without_application_services() {
    use rmcp::ServiceExt;
    let (server_io, client_io) = tokio::io::duplex(65536);
    let server = tokio::spawn(async move { Server.serve(server_io).await.unwrap() });
    let client = ().serve(client_io).await.unwrap();
    let server = server.await.unwrap();
    let tools = McpTools::discover(
        &(),
        vec![(
            "remote".to_string(),
            Arc::new(client.peer().clone()) as Arc<dyn McpToolClient<()>>,
        )],
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let call = CallToolRequestParams::new("remote__echo")
        .with_arguments(json!({"value": 7}).as_object().unwrap().clone());
    let result = tools
        .call(&(), "id", call, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        result.structured_content,
        Some(json!({"name":"echo", "arguments":{"value":7}}))
    );
    client.cancel().await.unwrap();
    server.cancel().await.unwrap();
}

struct RepeatingCursor;

#[async_trait]
impl McpToolClient<()> for RepeatingCursor {
    async fn list_tools(
        &self,
        _session: &(),
        _cursor: Option<String>,
        _cancel: CancellationToken,
    ) -> Result<ListToolsResult, ErrorData> {
        let mut result = ListToolsResult::with_all_items(vec![]);
        result.next_cursor = Some("same".into());
        Ok(result)
    }

    async fn call_tool(
        &self,
        _session: &(),
        _request_id: &str,
        _call: CallToolRequestParams,
        _cancel: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        unreachable!()
    }
}

#[tokio::test]
async fn repeated_discovery_cursor_returns_an_error_instead_of_looping() {
    let clients = vec![(
        "repeating".into(),
        Arc::new(RepeatingCursor) as Arc<dyn McpToolClient<()>>,
    )];
    assert!(McpTools::discover(&(), clients, CancellationToken::new())
        .await
        .err()
        .unwrap()
        .to_string()
        .contains("repeated"));
}
