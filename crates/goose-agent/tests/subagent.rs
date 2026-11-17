use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::future;
use futures::stream;
use futures::FutureExt;
use goose_agent::events::AgentEvent;
use goose_agent::operation::{
    ConversationEffect, Emitter, Operation, OperationResult, OperationStream,
};
use goose_agent::subagent::{SubagentOperation, SubagentOutcome, SubagentRunner, SubagentUpdate};
use goose_provider_types::conversation::{message::Message, Conversation};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct Parent;

enum Child {
    Finished(SubagentOutcome),
    Running(mpsc::UnboundedReceiver<SubagentUpdate>),
}

struct RunningChild(mpsc::UnboundedSender<SubagentUpdate>);

impl RunningChild {
    fn to_parent(&self, text: &str) {
        self.0
            .send(SubagentUpdate::ToParent(
                Message::assistant().with_text(text),
            ))
            .unwrap();
    }

    fn finish(&self, outcome: SubagentOutcome) {
        self.0.send(SubagentUpdate::Finished(outcome)).unwrap();
    }

    fn is_closed(&self) -> bool {
        self.0.is_closed()
    }
}

#[derive(Default)]
struct FakeRunner {
    children: Mutex<HashMap<String, Child>>,
    started: Mutex<Vec<String>>,
    events: Mutex<Vec<String>>,
}

impl FakeRunner {
    fn finished(&self, subagent_session_id: &str, outcome: SubagentOutcome) {
        self.children
            .lock()
            .unwrap()
            .insert(subagent_session_id.to_string(), Child::Finished(outcome));
    }

    fn running(&self, subagent_session_id: &str) -> RunningChild {
        let (tx, rx) = mpsc::unbounded_channel();
        self.children
            .lock()
            .unwrap()
            .insert(subagent_session_id.to_string(), Child::Running(rx));
        RunningChild(tx)
    }

    fn started(&self) -> Vec<String> {
        self.started.lock().unwrap().clone()
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }
}

#[async_trait]
impl SubagentRunner<Parent> for FakeRunner {
    fn requested_subagent_session_ids(
        &self,
        _parent_session: &Parent,
        message: &Message,
    ) -> Vec<String> {
        message
            .as_concat_text()
            .strip_prefix("start ")
            .map(|ids| ids.split(' ').map(str::to_string).collect())
            .unwrap_or_default()
    }

    async fn start(
        &self,
        _parent_session: &Parent,
        subagent_session_id: &str,
        _cancel: CancellationToken,
    ) -> OperationStream<'static, SubagentUpdate> {
        self.started
            .lock()
            .unwrap()
            .push(subagent_session_id.to_string());
        let child = self
            .children
            .lock()
            .unwrap()
            .remove(subagent_session_id)
            .expect("unknown subagent");
        match child {
            Child::Finished(outcome) => Box::pin(stream::once(future::ready(
                SubagentUpdate::Finished(outcome),
            ))),
            Child::Running(rx) => {
                self.events
                    .lock()
                    .unwrap()
                    .push(format!("started {subagent_session_id}"));
                Box::pin(stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.map(|update| (update, rx))
                }))
            }
        }
    }

    fn on_subagent_finished(
        &self,
        subagent_session_id: &str,
        _outcome: &SubagentOutcome,
        remaining: usize,
        _emit: &Emitter,
    ) {
        self.events.lock().unwrap().push(format!(
            "finished {subagent_session_id}, {remaining} remaining"
        ));
    }
}

fn emitter() -> Emitter {
    let (tx, _rx) = mpsc::unbounded_channel();
    Emitter::new(tx, CancellationToken::new())
}

fn watched_emitter() -> (Emitter, mpsc::UnboundedReceiver<AgentEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (Emitter::new(tx, CancellationToken::new()), rx)
}

fn shown(rx: &mut mpsc::UnboundedReceiver<AgentEvent>) -> Vec<String> {
    std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|event| match event {
            AgentEvent::Message(message) => Some(message.as_concat_text()),
            _ => None,
        })
        .collect()
}

fn start(subagent_session_ids: &[&str]) -> Message {
    Message::user().with_text(format!("start {}", subagent_session_ids.join(" ")))
}

fn appended(effects: Vec<ConversationEffect>) -> Vec<Message> {
    effects
        .into_iter()
        .map(|effect| match effect {
            ConversationEffect::AppendMessage(message) => message,
            _ => panic!("expected an appended message"),
        })
        .collect()
}

fn texts(messages: &[Message]) -> Vec<String> {
    messages.iter().map(Message::as_concat_text).collect()
}

fn operation(runner: &Arc<FakeRunner>) -> SubagentOperation<Parent> {
    SubagentOperation::new(runner.clone())
}

async fn run_once(operation: &SubagentOperation<Parent>, messages: &[Message]) -> Vec<Message> {
    run_once_with(operation, messages, &emitter()).await
}

async fn run_once_with(
    operation: &SubagentOperation<Parent>,
    messages: &[Message],
    emit: &Emitter,
) -> Vec<Message> {
    let conversation = Conversation::new_unvalidated(messages.to_vec());
    let result =
        Operation::<Parent, ConversationEffect>::run(operation, &Parent, &conversation, emit)
            .await
            .unwrap();
    match result {
        OperationResult::NotApplicable => Vec::new(),
        OperationResult::Applied(result) => appended(result.effects),
    }
}

fn interrupt(operation: &SubagentOperation<Parent>, messages: &[Message]) {
    let conversation = Conversation::new_unvalidated(messages.to_vec());
    let run =
        Operation::<Parent, ConversationEffect>::run(operation, &Parent, &conversation, &emitter())
            .now_or_never();
    assert!(run.is_none());
}

async fn stop(operation: &SubagentOperation<Parent>, messages: &[Message]) -> Vec<Message> {
    stop_with(operation, messages, &emitter()).await
}

async fn stop_with(
    operation: &SubagentOperation<Parent>,
    messages: &[Message],
    emit: &Emitter,
) -> Vec<Message> {
    let conversation = Conversation::new_unvalidated(messages.to_vec());
    appended(
        Operation::<Parent, ConversationEffect>::finalize_cancellation(
            operation,
            &Parent,
            &conversation,
            emit,
        )
        .await,
    )
}

#[tokio::test]
async fn a_finished_subagent_is_delivered_without_running() {
    let runner = Arc::new(FakeRunner::default());
    runner.finished("a", SubagentOutcome::Completed("done".into()));
    let operation = operation(&runner);
    let mut messages = vec![start(&["a"])];

    let delivered = run_once(&operation, &messages).await;

    assert_eq!(texts(&delivered), ["Subagent a completed: done"]);
    assert!(!delivered[0].is_user_visible());
    assert!(delivered[0].is_agent_visible());
    assert_eq!(runner.events(), ["finished a, 0 remaining"]);

    messages.extend(delivered);
    assert!(run_once(&operation, &messages).await.is_empty());
    assert_eq!(runner.started(), ["a"]);
}

#[tokio::test]
async fn running_subagents_are_delivered_one_per_pass_and_started_once() {
    let runner = Arc::new(FakeRunner::default());
    let a = runner.running("a");
    let b = runner.running("b");
    let operation = operation(&runner);
    let mut messages = vec![start(&["a", "b"])];

    b.finish(SubagentOutcome::Completed("b done".into()));
    let delivered = run_once(&operation, &messages).await;
    assert_eq!(texts(&delivered), ["Subagent b completed: b done"]);
    messages.extend(delivered);

    a.finish(SubagentOutcome::Failed("boom".into()));
    let delivered = run_once(&operation, &messages).await;
    assert_eq!(texts(&delivered), ["Subagent a failed: boom"]);
    messages.extend(delivered);

    assert!(run_once(&operation, &messages).await.is_empty());
    assert_eq!(runner.started(), ["a", "b"]);
    assert_eq!(
        runner.events(),
        [
            "started a",
            "started b",
            "finished b, 1 remaining",
            "finished a, 0 remaining",
        ]
    );
}

#[tokio::test]
async fn an_already_finished_subagent_does_not_hold_back_the_others() {
    let runner = Arc::new(FakeRunner::default());
    runner.finished("a", SubagentOutcome::Completed("a done".into()));
    let _b = runner.running("b");

    let delivered = run_once(&operation(&runner), &[start(&["a", "b"])]).await;

    assert_eq!(texts(&delivered), ["Subagent a completed: a done"]);
    assert_eq!(runner.started(), ["a", "b"]);
    assert_eq!(runner.events(), ["started b", "finished a, 1 remaining"]);
}

#[tokio::test]
async fn a_subagent_cancelled_elsewhere_is_reported_and_not_restarted() {
    let runner = Arc::new(FakeRunner::default());
    let a = runner.running("a");
    let operation = operation(&runner);
    let mut messages = vec![start(&["a"])];

    a.finish(SubagentOutcome::Cancelled);
    let delivered = run_once(&operation, &messages).await;

    assert_eq!(
        texts(&delivered),
        ["Subagent a was cancelled before it finished."]
    );
    messages.extend(delivered);
    assert!(run_once(&operation, &messages).await.is_empty());
    assert_eq!(runner.started(), ["a"]);
}

#[tokio::test]
async fn stop_delivers_finished_subagents_and_cancels_running_and_queued_children() {
    let runner = Arc::new(FakeRunner::default());
    let a = runner.running("a");
    let b = runner.running("b");
    runner.finished("c", SubagentOutcome::Completed("c done".into()));
    let operation = operation(&runner).with_max_concurrency(NonZeroUsize::new(2).unwrap());
    let mut messages = vec![start(&["a", "b", "c"])];

    interrupt(&operation, &messages);
    assert_eq!(runner.started(), ["a", "b"]);
    a.finish(SubagentOutcome::Completed("a done".into()));
    b.finish(SubagentOutcome::Cancelled);
    let saved = stop(&operation, &messages).await;

    assert!(b.is_closed());
    assert_eq!(
        texts(&saved),
        [
            "Subagent a completed: a done",
            "Subagent b was cancelled before it finished.\n\
             Subagent c was cancelled before it finished.",
        ]
    );
    assert_eq!(
        runner.events(),
        ["started a", "started b", "finished a, 2 remaining"]
    );

    messages.extend(saved);
    assert!(run_once(&operation, &messages).await.is_empty());
    assert_eq!(runner.started(), ["a", "b"]);
    assert!(stop(&operation, &messages).await.is_empty());
}

#[tokio::test]
async fn a_subagent_started_twice_before_it_finishes_runs_once() {
    let runner = Arc::new(FakeRunner::default());
    runner.finished("x", SubagentOutcome::Completed("done".into()));
    let operation = operation(&runner);
    let mut messages = vec![start(&["x"]), start(&["x"])];

    let delivered = run_once(&operation, &messages).await;
    assert_eq!(texts(&delivered), ["Subagent x completed: done"]);
    messages.extend(delivered);

    assert!(run_once(&operation, &messages).await.is_empty());
    assert_eq!(runner.started(), ["x"]);
}

#[tokio::test]
async fn a_failed_subagent_started_again_runs_until_it_finishes() {
    let runner = Arc::new(FakeRunner::default());
    runner.finished("x", SubagentOutcome::Failed("boom\ndetails".into()));
    let operation = operation(&runner);
    let mut messages = vec![start(&["x"])];

    let delivered = run_once(&operation, &messages).await;
    assert_eq!(texts(&delivered), ["Subagent x failed: boom\ndetails"]);
    messages.extend(delivered);
    assert!(run_once(&operation, &messages).await.is_empty());

    messages.push(start(&["x"]));
    runner.finished("x", SubagentOutcome::Completed("done".into()));
    let delivered = run_once(&operation, &messages).await;
    assert_eq!(texts(&delivered), ["Subagent x completed: done"]);
    messages.extend(delivered);

    assert!(run_once(&operation, &messages).await.is_empty());
    assert_eq!(runner.started(), ["x", "x"]);
}

#[tokio::test]
async fn reads_the_notes_goose_saves_today() {
    let mut delivered = Message::user()
        .with_text("Subagent a completed: done")
        .with_visibility(false, true);
    delivered
        .metadata
        .set_operation_note("foreground_subagent", "delivered", json!("a"));
    let mut cancelled = Message::user()
        .with_text("Subagent b was cancelled")
        .with_visibility(false, true);
    cancelled
        .metadata
        .set_operation_note("foreground_subagent", "cancelled", json!(["b"]));
    let messages = vec![start(&["a", "b", "c"]), delivered, cancelled];

    let runner = Arc::new(FakeRunner::default());
    runner.finished("c", SubagentOutcome::Completed("c done".into()));
    let delivered = run_once(&operation(&runner), &messages).await;

    assert_eq!(texts(&delivered), ["Subagent c completed: c done"]);
    assert_eq!(runner.started(), ["c"]);
}

#[tokio::test]
async fn messages_from_a_running_subagent_reach_the_parent_before_its_result() {
    let runner = Arc::new(FakeRunner::default());
    let a = runner.running("a");
    let operation = operation(&runner);
    let (emit, mut rx) = watched_emitter();

    a.to_parent("may I run cargo test?");
    a.finish(SubagentOutcome::Completed("done".into()));
    let delivered = run_once_with(&operation, &[start(&["a"])], &emit).await;

    assert_eq!(shown(&mut rx), ["may I run cargo test?"]);
    assert_eq!(texts(&delivered), ["Subagent a completed: done"]);
}

#[tokio::test]
async fn a_subagent_whose_updates_end_without_an_outcome_has_failed() {
    let runner = Arc::new(FakeRunner::default());
    drop(runner.running("a"));

    let delivered = run_once(&operation(&runner), &[start(&["a"])]).await;

    assert_eq!(
        texts(&delivered),
        ["Subagent a failed: it ended without an outcome"]
    );
}

#[tokio::test]
async fn updates_after_the_outcome_are_ignored() {
    let runner = Arc::new(FakeRunner::default());
    let a = runner.running("a");
    let operation = operation(&runner);
    let (emit, mut rx) = watched_emitter();

    a.finish(SubagentOutcome::Completed("done".into()));
    a.to_parent("too late");
    a.finish(SubagentOutcome::Failed("again".into()));
    let delivered = run_once_with(&operation, &[start(&["a"])], &emit).await;

    assert_eq!(texts(&delivered), ["Subagent a completed: done"]);
    assert!(shown(&mut rx).is_empty());
    assert!(a.is_closed());
}

#[tokio::test]
async fn stop_skips_messages_queued_ahead_of_a_finished_result() {
    let runner = Arc::new(FakeRunner::default());
    let a = runner.running("a");
    let operation = operation(&runner);
    let messages = [start(&["a"])];
    let (emit, mut rx) = watched_emitter();

    interrupt(&operation, &messages);
    a.to_parent("may I run cargo test?");
    a.finish(SubagentOutcome::Completed("done".into()));
    let saved = stop_with(&operation, &messages, &emit).await;

    assert_eq!(texts(&saved), ["Subagent a completed: done"]);
    assert!(shown(&mut rx).is_empty());
}

#[tokio::test]
async fn concurrency_limit_queues_subagents_until_a_slot_is_available() {
    let runner = Arc::new(FakeRunner::default());
    let a = runner.running("a");
    let b = runner.running("b");
    let c = runner.running("c");
    let operation = operation(&runner).with_max_concurrency(NonZeroUsize::new(2).unwrap());
    let mut messages = vec![start(&["a", "b", "c"])];

    interrupt(&operation, &messages);
    assert_eq!(runner.started(), ["a", "b"]);

    b.finish(SubagentOutcome::Failed("boom".into()));
    messages.extend(run_once(&operation, &messages).await);
    assert_eq!(runner.started(), ["a", "b"]);

    interrupt(&operation, &messages);
    assert_eq!(runner.started(), ["a", "b", "c"]);

    c.finish(SubagentOutcome::Completed("c done".into()));
    messages.extend(run_once(&operation, &messages).await);
    a.finish(SubagentOutcome::Completed("a done".into()));
    messages.extend(run_once(&operation, &messages).await);

    assert!(run_once(&operation, &messages).await.is_empty());
    assert_eq!(runner.started(), ["a", "b", "c"]);
}
