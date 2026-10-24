use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use async_trait::async_trait;
use goose_agent::{
    events::AgentEvent,
    hook::{Hook, HookDecision},
    operation::{ConversationEffect, Emitter, Operation, OperationResult, RunStatus},
    stop_hook::{Stop, StopHookOperation, DENIED},
};
use goose_provider_types::conversation::{
    message::{Message, MessageContent, SystemNotificationType},
    Conversation,
};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct FakeHook {
    deny_with: Option<&'static str>,
    calls: AtomicUsize,
}

impl FakeHook {
    fn allowing() -> Arc<Self> {
        Arc::new(Self {
            deny_with: None,
            calls: AtomicUsize::new(0),
        })
    }

    fn denying(reason: &'static str) -> Arc<Self> {
        Arc::new(Self {
            deny_with: Some(reason),
            calls: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl<'a> Hook<(), Stop<'a>> for FakeHook {
    async fn decide(&self, _session: &(), _event: &Stop<'a>) -> HookDecision {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let Some(reason) = self.deny_with else {
            return HookDecision::Allow;
        };
        HookDecision::Deny {
            reason: reason.to_string(),
            user_message: Some(notice("hook denied")),
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

async fn run(operation: &StopHookOperation<()>, messages: Vec<Message>) -> Outcome {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    let conversation = Conversation::new_unvalidated(messages);
    let result = Operation::<(), ConversationEffect>::run(operation, &(), &conversation, &emit)
        .await
        .unwrap();
    let shown = std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|event| match event {
            AgentEvent::Message(message) => Some(notification_text(&message)),
            _ => None,
        })
        .collect();
    match result {
        OperationResult::NotApplicable => Outcome {
            status: None,
            appended: Vec::new(),
            shown,
        },
        OperationResult::Applied(result) => Outcome {
            status: Some(result.status),
            appended: result
                .effects
                .into_iter()
                .map(|effect| match effect {
                    ConversationEffect::AppendMessage(message) => message,
                    _ => panic!("expected an appended message"),
                })
                .collect(),
            shown,
        },
    }
}

fn denial_message() -> Message {
    let mut message = Message::user()
        .with_text("keep going")
        .with_visibility(false, true);
    message
        .metadata
        .set_operation_note("stop_hook", DENIED, json!(true));
    message
}

fn is_denied(message: &Message) -> bool {
    message
        .metadata
        .operation_note("stop_hook", DENIED)
        .is_some()
}

#[tokio::test]
async fn does_nothing_before_the_turn_ends() {
    let hook = FakeHook::denying("keep going");
    let operation = StopHookOperation::new(vec![hook.clone()]);

    let outcome = run(&operation, vec![Message::user().with_text("hi")]).await;

    assert!(outcome.status.is_none());
    assert_eq!(hook.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn allow_ends_the_turn() {
    let hook = FakeHook::allowing();
    let operation = StopHookOperation::new(vec![hook.clone()]);

    let outcome = run(
        &operation,
        vec![
            Message::user().with_text("hi"),
            Message::assistant().with_text("done"),
        ],
    )
    .await;

    assert_eq!(outcome.status, Some(RunStatus::Yielded));
    assert!(outcome.appended.is_empty());
    assert_eq!(hook.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn deny_adds_a_marked_denial_message_and_shows_the_user_message() {
    let operation = StopHookOperation::new(vec![FakeHook::denying("keep going")]);

    let outcome = run(
        &operation,
        vec![
            Message::user().with_text("hi"),
            Message::assistant().with_text("done"),
        ],
    )
    .await;

    assert_eq!(outcome.status, Some(RunStatus::Continuing));
    assert_eq!(outcome.appended.len(), 1);
    assert_eq!(outcome.appended[0].as_concat_text(), "keep going");
    assert!(is_denied(&outcome.appended[0]));
    assert!(!outcome.appended[0].is_user_visible());
    assert!(outcome.appended[0].is_agent_visible());
    assert_eq!(outcome.shown, vec!["hook denied"]);
}

#[tokio::test]
async fn the_first_denying_hook_wins_and_later_hooks_are_skipped() {
    let first = FakeHook::allowing();
    let second = FakeHook::denying("from second");
    let third = FakeHook::denying("from third");
    let operation = StopHookOperation::new(vec![first.clone(), second.clone(), third.clone()]);

    let outcome = run(
        &operation,
        vec![
            Message::user().with_text("hi"),
            Message::assistant().with_text("done"),
        ],
    )
    .await;

    assert_eq!(outcome.appended[0].as_concat_text(), "from second");
    assert_eq!(first.calls.load(Ordering::SeqCst), 1);
    assert_eq!(second.calls.load(Ordering::SeqCst), 1);
    assert_eq!(third.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn without_hooks_the_turn_ends() {
    let operation = StopHookOperation::<()>::new(Vec::new());

    let outcome = run(
        &operation,
        vec![
            Message::user().with_text("hi"),
            Message::assistant().with_text("done"),
        ],
    )
    .await;

    assert_eq!(outcome.status, Some(RunStatus::Yielded));
    assert!(outcome.appended.is_empty());
}

#[tokio::test]
async fn past_the_deny_limit_the_turn_ends_with_the_limit_message() {
    let operation = StopHookOperation::new(vec![FakeHook::denying("keep going")])
        .with_deny_limit_per_turn(1)
        .with_deny_limit_message(|| notice("deny limit reached"));

    let outcome = run(
        &operation,
        vec![
            Message::user().with_text("hi"),
            Message::assistant().with_text("done"),
            denial_message(),
            Message::assistant().with_text("done again"),
        ],
    )
    .await;

    assert_eq!(outcome.status, Some(RunStatus::Yielded));
    assert_eq!(outcome.appended.len(), 1);
    assert_eq!(
        notification_text(&outcome.appended[0]),
        "deny limit reached"
    );
    assert_eq!(outcome.shown, vec!["deny limit reached"]);
}

#[tokio::test]
async fn past_the_deny_limit_without_a_message_the_turn_ends_quietly() {
    let operation =
        StopHookOperation::new(vec![FakeHook::denying("keep going")]).with_deny_limit_per_turn(1);

    let outcome = run(
        &operation,
        vec![
            Message::user().with_text("hi"),
            Message::assistant().with_text("done"),
            denial_message(),
            Message::assistant().with_text("done again"),
        ],
    )
    .await;

    assert_eq!(outcome.status, Some(RunStatus::Yielded));
    assert!(outcome.appended.is_empty());
    assert!(outcome.shown.is_empty());
}

#[tokio::test]
async fn the_deny_limit_counts_only_the_current_turn() {
    let operation =
        StopHookOperation::new(vec![FakeHook::denying("keep going")]).with_deny_limit_per_turn(1);

    let outcome = run(
        &operation,
        vec![
            Message::user().with_text("first"),
            Message::assistant().with_text("done"),
            denial_message(),
            Message::assistant().with_text("done again"),
            Message::user().with_text("second"),
            Message::assistant().with_text("done"),
        ],
    )
    .await;

    assert_eq!(outcome.status, Some(RunStatus::Continuing));
    assert!(is_denied(&outcome.appended[0]));
}
