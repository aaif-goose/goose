use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::empty_response::EmptyResponseOperation;
use goose_agent::inference::{is_empty_response_marker, InferenceEffect, InferenceRunner};
use goose_agent::machine::{MachineSession, StateMachine, Step};
use goose_agent::operation::{ConversationEffect, Emitter, MachineEffect, RunStatus};
use goose_provider_types::base::{MessageStream, Provider};
use goose_provider_types::conversation::message::Message;
use goose_provider_types::conversation::token_usage::ProviderUsage;
use goose_provider_types::conversation::Conversation;
use goose_provider_types::errors::ProviderError;
use goose_provider_types::model::ModelConfig;
use tokio_util::sync::CancellationToken;

struct Session(Conversation);

impl MachineSession for Session {
    fn id(&self) -> &str {
        "empty-response-test"
    }

    fn conversation(&self) -> Option<&Conversation> {
        Some(&self.0)
    }
}

enum Effect {
    Conversation(ConversationEffect),
    Usage,
}

impl From<Message> for Effect {
    fn from(message: Message) -> Self {
        ConversationEffect::from(message).into()
    }
}

impl From<ConversationEffect> for Effect {
    fn from(effect: ConversationEffect) -> Self {
        Self::Conversation(effect)
    }
}

impl MachineEffect for Effect {
    fn ensure_message_ids(&mut self) {
        if let Self::Conversation(effect) = self {
            effect.ensure_message_ids();
        }
    }
}

impl InferenceEffect for Effect {
    fn record_usage(_usage: ProviderUsage) -> Self {
        Self::Usage
    }
}

struct ScriptedProvider {
    replies: Mutex<VecDeque<Option<String>>>,
    calls: AtomicUsize,
}

impl ScriptedProvider {
    fn new(replies: impl IntoIterator<Item = Option<&'static str>>) -> Self {
        Self {
            replies: Mutex::new(
                replies
                    .into_iter()
                    .map(|reply| reply.map(str::to_string))
                    .collect(),
            ),
            calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl Provider for ScriptedProvider {
    fn get_name(&self) -> &str {
        "empty-response-test"
    }

    async fn stream(
        &self,
        _model_config: &ModelConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[rmcp::model::Tool],
    ) -> Result<MessageStream, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let reply = self.replies.lock().unwrap().pop_front().flatten();
        let chunks = reply
            .map(|text| Ok::<_, ProviderError>((Some(Message::assistant().with_text(text)), None)))
            .into_iter();
        Ok(Box::pin(futures::stream::iter(chunks)))
    }
}

fn apply(session: &mut Session, effects: Vec<Effect>) {
    for effect in effects {
        let Effect::Conversation(effect) = effect else {
            continue;
        };
        match effect {
            ConversationEffect::AppendMessage(message) => session.0.push(message),
            ConversationEffect::SetMessageVisibility {
                message_id,
                user_visible,
                agent_visible,
            } => {
                let message = session
                    .0
                    .messages_mut()
                    .iter_mut()
                    .find(|message| message.id.as_deref() == Some(&message_id))
                    .unwrap();
                message.metadata.user_visible = user_visible;
                message.metadata.agent_visible = agent_visible;
            }
            _ => panic!("unexpected effect"),
        }
    }
}

async fn run(
    replies: impl IntoIterator<Item = Option<&'static str>>,
    recover: bool,
) -> Result<(Session, Arc<ScriptedProvider>)> {
    let provider = Arc::new(ScriptedProvider::new(replies));
    let inference = Arc::new(InferenceRunner::new(
        provider.clone(),
        ModelConfig::new("test"),
    ));
    let mut steps: Vec<Step<'_, Session, Effect>> = Vec::new();
    if recover {
        steps.push(Step::Operation(Arc::new(EmptyResponseOperation)));
    }
    steps.push(Step::Inference(inference));
    let machine: StateMachine<Session, Effect> = StateMachine::new(steps, CancellationToken::new());
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    let mut session = Session(Conversation::new_unvalidated([
        Message::user().with_text("hello")
    ]));

    for _ in 0..12 {
        let result = machine.step(&session, &emit).await?;
        apply(&mut session, result.effects);
        if result.status != RunStatus::Continuing {
            return Ok((session, provider));
        }
    }
    panic!("state machine did not yield");
}

#[tokio::test]
async fn empty_response_ends_a_pipeline_without_recovery() -> Result<()> {
    let (session, provider) = run([None], false).await?;

    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    let fallback = session.0.last().unwrap();
    assert!(is_empty_response_marker(fallback));
    assert!(fallback.is_user_visible());
    assert!(!fallback.is_agent_visible());
    Ok(())
}

#[tokio::test]
async fn recovery_retries_three_times_and_can_finish_early() -> Result<()> {
    let (exhausted, provider) = run([None, None, None, None], true).await?;
    assert_eq!(provider.calls.load(Ordering::SeqCst), 4);
    let markers = exhausted
        .0
        .iter()
        .filter(|message| is_empty_response_marker(message))
        .collect::<Vec<_>>();
    assert_eq!(markers.len(), 4);
    assert_eq!(
        markers
            .iter()
            .filter(|message| message.is_user_visible())
            .count(),
        1
    );

    let (recovered, provider) = run([None, None, Some("done")], true).await?;
    assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
    assert_eq!(recovered.0.last().unwrap().as_concat_text(), "done");
    assert!(!recovered
        .0
        .iter()
        .filter(|message| is_empty_response_marker(message))
        .any(Message::is_user_visible));
    Ok(())
}
