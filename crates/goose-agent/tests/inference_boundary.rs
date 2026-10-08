#![cfg(not(target_arch = "wasm32"))]

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::inference::{
    InferenceEffect, InferenceRequestPreparer, InferenceRunner, PreparedInferenceRequest,
};
use goose_agent::machine::{MachineSession, StateMachine, Step};
use goose_agent::operation::{
    not_applicable, yielded, Emitter, Inference, InferenceInput, MachineEffect, Operation,
    OperationResult,
};
use goose_provider_types::base::{MessageStream, Provider};
use goose_provider_types::conversation::message::Message;
use goose_provider_types::conversation::token_usage::ProviderUsage;
use goose_provider_types::conversation::Conversation;
use goose_provider_types::errors::ProviderError;
use goose_provider_types::model::ModelConfig;
use rmcp::model::Tool;
use tokio_util::sync::CancellationToken;

enum Effect {
    Message(Message),
    Usage,
}

impl From<Message> for Effect {
    fn from(message: Message) -> Self {
        Self::Message(message)
    }
}

impl InferenceEffect for Effect {
    fn record_usage(_: ProviderUsage) -> Self {
        Self::Usage
    }
}

impl MachineEffect for Effect {
    fn ensure_message_ids(&mut self) {
        if let Self::Message(message) = self {
            if message.id.is_none() {
                message.id = Some("contract-message".into());
            }
        }
    }
}

struct Session(Conversation);

impl MachineSession for Session {
    fn id(&self) -> &str {
        "contract-session"
    }

    fn conversation(&self) -> Option<&Conversation> {
        Some(&self.0)
    }
}

fn emitter() -> (
    Emitter,
    tokio::sync::mpsc::Receiver<goose_agent::events::AgentEvent>,
) {
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    (Emitter::new(tx, CancellationToken::new()), rx)
}

struct PanicContributor(Arc<AtomicUsize>);

#[async_trait]
impl Operation<Session, Effect> for PanicContributor {
    fn name(&self) -> &'static str {
        "application-operation"
    }

    async fn run(
        &self,
        _: &Session,
        _: &Conversation,
        _: &Emitter,
    ) -> Result<OperationResult<Effect>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        not_applicable()
    }

    async fn inference_tools(&self, _: &Session) -> Result<Vec<Tool>> {
        panic!("protocol inference must not request application tools")
    }

    async fn prompt_parts(&self, _: &Session, _: &Conversation) -> Result<Vec<(String, String)>> {
        panic!("protocol inference must not request application prompts")
    }

    async fn moim_parts(&self, _: &Session, _: &Conversation) -> Result<Vec<String>> {
        panic!("protocol inference must not request application context")
    }
}

struct ProtocolInference;

#[async_trait]
impl Operation<Session, Effect> for ProtocolInference {
    fn name(&self) -> &'static str {
        "protocol"
    }
}

#[async_trait]
impl Inference<Session, Effect> for ProtocolInference {
    fn applies(&self, _: &Conversation) -> bool {
        true
    }

    async fn prepare_input(
        &self,
        _: &Session,
        _: &Conversation,
        operations: &[&dyn Operation<Session, Effect>],
        _: &CancellationToken,
    ) -> Result<InferenceInput> {
        assert_eq!(operations.len(), 2);
        Ok(InferenceInput::default())
    }

    async fn infer(
        &self,
        _: &Session,
        _: &Conversation,
        input: InferenceInput,
        _: &Emitter,
    ) -> Result<OperationResult<Effect>> {
        assert!(input.tools.is_empty());
        assert!(input.prompt_parts.is_empty());
        assert!(input.moim_parts.is_empty());
        yielded()
    }
}

#[tokio::test]
async fn protocol_preparation_bypasses_contributors_without_bypassing_operation_run() {
    let runs = Arc::new(AtomicUsize::new(0));
    let machine = StateMachine::new(
        vec![
            Step::Operation(Arc::new(PanicContributor(runs.clone()))),
            Step::Inference(Arc::new(ProtocolInference)),
        ],
        CancellationToken::new(),
    );
    let (emit, _events) = emitter();
    let result = machine
        .step(&Session(Conversation::empty()), &emit)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(runs.load(Ordering::SeqCst), 1);
    assert_eq!(result.applied_step, Some("protocol"));
    assert!(result.yield_to_client);
}

struct DefaultInference;

#[async_trait]
impl Operation<(), Effect> for DefaultInference {
    fn name(&self) -> &'static str {
        "default"
    }
}

#[async_trait]
impl Inference<(), Effect> for DefaultInference {
    fn applies(&self, _: &Conversation) -> bool {
        true
    }

    async fn infer(
        &self,
        _: &(),
        _: &Conversation,
        _: InferenceInput,
        _: &Emitter,
    ) -> Result<OperationResult<Effect>> {
        not_applicable()
    }
}

struct CancelDuringTools(CancellationToken);

#[async_trait]
impl Operation<(), Effect> for CancelDuringTools {
    fn name(&self) -> &'static str {
        "pending-tools"
    }

    async fn inference_tools(&self, _: &()) -> Result<Vec<Tool>> {
        self.0.cancel();
        futures::future::pending().await
    }

    async fn prompt_parts(&self, _: &(), _: &Conversation) -> Result<Vec<(String, String)>> {
        panic!("cancelled tools must stop preparation")
    }
}

#[tokio::test]
async fn default_preparation_cancels_pending_tool_discovery() {
    let cancel = CancellationToken::new();
    let input = DefaultInference
        .prepare_input(
            &(),
            &Conversation::empty(),
            &[&CancelDuringTools(cancel.clone())],
            &cancel,
        )
        .await
        .unwrap();
    assert!(cancel.is_cancelled());
    assert!(input.tools.is_empty());
    assert!(input.prompt_parts.is_empty());
    assert!(input.moim_parts.is_empty());
}

struct PrefixPreparer;

#[async_trait]
impl InferenceRequestPreparer<()> for PrefixPreparer {
    async fn prepare(
        &self,
        _: &(),
        _: &Conversation,
        _: InferenceInput,
    ) -> Result<PreparedInferenceRequest> {
        Ok(PreparedInferenceRequest {
            system_prompt: "prepared system".into(),
            tools: vec![],
            additional_messages: vec![Message::user().with_text("persisted prefix")],
        })
    }
}

struct BoundaryProvider {
    stream_error: bool,
}

#[async_trait]
impl Provider for BoundaryProvider {
    fn get_name(&self) -> &str {
        "boundary-test"
    }

    async fn stream(
        &self,
        model: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> std::result::Result<MessageStream, ProviderError> {
        assert_eq!(model.model_name, "boundary-model");
        assert_eq!(system, "prepared system");
        assert!(tools.is_empty());
        assert!(messages
            .iter()
            .any(|message| message.as_concat_text().contains("persisted prefix")));
        if self.stream_error {
            Ok(Box::pin(futures::stream::iter([Err(
                ProviderError::RequestFailed("stream failed".into()),
            )])))
        } else {
            Ok(Box::pin(futures::stream::empty()))
        }
    }
}

async fn prefix_result(stream_error: bool) -> goose_agent::operation::StepResult<Effect> {
    let runner = InferenceRunner::<(), Effect>::new(
        Arc::new(BoundaryProvider { stream_error }),
        ModelConfig::new("boundary-model"),
    )
    .with_request_preparer(Arc::new(PrefixPreparer));
    let conversation = Conversation::new_unvalidated([Message::user().with_text("kickoff")]);
    let (emit, _events) = emitter();
    let result = runner
        .infer(&(), &conversation, InferenceInput::default(), &emit)
        .await
        .unwrap();
    let OperationResult::Applied(step) = result else {
        panic!("expected persistence effects")
    };
    assert_eq!(step.effects.len(), 2);
    assert!(
        matches!(&step.effects[0], Effect::Message(message) if message.as_concat_text() == "persisted prefix")
    );
    step
}

#[tokio::test]
async fn runner_persists_prepared_prefix_before_empty_completion_fallback() {
    let step = prefix_result(false).await;
    assert!(!step.yield_to_client);
    assert!(
        matches!(&step.effects[1], Effect::Message(message) if goose_agent::inference::is_empty_response_marker(message) && !message.is_user_visible() && !message.is_agent_visible())
    );
}

#[tokio::test]
async fn runner_persists_prepared_prefix_before_stream_error() {
    let step = prefix_result(true).await;
    assert!(!step.yield_to_client);
    assert!(matches!(&step.effects[1], Effect::Message(message) if message.error_kind().is_some()));
}
