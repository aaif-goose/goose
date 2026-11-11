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
    async fn connect(
        &self,
        _scope: &str,
        _cancel: CancellationToken,
    ) -> Result<Option<Arc<dyn Client>>> {
        self.0.starts.fetch_add(1, Ordering::SeqCst);
        Ok(Some(self.0.clone()))
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
async fn operation_pins_execution_and_rejects_missing_binding() {
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
    let tools =
        <McpOperation<()> as Operation<(), ConversationEffect>>::inference_tools(&operation, &())
            .await
            .unwrap();
    assert_eq!(tools[0].meta.as_ref().unwrap().0[BINDING_META_KEY], id);
    for binding in [None, Some(serde_json::json!({ BINDING_META_KEY: id }))] {
        let bound = binding.is_some();
        let conversation = Conversation::new_unvalidated(vec![
            Message::user().with_text("go"),
            Message::assistant().with_tool_request_with_metadata(
                "call",
                Ok(CallToolRequestParams::new("fixture__first")),
                None,
                binding,
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
            panic!("must respond")
        };
        let ConversationEffect::AppendMessage(message) = &result.effects[0] else {
            panic!("must append")
        };
        let response = &message.content[0].as_tool_response().unwrap().tool_result;
        if bound {
            assert_eq!(
                response.as_ref().unwrap().content[0]
                    .as_text()
                    .unwrap()
                    .text,
                "old:first"
            );
        } else {
            assert!(response.is_err());
        }
    }
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
    let (old_ready, current_ready) = tokio::join!(
        old.ready(CancellationToken::new()),
        current.ready(CancellationToken::new())
    );
    old_ready.unwrap();
    current_ready.unwrap();
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

enum Failure {
    Connect,
    Discover,
    Hang,
}
#[async_trait]
impl Connector for Failure {
    async fn connect(&self, _: &str, _: CancellationToken) -> Result<Option<Arc<dyn Client>>> {
        match self {
            Self::Hang => std::future::pending().await,
            _ => anyhow::bail!("unavailable"),
        }
    }
}

#[tokio::test]
async fn readiness_is_explicitly_strict_or_best_effort() {
    use goose_agent::mcp::StartupPolicy;
    let lease = Manager::new()
        .resolve(
            "scope",
            vec![Registration::new("broken", Arc::new(Failure::Connect))],
        )
        .await
        .unwrap();
    assert!(lease.catalog(CancellationToken::new()).await.is_err());
    let error = lease.ready(CancellationToken::new()).await.unwrap_err();
    assert!(error.to_string().contains("broken (Connect): unavailable"));
    let best = Manager::new()
        .resolve_with_policy(
            "scope",
            vec![Registration::new("broken", Arc::new(Failure::Connect))],
            StartupPolicy::BestEffort,
        )
        .await
        .unwrap();
    let reports = best.ready(CancellationToken::new()).await.unwrap();
    assert_eq!(reports[0].error.as_deref(), Some("unavailable"));
    assert!(best
        .catalog(CancellationToken::new())
        .await
        .unwrap()
        .entries
        .is_empty());
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

#[async_trait]
impl Client for Failure {
    async fn call_tool(
        &self,
        _: &goose_agent::mcp::CallContext,
        _: &str,
        _: Option<rmcp::model::JsonObject>,
        _: CancellationToken,
    ) -> Result<CallToolResult, rmcp::ServiceError> {
        unreachable!("failed servers have no tools")
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
        if matches!(self, Self::Hang) {
            std::future::pending().await
        } else {
            Err(rmcp::ServiceError::McpError(ErrorData::internal_error(
                "discovery failed",
                None,
            )))
        }
    }
}

#[tokio::test]
async fn readiness_cancels_uncooperative_startup_and_discovery() {
    for registration in [
        Registration::new("hanging", Arc::new(Failure::Hang)),
        Registration::attached("hanging", Arc::new(Failure::Hang)),
    ] {
        let lease = Manager::new()
            .resolve("scope", vec![registration])
            .await
            .unwrap();
        let cancel = CancellationToken::new();
        let stop = async {
            tokio::task::yield_now().await;
            cancel.cancel();
        };
        let (result, ()) = tokio::join!(lease.ready(cancel.clone()), stop);
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }
}

#[tokio::test]
async fn best_effort_excludes_discovery_failures_and_new_binding_keeps_connections() {
    use goose_agent::mcp::{Lease, LoadStage, StartupPolicy};
    let lease = Lease::connect(
        "scope",
        vec![Registration::attached(
            "broken",
            Arc::new(Failure::Discover),
        )],
        StartupPolicy::BestEffort,
        CancellationToken::new(),
    )
    .await
    .unwrap();
    let reports = lease.ready(CancellationToken::new()).await.unwrap();
    assert_eq!(reports[0].stage, LoadStage::Discover);
    assert!(lease
        .catalog(CancellationToken::new())
        .await
        .unwrap()
        .entries
        .is_empty());
    let binding = lease.new_binding();
    assert_ne!(lease.id(), binding.id());
    assert!(Arc::ptr_eq(
        &lease
            .server("broken", CancellationToken::new())
            .await
            .unwrap(),
        &binding
            .server("broken", CancellationToken::new())
            .await
            .unwrap()
    ));
}

struct Hosted {
    label: &'static str,
    version: std::sync::atomic::AtomicU64,
    lists: AtomicUsize,
}
#[async_trait]
impl Client for Hosted {
    fn get_info(&self) -> Option<&rmcp::model::InitializeResult> {
        None
    }
    async fn list_tools(
        &self,
        _: &str,
        _: Option<String>,
        _: CancellationToken,
    ) -> Result<ListToolsResult, rmcp::ServiceError> {
        self.lists.fetch_add(1, Ordering::SeqCst);
        Ok(ListToolsResult {
            tools: vec![Tool::new(
                "tool",
                self.label,
                Arc::new(serde_json::Map::new()),
            )],
            ..Default::default()
        })
    }
    async fn call_tool(
        &self,
        _: &goose_agent::mcp::CallContext,
        _: &str,
        _: Option<rmcp::model::JsonObject>,
        _: CancellationToken,
    ) -> Result<CallToolResult, rmcp::ServiceError> {
        Ok(CallToolResult::success(vec![]))
    }
}
impl goose_agent::mcp::Server for Hosted {
    type Context = goose_agent::mcp::CallContext;
    fn client(&self) -> &dyn Client {
        self
    }
    fn tools_version(&self) -> Option<u64> {
        Some(self.version.load(Ordering::SeqCst))
    }
}
fn hosted(label: &'static str) -> Arc<Hosted> {
    Arc::new(Hosted {
        label,
        version: Default::default(),
        lists: AtomicUsize::new(0),
    })
}

/// Starts `hosted`, or declines, fails, or waits for a gate, as configured.
#[derive(Default)]
struct HostedConnector {
    server: Option<Arc<Hosted>>,
    starts: AtomicUsize,
    fail_first: bool,
    current: std::sync::atomic::AtomicBool,
    gate: Option<Arc<tokio::sync::Barrier>>,
}
#[async_trait]
impl Connector<Hosted> for HostedConnector {
    async fn connect(&self, _: &str, _: CancellationToken) -> Result<Option<Arc<Hosted>>> {
        let start = self.starts.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = &self.gate {
            gate.wait().await;
        }
        if self.fail_first && start == 0 {
            anyhow::bail!("first start fails");
        }
        Ok(self.server.clone())
    }
    async fn is_current(&self, _: goose_agent::mcp::Started<'_, Hosted>) -> bool {
        self.current.load(Ordering::SeqCst)
    }
}
fn hosted_connector(server: Option<Arc<Hosted>>) -> Arc<HostedConnector> {
    Arc::new(HostedConnector {
        server,
        current: true.into(),
        ..Default::default()
    })
}
fn hosted_registration(
    id: &str,
    connector: &Arc<HostedConnector>,
) -> Registration<Hosted, &'static str> {
    Registration {
        id: id.into(),
        revision: "r1",
        connector: connector.clone(),
        available_tools: vec![],
        unprefixed: false,
    }
}

#[tokio::test]
async fn declined_servers_are_selected_without_failing_a_strict_binding() {
    let manager = Manager::<Hosted, &str>::default();
    let declining = hosted_connector(None);
    let lease = manager
        .resolve("scope", vec![hosted_registration("declined", &declining)])
        .await
        .unwrap();
    let reports = lease.ready(CancellationToken::new()).await.unwrap();
    assert_eq!(reports[0].stage, goose_agent::mcp::LoadStage::Declined);
    assert!(reports[0].error.is_none());
    assert!(lease
        .catalog(CancellationToken::new())
        .await
        .unwrap()
        .entries
        .is_empty());
    assert!(lease.servers(CancellationToken::new()).await.is_empty());
    assert_eq!(
        lease
            .registrations()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        ["declined"]
    );
}

#[tokio::test]
async fn a_stale_connection_is_replaced_even_with_an_unchanged_revision() {
    let manager = Manager::<Hosted, &str>::default();
    let connector = hosted_connector(Some(hosted("server")));
    let first = manager
        .resolve("scope", vec![hosted_registration("server", &connector)])
        .await
        .unwrap();
    first.ready(CancellationToken::new()).await.unwrap();
    let kept = manager
        .resolve("scope", vec![hosted_registration("server", &connector)])
        .await
        .unwrap();
    kept.ready(CancellationToken::new()).await.unwrap();
    assert_eq!(connector.starts.load(Ordering::SeqCst), 1);

    connector.current.store(false, Ordering::SeqCst);
    let replaced = manager
        .resolve("scope", vec![hosted_registration("server", &connector)])
        .await
        .unwrap();
    replaced.ready(CancellationToken::new()).await.unwrap();
    assert_eq!(connector.starts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn new_bindings_reuse_tools_until_the_server_reports_a_change() {
    let server = hosted("server");
    let manager = Manager::<Hosted, &str>::default();
    let connector = hosted_connector(Some(server.clone()));
    let lease = manager
        .resolve("scope", vec![hosted_registration("server", &connector)])
        .await
        .unwrap();
    lease.catalog(CancellationToken::new()).await.unwrap();
    let next = lease.new_binding();
    next.catalog(CancellationToken::new()).await.unwrap();
    let resolved = manager
        .resolve("scope", vec![hosted_registration("server", &connector)])
        .await
        .unwrap();
    resolved.catalog(CancellationToken::new()).await.unwrap();
    assert_eq!(server.lists.load(Ordering::SeqCst), 1);

    server.version.fetch_add(1, Ordering::SeqCst);
    lease
        .new_binding()
        .catalog(CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(server.lists.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn servers_start_concurrently() {
    let gate = Arc::new(tokio::sync::Barrier::new(2));
    let connectors: Vec<_> = ["a", "b"]
        .into_iter()
        .map(|label| {
            Arc::new(HostedConnector {
                server: Some(hosted(label)),
                current: true.into(),
                gate: Some(gate.clone()),
                ..Default::default()
            })
        })
        .collect();
    let lease = Manager::<Hosted, &str>::default()
        .resolve(
            "scope",
            vec![
                hosted_registration("a", &connectors[0]),
                hosted_registration("b", &connectors[1]),
            ],
        )
        .await
        .unwrap();
    let catalog = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        lease.catalog(CancellationToken::new()),
    )
    .await
    .expect("sequential startup would wait on the gate forever")
    .unwrap();
    assert_eq!(catalog.entries.len(), 2);
}

#[tokio::test]
async fn keep_first_collision_policy_is_an_explicit_opt_in() {
    let connectors = [
        hosted_connector(Some(hosted("first"))),
        hosted_connector(Some(hosted("second"))),
    ];
    let registrations = || {
        connectors
            .iter()
            .enumerate()
            .map(|(index, connector)| Registration {
                unprefixed: true,
                ..hosted_registration(["a", "b"][index], connector)
            })
            .collect::<Vec<_>>()
    };
    let manager = Manager::<Hosted, &str>::default();
    let strict = manager.resolve("scope", registrations()).await.unwrap();
    assert!(strict.catalog(CancellationToken::new()).await.is_err());
    let legacy = manager
        .resolve("scope", registrations())
        .await
        .unwrap()
        .with_collision_policy(CollisionPolicy::KeepFirst);
    let catalog = legacy.catalog(CancellationToken::new()).await.unwrap();
    assert_eq!(catalog.entries.len(), 1);
    assert_eq!(
        catalog.entries[0].tool.description.as_deref(),
        Some("first")
    );
}

#[tokio::test]
async fn concurrent_enables_and_resolves_share_one_start() {
    let gate = Arc::new(tokio::sync::Barrier::new(2));
    let connector = Arc::new(HostedConnector {
        server: Some(hosted("server")),
        current: true.into(),
        gate: Some(gate.clone()),
        ..Default::default()
    });
    let manager = Manager::<Hosted, &str>::default();
    let commit = |load: goose_agent::mcp::LoadResult| async move {
        anyhow::ensure!(load.error.is_none());
        Ok::<_, anyhow::Error>(())
    };
    let enable = || {
        manager.enable(
            "scope",
            hosted_registration("server", &connector),
            CancellationToken::new(),
            commit,
        )
    };
    let resolve_during_enable = async {
        tokio::task::yield_now().await;
        let lease = manager
            .resolve("scope", vec![hosted_registration("server", &connector)])
            .await
            .unwrap();
        gate.wait().await;
        lease.ready(CancellationToken::new()).await.unwrap();
    };
    let (first, second, ()) = tokio::join!(enable(), enable(), resolve_during_enable);
    first.unwrap();
    second.unwrap();
    assert_eq!(connector.starts.load(Ordering::SeqCst), 1);
    let selected = manager
        .selected("scope", goose_agent::mcp::StartupPolicy::Strict)
        .await;
    assert_eq!(
        selected
            .registrations()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        ["server"]
    );
}

#[tokio::test]
async fn failed_or_rejected_enables_are_not_selected_and_retry_on_the_next_enable() {
    let connector = Arc::new(HostedConnector {
        server: Some(hosted("server")),
        current: true.into(),
        fail_first: true,
        ..Default::default()
    });
    let manager = Manager::<Hosted, &str>::default();
    let enable = |accept: bool| {
        manager.enable(
            "scope",
            hosted_registration("server", &connector),
            CancellationToken::new(),
            move |load: goose_agent::mcp::LoadResult| async move {
                anyhow::ensure!(load.error.is_none(), "start failed");
                anyhow::ensure!(accept, "host rejected the selection");
                Ok(())
            },
        )
    };
    let selected = || manager.selected("scope", goose_agent::mcp::StartupPolicy::Strict);
    assert!(enable(true).await.is_err());
    assert_eq!(selected().await.registrations().count(), 0);
    assert!(enable(false).await.is_err());
    assert_eq!(selected().await.registrations().count(), 0);
    assert_eq!(connector.starts.load(Ordering::SeqCst), 2);
    // A rejected selection drops its connection rather than keeping it for later.
    enable(true).await.unwrap();
    assert_eq!(
        selected()
            .await
            .registrations()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        ["server"]
    );
    assert_eq!(connector.starts.load(Ordering::SeqCst), 3);
}
