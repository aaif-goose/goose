//! A consumer of oi-kit-agent running a turn against a real stdio MCP server.
//!
//! The test binary doubles as the server: run with `--mcp-server`, it serves
//! MCP over stdio instead of running the test.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use oi_kit_agent::{
    action_required_manager::ActionRequiredManager,
    extension::ExtensionConfig,
    inference::{InferenceEffect, InferenceRunner},
    machine::{EffectHandler, MachineSession, SessionLoader, StateMachine, Step},
    mcp::{
        ClientContext, ExtensionHost, ExtensionLease, ExtensionManager, ExtensionOperation,
        ExtensionSet, ExtensionSource,
    },
    operation::{ConversationEffect, Emitter, MachineEffect},
    tool_approval::{ApprovalDecision, ToolApproval, ToolApprovalOperation},
    unhandled_tool_calls::UnhandledToolCallsOperation,
};
use oi_kit_provider_types::{
    base::{MessageStream, Provider},
    conversation::{
        message::{Message, MessageContent, ToolRequest},
        token_usage::{ProviderUsage, Usage},
        Conversation,
    },
    errors::ProviderError,
    model::ModelConfig,
    permission::Permission,
};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResult, ContentBlock, ServerCapabilities, ServerConfig, Tool,
    },
    schemars, tool, tool_handler, tool_router, ErrorData, ServerHandler, ServiceExt,
};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const SERVER_FLAG: &str = "--mcp-server";

#[derive(Deserialize, schemars::JsonSchema)]
struct AddParams {
    left: i64,
    right: i64,
}

#[derive(Clone)]
struct Calculator {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl Calculator {
    #[tool(description = "Add two integers")]
    fn add(&self, Parameters(params): Parameters<AddParams>) -> Result<CallToolResult, ErrorData> {
        Ok(CallToolResult::success(vec![ContentBlock::text(
            (params.left + params.right).to_string(),
        )]))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Calculator {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }
}

async fn serve() -> Result<()> {
    let server = Calculator {
        tool_router: Calculator::tool_router(),
    };
    server
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}

/// The host oi-kit-agent needs for stdio and HTTP extensions: nothing.
struct Host;

impl ExtensionHost<ExtensionConfig> for Host {}

struct Extensions {
    manager: Arc<ExtensionManager<ExtensionConfig>>,
    server: ExtensionConfig,
}

#[async_trait]
impl ExtensionSource<Session, ExtensionConfig> for Extensions {
    async fn advertise(&self, session: &Session) -> Result<ExtensionLease<ExtensionConfig>> {
        let set = ExtensionSet::new(session.id.clone(), None, vec![self.server.clone()])?;
        Ok(self.manager.resolve(&set).await)
    }
}

struct AllowAll;

#[async_trait]
impl ToolApproval<Session> for AllowAll {
    async fn check(
        &self,
        _session: &Session,
        _conversation: &Conversation,
        requests: &[&ToolRequest],
    ) -> Result<Vec<ApprovalDecision>> {
        Ok(requests.iter().map(|_| ApprovalDecision::Allow).collect())
    }

    async fn on_confirmation(
        &self,
        _session: &Session,
        _request: &ToolRequest,
        _permission: &Permission,
    ) -> Result<()> {
        Ok(())
    }
}

/// Calls `calculator__add` if the model was offered it, then answers with text.
struct ScriptedProvider {
    advertised: Mutex<Vec<Vec<String>>>,
}

#[async_trait]
impl Provider for ScriptedProvider {
    fn get_name(&self) -> &str {
        "scripted"
    }

    async fn stream(
        &self,
        _model_config: &ModelConfig,
        _system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        self.advertised
            .lock()
            .unwrap()
            .push(tools.iter().map(|tool| tool.name.to_string()).collect());
        let reply = if messages.last().is_some_and(Message::is_tool_response) {
            Message::assistant().with_text("done")
        } else {
            Message::assistant().with_tool_request(
                "call-1",
                Ok(CallToolRequestParams::new("calculator__add")
                    .with_arguments(json!({"left": 2, "right": 3}).as_object().unwrap().clone())),
            )
        };
        let usage = ProviderUsage::new("scripted-model".to_string(), Usage::default());
        Ok(Box::pin(futures::stream::once(async move {
            Ok((Some(reply), Some(usage)))
        })))
    }
}

enum Effect {
    Conversation(Box<ConversationEffect>),
    Usage,
}

impl From<Message> for Effect {
    fn from(message: Message) -> Self {
        Effect::Conversation(Box::new(message.into()))
    }
}

impl From<ConversationEffect> for Effect {
    fn from(effect: ConversationEffect) -> Self {
        Effect::Conversation(Box::new(effect))
    }
}

impl InferenceEffect for Effect {
    fn record_usage(_usage: ProviderUsage) -> Self {
        Effect::Usage
    }
}

impl MachineEffect for Effect {
    fn ensure_message_ids(&mut self) {
        if let Effect::Conversation(effect) = self {
            effect.ensure_message_ids();
        }
    }
}

#[derive(Clone)]
struct Session {
    id: String,
    conversation: Conversation,
}

impl MachineSession for Session {
    fn id(&self) -> &str {
        &self.id
    }

    fn conversation(&self) -> Option<&Conversation> {
        Some(&self.conversation)
    }
}

struct Store {
    session: Mutex<Session>,
}

#[async_trait]
impl SessionLoader<Session> for Store {
    async fn load(&self, _session_id: &str) -> Result<Session> {
        Ok(self.session.lock().unwrap().clone())
    }
}

#[async_trait]
impl EffectHandler<Session, Effect> for Store {
    async fn apply_effects(
        &self,
        _session: &Session,
        effects: &mut [Effect],
        _emit: &Emitter,
    ) -> Result<()> {
        let mut session = self.session.lock().unwrap();
        let mut messages = session.conversation.messages().clone();
        for effect in effects {
            let Effect::Conversation(effect) = effect else {
                continue;
            };
            match effect.as_ref() {
                ConversationEffect::AppendMessage(message) => messages.push(message.clone()),
                ConversationEffect::PatchToolRequestMeta {
                    tool_call_id,
                    patch,
                } => patch_tool_request(&mut messages, tool_call_id, patch),
                _ => {}
            }
        }
        session.conversation = Conversation::new_unvalidated(messages);
        Ok(())
    }
}

fn patch_tool_request(messages: &mut [Message], id: &str, patch: &serde_json::Value) {
    let request = messages
        .iter_mut()
        .flat_map(|message| message.content.iter_mut())
        .find_map(|content| match content {
            MessageContent::ToolRequest(request) if request.id == id => Some(request),
            _ => None,
        })
        .expect("patched tool request exists");
    let meta = request.tool_meta.get_or_insert_with(|| json!({}));
    for (key, value) in patch.as_object().unwrap() {
        meta[key] = value.clone();
    }
}

async fn runs_a_turn_against_a_stdio_server() -> Result<()> {
    let server = ExtensionConfig::stdio(
        "calculator".to_string(),
        std::env::current_exe()?.to_string_lossy().into_owned(),
        String::new(),
        30u64,
    )
    .with_args([SERVER_FLAG]);
    let manager = Arc::new(ExtensionManager::new(
        Arc::new(Host),
        Arc::new(ActionRequiredManager::new()),
        ClientContext::new("oi-kit-agent-test", "1.0.0"),
    ));
    let provider = Arc::new(ScriptedProvider {
        advertised: Mutex::default(),
    });
    let cancel = CancellationToken::new();
    let machine = StateMachine::<Session, Effect>::new(
        vec![
            Step::Operation(Arc::new(ToolApprovalOperation::new(Arc::new(AllowAll)))),
            Step::Operation(Arc::new(ExtensionOperation::new(Arc::new(Extensions {
                manager: Arc::clone(&manager),
                server,
            })))),
            Step::Operation(Arc::new(UnhandledToolCallsOperation::new())),
            Step::Inference(Arc::new(InferenceRunner::new(
                provider.clone(),
                ModelConfig::new("scripted-model"),
            ))),
        ],
        cancel.clone(),
    );
    let store = Store {
        session: Mutex::new(Session {
            id: "consumer".to_string(),
            conversation: Conversation::new_unvalidated([Message::user().with_text("add 2 and 3")]),
        }),
    };
    let (tx, _rx) = mpsc::unbounded_channel();

    let session = machine
        .run(&store, "consumer", &Emitter::new(tx, cancel))
        .await?;
    manager.release("consumer").await;

    assert_eq!(
        *provider.advertised.lock().unwrap(),
        vec![vec!["calculator__add".to_string()]; 2]
    );
    let messages = session.conversation.messages();
    let response = messages
        .iter()
        .flat_map(|message| &message.content)
        .find_map(MessageContent::as_tool_response)
        .expect("the call was answered");
    assert_eq!(response.id, "call-1");
    let result = response.tool_result.as_ref().expect("the call succeeded");
    assert_eq!(result.is_error, Some(false));
    assert_eq!(
        result.content[0].as_text().map(|text| text.text.as_str()),
        Some("5")
    );
    let MessageContent::Text(reply) = &messages.last().unwrap().content[0] else {
        panic!("the turn ends with the model's reply");
    };
    assert_eq!(reply.text, "done");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().any(|arg| arg == SERVER_FLAG) {
        return serve().await;
    }
    runs_a_turn_against_a_stdio_server().await?;
    println!("test runs_a_turn_against_a_stdio_server ... ok");
    Ok(())
}
