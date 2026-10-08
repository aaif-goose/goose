use std::{
    borrow::Cow,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::{
    machine::{EffectHandler, MachineSession, SessionLoader, StateMachine, Step},
    operation::{applied, not_applicable, ConversationEffect, Emitter, Operation, OperationResult},
    tool::{ToolOperation, ToolProvider},
};
use goose_provider_types::conversation::{
    message::{Message, MessageContent},
    Conversation,
};
use rmcp::{
    handler::server::router::tool::{SyncTool, ToolBase},
    model::{CallToolRequestParams, CallToolResult, ContentBlock, ErrorData, Tool},
};
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

struct Runtime(Mutex<Session>);

impl Runtime {
    fn new(messages: impl IntoIterator<Item = Message>) -> Self {
        Self(Mutex::new(Session(Conversation::new_unvalidated(messages))))
    }
}

#[async_trait]
impl SessionLoader<Session> for Runtime {
    async fn load(&self, _session_id: &str) -> Result<Session> {
        Ok(self.0.lock().unwrap().clone())
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
        for effect in effects {
            let ConversationEffect::AppendMessage(message) = effect else {
                panic!("unexpected effect");
            };
            assert!(message.id.is_some());
            self.0.lock().unwrap().0.push(message.clone());
        }
        Ok(())
    }
}

fn texts(session: &Session) -> Vec<String> {
    session.0.iter().map(Message::as_concat_text).collect()
}

struct ExecutionGuard(Arc<Mutex<bool>>);

impl Drop for ExecutionGuard {
    fn drop(&mut self) {
        *self.0.lock().unwrap() = true;
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Behavior {
    NotApplicable,
    HangAfterStop,
    ReturnAfterStop,
    FailAfterStop,
    WatchStop,
}

struct Recorder {
    name: &'static str,
    behavior: Behavior,
    execution_dropped: Arc<Mutex<bool>>,
    saw_stop: Mutex<bool>,
    cancel_calls: Mutex<usize>,
}

impl Recorder {
    fn new(name: &'static str, behavior: Behavior) -> Arc<Self> {
        Arc::new(Self {
            name,
            behavior,
            execution_dropped: Arc::new(Mutex::new(false)),
            saw_stop: Mutex::new(false),
            cancel_calls: Mutex::new(0),
        })
    }
}

#[async_trait]
impl Operation<Session> for Recorder {
    fn name(&self) -> &'static str {
        self.name
    }

    async fn run(
        &self,
        _session: &Session,
        _conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult> {
        match self.behavior {
            Behavior::NotApplicable => not_applicable(),
            Behavior::HangAfterStop => {
                let _guard = ExecutionGuard(self.execution_dropped.clone());
                emit.cancel_token().cancel();
                std::future::pending().await
            }
            Behavior::ReturnAfterStop => {
                emit.cancel_token().cancel();
                applied([Message::assistant().with_text("returned").into()])
            }
            Behavior::FailAfterStop => {
                emit.cancel_token().cancel();
                Err(anyhow::anyhow!("stopped"))
            }
            Behavior::WatchStop => {
                emit.cancelled().await;
                *self.saw_stop.lock().unwrap() = true;
                std::future::pending().await
            }
        }
    }

    async fn cancel(
        &self,
        _session: &Session,
        _conversation: &Conversation,
        _emit: &Emitter,
    ) -> Vec<ConversationEffect> {
        if self.behavior == Behavior::HangAfterStop {
            assert!(*self.execution_dropped.lock().unwrap());
        }
        *self.cancel_calls.lock().unwrap() += 1;
        vec![Message::assistant().with_text(self.name).into()]
    }
}

#[tokio::test]
async fn stop_saves_the_interrupted_step_then_unanswered_calls_then_the_rest() -> Result<()> {
    let before = Recorder::new("before", Behavior::NotApplicable);
    let active = Recorder::new("active", Behavior::HangAfterStop);
    let after = Recorder::new("after", Behavior::NotApplicable);
    let cancel = CancellationToken::new();
    let machine = StateMachine::new(
        vec![
            Step::Operation(before.clone()),
            Step::Operation(active.clone()),
            Step::Operation(after.clone()),
        ],
        cancel.clone(),
    );
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, cancel);
    let runtime = Runtime::new([
        Message::user().with_text("kickoff"),
        Message::assistant()
            .with_tool_request("unanswered", Ok(CallToolRequestParams::new("tool"))),
    ]);

    let session = tokio::time::timeout(
        Duration::from_secs(5),
        machine.run(&runtime, "session", &emit),
    )
    .await??;

    assert_eq!(
        texts(&session),
        ["kickoff", "", "active", "", "before", "after"]
    );
    assert!(interrupted(&session.0.messages()[3].content[0]));
    for operation in [&before, &active, &after] {
        assert_eq!(*operation.cancel_calls.lock().unwrap(), 1);
    }
    Ok(())
}

#[tokio::test]
async fn what_a_step_returns_after_stop_is_kept_or_dropped() -> Result<()> {
    for (behavior, expected) in [
        (
            Behavior::ReturnAfterStop,
            &["kickoff", "returned", "active"][..],
        ),
        (Behavior::FailAfterStop, &["kickoff", "active"][..]),
    ] {
        let active = Recorder::new("active", behavior);
        let cancel = CancellationToken::new();
        let machine = StateMachine::new(vec![Step::Operation(active.clone())], cancel.clone());
        let (tx, _rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, cancel);
        let runtime = Runtime::new([Message::user().with_text("kickoff")]);

        let session = machine.run(&runtime, "session", &emit).await?;

        assert_eq!(texts(&session), expected);
        assert_eq!(*active.cancel_calls.lock().unwrap(), 1);
    }
    Ok(())
}

#[tokio::test]
async fn a_running_step_sees_stop_before_it_is_dropped() -> Result<()> {
    let watcher = Recorder::new("watcher", Behavior::WatchStop);
    let cancel = CancellationToken::new();
    let machine = StateMachine::new(vec![Step::Operation(watcher.clone())], cancel.clone());
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, cancel.clone());
    let runtime = Runtime::new([Message::user().with_text("kickoff")]);
    let run = machine.run(&runtime, "session", &emit);
    tokio::pin!(run);

    assert!(futures::poll!(run.as_mut()).is_pending());
    cancel.cancel();
    let session = tokio::time::timeout(Duration::from_secs(5), run).await??;

    assert!(*watcher.saw_stop.lock().unwrap());
    assert_eq!(texts(&session), ["kickoff", "watcher"]);
    Ok(())
}

async fn run_and_stop(
    operation: Arc<ToolOperation<Session>>,
    request: Message,
    started: impl Fn() -> bool,
    deadline: Duration,
) -> Vec<Message> {
    let cancel = CancellationToken::new();
    let machine = StateMachine::new(vec![Step::Operation(operation)], cancel.clone());
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, cancel.clone());
    let runtime = Runtime::new([Message::user().with_text("kickoff"), request]);
    let run = tokio::spawn(async move { machine.run(&runtime, "session", &emit).await });
    while !started() {
        tokio::task::yield_now().await;
    }
    cancel.cancel();
    tokio::time::timeout(deadline, run)
        .await
        .expect("Stop should not wait for the running call")
        .unwrap()
        .unwrap()
        .0
        .messages()
        .clone()
}

fn interrupted(response: &MessageContent) -> bool {
    let result = response
        .as_tool_response()
        .unwrap()
        .tool_result
        .as_ref()
        .unwrap();
    result.is_error == Some(true)
        && result.content[0].as_text().unwrap().text
            == "Tool call was interrupted before completing"
}

static BLOCKING_SYNC_STARTED: AtomicBool = AtomicBool::new(false);

struct BlockingSyncTool;

impl ToolBase for BlockingSyncTool {
    type Parameter = ();
    type Output = ();
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        "blocking_sync".into()
    }

    fn input_schema() -> Option<Arc<serde_json::Map<String, serde_json::Value>>> {
        None
    }
}

impl SyncTool<Session> for BlockingSyncTool {
    fn invoke(_session: &Session, _input: ()) -> Result<(), ErrorData> {
        BLOCKING_SYNC_STARTED.store(true, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(100));
        Ok(())
    }
}

struct FinishingTools;

#[async_trait]
impl ToolProvider<Session> for FinishingTools {
    async fn tools(&self, _session: &Session) -> Result<Vec<Tool>> {
        Ok(vec![Tool::new(
            "finish",
            "A tool that finishes",
            Arc::new(serde_json::Map::new()),
        )])
    }

    async fn call(
        &self,
        _session: &Session,
        _request_id: &str,
        _call: CallToolRequestParams,
        _emit: &Emitter,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(CallToolResult::success(vec![ContentBlock::text(
            "finished output",
        )]))
    }
}

#[tokio::test]
async fn stop_saves_completed_results_then_answers_each_unanswered_request_once() {
    let operation = Arc::new(
        ToolOperation::new()
            .with_provider(Arc::new(FinishingTools))
            .with_sync_tool::<BlockingSyncTool>(),
    );

    let messages = run_and_stop(
        operation.clone(),
        Message::assistant()
            .with_tool_request("completed", Ok(CallToolRequestParams::new("finish")))
            .with_tool_request("blocking", Ok(CallToolRequestParams::new("blocking_sync")))
            .with_tool_request("remaining", Ok(CallToolRequestParams::new("finish")))
            .with_tool_request("remaining", Ok(CallToolRequestParams::new("finish")))
            .with_tool_request("invalid", Err(ErrorData::invalid_params("malformed", None)))
            .with_tool_request("unavailable", Ok(CallToolRequestParams::new("missing")))
            .with_tool_request_with_metadata(
                "external",
                Ok(CallToolRequestParams::new("external")),
                None,
                Some(json!({ "goose.external_dispatch": true })),
            ),
        || BLOCKING_SYNC_STARTED.load(Ordering::SeqCst),
        Duration::from_millis(50),
    )
    .await;

    assert_eq!(messages.len(), 4);
    let output = messages[2].content[0].as_tool_response().unwrap();
    assert_eq!(output.id, "completed");
    assert_eq!(
        output.tool_result.as_ref().unwrap().content[0]
            .as_text()
            .unwrap()
            .text,
        "finished output"
    );
    let interrupted_ids: Vec<_> = messages[3]
        .content
        .iter()
        .map(|content| content.as_tool_response().unwrap().id.as_str())
        .collect();
    assert_eq!(
        interrupted_ids,
        [
            "blocking",
            "remaining",
            "invalid",
            "unavailable",
            "external"
        ]
    );
    assert!(messages[3].content.iter().all(interrupted));
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    let session = Session(Conversation::new_unvalidated(messages));
    assert!(
        Operation::<Session>::cancel(operation.as_ref(), &session, &session.0, &emit)
            .await
            .is_empty()
    );
}
