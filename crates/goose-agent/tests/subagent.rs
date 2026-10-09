use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use futures::future;
use futures::FutureExt;
use goose_agent::operation::{
    ConversationEffect, Emitter, Operation, OperationFuture, OperationResult,
};
use goose_agent::subagent::{ForegroundSubagentOperation, SubagentOutcome, SubagentRunner};
use goose_provider_types::conversation::{message::Message, Conversation};
use serde_json::json;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct Parent;

enum Child {
    Finished(SubagentOutcome),
    Running(oneshot::Receiver<SubagentOutcome>),
}

#[derive(Default)]
struct FakeRunner {
    children: Mutex<HashMap<String, Child>>,
    started: Mutex<Vec<String>>,
    events: Mutex<Vec<String>>,
    started_subagent_session_ids: Vec<String>,
    blocked: bool,
}

impl FakeRunner {
    fn finished(&self, subagent_session_id: &str, outcome: SubagentOutcome) {
        self.children
            .lock()
            .unwrap()
            .insert(subagent_session_id.to_string(), Child::Finished(outcome));
    }

    fn running(&self, subagent_session_id: &str) -> oneshot::Sender<SubagentOutcome> {
        let (tx, rx) = oneshot::channel();
        self.children
            .lock()
            .unwrap()
            .insert(subagent_session_id.to_string(), Child::Running(rx));
        tx
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
    fn started_subagent_session_ids(
        &self,
        _parent_session: &Parent,
        _conversation: &Conversation,
    ) -> Vec<String> {
        self.started_subagent_session_ids.clone()
    }

    fn ready(&self, _parent_session: &Parent, _conversation: &Conversation) -> Result<bool> {
        Ok(!self.blocked)
    }

    async fn start(
        &self,
        _parent_session: &Parent,
        subagent_session_id: &str,
        _emit: &Emitter,
    ) -> OperationFuture<'static, SubagentOutcome> {
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
            Child::Finished(outcome) => Box::pin(future::ready(outcome)),
            Child::Running(rx) => {
                self.events
                    .lock()
                    .unwrap()
                    .push(format!("started {subagent_session_id}"));
                Box::pin(async move { rx.await.unwrap_or(SubagentOutcome::Cancelled) })
            }
        }
    }

    async fn on_subagent_finished(
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

fn kickoff() -> Message {
    Message::user().with_text("go")
}

fn fake(subagent_session_ids: &[&str]) -> FakeRunner {
    FakeRunner {
        started_subagent_session_ids: subagent_session_ids
            .iter()
            .map(|id| id.to_string())
            .collect(),
        ..FakeRunner::default()
    }
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

fn operation(runner: &Arc<FakeRunner>) -> ForegroundSubagentOperation<Parent> {
    ForegroundSubagentOperation::new(runner.clone())
}

async fn run_once(
    operation: &ForegroundSubagentOperation<Parent>,
    messages: &[Message],
) -> Vec<Message> {
    let conversation = Conversation::new_unvalidated(messages.to_vec());
    let result =
        Operation::<Parent, ConversationEffect>::run(operation, &Parent, &conversation, &emitter())
            .await
            .unwrap();
    match result {
        OperationResult::NotApplicable => Vec::new(),
        OperationResult::Applied(result) => appended(result.effects),
    }
}

#[tokio::test]
async fn nothing_starts_until_the_runner_is_ready() {
    let runner = Arc::new(FakeRunner {
        blocked: true,
        ..fake(&["a"])
    });
    runner.finished("a", SubagentOutcome::Completed("done".into()));

    let delivered = run_once(&operation(&runner), &[kickoff()]).await;

    assert!(delivered.is_empty());
    assert!(runner.started().is_empty());
    assert!(runner.events().is_empty());
}

#[tokio::test]
async fn a_finished_subagent_is_delivered_without_running() {
    let runner = Arc::new(fake(&["a"]));
    runner.finished("a", SubagentOutcome::Completed("done".into()));
    let operation = operation(&runner);
    let mut messages = vec![kickoff()];

    let delivered = run_once(&operation, &messages).await;

    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].as_concat_text(), "Subagent a completed: done");
    assert!(!delivered[0].is_user_visible());
    assert!(delivered[0].is_agent_visible());
    assert_eq!(runner.events(), ["finished a, 0 remaining"]);

    messages.extend(delivered);
    assert!(run_once(&operation, &messages).await.is_empty());
    assert_eq!(runner.started(), ["a"]);
}

#[tokio::test]
async fn running_subagents_are_delivered_one_per_pass_and_started_once() {
    let runner = Arc::new(fake(&["a", "b"]));
    let a = runner.running("a");
    let b = runner.running("b");
    let operation = operation(&runner);
    let mut messages = vec![kickoff()];

    b.send(SubagentOutcome::Completed("b done".into())).unwrap();
    let delivered = run_once(&operation, &messages).await;
    assert_eq!(delivered.len(), 1);
    assert_eq!(
        delivered[0].as_concat_text(),
        "Subagent b completed: b done"
    );
    messages.extend(delivered);

    a.send(SubagentOutcome::Failed("boom".into())).unwrap();
    let delivered = run_once(&operation, &messages).await;
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].as_concat_text(), "Subagent a failed: boom");
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
    let runner = Arc::new(fake(&["a", "b"]));
    runner.finished("a", SubagentOutcome::Completed("a done".into()));
    let _b = runner.running("b");

    let delivered = run_once(&operation(&runner), &[kickoff()]).await;

    assert_eq!(delivered.len(), 1);
    assert_eq!(
        delivered[0].as_concat_text(),
        "Subagent a completed: a done"
    );
    assert_eq!(runner.started(), ["a", "b"]);
    assert_eq!(runner.events(), ["started b", "finished a, 1 remaining"]);
}

#[tokio::test]
async fn a_cancelled_subagent_does_not_end_the_wait_for_the_others() {
    let runner = Arc::new(fake(&["a", "b"]));
    let a = runner.running("a");
    let b = runner.running("b");
    let operation = operation(&runner);
    let messages = [kickoff()];
    a.send(SubagentOutcome::Cancelled).unwrap();

    let (delivered, ()) = tokio::join!(run_once(&operation, &messages), async {
        tokio::task::yield_now().await;
        b.send(SubagentOutcome::Completed("b done".into())).unwrap();
    });

    assert_eq!(delivered.len(), 1);
    assert_eq!(
        delivered[0].as_concat_text(),
        "Subagent b completed: b done"
    );
}

#[tokio::test]
async fn a_cancelled_subagent_stays_pending() {
    let runner = Arc::new(fake(&["a"]));
    let a = runner.running("a");
    let operation = operation(&runner);
    let messages = vec![kickoff()];

    a.send(SubagentOutcome::Cancelled).unwrap();
    assert!(run_once(&operation, &messages).await.is_empty());
    assert_eq!(runner.events(), ["started a"]);

    runner.finished("a", SubagentOutcome::Completed("done".into()));
    let delivered = run_once(&operation, &messages).await;

    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].as_concat_text(), "Subagent a completed: done");
    assert_eq!(runner.started(), ["a", "a"]);
}

#[tokio::test]
async fn stop_delivers_finished_subagents_and_cancels_the_rest() {
    let runner = Arc::new(fake(&["a", "b"]));
    let a = runner.running("a");
    let b = runner.running("b");
    let operation = operation(&runner);
    let mut messages = vec![kickoff()];
    let conversation = Conversation::new_unvalidated(messages.clone());

    let interrupted = Operation::<Parent, ConversationEffect>::run(
        &operation,
        &Parent,
        &conversation,
        &emitter(),
    )
    .now_or_never();
    assert!(interrupted.is_none());

    a.send(SubagentOutcome::Completed("a done".into())).unwrap();
    let effects = Operation::<Parent, ConversationEffect>::cancel(
        &operation,
        &Parent,
        &conversation,
        &emitter(),
    )
    .await;
    let saved = appended(effects);

    assert_eq!(saved.len(), 2);
    assert_eq!(saved[0].as_concat_text(), "Subagent a completed: a done");
    assert_eq!(
        saved[1].as_concat_text(),
        "Subagent b was cancelled before it finished and will not run again."
    );
    assert!(b.is_closed());
    assert_eq!(
        runner.events(),
        ["started a", "started b", "finished a, 1 remaining",]
    );

    messages.extend(saved);
    assert!(run_once(&operation, &messages).await.is_empty());
    assert_eq!(runner.started(), ["a", "b"]);

    let stopped_again = Operation::<Parent, ConversationEffect>::cancel(
        &operation,
        &Parent,
        &Conversation::new_unvalidated(messages),
        &emitter(),
    )
    .await;
    assert!(stopped_again.is_empty());
}

#[tokio::test]
async fn a_repeated_id_starts_once() {
    let runner = Arc::new(fake(&["x", "x"]));
    runner.finished("x", SubagentOutcome::Failed("boom\ndetails".into()));

    let delivered = run_once(&operation(&runner), &[kickoff()]).await;

    assert_eq!(delivered.len(), 1);
    assert_eq!(
        delivered[0].as_concat_text(),
        "Subagent x failed: boom\ndetails"
    );
    assert_eq!(runner.started(), ["x"]);
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
    let messages = vec![kickoff(), delivered, cancelled];

    let runner = Arc::new(fake(&["a", "b", "c"]));
    runner.finished("c", SubagentOutcome::Completed("c done".into()));
    let delivered = run_once(&operation(&runner), &messages).await;

    assert_eq!(delivered.len(), 1);
    assert_eq!(
        delivered[0].as_concat_text(),
        "Subagent c completed: c done"
    );
    assert_eq!(runner.started(), ["c"]);
}
