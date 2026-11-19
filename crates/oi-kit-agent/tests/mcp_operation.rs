#![cfg(not(target_arch = "wasm32"))]

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use oi_kit_agent::{
    action_required_manager::ActionRequiredManager,
    extension::{ExtensionConfig, ExtensionResult},
    machine::{MachineSession, StateMachine, Step},
    mcp::{
        client::{ConnectContext, McpCallContext, McpClientTrait},
        ClientContext, Container, ExtensionHost, ExtensionLease, ExtensionManager,
        ExtensionOperation, ExtensionSet, ExtensionSource, ExtensionSpec,
    },
    operation::{ConversationEffect, Emitter},
    tool_call_state::EXECUTABLE_KEY,
    unhandled_tool_calls::UnhandledToolCallsOperation,
};
use oi_kit_provider_types::conversation::{
    message::{Message, MessageContent},
    Conversation,
};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, InitializeResult, JsonObject,
    ListToolsResult, Tool,
};
use tokio::sync::{mpsc, Notify};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, PartialEq)]
struct Config(&'static str);

impl ExtensionSpec for Config {
    fn name(&self) -> String {
        "server".into()
    }

    fn is_tool_available(&self, _tool: &str) -> bool {
        true
    }

    fn transport(&self) -> Option<&ExtensionConfig> {
        None
    }
}

#[derive(Default)]
struct Host {
    dropped: Mutex<Vec<&'static str>>,
    started: Notify,
    finish: Notify,
}

struct Client {
    label: &'static str,
    host: Arc<Host>,
}

impl Drop for Client {
    fn drop(&mut self) {
        self.host.dropped.lock().unwrap().push(self.label);
    }
}

#[async_trait]
impl ExtensionHost<Config> for Arc<Host> {
    async fn connect(
        &self,
        config: &Config,
        _container: Option<&Container>,
        _ctx: ConnectContext,
    ) -> ExtensionResult<Box<dyn McpClientTrait>> {
        Ok(Box::new(Client {
            label: config.0,
            host: self.clone(),
        }))
    }
}

#[async_trait]
impl McpClientTrait for Client {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Result<ListToolsResult, rmcp::ServiceError> {
        Ok(ListToolsResult {
            tools: vec![Tool::new("echo", self.label, Arc::new(JsonObject::new()))],
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        _ctx: &McpCallContext,
        _name: &str,
        _arguments: Option<JsonObject>,
        _cancel_token: CancellationToken,
    ) -> Result<CallToolResult, rmcp::ServiceError> {
        self.host.started.notify_one();
        self.host.finish.notified().await;
        Ok(CallToolResult::success(vec![ContentBlock::text(
            self.label,
        )]))
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        None
    }
}

struct Session {
    conversation: Conversation,
}

impl MachineSession for Session {
    fn id(&self) -> &str {
        "scope"
    }

    fn conversation(&self) -> Option<&Conversation> {
        Some(&self.conversation)
    }
}

struct Source {
    manager: Arc<ExtensionManager<Config>>,
    selection: Mutex<Vec<Config>>,
}

#[async_trait]
impl ExtensionSource<Session, Config> for Source {
    async fn advertise(&self, session: &Session) -> Result<ExtensionLease<Config>> {
        let set = ExtensionSet::new(session.id(), None, self.selection.lock().unwrap().clone())?;
        Ok(self.manager.resolve(&set).await)
    }
}

struct Fixture {
    host: Arc<Host>,
    manager: Arc<ExtensionManager<Config>>,
    source: Arc<Source>,
    machine: StateMachine<'static, Session>,
    session: Session,
}

impl Fixture {
    fn new() -> Self {
        let host = Arc::new(Host::default());
        let manager = Arc::new(ExtensionManager::new(
            Arc::new(host.clone()),
            Arc::new(ActionRequiredManager::new()),
            ClientContext::new("test", "1"),
        ));
        let source = Arc::new(Source {
            manager: manager.clone(),
            selection: Mutex::new(vec![Config("original")]),
        });
        let operation = Arc::new(ExtensionOperation::new(source.clone()));
        let machine = StateMachine::new(
            vec![
                Step::Operation(operation),
                Step::Operation(Arc::new(UnhandledToolCallsOperation::new())),
            ],
            CancellationToken::new(),
        );
        Self {
            host,
            manager,
            source,
            machine,
            session: Session {
                conversation: Conversation::new_unvalidated([Message::user().with_text("echo")]),
            },
        }
    }

    async fn advertise(&self) -> Result<()> {
        let input = self
            .machine
            .collect_inference_input(&self.session, &self.session.conversation)
            .await?;
        assert_eq!(input.tools[0].description.as_deref(), Some("original"));
        Ok(())
    }

    fn approve_call(&mut self) {
        let mut message = Message::assistant()
            .with_tool_request("call", Ok(CallToolRequestParams::new("server__echo")));
        let MessageContent::ToolRequest(request) = &mut message.content[0] else {
            unreachable!();
        };
        request.tool_meta = Some(serde_json::json!({ EXECUTABLE_KEY: true }));
        self.session.conversation.push(message);
    }

    async fn execute(&self) -> Result<CallToolResult> {
        let (tx, _rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, CancellationToken::new());
        self.host.finish.notify_one();
        let result = self.machine.step(&self.session, &emit).await?;
        let ConversationEffect::AppendMessage(message) = &result.effects[0] else {
            panic!("expected a tool response");
        };
        Ok(message.content[0]
            .as_tool_response()
            .unwrap()
            .tool_result
            .as_ref()
            .unwrap()
            .clone())
    }
}

#[tokio::test]
async fn execution_uses_current_selection_without_another_tool_collection() -> Result<()> {
    let mut fixture = Fixture::new();
    fixture.advertise().await?;
    *fixture.source.selection.lock().unwrap() = vec![Config("replacement")];
    fixture.approve_call();
    let result = fixture.execute().await?;
    assert_eq!(result.content[0].as_text().unwrap().text, "replacement");
    assert!(fixture.host.dropped.lock().unwrap().contains(&"original"));
    Ok(())
}

#[tokio::test]
async fn execution_without_inference_or_after_manager_release_acquires_a_lease() -> Result<()> {
    let mut fixture = Fixture::new();
    fixture.approve_call();
    let result = fixture.execute().await?;
    assert_eq!(result.content[0].as_text().unwrap().text, "original");

    fixture.manager.release("scope").await;
    assert_eq!(*fixture.host.dropped.lock().unwrap(), vec!["original"]);
    let result = fixture.execute().await?;
    assert_eq!(result.content[0].as_text().unwrap().text, "original");
    Ok(())
}

#[tokio::test]
async fn removed_tool_returns_an_error_instead_of_using_the_inference_lease() -> Result<()> {
    let mut fixture = Fixture::new();
    fixture.advertise().await?;
    *fixture.source.selection.lock().unwrap() = Vec::new();
    fixture.approve_call();
    let result = fixture.execute().await?;
    assert_eq!(result.is_error, Some(true));
    assert_eq!(*fixture.host.dropped.lock().unwrap(), vec!["original"]);
    Ok(())
}

#[tokio::test]
async fn manager_release_drops_resources_without_dropping_the_operation() -> Result<()> {
    let fixture = Fixture::new();
    fixture.advertise().await?;
    fixture.manager.release("scope").await;
    assert_eq!(*fixture.host.dropped.lock().unwrap(), vec!["original"]);
    Ok(())
}

#[tokio::test]
async fn manager_release_keeps_an_in_flight_call_alive_until_completion() -> Result<()> {
    let mut fixture = Fixture::new();
    fixture.approve_call();
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    let execution = fixture.machine.step(&fixture.session, &emit);
    let cleanup = async {
        fixture.host.started.notified().await;
        fixture.manager.release("scope").await;
        assert!(fixture.host.dropped.lock().unwrap().is_empty());
        fixture.host.finish.notify_one();
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(execution, cleanup)
    })
    .await?;
    assert_eq!(result?.applied_step, Some("extensions"));
    assert_eq!(*fixture.host.dropped.lock().unwrap(), vec!["original"]);
    Ok(())
}
