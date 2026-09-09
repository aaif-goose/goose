use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use goose::agents::state_machine::{
    yielded_with, Emitter, GooseEffect, Inference, InferenceInput, Operation, OperationResult,
    StateMachine, Step,
};
use goose::agents::AgentEvent;
use goose::config::GooseMode;
use goose::conversation::message::Message;
use goose::conversation::Conversation;
use goose::providers::base::ProviderUsage;
use goose::session::session_manager::token_state_from_session_and_totals;
use goose::session::{Session, SessionManager, SessionType};
use goose_providers::conversation::token_usage::Usage;
use goose_providers::model::ModelConfig;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct PromptPart;

#[async_trait]
impl Operation<Session, GooseEffect> for PromptPart {
    fn name(&self) -> &'static str {
        "prompt_part"
    }

    async fn prompt_parts(
        &self,
        _session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        Ok(vec![("test".to_string(), "custom context".to_string())])
    }
}

struct TestInference;

#[async_trait]
impl Operation<Session, GooseEffect> for TestInference {
    fn name(&self) -> &'static str {
        "test_inference"
    }
}

#[async_trait]
impl Inference<Session, GooseEffect> for TestInference {
    fn applies(&self, _conversation: &Conversation) -> bool {
        true
    }

    async fn infer(
        &self,
        _session: &Session,
        conversation: &Conversation,
        input: InferenceInput,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        assert_eq!(
            input.prompt_parts,
            [("test".to_string(), "custom context".to_string())]
        );
        let prompt = conversation
            .messages()
            .iter()
            .rev()
            .find(|message| message.role == rmcp::model::Role::User)
            .map(Message::as_concat_text)
            .unwrap();
        let message = emit
            .message(Message::assistant().with_text(format!("{prompt} answered")))
            .await;
        yielded_with([
            GooseEffect::from(message),
            GooseEffect::RecordUsage(ProviderUsage::new(
                "test-model".to_string(),
                Usage::new(Some(5), Some(7), Some(12)),
            )),
        ])
    }
}

#[tokio::test]
async fn custom_pipeline_supports_step_apply_run_and_usage() -> Result<()> {
    let _env = goose_test_support::otel::clear_otel_env(&[(
        "OTEL_INSTRUMENTATION_GENAI_CAPTURE_MESSAGE_CONTENT",
        "true",
    )]);
    let temp_dir = tempfile::tempdir()?;
    let session_manager = SessionManager::new(temp_dir.path().to_path_buf());
    let session = session_manager
        .create_session(
            temp_dir.path().to_path_buf(),
            "state-machine-api".to_string(),
            SessionType::Hidden,
            GooseMode::Auto,
        )
        .await?;
    session_manager
        .update(&session.id)
        .model_config(ModelConfig::new("test-model").with_context_limit(Some(1_000)))
        .apply()
        .await?;
    session_manager
        .add_message(&session.id, &Message::user().with_text("first turn"))
        .await?;

    let cancel = CancellationToken::new();
    let (tx, mut rx) = mpsc::channel(16);
    let emit = Emitter::new(tx, cancel.clone());
    let machine = StateMachine::new(
        vec![
            Step::Operation(Arc::new(PromptPart)),
            Step::Inference(Arc::new(TestInference)),
        ],
        cancel,
    );

    let session = session_manager.get_session(&session.id, true).await?;
    let mut result = machine.step(&session, &emit).await?.unwrap();
    assert!(matches!(
        result.effects.first(),
        Some(GooseEffect::Conversation(
            goose::agents::state_machine::ConversationEffect::AppendMessage(message)
        )) if message.id.is_some()
    ));
    machine
        .apply(&session_manager, &session, &mut result, &emit)
        .await?;
    assert!(result.yield_to_client);
    let session = session_manager.get_session(&session.id, true).await?;
    let persisted = session
        .conversation
        .as_ref()
        .and_then(Conversation::last)
        .expect("persisted inference response");
    let emitted = match rx.recv().await {
        Some(AgentEvent::Message(message)) => message,
        other => panic!("expected emitted inference response, got {other:?}"),
    };
    assert_eq!(emitted.id, persisted.id);
    assert_eq!(persisted.as_concat_text(), "first turn answered");

    session_manager
        .add_message(&session.id, &Message::user().with_text("second turn"))
        .await?;
    let session = machine.run(&session_manager, &session.id, &emit).await?;

    assert_eq!(
        session
            .conversation
            .as_ref()
            .and_then(Conversation::last)
            .map(Message::as_concat_text)
            .as_deref(),
        Some("second turn answered")
    );
    assert_eq!(session.usage.total_tokens, Some(12));
    let usage = session
        .conversation
        .as_ref()
        .and_then(Conversation::last)
        .and_then(|message| message.metadata.usage.as_deref())
        .unwrap();
    assert_eq!(usage.total_tokens, Some(12));
    let totals = session_manager
        .get_session_usage_totals(&session.id)
        .await?;
    let terminal_usage = token_state_from_session_and_totals(&session, &totals);
    assert_eq!(terminal_usage.total_tokens, 12);
    assert_eq!(terminal_usage.accumulated_total_tokens, 24);

    Ok(())
}
