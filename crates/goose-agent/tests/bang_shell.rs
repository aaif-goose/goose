use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::{
    bang_shell::BangShellOperation,
    machine::{EffectHandler, MachineSession, SessionLoader, StateMachine, Step},
    operation::{
        yielded_with, ConversationEffect, Emitter, Inference, InferenceInput, Operation,
        OperationResult,
    },
    tool::{ToolOperation, ToolProvider},
    tool_approval::{ApprovalDecision, ToolApproval, ToolApprovalOperation},
    unhandled_tool_calls::UnhandledToolCallsOperation,
};
use goose_provider_types::conversation::{
    message::{Message, MessageContent, ToolRequest},
    Conversation,
};
use goose_provider_types::permission::Permission;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, ErrorData, Tool};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct Session(Conversation);

impl MachineSession for Session {
    fn id(&self) -> &str {
        "session"
    }

    fn conversation(&self) -> Option<&Conversation> {
        Some(&self.0)
    }
}

struct Runtime(Mutex<Vec<Message>>);

#[async_trait]
impl SessionLoader<Session> for Runtime {
    async fn load(&self, _session_id: &str) -> Result<Session> {
        Ok(Session(Conversation::new_unvalidated(
            self.0.lock().unwrap().clone(),
        )))
    }
}

#[async_trait]
impl EffectHandler<Session, ConversationEffect> for Runtime {
    async fn apply_effects(
        &self,
        _session: &Session,
        effects: &mut [ConversationEffect],
        _emit: &Emitter,
    ) -> Result<()> {
        let mut messages = self.0.lock().unwrap();
        for effect in effects {
            match effect {
                ConversationEffect::AppendMessage(message) => messages.push(message.clone()),
                ConversationEffect::PatchToolRequestMeta {
                    tool_call_id,
                    patch,
                } => {
                    let request = messages
                        .iter_mut()
                        .flat_map(|message| &mut message.content)
                        .find_map(|content| match content {
                            MessageContent::ToolRequest(request) if request.id == *tool_call_id => {
                                Some(request)
                            }
                            _ => None,
                        })
                        .expect("patched request exists");
                    let meta = request.tool_meta.get_or_insert_with(|| json!({}));
                    for (key, value) in patch.as_object().unwrap() {
                        meta[key] = value.clone();
                    }
                }
                _ => panic!("unexpected effect"),
            }
        }
        Ok(())
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

#[derive(Default)]
struct Shell(Mutex<Vec<String>>);

#[async_trait]
impl ToolProvider<Session> for Shell {
    async fn tools(&self, _session: &Session) -> Result<Vec<Tool>> {
        Ok(vec![Tool::new(
            "run_command",
            "Run a shell command",
            Arc::new(serde_json::Map::new()),
        )])
    }

    async fn call(
        &self,
        _session: &Session,
        _request_id: &str,
        call: CallToolRequestParams,
        _emit: &Emitter,
    ) -> Result<CallToolResult, ErrorData> {
        let command = call.arguments.unwrap()["command"]
            .as_str()
            .unwrap()
            .to_string();
        self.0.lock().unwrap().push(command.clone());
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "ran {command}"
        ))]))
    }
}

#[derive(Default)]
struct Model(AtomicUsize);

#[async_trait]
impl Operation<Session> for Model {
    fn name(&self) -> &'static str {
        "model"
    }

    async fn run(
        &self,
        _session: &Session,
        _conversation: &Conversation,
        _emit: &Emitter,
    ) -> Result<OperationResult> {
        unreachable!("the machine calls infer")
    }
}

#[async_trait]
impl Inference<Session> for Model {
    fn applies(&self, _conversation: &Conversation) -> bool {
        true
    }

    async fn infer(
        &self,
        _session: &Session,
        _conversation: &Conversation,
        _input: InferenceInput,
        emit: &Emitter,
    ) -> Result<OperationResult> {
        self.0.fetch_add(1, Ordering::SeqCst);
        yielded_with([emit
            .message(Message::assistant().with_text("from the model"))
            .into()])
    }
}

async fn run_turn(kickoff: &str) -> Result<(Vec<Message>, Vec<String>, usize)> {
    let shell = Arc::new(Shell::default());
    let model = Arc::new(Model::default());
    let cancel = CancellationToken::new();
    let machine = StateMachine::new(
        vec![
            Step::Operation(Arc::new(BangShellOperation::new("run_command"))),
            Step::Operation(Arc::new(ToolApprovalOperation::new(Arc::new(AllowAll)))),
            Step::Operation(Arc::new(ToolOperation::new().with_provider(shell.clone()))),
            Step::Operation(Arc::new(UnhandledToolCallsOperation::new())),
            Step::Inference(model.clone()),
        ],
        cancel.clone(),
    );
    let runtime = Runtime(Mutex::new(vec![Message::user().with_text(kickoff)]));
    let (tx, _rx) = mpsc::unbounded_channel();
    machine
        .run(&runtime, "session", &Emitter::new(tx, cancel))
        .await?;
    let messages = runtime.0.into_inner().unwrap();
    let commands = shell.0.lock().unwrap().clone();
    Ok((messages, commands, model.0.load(Ordering::SeqCst)))
}

#[tokio::test]
async fn bang_command_runs_through_the_host_tool_pipeline_without_inference() -> Result<()> {
    let (messages, commands, inferences) = run_turn("  !  ls -la").await?;

    assert_eq!(commands, ["ls -la"]);
    assert_eq!(inferences, 0);
    assert_eq!(messages.len(), 3);
    assert_eq!(
        messages[2].content[0].as_tool_response_text().as_deref(),
        Some("ran ls -la")
    );
    Ok(())
}

#[tokio::test]
async fn ordinary_and_empty_bang_messages_go_to_the_model() -> Result<()> {
    for kickoff in ["hello", "!", "say !ls"] {
        let (_, commands, inferences) = run_turn(kickoff).await?;
        assert!(commands.is_empty(), "{kickoff}");
        assert_eq!(inferences, 1, "{kickoff}");
    }
    Ok(())
}
