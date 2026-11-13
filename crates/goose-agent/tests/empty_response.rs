use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::empty_response::{EmptyResponseOperation, EMPTY_RESPONSE_MESSAGE};
use goose_agent::inference::{
    InferenceEffect, InferenceRequestPreparer, InferenceRunner, PreparedInferenceRequest,
};
use goose_agent::machine::{MachineSession, StateMachine, Step};
use goose_agent::max_turns::{MaxTurnsOperation, MAX_TURNS_MESSAGE};
use goose_agent::operation::{
    Emitter, InferenceInput, MachineEffect, Operation, OperationResult, RunStatus,
};
use goose_provider_types::base::{MessageStream, Provider};
use goose_provider_types::conversation::message::Message;
use goose_provider_types::conversation::token_usage::ProviderUsage;
use goose_provider_types::conversation::Conversation;
use goose_provider_types::errors::ProviderError;
use goose_provider_types::model::ModelConfig;
use rmcp::model::CallToolResult;
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
    Message(Message),
    Usage,
}

impl From<Message> for Effect {
    fn from(message: Message) -> Self {
        Self::Message(message)
    }
}

impl MachineEffect for Effect {
    fn ensure_message_ids(&mut self) {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        if let Self::Message(message) = self {
            message.id.get_or_insert_with(|| {
                format!("test-message-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed))
            });
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

struct ContextPreparer(AtomicUsize);

#[async_trait]
impl InferenceRequestPreparer<Session> for ContextPreparer {
    async fn prepare(
        &self,
        _session: &Session,
        _conversation: &Conversation,
        _input: InferenceInput,
    ) -> Result<PreparedInferenceRequest> {
        let attempt = self.0.fetch_add(1, Ordering::SeqCst);
        Ok(PreparedInferenceRequest {
            system_prompt: String::new(),
            tools: Vec::new(),
            additional_messages: vec![Message::user()
                .with_text(format!("context {attempt}"))
                .with_visibility(false, true)],
        })
    }
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
        if let Effect::Message(message) = effect {
            session.0.push(message);
        }
    }
}

async fn run(
    replies: impl IntoIterator<Item = Option<&'static str>>,
    recover: bool,
    max_turns: Option<u32>,
) -> Result<(Session, Arc<ScriptedProvider>)> {
    let provider = Arc::new(ScriptedProvider::new(replies));
    let inference = Arc::new(
        InferenceRunner::new(provider.clone(), ModelConfig::new("test"))
            .with_request_preparer(Arc::new(ContextPreparer(AtomicUsize::new(0)))),
    );
    let mut steps: Vec<Step<'_, Session, Effect>> = Vec::new();
    if recover {
        steps.push(Step::Operation(Arc::new(EmptyResponseOperation)));
    }
    if let Some(max_turns) = max_turns {
        steps.push(Step::Operation(Arc::new(MaxTurnsOperation::new(max_turns))));
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
    let (session, provider) = run([None], false, None).await?;

    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    let empty = session.0.last().unwrap();
    assert_eq!(empty.role, rmcp::model::Role::Assistant);
    assert!(empty.content.is_empty());
    Ok(())
}

#[tokio::test]
async fn recovery_retries_three_times_and_can_finish_early() -> Result<()> {
    let (exhausted, provider) = run([None, None, None, None], true, None).await?;
    assert_eq!(provider.calls.load(Ordering::SeqCst), 4);
    let empty_responses = exhausted
        .0
        .iter()
        .filter(|message| {
            message.role == rmcp::model::Role::Assistant && message.content.is_empty()
        })
        .count();
    assert_eq!(empty_responses, 4);
    assert_eq!(
        exhausted.0.last().unwrap().as_concat_text(),
        EMPTY_RESPONSE_MESSAGE
    );

    let (recovered, provider) = run([None, None, Some("done")], true, None).await?;
    assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
    assert_eq!(recovered.0.last().unwrap().as_concat_text(), "done");
    assert_eq!(
        recovered
            .0
            .iter()
            .filter(|message| {
                message.role == rmcp::model::Role::Assistant && message.content.is_empty()
            })
            .count(),
        2
    );
    Ok(())
}

#[tokio::test]
async fn turn_budget_stops_empty_response_retries() -> Result<()> {
    let (session, provider) = run([None, Some("not reached")], true, Some(1)).await?;

    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        session.0.last().unwrap().as_concat_text(),
        MAX_TURNS_MESSAGE
    );
    Ok(())
}

#[tokio::test]
async fn successful_tool_completion_does_not_retry() -> Result<()> {
    let conversation = Conversation::new_unvalidated([
        Message::user().with_text("hello"),
        Message::user().with_tool_response("call", Ok(CallToolResult::success(Vec::new()))),
        Message::assistant(),
    ]);
    let operation: &dyn Operation<(), Message> = &EmptyResponseOperation;
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());

    assert!(matches!(
        operation.run(&(), &conversation, &emit).await?,
        OperationResult::NotApplicable
    ));
    Ok(())
}
