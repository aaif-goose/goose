use crate::agents::state_machine::{
    ops_compaction::CompactionOperation, ops_context_relevance::ContextRelevanceOperation, Emitter,
    GooseEffect, Operation, OperationResult, StateMachine, Step,
};
use crate::config::GooseMode;
use crate::conversation::{message::Message, Conversation};
use crate::providers::base::{MessageStream, Provider};
use crate::session::{Session, SessionManager, SessionType};
use async_trait::async_trait;
use goose_providers::api_client::{ApiClient, AuthMethod};
use goose_providers::decision::{
    DecisionAnswer, DecisionProvider, DecisionRequest, DecisionResponse, DecisionUsage,
};
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;
use goose_providers::typesafe::TypeSafeProvider;
use rmcp::model::{Annotations, CallToolRequestParams, CallToolResult, ContentBlock, Role};
use serde_json::json;
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct FakeDecisionProvider {
    probability: f64,
    calls: AtomicUsize,
}

struct FailingCompactionProvider;

#[async_trait]
impl Provider for FailingCompactionProvider {
    fn get_name(&self) -> &str {
        "unavailable-compaction"
    }

    async fn stream(
        &self,
        _model_config: &ModelConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[rmcp::model::Tool],
    ) -> Result<MessageStream, ProviderError> {
        Err(ProviderError::RequestFailed(
            "synthetic compaction unavailable".to_string(),
        ))
    }
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
async fn request_goal_excludes_non_agent_content() {
    struct CaptureRequest(std::sync::Mutex<Option<DecisionRequest>>);
    #[async_trait]
    impl DecisionProvider for CaptureRequest {
        async fn create_decision(
            &self,
            request: &DecisionRequest,
        ) -> Result<DecisionResponse, ProviderError> {
            *self.0.lock().unwrap() = Some(request.clone());
            Ok(DecisionResponse {
                model: request.model.clone(),
                answers: request
                    .questions
                    .keys()
                    .map(|key| (key.clone(), DecisionAnswer::Noul { noul: 0.9 }))
                    .collect(),
                usage: DecisionUsage {
                    input_tokens: None,
                    output_tokens: None,
                    cost: None,
                },
                id: None,
                provider: None,
            })
        }
    }
    let provider = Arc::new(CaptureRequest(std::sync::Mutex::new(None)));
    let operation = ContextRelevanceOperation::new(provider.clone(), 10, 0.3);
    let mut messages = fixture().messages().clone();
    let kickoff = messages.last_mut().unwrap();
    kickoff
        .content
        .push(crate::conversation::message::MessageContent::Text(
            rmcp::model::TextContent::new("HIDDEN_SENTINEL")
                .with_annotations(Annotations::default().with_audience(vec![Role::User])),
        ));
    let conversation = Conversation::new_unvalidated(messages);
    let mut session = Session::default();
    session.usage.total_tokens = Some(4);
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let emit = Emitter::new(tx, tokio_util::sync::CancellationToken::new());
    let OperationResult::Applied(result) =
        operation.run(&session, &conversation, &emit).await.unwrap()
    else {
        panic!("expected decision")
    };
    let GooseEffect::SetExtensionState { value, .. } = &result.effects[0] else {
        panic!("expected metadata")
    };
    assert_eq!(value["status"], "observed");
    let request = provider.0.lock().unwrap().clone().unwrap();
    assert_eq!(request.state["goal"], "current goal");
    assert!(!serde_json::to_string(&request)
        .unwrap()
        .contains("HIDDEN_SENTINEL"));
}

#[tokio::test]
async fn request_uses_only_visible_complete_pairs_and_recent_evidence() {
    let request = Message::assistant()
        .with_id("private-request")
        .with_tool_request(
            "private-pair",
            Ok(CallToolRequestParams::new("read".to_string())),
        )
        .with_visibility(true, false);
    let result = Message::user()
        .with_id("private-response")
        .with_tool_response(
            "private-pair",
            Ok(CallToolResult::success(vec![ContentBlock::text(
                "PRIVATE_SENTINEL".repeat(60),
            )])),
        )
        .with_visibility(true, false);
    let orphan = Message::user().with_tool_response(
        "orphan",
        Ok(CallToolResult::success(vec![ContentBlock::text(
            "ORPHAN_SENTINEL".repeat(60),
        )])),
    );
    let hidden_content_request = Message::assistant()
        .with_tool_request(
            "hidden-content",
            Ok(CallToolRequestParams::new("read".to_string())),
        )
        .with_visibility(true, false);
    let hidden_content_result = Message::user().with_tool_response(
        "hidden-content",
        Ok(CallToolResult::success(vec![ContentBlock::text(
            "HIDDEN_PAIR_SENTINEL".repeat(60),
        )])),
    );
    let external_request = Message::assistant().with_tool_request_with_metadata(
        "externally-executed",
        Ok(CallToolRequestParams::new("read".to_string())),
        None,
        Some(json!({"goose.external_dispatch": true})),
    );
    let external_result = Message::user().with_tool_response(
        "externally-executed",
        Ok(CallToolResult::success(vec![ContentBlock::text(
            "EXTERNALLY_EXECUTED_SENTINEL".repeat(60),
        )])),
    );
    let bundle_request = Message::assistant()
        .with_tool_request(
            "bundle-1",
            Ok(CallToolRequestParams::new("read".to_string())),
        )
        .with_tool_request(
            "bundle-2",
            Ok(CallToolRequestParams::new("read".to_string())),
        );
    let bundle_result = Message::user()
        .with_tool_response(
            "bundle-1",
            Ok(CallToolResult::success(vec![ContentBlock::text(
                "BUNDLE_SENTINEL".repeat(60),
            )])),
        )
        .with_tool_response(
            "bundle-2",
            Ok(CallToolResult::success(vec![ContentBlock::text(
                "BUNDLE_SENTINEL".repeat(60),
            )])),
        );
    let mut messages = vec![Message::user().with_text("prior goal")];
    messages.extend([
        request,
        result,
        orphan,
        hidden_content_request,
        hidden_content_result,
        external_request,
        external_result,
        bundle_request,
        bundle_result,
    ]);
    let complete_pair = fixture();
    messages.extend(complete_pair.messages()[1..3].iter().cloned());
    let cutoff = messages.len();
    messages.push(
        Message::user()
            .with_tool_response(
                "protected",
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    "RECENT_SENTINEL".repeat(60),
                )])),
            )
            .with_visibility(true, false),
    );
    messages.push(Message::user().with_text("current goal"));
    let conversation = Conversation::new_unvalidated(messages);

    let request = ContextRelevanceOperation::request(&conversation, cutoff, "current goal".into());
    let serialized = serde_json::to_string(&request).unwrap();
    assert_eq!(request.questions.len(), 1);
    assert!(serialized.contains("synthetic output"));
    assert!(!serialized.contains("PRIVATE_SENTINEL"));
    assert!(!serialized.contains("HIDDEN_PAIR_SENTINEL"));
    assert!(!serialized.contains("EXTERNALLY_EXECUTED_SENTINEL"));
    assert!(!serialized.contains("ORPHAN_SENTINEL"));
    assert!(!serialized.contains("BUNDLE_SENTINEL"));
    assert!(!serialized.contains("RECENT_SENTINEL"));
}

struct FailingDecisionProvider;

#[async_trait]
impl DecisionProvider for FailingDecisionProvider {
    async fn create_decision(
        &self,
        _request: &DecisionRequest,
    ) -> Result<DecisionResponse, ProviderError> {
        Err(ProviderError::RequestFailed(
            "synthetic decision failure".into(),
        ))
    }
}

#[tokio::test]
async fn provider_failure_records_metadata_without_dropping_context() {
    let operation = ContextRelevanceOperation::new(Arc::new(FailingDecisionProvider), 10, 0.3);
    let conversation = fixture();
    let before = serde_json::to_value(&conversation).unwrap();
    let mut session = Session::default();
    session.usage.total_tokens = Some(4);
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let emit = Emitter::new(tx, tokio_util::sync::CancellationToken::new());

    let OperationResult::Applied(result) =
        operation.run(&session, &conversation, &emit).await.unwrap()
    else {
        panic!("expected failure metadata")
    };
    let GooseEffect::SetExtensionState { value, .. } = &result.effects[0] else {
        panic!("expected metadata effect")
    };
    assert_eq!(value["status"], "provider_error");
    assert!(value.get("observations").is_none());
    assert_eq!(serde_json::to_value(&conversation).unwrap(), before);
}

#[tokio::test]
async fn invalid_probabilities_never_propose_omission() {
    for probability in [f64::NAN, -0.1, 1.1] {
        let operation = ContextRelevanceOperation::new(
            Arc::new(FakeDecisionProvider {
                probability,
                calls: AtomicUsize::new(0),
            }),
            10,
            0.3,
        );
        let conversation = fixture();
        let mut session = Session::default();
        session.usage.total_tokens = Some(4);
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let emit = Emitter::new(tx, tokio_util::sync::CancellationToken::new());
        let OperationResult::Applied(result) =
            operation.run(&session, &conversation, &emit).await.unwrap()
        else {
            panic!("expected an observation")
        };
        let GooseEffect::SetExtensionState { value, .. } = &result.effects[0] else {
            panic!("expected metadata effect")
        };
        assert!(value["observations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|observation| {
                observation["valid"] == false && observation["would_drop"] == false
            }));
    }
}

struct PendingDecisionProvider;

#[async_trait]
impl DecisionProvider for PendingDecisionProvider {
    async fn create_decision(
        &self,
        _request: &DecisionRequest,
    ) -> Result<DecisionResponse, ProviderError> {
        std::future::pending().await
    }
}

#[tokio::test(start_paused = true)]
async fn timeout_records_failure_without_mutating_conversation() {
    let op = ContextRelevanceOperation::new(Arc::new(PendingDecisionProvider), 10, 0.3);
    let conversation = fixture();
    let before = serde_json::to_value(&conversation).unwrap();
    let mut session = Session::default();
    session.usage.total_tokens = Some(4);
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let emit = Emitter::new(tx, tokio_util::sync::CancellationToken::new());
    let OperationResult::Applied(result) = op.run(&session, &conversation, &emit).await.unwrap()
    else {
        panic!("expected failure metadata")
    };
    let GooseEffect::SetExtensionState { value, .. } = &result.effects[0] else {
        panic!("expected a metadata effect")
    };
    assert_eq!(value["status"], "timeout");
    assert_eq!(serde_json::to_value(&conversation).unwrap(), before);
}

#[tokio::test]
async fn cancellation_does_not_record_a_decision() {
    let op = ContextRelevanceOperation::new(Arc::new(PendingDecisionProvider), 10, 0.3);
    let conversation = fixture();
    let mut session = Session::default();
    session.usage.total_tokens = Some(4);
    let cancel = tokio_util::sync::CancellationToken::new();
    cancel.cancel();
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let emit = Emitter::new(tx, cancel);
    assert!(matches!(
        op.run(&session, &conversation, &emit).await.unwrap(),
        OperationResult::NotApplicable
    ));
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
    let GooseEffect::SetExtensionState {
        extension_name,
        version,
        value: note,
    } = &result.effects[0]
    else {
        panic!("expected metadata effect")
    };
    assert_eq!((*extension_name, *version), ("context_relevance", "v1"));
    assert_eq!(note["status"], "observed");
    assert_eq!(note["mode"], "shadow");
    assert!(note.get("active_omitted_tool_call_ids").is_none());
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(serde_json::to_value(&conversation).unwrap(), before);
}

#[tokio::test]
async fn persisted_decision_is_idempotent_and_compaction_still_runs() {
    let temp = tempfile::tempdir().unwrap();
    let manager = SessionManager::new(temp.path().join("sessions"));
    let session = manager
        .create_session(
            temp.path().to_path_buf(),
            "context relevance test".to_string(),
            SessionType::Hidden,
            GooseMode::Auto,
        )
        .await
        .unwrap();
    for message in fixture().messages() {
        manager.add_message(&session.id, message).await.unwrap();
    }
    manager
        .update(&session.id)
        .usage(goose_providers::conversation::token_usage::Usage::new(
            None,
            None,
            Some(4),
        ))
        .apply()
        .await
        .unwrap();
    let provider = Arc::new(FakeDecisionProvider {
        probability: 0.1,
        calls: AtomicUsize::new(0),
    });
    let operation = Arc::new(ContextRelevanceOperation::new(provider.clone(), 10, 0.3));
    let machine = StateMachine::new(
        vec![Step::Operation(operation.clone())],
        tokio_util::sync::CancellationToken::new(),
    );
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let emit = Emitter::new(tx, tokio_util::sync::CancellationToken::new());
    let before = serde_json::to_value(
        manager
            .get_session(&session.id, true)
            .await
            .unwrap()
            .conversation,
    )
    .unwrap();
    let applied = machine.run(&manager, &session.id, &emit).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(serde_json::to_value(applied.conversation).unwrap(), before);
    let stored = applied
        .extension_data
        .get_extension_state("context_relevance", "v1")
        .unwrap();
    assert_eq!(stored["status"], "observed");
    assert!(stored["observations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|observation| observation["would_drop"] == true));
    machine.run(&manager, &session.id, &emit).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);

    let session = manager.get_session(&session.id, true).await.unwrap();
    let compaction = CompactionOperation::new(
        Arc::new(FailingCompactionProvider),
        ModelConfig::new("test"),
        10,
        0.3,
    );
    let result = compaction
        .run(&session, session.conversation.as_ref().unwrap(), &emit)
        .await
        .unwrap();
    assert!(
        !matches!(result, OperationResult::NotApplicable),
        "shadow metadata must not suppress ordinary compaction"
    );
    assert!(matches!(
        rx.try_recv(),
        Ok(crate::agents::AgentEvent::Message(_))
    ));
}

#[tokio::test]
async fn typesafe_provider_records_shadow_decisions_through_the_operation() {
    let server = MockServer::start().await;
    let conversation = fixture();
    let cutoff = conversation.len() - 1;
    let request = ContextRelevanceOperation::request(&conversation, cutoff, "current goal".into());
    let answers: serde_json::Map<_, _> = request
        .questions
        .keys()
        .map(|key| (key.clone(), json!({ "type": "noul", "noul": 0.1 })))
        .collect();
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer test-key"))
        .and(body_partial_json(json!({
            "model": "jev-latest",
            "state": { "goal": "current goal" },
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model": "jev-test",
            "answers": answers,
            "usage": { "input_tokens": 12, "output_tokens": 3 }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let provider = TypeSafeProvider::new(
        ApiClient::new_with_tls(
            server.uri(),
            AuthMethod::BearerToken("test-key".to_string()),
            None,
        )
        .unwrap(),
    );
    let operation = ContextRelevanceOperation::new(Arc::new(provider), 10, 0.3);
    let mut session = Session::default();
    session.usage.total_tokens = Some(4);
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let emit = Emitter::new(tx, tokio_util::sync::CancellationToken::new());
    let OperationResult::Applied(result) =
        operation.run(&session, &conversation, &emit).await.unwrap()
    else {
        panic!("expected a shadow observation")
    };
    let GooseEffect::SetExtensionState { value, .. } = &result.effects[0] else {
        panic!("expected metadata effect")
    };
    assert_eq!(value["status"], "observed");
    assert_eq!(value["decision_model"], "jev-test");
    assert_eq!(value["usage"]["input_tokens"], 12);
    assert!(value["observations"]
        .as_array()
        .unwrap()
        .iter()
        .all(|observation| observation["p_needed"] == 0.1 && observation["would_drop"] == true));
    assert!(value.get("active_omitted_tool_call_ids").is_none());
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
    let GooseEffect::SetExtensionState {
        extension_name,
        version,
        value: note,
    } = &result.effects[0]
    else {
        panic!("expected metadata effect")
    };
    assert_eq!((*extension_name, *version), ("context_relevance", "v1"));
    assert!(note["observations"]
        .as_array()
        .unwrap()
        .iter()
        .all(|observation| observation["would_drop"] == true));
    assert!(note.get("active_omitted_tool_call_ids").is_none());
}
