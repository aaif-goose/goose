#![cfg(not(target_arch = "wasm32"))]

use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use async_trait::async_trait;
use goose_agent::{
    events::AgentEvent,
    inference::{InferenceEffect, InferenceRunner},
    machine::{MachineSession, StateMachine, Step},
    operation::{Emitter, MachineEffect, RunStatus},
};
use goose_provider_types::{
    base::{MessageStream, Provider},
    conversation::{
        message::{Message, MessageContent},
        token_usage::ProviderUsage,
        Conversation,
    },
    errors::ProviderError,
    model::ModelConfig,
};
use rmcp::model::Tool;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct Session(Conversation);
impl MachineSession for Session {
    fn id(&self) -> &str {
        "session"
    }
    fn conversation(&self) -> Option<&Conversation> {
        Some(&self.0)
    }
}

struct Effect(Option<Message>);
impl From<Message> for Effect {
    fn from(message: Message) -> Self {
        Self(Some(message))
    }
}
impl MachineEffect for Effect {
    fn ensure_message_ids(&mut self) {}
}
impl InferenceEffect for Effect {
    fn record_usage(_: ProviderUsage) -> Self {
        Self(None)
    }
}

enum Reply {
    Failure,
    Backoff,
    Answer,
    Partial,
    PendingCreation,
    PendingStream,
}
struct ScriptedProvider {
    replies: Mutex<VecDeque<Reply>>,
    calls: AtomicUsize,
}

#[async_trait]
impl Provider for ScriptedProvider {
    fn get_name(&self) -> &str {
        "scripted"
    }
    fn provider_session_id(&self) -> Option<String> {
        (self.calls.load(Ordering::SeqCst) > 0).then(|| "provider-session".into())
    }
    async fn stream(
        &self,
        _: &ModelConfig,
        _: &str,
        _: &[Message],
        _: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected replay");
        match reply {
            reply @ (Reply::Failure | Reply::Backoff) => Err(ProviderError::RateLimitExceeded {
                details: "busy".into(),
                retry_delay: Some(if matches!(reply, Reply::Backoff) {
                    Duration::from_secs(60)
                } else {
                    Duration::ZERO
                }),
            }),
            Reply::Answer => Ok(Box::pin(futures::stream::iter([Ok((
                Some(Message::assistant().with_text("answer")),
                None,
            ))]))),
            Reply::Partial => Ok(Box::pin(futures::stream::iter([
                Ok((Some(Message::assistant().with_text("partial")), None)),
                Err(ProviderError::NetworkError("disconnected".into())),
            ]))),
            Reply::PendingCreation => futures::future::pending().await,
            Reply::PendingStream => Ok(Box::pin(futures::stream::pending())),
        }
    }
}

fn setup(
    replies: Vec<Reply>,
    budget: Duration,
) -> (
    Arc<ScriptedProvider>,
    StateMachine<'static, Session, Effect>,
    Emitter,
    mpsc::UnboundedReceiver<AgentEvent>,
) {
    let provider = Arc::new(ScriptedProvider {
        replies: Mutex::new(replies.into()),
        calls: AtomicUsize::new(0),
    });
    let cancel = CancellationToken::new();
    let machine = StateMachine::new(
        vec![Step::Inference(Arc::new(
            InferenceRunner::new(provider.clone(), ModelConfig::new("model"))
                .with_recovery_budget(budget),
        ))],
        cancel.clone(),
    );
    let (tx, rx) = mpsc::unbounded_channel();
    (provider, machine, Emitter::new(tx, cancel), rx)
}

fn session() -> Session {
    Session(Conversation::new_unvalidated([
        Message::user().with_text("hi")
    ]))
}

#[tokio::test]
async fn retries_and_emits_progress_without_persisting_it() {
    let (provider, machine, emit, mut rx) =
        setup(vec![Reply::Failure, Reply::Answer], Duration::from_secs(1));
    let result = machine.step(&session(), &emit).await.unwrap();
    let messages: Vec<_> = result
        .effects
        .into_iter()
        .filter_map(|effect| effect.0)
        .collect();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].as_concat_text(), "answer");
    assert_eq!(
        messages[0]
            .metadata
            .inference
            .as_ref()
            .unwrap()
            .provider_session_id
            .as_deref(),
        Some("provider-session")
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    let AgentEvent::Message(progress) = rx.try_recv().unwrap() else {
        panic!("expected progress")
    };
    assert!(!progress.is_agent_visible());
    assert!(matches!(
        progress.content[0],
        MessageContent::SystemNotification(_)
    ));
}

#[tokio::test]
async fn zero_budget_reports_failure_without_retrying() {
    let (provider, machine, emit, _) = setup(vec![Reply::Failure], Duration::ZERO);
    let result = machine.step(&session(), &emit).await.unwrap();
    let message = result
        .effects
        .into_iter()
        .find_map(|effect| effect.0)
        .unwrap();
    assert!(message.error_kind().is_some());
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn recovery_budget_bounds_request_creation_and_waiting_for_output() {
    for reply in [Reply::PendingCreation, Reply::PendingStream] {
        let (provider, machine, emit, _) =
            setup(vec![Reply::Failure, reply], Duration::from_millis(50));
        let result = tokio::time::timeout(Duration::from_secs(2), machine.step(&session(), &emit))
            .await
            .unwrap()
            .unwrap();
        assert!(result
            .effects
            .iter()
            .filter_map(|effect| effect.0.as_ref())
            .any(|m| m.error_kind().is_some()));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn partial_output_is_saved_alongside_the_error() {
    let (provider, machine, emit, _) = setup(vec![Reply::Partial], Duration::from_secs(1));
    let result = machine.step(&session(), &emit).await.unwrap();
    let messages: Vec<_> = result
        .effects
        .into_iter()
        .filter_map(|effect| effect.0)
        .collect();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].as_concat_text(), "partial");
    assert!(messages[1].error_kind().is_some());
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cancellation_interrupts_pending_recovery_attempts() {
    for reply in [Reply::PendingCreation, Reply::PendingStream] {
        let (provider, machine, emit, mut rx) =
            setup(vec![Reply::Failure, reply], Duration::from_secs(120));
        let session = session();
        let execution = machine.step(&session, &emit);
        tokio::pin!(execution);
        tokio::select! {
            _ = &mut execution => panic!("inference should wait"),
            progress = rx.recv() => assert!(progress.is_some()),
        }
        tokio::select! {
            _ = &mut execution => panic!("inference should wait"),
            _ = tokio::time::sleep(Duration::from_millis(10)) => {},
        }
        assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
        emit.cancel_token().cancel();
        let result = tokio::time::timeout(Duration::from_secs(1), execution)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(result.status, RunStatus::Cancelled));
    }
}

#[tokio::test]
async fn cancellation_interrupts_backoff_before_another_request() {
    let (provider, machine, emit, mut rx) = setup(vec![Reply::Backoff], Duration::from_secs(120));
    let session = session();
    let execution = machine.step(&session, &emit);
    tokio::pin!(execution);
    tokio::select! {
        _ = &mut execution => panic!("inference should wait"),
        progress = rx.recv() => assert!(progress.is_some()),
    }
    emit.cancel_token().cancel();
    let result = tokio::time::timeout(Duration::from_secs(1), execution)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result.status, RunStatus::Cancelled));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}
