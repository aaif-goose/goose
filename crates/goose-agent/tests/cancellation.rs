use std::{
    borrow::Cow,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
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

struct ExecutionGuard<'a>(&'a AtomicBool);

impl Drop for ExecutionGuard<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
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
    execution_dropped: AtomicBool,
    saw_stop: AtomicBool,
    cancel_calls: AtomicUsize,
}

impl Recorder {
    fn new(name: &'static str, behavior: Behavior) -> Arc<Self> {
        Arc::new(Self {
            name,
            behavior,
            execution_dropped: AtomicBool::new(false),
            saw_stop: AtomicBool::new(false),
            cancel_calls: AtomicUsize::new(0),
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
                let _guard = ExecutionGuard(&self.execution_dropped);
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
                self.saw_stop.store(true, Ordering::SeqCst);
                std::future::pending().await
            }
        }
    }

    async fn finalize_cancellation(
        &self,
        _session: &Session,
        _conversation: &Conversation,
        _emit: &Emitter,
    ) -> Vec<ConversationEffect> {
        if self.behavior == Behavior::HangAfterStop {
            assert!(self.execution_dropped.load(Ordering::SeqCst));
        }
        self.cancel_calls.fetch_add(1, Ordering::SeqCst);
        vec![Message::assistant().with_text(self.name).into()]
    }
}

#[tokio::test]
async fn stop_saves_the_interrupted_step_then_unanswered_calls_then_the_rest() -> Result<()> {
    for (behavior, expected) in [
        (
            Behavior::HangAfterStop,
            &["kickoff", "", "active", "", "before", "after"][..],
        ),
        (
            Behavior::FailAfterStop,
            &["kickoff", "", "active", "", "before", "after"][..],
        ),
        (
            Behavior::ReturnAfterStop,
            &["kickoff", "", "returned", "", "before", "active", "after"][..],
        ),
    ] {
        let before = Recorder::new("before", Behavior::NotApplicable);
        let active = Recorder::new("active", behavior);
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

        assert_eq!(texts(&session), expected);
        let answer = expected.iter().rposition(|text| text.is_empty()).unwrap();
        assert!(interrupted(&session.0.messages()[answer].content[0]));
        for operation in [&before, &active, &after] {
            assert_eq!(operation.cancel_calls.load(Ordering::SeqCst), 1);
        }
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

    assert!(watcher.saw_stop.load(Ordering::SeqCst));
    assert_eq!(texts(&session), ["kickoff", "watcher"]);
    Ok(())
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
    let operation = ToolOperation::new()
        .with_provider(Arc::new(FinishingTools))
        .with_sync_tool::<BlockingSyncTool>();

    let cancel = CancellationToken::new();
    let machine = StateMachine::new(vec![Step::Operation(Arc::new(operation))], cancel.clone());
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, cancel.clone());
    let runtime = Runtime::new([
        Message::user().with_text("kickoff"),
        Message::assistant()
            .with_tool_request("completed", Ok(CallToolRequestParams::new("finish")))
            .with_tool_request("blocking", Ok(CallToolRequestParams::new("blocking_sync")))
            .with_tool_request("remaining", Ok(CallToolRequestParams::new("finish")))
            .with_tool_request("remaining", Ok(CallToolRequestParams::new("finish"))),
    ]);
    let run = tokio::spawn(async move { machine.run(&runtime, "session", &emit).await });
    while !BLOCKING_SYNC_STARTED.load(Ordering::SeqCst) {
        tokio::task::yield_now().await;
    }
    cancel.cancel();
    let messages = tokio::time::timeout(Duration::from_millis(50), run)
        .await
        .expect("Stop should not wait for the running call")
        .unwrap()
        .unwrap()
        .0
        .messages()
        .clone();

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
    assert_eq!(interrupted_ids, ["blocking", "remaining"]);
    assert!(messages[3].content.iter().all(interrupted));
}
