use anyhow::Result;
use async_trait::async_trait;
use goose_agent::{
    mcp::{
        Catalog, CatalogEntry, Client, CollisionPolicy, Connector, Manager, McpOperation,
        Registration, BINDING_META_KEY,
    },
    operation::{ConversationEffect, Emitter, Operation, OperationResult},
};
use goose_provider_types::conversation::{message::Message, Conversation};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ErrorData, ListToolsResult, Tool,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio_util::sync::CancellationToken;

struct Fixture {
    starts: AtomicUsize,
    label: &'static str,
}
#[async_trait]
impl Client for Fixture {
    fn get_info(&self) -> Option<&rmcp::model::InitializeResult> {
        None
    }
    async fn list_tools(
        &self,
        _scope: &str,
        cursor: Option<String>,
        _cancel: CancellationToken,
    ) -> Result<ListToolsResult, rmcp::ServiceError> {
        let mut page = ListToolsResult::default();
        page.tools.push(Tool::new(
            if cursor.is_none() { "first" } else { "second" },
            self.label,
            Arc::new(serde_json::Map::new()),
        ));
        if cursor.is_none() {
            page.next_cursor = Some("next".into());
        }
        Ok(page)
    }
    async fn call_tool(
        &self,
        _context: &goose_agent::mcp::CallContext,
        name: &str,
        _arguments: Option<rmcp::model::JsonObject>,
        _cancel: CancellationToken,
    ) -> Result<CallToolResult, rmcp::ServiceError> {
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "{}:{}",
            self.label, name
        ))]))
    }
}
struct Factory(Arc<Fixture>);
#[async_trait]
impl Connector for Factory {
    async fn connect(&self, _scope: &str, _cancel: CancellationToken) -> Result<Arc<dyn Client>> {
        self.0.starts.fetch_add(1, Ordering::SeqCst);
        Ok(self.0.clone())
    }
}
fn registration(fixture: Arc<Fixture>, revision: u64) -> Registration {
    Registration {
        id: "fixture".into(),
        revision,
        connector: Arc::new(Factory(fixture)),
        available_tools: vec![],
        unprefixed: false,
    }
}
fn fixture(label: &'static str) -> Arc<Fixture> {
    Arc::new(Fixture {
        starts: AtomicUsize::new(0),
        label,
    })
}

#[tokio::test]
async fn single_flight_pagination_and_held_selection() {
    let manager = Manager::new();
    let old = fixture("old");
    let held = manager
        .resolve("scope", vec![registration(old.clone(), 0)])
        .await
        .unwrap();
    let next = manager
        .resolve("scope", vec![registration(old.clone(), 0)])
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        held.catalog(CancellationToken::new()),
        next.catalog(CancellationToken::new())
    );
    assert_eq!(a.unwrap().entries.len(), 2);
    assert_eq!(b.unwrap().entries.len(), 2);
    assert_eq!(old.starts.load(Ordering::SeqCst), 1);
    let new = fixture("new");
    let replaced = manager
        .resolve("scope", vec![registration(new, 1)])
        .await
        .unwrap();
    manager.release("scope").await;
    assert_eq!(
        held.catalog(CancellationToken::new())
            .await
            .unwrap()
            .entries[0]
            .tool
            .description
            .as_deref(),
        Some("old")
    );
    assert_eq!(
        replaced
            .catalog(CancellationToken::new())
            .await
            .unwrap()
            .entries[0]
            .tool
            .description
            .as_deref(),
        Some("new")
    );
}

#[tokio::test]
async fn operation_preserves_original_binding_and_rejects_reconstruction() {
    let manager = Manager::new();
    let lease = manager
        .resolve("scope", vec![registration(fixture("old"), 0)])
        .await
        .unwrap();
    let id = lease.id().to_string();
    let operation = McpOperation::<()>::new(Arc::new(lease));
    let conversation = Conversation::new_unvalidated(vec![
        Message::user().with_text("go"),
        Message::assistant()
            .with_tool_request("call", Ok(CallToolRequestParams::new("fixture__first"))),
    ]);
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    let tools =
        <McpOperation<()> as Operation<(), ConversationEffect>>::inference_tools(&operation, &())
            .await
            .unwrap();
    assert_eq!(tools[0].meta.as_ref().unwrap().0[BINDING_META_KEY], id);
    let result = <McpOperation<()> as Operation<(), ConversationEffect>>::run(
        &operation,
        &(),
        &conversation,
        &emit,
    )
    .await
    .unwrap();
    let OperationResult::Applied(result) = result else {
        panic!("must respond")
    };
    let ConversationEffect::AppendMessage(message) = &result.effects[0] else {
        panic!("must append")
    };
    assert!(
        message.as_concat_text().contains("expired")
            || message.content.iter().any(|content| content
                .as_tool_response()
                .is_some_and(|response| response.tool_result.is_err()))
    );
}

#[test]
fn catalog_rejects_collisions_and_preserves_original_name() {
    let tool = Tool::new(
        "server__db.query",
        "query",
        Arc::new(serde_json::Map::new()),
    );
    let make = || CatalogEntry {
        tool: tool.clone(),
        owner: "server",
        server_name: "db.query".into(),
    };
    assert!(Catalog::new([make(), make()], CollisionPolicy::Reject).is_err());
    let catalog = Catalog::new([make()], CollisionPolicy::Reject).unwrap();
    assert!(catalog.entries[0].tool.name.len() <= 64);
    assert_eq!(catalog.entries[0].server_name, "db.query");
}

#[tokio::test]
async fn original_binding_executes_after_replacement() {
    let manager = Manager::new();
    let lease = manager
        .resolve("scope", vec![registration(fixture("old"), 0)])
        .await
        .unwrap();
    let id = lease.id().to_string();
    let operation = McpOperation::<()>::new(Arc::new(lease));
    manager
        .resolve("scope", vec![registration(fixture("new"), 1)])
        .await
        .unwrap();
    let conversation = Conversation::new_unvalidated(vec![
        Message::user().with_text("go"),
        Message::assistant().with_tool_request_with_metadata(
            "call",
            Ok(CallToolRequestParams::new("fixture__first")),
            None,
            Some(serde_json::json!({ BINDING_META_KEY: id })),
        ),
    ]);
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    let result = <McpOperation<()> as Operation<(), ConversationEffect>>::run(
        &operation,
        &(),
        &conversation,
        &emit,
    )
    .await
    .unwrap();
    let OperationResult::Applied(result) = result else {
        panic!("must execute")
    };
    let ConversationEffect::AppendMessage(message) = &result.effects[0] else {
        panic!("must append")
    };
    let response = message.content[0]
        .as_tool_response()
        .unwrap()
        .tool_result
        .as_ref()
        .unwrap();
    assert_eq!(response.content[0].as_text().unwrap().text, "old:first");
}

#[tokio::test]
async fn duplicate_registrations_do_not_replace_selection() {
    let manager = Manager::new();
    let fixture = fixture("old");
    let old = manager
        .resolve("scope", vec![registration(fixture.clone(), 0)])
        .await
        .unwrap();
    assert!(manager
        .resolve(
            "scope",
            vec![
                registration(fixture.clone(), 1),
                registration(fixture.clone(), 2)
            ]
        )
        .await
        .is_err());
    let current = manager
        .resolve("scope", vec![registration(fixture.clone(), 0)])
        .await
        .unwrap();
    tokio::join!(
        old.start(CancellationToken::new()),
        current.start(CancellationToken::new())
    );
    assert_eq!(fixture.starts.load(Ordering::SeqCst), 1);
}

struct ProtocolServer {
    started: CancellationToken,
    cancelled: CancellationToken,
}
impl rmcp::ServerHandler for ProtocolServer {
    fn get_info(&self) -> rmcp::model::ServerConfig {
        rmcp::model::ServerConfig::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
    }
    async fn call_tool(
        &self,
        _request: CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, ErrorData> {
        self.started.cancel();
        let cancelled = self.cancelled.clone();
        let token = context.ct.clone();
        tokio::spawn(async move {
            token.cancelled().await;
            cancelled.cancel();
        });
        futures::future::pending().await
    }
}

#[tokio::test]
async fn dropping_transport_call_notifies_peer_of_cancellation() {
    use rmcp::ServiceExt;
    let started = CancellationToken::new();
    let cancelled = CancellationToken::new();
    let (server_io, client_io) = tokio::io::duplex(4096);
    let server = ProtocolServer {
        started: started.clone(),
        cancelled: cancelled.clone(),
    };
    let task = tokio::spawn(async move { server.serve(server_io).await.unwrap().waiting().await });
    let client = goose_agent::mcp::TransportClient::connect(client_io, Default::default())
        .await
        .unwrap();
    let context = goose_agent::mcp::CallContext::default();
    let mut call = Box::pin(client.call_tool(&context, "wait", None, CancellationToken::new()));
    tokio::select! {
        result = &mut call => panic!("call unexpectedly completed: {result:?}"),
        _ = started.cancelled() => {},
    }
    drop(call);
    tokio::time::timeout(std::time::Duration::from_secs(2), cancelled.cancelled())
        .await
        .expect("peer must receive cancellation even when caller future is dropped");
    drop(client);
    task.abort();
}

struct Unavailable;
#[async_trait]
impl Connector for Unavailable {
    async fn connect(&self, _: &str, _: CancellationToken) -> Result<Arc<dyn Client>> {
        anyhow::bail!("unavailable")
    }
}

struct Hanging;
#[async_trait]
impl Connector for Hanging {
    async fn connect(&self, _: &str, _: CancellationToken) -> Result<Arc<dyn Client>> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn readiness_is_explicitly_strict_or_best_effort() {
    use goose_agent::mcp::StartupPolicy;
    let lease = Manager::new()
        .resolve(
            "scope",
            vec![Registration::new("broken", 0, Arc::new(Unavailable))],
        )
        .await
        .unwrap();
    // Building a partial catalog does not accidentally satisfy strict readiness.
    lease.catalog(CancellationToken::new()).await.unwrap();
    let error = lease
        .ready(StartupPolicy::Strict, CancellationToken::new())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("broken: unavailable"));
    let reports = lease
        .ready(StartupPolicy::BestEffort, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(reports[0].error.as_deref(), Some("unavailable"));
}

#[tokio::test]
async fn readiness_cancels_uncooperative_startup() {
    let lease = Manager::new()
        .resolve(
            "scope",
            vec![Registration::new("hanging", 0, Arc::new(Hanging))],
        )
        .await
        .unwrap();
    let cancel = CancellationToken::new();
    let future = lease.ready(goose_agent::mcp::StartupPolicy::Strict, cancel.clone());
    let stop = async {
        tokio::task::yield_now().await;
        cancel.cancel();
    };
    let (result, ()) = tokio::join!(future, stop);
    assert!(result.unwrap_err().to_string().contains("cancelled"));
}

#[cfg(feature = "mcp-stdio")]
#[test]
fn stdio_inputs_are_validated_and_host_policy_is_explicit() {
    use goose_agent::mcp::{ClientOptions, StdioConnector};
    assert!(StdioConnector::new("  ", ClientOptions::default()).is_err());
    let connector = StdioConnector::new("server", ClientOptions::default()).unwrap();
    assert!(!connector.inherit_env);
    assert!(connector.cwd.is_none());
}

#[cfg(feature = "mcp-http")]
#[test]
fn http_inputs_are_validated_before_startup() {
    use goose_agent::mcp::{ClientOptions, HttpConnector};
    for url in [
        "file:///tmp/server",
        "not a url",
        "https://user:secret@example.com",
    ] {
        assert!(HttpConnector::new(url, [], ClientOptions::default()).is_err());
    }
    assert!(HttpConnector::new(
        "https://example.com",
        [("invalid header".into(), "x".into())],
        ClientOptions::default()
    )
    .is_err());
    assert!(HttpConnector::new(
        "https://example.com",
        [("Authorization".into(), "bad\nvalue".into())],
        ClientOptions::default()
    )
    .is_err());
}

struct HangingDiscovery;
#[async_trait]
impl Client for HangingDiscovery {
    async fn call_tool(
        &self,
        _: &goose_agent::mcp::CallContext,
        _: &str,
        _: Option<rmcp::model::JsonObject>,
        _: CancellationToken,
    ) -> Result<CallToolResult, rmcp::ServiceError> {
        unreachable!("discovery never completes")
    }

    fn get_info(&self) -> Option<&rmcp::model::InitializeResult> {
        None
    }
    async fn list_tools(
        &self,
        _: &str,
        _: Option<String>,
        _: CancellationToken,
    ) -> Result<ListToolsResult, rmcp::ServiceError> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn readiness_cancels_uncooperative_discovery() {
    let lease = Manager::new()
        .resolve(
            "scope",
            vec![Registration::attached(
                "hanging",
                0,
                Arc::new(HangingDiscovery),
            )],
        )
        .await
        .unwrap();
    let cancel = CancellationToken::new();
    let stop = async {
        tokio::task::yield_now().await;
        cancel.cancel();
    };
    let (result, ()) = tokio::join!(
        lease.ready(goose_agent::mcp::StartupPolicy::Strict, cancel.clone()),
        stop
    );
    assert!(result.unwrap_err().to_string().contains("cancelled"));
}
