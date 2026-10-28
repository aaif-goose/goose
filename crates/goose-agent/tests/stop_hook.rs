use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::{
    events::AgentEvent,
    operation::{ConversationEffect, Emitter, Operation, OperationResult, RunStatus},
    stop_hook::{StopHook, StopHookDecision, StopHookOperation},
};
use goose_provider_types::conversation::{
    message::{Message, MessageContent, SystemNotificationType},
    Conversation,
};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy)]
enum Reply {
    Allow,
    Deny(&'static str),
    Fail,
}

struct FakeHook {
    reply: Reply,
    calls: AtomicUsize,
}

impl FakeHook {
    fn new(reply: Reply) -> Arc<Self> {
        Arc::new(Self {
            reply,
            calls: AtomicUsize::new(0),
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl StopHook<()> for FakeHook {
    async fn on_stop(
        &self,
        _session: &(),
        _conversation: &Conversation,
    ) -> Result<StopHookDecision> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.reply {
            Reply::Allow => Ok(StopHookDecision::Allow),
            Reply::Deny(reason) => Ok(StopHookDecision::Deny {
                reason: reason.to_string(),
                user_message: Some(notice("hook denied")),
            }),
            Reply::Fail => anyhow::bail!("stop hook crashed"),
        }
    }
}

fn notice(text: &str) -> Message {
    Message::assistant().with_system_notification(SystemNotificationType::InlineMessage, text)
}

fn notification_text(message: &Message) -> String {
    message
        .content
        .iter()
        .find_map(|content| match content {
            MessageContent::SystemNotification(notification) => Some(notification.msg.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

struct Outcome {
    status: Option<RunStatus>,
    appended: Vec<Message>,
    shown: Vec<String>,
}

async fn run(operation: &StopHookOperation<()>, messages: Vec<Message>) -> Result<Outcome> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    let conversation = Conversation::new_unvalidated(messages);
    let result =
        Operation::<(), ConversationEffect>::run(operation, &(), &conversation, &emit).await?;
    let shown = std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|event| match event {
            AgentEvent::Message(message) => Some(notification_text(&message)),
            _ => None,
        })
        .collect();
    let (status, appended) = match result {
        OperationResult::NotApplicable => (None, Vec::new()),
        OperationResult::Applied(result) => (
            Some(result.status),
            result
                .effects
                .into_iter()
                .map(|effect| match effect {
                    ConversationEffect::AppendMessage(message) => message,
                    _ => panic!("expected an appended message"),
                })
                .collect(),
        ),
    };
    Ok(Outcome {
        status,
        appended,
        shown,
    })
}

fn finished_turn() -> Vec<Message> {
    vec![
        Message::user().with_text("hi"),
        Message::assistant().with_text("done"),
    ]
}

fn denied_once_then_finished() -> Vec<Message> {
    let mut messages = finished_turn();
    messages.push(denial_message());
    messages.push(Message::assistant().with_text("done again"));
    messages
}

fn denial_message() -> Message {
    let mut message = Message::user()
        .with_text("keep going")
        .with_visibility(false, true);
    message
        .metadata
        .set_operation_note("stop_hook", "denied", json!(true));
    message
}

fn is_denied(message: &Message) -> bool {
    message
        .metadata
        .operation_note("stop_hook", "denied")
        .is_some()
}

#[tokio::test]
async fn does_nothing_before_the_turn_ends() -> Result<()> {
    let hook = FakeHook::new(Reply::Deny("keep going"));

    let outcome = run(
        &StopHookOperation::new(hook.clone()),
        vec![Message::user().with_text("hi")],
    )
    .await?;

    assert!(outcome.status.is_none());
    assert_eq!(hook.calls(), 0);
    Ok(())
}

#[tokio::test]
async fn allow_ends_the_turn() -> Result<()> {
    let hook = FakeHook::new(Reply::Allow);

    let outcome = run(&StopHookOperation::new(hook.clone()), finished_turn()).await?;

    assert_eq!(outcome.status, Some(RunStatus::Yielded));
    assert!(outcome.appended.is_empty());
    assert_eq!(hook.calls(), 1);
    Ok(())
}

#[tokio::test]
async fn deny_adds_a_marked_denial_message_and_shows_the_user_message() -> Result<()> {
    let operation = StopHookOperation::new(FakeHook::new(Reply::Deny("keep going")));

    let outcome = run(&operation, finished_turn()).await?;

    assert_eq!(outcome.status, Some(RunStatus::Continuing));
    let [denial] = outcome.appended.as_slice() else {
        panic!("expected one denial message");
    };
    assert_eq!(denial.as_concat_text(), "keep going");
    assert!(is_denied(denial));
    assert!(!denial.is_user_visible());
    assert!(denial.is_agent_visible());
    assert_eq!(outcome.shown, vec!["hook denied"]);
    Ok(())
}

#[tokio::test]
async fn past_the_deny_limit_the_turn_ends_with_the_limit_message_if_set() -> Result<()> {
    let limited = || {
        StopHookOperation::new(FakeHook::new(Reply::Deny("keep going"))).with_deny_limit_per_turn(1)
    };

    for (operation, expected) in [
        (
            limited().with_deny_limit_message(|| notice("deny limit reached")),
            vec!["deny limit reached"],
        ),
        (limited(), vec![]),
    ] {
        let outcome = run(&operation, denied_once_then_finished()).await?;

        assert_eq!(outcome.status, Some(RunStatus::Yielded));
        let appended: Vec<_> = outcome.appended.iter().map(notification_text).collect();
        assert_eq!(appended, expected);
        assert_eq!(outcome.shown, expected);
    }
    Ok(())
}

#[tokio::test]
async fn the_deny_limit_counts_only_the_current_turn() -> Result<()> {
    let operation = StopHookOperation::new(FakeHook::new(Reply::Deny("keep going")))
        .with_deny_limit_per_turn(1);
    let mut messages = denied_once_then_finished();
    messages.extend(finished_turn());

    let outcome = run(&operation, messages).await?;

    assert_eq!(outcome.status, Some(RunStatus::Continuing));
    assert!(is_denied(&outcome.appended[0]));
    Ok(())
}

#[tokio::test]
async fn a_failing_hook_fails_the_operation() {
    let operation = StopHookOperation::new(FakeHook::new(Reply::Fail));

    let error = run(&operation, finished_turn()).await.err().unwrap();

    assert_eq!(error.to_string(), "stop hook crashed");
}
