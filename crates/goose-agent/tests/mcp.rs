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
    async fn list_tools(
        &self,
        cursor: Option<String>,
        _cancel: CancellationToken,
    ) -> Result<ListToolsResult> {
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
        _request: &str,
        call: CallToolRequestParams,
        _cancel: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "{}:{}",
            self.label, call.name
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
