use crate::agents::state_machine::{
    ops_context_relevance::ContextRelevanceOperation, Emitter, GooseEffect, Operation,
    OperationResult,
};
use crate::conversation::{message::Message, Conversation};
use crate::session::Session;
use async_trait::async_trait;
use env_lock::lock_env;
use goose_providers::decision::{
    DecisionAnswer, DecisionProvider, DecisionRequest, DecisionResponse, DecisionUsage,
};
use goose_providers::errors::ProviderError;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[test]
fn shadow_mode_requires_explicit_opt_in() {
    let _guard = lock_env([
        ("GOOSE_CONTEXT_RELEVANCE_SHADOW", None::<&str>),
        ("TYPESAFE_API_KEY", None::<&str>),
    ]);
    assert!(ContextRelevanceOperation::from_config(10, 0.3).is_none());
}

struct FakeDecisionProvider {
    probability: f64,
    calls: AtomicUsize,
}

#[async_trait]
impl DecisionProvider for FakeDecisionProvider {
    async fn create_decision(
        &self,
        request: &DecisionRequest,
    ) -> Result<DecisionResponse, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(serde_json::to_vec(request).unwrap().len() <= 48_000);
        Ok(DecisionResponse {
            model: request.model.clone(),
            answers: request
                .questions
                .keys()
                .map(|key| {
                    (
                        key.clone(),
                        DecisionAnswer::Noul {
                            noul: self.probability,
                        },
                    )
                })
                .collect(),
            usage: DecisionUsage {
                input_tokens: Some(12),
                output_tokens: Some(3),
                cost: None,
            },
            id: None,
            provider: None,
        })
    }
}

fn fixture() -> Conversation {
    let mut messages = vec![Message::user().with_id("old-goal").with_text("old goal")];
    for index in 0..4 {
        let id = format!("call-{index}");
        messages.push(
            Message::assistant()
                .with_id(format!("request-{index}"))
                .with_tool_request(
                    id.clone(),
                    Ok(CallToolRequestParams::new("read".to_string())),
                ),
        );
        messages.push(
            Message::user()
                .with_id(format!("response-{index}"))
                .with_tool_response(
                    id,
                    Ok(CallToolResult::success(vec![ContentBlock::text(
                        "synthetic output ".repeat(100),
                    )])),
                ),
        );
    }
    messages.push(Message::user().with_id("kickoff").with_text("current goal"));
    Conversation::new_unvalidated(messages)
}

#[tokio::test]
async fn operation_waits_until_context_exceeds_compaction_threshold() {
    let provider = Arc::new(FakeDecisionProvider {
        probability: 0.9,
        calls: AtomicUsize::new(0),
    });
    let operation = ContextRelevanceOperation::new(provider.clone(), 10, 0.3);
    let conversation = fixture();
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let emit = Emitter::new(tx, tokio_util::sync::CancellationToken::new());
    let mut session = Session::default();
    session.usage.total_tokens = Some(3);
    assert!(matches!(
        operation.run(&session, &conversation, &emit).await.unwrap(),
        OperationResult::NotApplicable
    ));
    session.usage.total_tokens = Some(4);
    assert!(matches!(
        operation.run(&session, &conversation, &emit).await.unwrap(),
        OperationResult::Applied(_)
    ));
}

#[tokio::test]
async fn operation_records_decisions_without_mutating_conversation() {
    let provider = Arc::new(FakeDecisionProvider {
        probability: 0.9,
        calls: AtomicUsize::new(0),
    });
    let operation = ContextRelevanceOperation::new(provider.clone(), 10, 0.3);
    let conversation = fixture();
    let before = serde_json::to_value(&conversation).unwrap();
    let mut session = Session::default();
    session.usage.total_tokens = Some(4);
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let emit = Emitter::new(tx, tokio_util::sync::CancellationToken::new());

    let OperationResult::Applied(result) =
        operation.run(&session, &conversation, &emit).await.unwrap()
    else {
        panic!("expected observation")
    };
    let GooseEffect::SetExtensionData(data) = &result.effects[0] else {
        panic!("expected metadata effect")
    };
    let note = data.get_extension_state("context_relevance", "v1").unwrap();
    assert_eq!(note["status"], "observed");
    assert_eq!(note["mode"], "shadow");
    assert!(note.get("active_omitted_tool_call_ids").is_none());
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(serde_json::to_value(&conversation).unwrap(), before);
}

#[tokio::test]
async fn low_confidence_is_only_a_hypothetical_observation() {
    let provider = Arc::new(FakeDecisionProvider {
        probability: 0.1,
        calls: AtomicUsize::new(0),
    });
    let operation = ContextRelevanceOperation::new(provider, 10, 0.3);
    let conversation = fixture();
    let mut session = Session::default();
    session.usage.total_tokens = Some(4);
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let emit = Emitter::new(tx, tokio_util::sync::CancellationToken::new());

    let OperationResult::Applied(result) =
        operation.run(&session, &conversation, &emit).await.unwrap()
    else {
        panic!("expected observation")
    };
    let GooseEffect::SetExtensionData(data) = &result.effects[0] else {
        panic!("expected metadata effect")
    };
    let note = data.get_extension_state("context_relevance", "v1").unwrap();
    assert!(note["observations"]
        .as_array()
        .unwrap()
        .iter()
        .all(|observation| observation["would_drop"] == true));
    assert!(note.get("active_omitted_tool_call_ids").is_none());
}
