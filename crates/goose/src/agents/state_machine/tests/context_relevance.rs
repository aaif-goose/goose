use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use goose_providers::decision::{
    DecisionAnswer, DecisionProvider, DecisionRequest, DecisionResponse, DecisionUsage,
};
use goose_providers::errors::ProviderError;
use rmcp::model::{
    Annotations, CallToolRequestParams, CallToolResult, ContentBlock, Role, TextContent,
};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::dummy_api::DummyApi;
use super::dummy_api::ProviderFeatures;
use super::pipeline::{
    test_pipeline, test_pipeline_with, MessageKind::Agent, TestPipeline, TestRun,
};
use crate::agents::state_machine::ops_context_relevance::{
    truncated_json, with_turn, ContextRelevanceOperation, NAME, VERSION,
};
use crate::agents::state_machine::{Emitter, GooseEffect, Operation, OperationResult};
use crate::agents::state_machine_builder::context_relevance_provider;
use crate::config::Config;
use crate::conversation::message::{Message, MessageContent};
use crate::conversation::Conversation;
use crate::session::Session;

enum Reply {
    Probability(f64),
    OversizedModel,
    Fail,
    Hang,
}

struct FakeDecisions {
    reply: Reply,
    calls: AtomicUsize,
    last_request: Mutex<Option<DecisionRequest>>,
}

impl FakeDecisions {
    fn new(reply: Reply) -> Arc<Self> {
        Arc::new(Self {
            reply,
            calls: AtomicUsize::new(0),
            last_request: Mutex::new(None),
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl DecisionProvider for FakeDecisions {
    async fn create_decision(
        &self,
        request: &DecisionRequest,
    ) -> Result<DecisionResponse, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.last_request.lock().unwrap() = Some(request.clone());
        let noul = match self.reply {
            Reply::Probability(probability) => probability,
            Reply::OversizedModel => 0.9,
            Reply::Fail => {
                return Err(ProviderError::RequestFailed(format!(
                    "synthetic failure {}",
                    "x".repeat(1_000)
                )))
            }
            Reply::Hang => std::future::pending().await,
        };
        Ok(DecisionResponse {
            model: if matches!(self.reply, Reply::OversizedModel) {
                "é🦆".repeat(1_000)
            } else {
                request.model.clone()
            },
            answers: request
                .questions
                .keys()
                .map(|key| (key.clone(), DecisionAnswer::Noul { noul }))
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

fn tool_pair(id: &str, output: &str) -> [Message; 2] {
    [
        Message::assistant().with_tool_request(
            id,
            Ok(CallToolRequestParams::new("read")
                .with_arguments(json!({ "path": id }).as_object().unwrap().clone())),
        ),
        Message::user().with_tool_response(
            id,
            Ok(CallToolResult::success(vec![ContentBlock::text(output)])),
        ),
    ]
}

fn old_output() -> String {
    "old output ".repeat(50)
}

/// Four old read pairs leave exactly one candidate outside the protected
/// recent messages. The usage ratio is measured against the 0.8 default
/// compaction threshold.
async fn shadow_pipeline(
    decisions: Arc<FakeDecisions>,
    pairs: usize,
    output: &str,
    usage_ratio: f64,
) -> Result<(TestPipeline, Arc<DummyApi>)> {
    let (pipeline, api) = test_pipeline().await?;
    let pipeline = pipeline.with_context_relevance(decisions);
    pipeline
        .seed([Message::user().with_text("old goal")])
        .await?;
    for index in 0..pairs {
        pipeline
            .seed(tool_pair(&format!("call-{index}"), output))
            .await?;
    }
    pipeline
        .set_total_tokens((pipeline.context_limit() as f64 * usage_ratio) as i32)
        .await;
    api.on("goal").reply("answered");
    api.on("Please summarize the conversation history")
        .reply("summary");
    api.on("Your context was compacted")
        .reply("continued after compaction");
    Ok((pipeline, api))
}

fn turns(run: &TestRun) -> Vec<Value> {
    run.session
        .extension_data
        .get_extension_state(NAME, VERSION)
        .map(|state| state["turns"].as_array().unwrap().clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn pipeline_observes_once_before_compacting() -> Result<()> {
    let decisions = FakeDecisions::new(Reply::Probability(0.1));
    let (pipeline, _api) = shadow_pipeline(decisions.clone(), 4, &old_output(), 1.0).await?;

    let (_, run, _) = pipeline
        .run_reconstructing_each_step("current goal")
        .await?;

    run.assert_message(-1, Agent, "continued after compaction");
    assert_eq!(run.history_replacements(), 1);
    assert_eq!(decisions.calls(), 1);
    let [note] = turns(&run).try_into().unwrap();
    assert_eq!(note["status"], "observed");
    let [observation] = note["observations"]
        .as_array()
        .unwrap()
        .clone()
        .try_into()
        .unwrap();
    assert_eq!(observation["tool_call_ids"], json!(["call-0"]));
    assert!(observation["message_id"].is_string());
    assert_eq!(observation["would_drop"], true);
    Ok(())
}

#[tokio::test]
async fn observes_only_above_the_compaction_threshold() -> Result<()> {
    let decisions = FakeDecisions::new(Reply::Probability(0.9));
    let (pipeline, _api) = shadow_pipeline(decisions.clone(), 4, &old_output(), 0.79).await?;
    let below = pipeline.run(["current goal"]).await?;
    assert!(turns(&below).is_empty());
    assert_eq!(decisions.calls(), 0);

    let (pipeline, _api) = shadow_pipeline(decisions.clone(), 4, &old_output(), 0.81).await?;
    let above = pipeline.run(["current goal"]).await?;
    assert_eq!(turns(&above)[0]["status"], "observed");
    assert_eq!(decisions.calls(), 1);
    Ok(())
}

#[tokio::test]
async fn observations_from_earlier_turns_are_kept() -> Result<()> {
    let decisions = FakeDecisions::new(Reply::Probability(0.9));
    let (pipeline, _api) = shadow_pipeline(decisions.clone(), 4, &old_output(), 1.0).await?;
    pipeline.run(["first goal"]).await?;
    for index in 4..8 {
        pipeline
            .seed(tool_pair(&format!("call-{index}"), &old_output()))
            .await?;
    }
    pipeline
        .set_total_tokens(pipeline.context_limit() as i32)
        .await;

    let second = pipeline.run(["second goal"]).await?;

    let turns = turns(&second);
    assert_eq!(turns.len(), 2);
    assert_ne!(turns[0]["kickoff_id"], turns[1]["kickoff_id"]);
    assert_eq!(decisions.calls(), 2);
    Ok(())
}

#[test]
fn history_keeps_the_newest_twenty_turns() {
    let history = (0..25).fold(None, |history, turn| {
        Some(with_turn(history.as_ref(), json!({ "kickoff_id": turn })))
    });
    let turns = history.unwrap()["turns"].as_array().unwrap().clone();
    assert_eq!(turns.len(), 20);
    assert_eq!(turns[0]["kickoff_id"], 5);
    assert_eq!(turns[19]["kickoff_id"], 24);
}

#[tokio::test]
async fn goal_is_bounded_visible_text_and_marks_truncation() -> Result<()> {
    for (visible, goal, truncated) in [
        (
            "current goal".to_string(),
            "current goal".to_string(),
            false,
        ),
        ("goal ".repeat(200), "goal ".repeat(160), true),
    ] {
        let decisions = FakeDecisions::new(Reply::Probability(0.9));
        let (pipeline, _api) = shadow_pipeline(decisions.clone(), 4, &old_output(), 1.0).await?;

        pipeline
            .run_message(
                Message::user()
                    .with_content(MessageContent::Text(
                        TextContent::new("HIDDEN_SENTINEL").with_annotations(
                            Annotations::default().with_audience(vec![Role::User]),
                        ),
                    ))
                    .with_text(visible),
            )
            .await?;

        let request = decisions.last_request.lock().unwrap().clone().unwrap();
        assert_eq!(request.state["goal"], goal);
        assert_eq!(request.state["goal_truncated"], truncated);
        assert!(!serde_json::to_string(&request)?.contains("HIDDEN_SENTINEL"));
    }
    Ok(())
}

#[tokio::test]
async fn response_model_is_bounded_before_persistence() -> Result<()> {
    let decisions = FakeDecisions::new(Reply::OversizedModel);
    let (pipeline, _api) = shadow_pipeline(decisions.clone(), 4, &old_output(), 1.0).await?;

    let run = pipeline.run(["current goal"]).await?;

    let note = &turns(&run)[0];
    assert_eq!(note["status"], "observed");
    assert_eq!(note["decision_model"], "é🦆".repeat(100));
    assert_eq!(decisions.calls(), 1);
    assert_eq!(run.history_replacements(), 1);
    Ok(())
}

#[tokio::test]
async fn mixed_media_goals_are_marked_incomplete_without_sending_media() -> Result<()> {
    let mut hidden_image = MessageContent::image("HIDDEN_IMAGE_SENTINEL", "image/png");
    let MessageContent::Image(image) = &mut hidden_image else {
        unreachable!()
    };
    image.annotations = Some(Annotations::default().with_audience(vec![Role::User]));
    for (message, truncated) in [
        (
            Message::user()
                .with_text("compare this goal")
                .with_image("VISIBLE_IMAGE_SENTINEL", "image/png"),
            true,
        ),
        (
            Message::user()
                .with_text("compare this goal")
                .with_document("DOCUMENT_SENTINEL", "application/pdf", None),
            true,
        ),
        (
            Message::user()
                .with_content(hidden_image)
                .with_text("compare this goal"),
            false,
        ),
    ] {
        let decisions = FakeDecisions::new(Reply::Probability(0.9));
        let (pipeline, _api) = shadow_pipeline(decisions.clone(), 4, &old_output(), 1.0).await?;

        pipeline.run_message(message).await?;

        let request = decisions.last_request.lock().unwrap().clone().unwrap();
        assert_eq!(request.state["goal"], "compare this goal");
        assert_eq!(request.state["goal_truncated"], truncated);
        assert!(!serde_json::to_string(&request)?.contains("SENTINEL"));
    }
    Ok(())
}

#[tokio::test]
async fn steered_goal_keeps_the_original_kickoff_and_protects_the_whole_turn() -> Result<()> {
    let decisions = FakeDecisions::new(Reply::Probability(0.9));
    let (pipeline, _api) = shadow_pipeline(decisions.clone(), 4, &old_output(), 1.0).await?;
    pipeline
        .seed([Message::user()
            .with_id("original-kickoff")
            .with_text("paint it")])
        .await?;
    let mut turn_context = Message::user()
        .with_text("TURN_CONTEXT_SENTINEL")
        .agent_only();
    turn_context.metadata.turn_context = true;
    pipeline
        .seed([
            turn_context,
            Message::user()
                .with_text("HOOK_NUDGE_SENTINEL")
                .agent_only(),
        ])
        .await?;
    for index in 0..5 {
        pipeline
            .seed(tool_pair(&format!("current-{index}"), &old_output()))
            .await?;
    }
    pipeline
        .steer(Message::user().with_text("first use blue"))
        .await;
    pipeline
        .steer(
            Message::user()
                .with_text("HIDDEN_STEER_SENTINEL")
                .user_only(),
        )
        .await;
    pipeline
        .steer(Message::user().with_text("then make it matte"))
        .await;

    let run = pipeline.resume().await?;

    assert_eq!(decisions.calls(), 1);
    assert_eq!(run.history_replacements(), 1);
    assert_eq!(turns(&run)[0]["kickoff_id"], "original-kickoff");
    let request = decisions.last_request.lock().unwrap().clone().unwrap();
    assert_eq!(
        request.state["goal"],
        "paint it\nfirst use blue\nthen make it matte"
    );
    assert_eq!(request.state["goal_truncated"], false);
    let candidates = request.state["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 4);
    assert!(candidates
        .iter()
        .all(|candidate| candidate["tool_call_ids"][0]
            .as_str()
            .unwrap()
            .starts_with("call-")));
    assert!(!serde_json::to_string(&request)?.contains("SENTINEL"));
    Ok(())
}

#[tokio::test]
async fn steering_shares_one_goal_budget_and_marks_omitted_media() -> Result<()> {
    for (opening, steer, expected, truncated) in [
        (
            Message::user().with_text("a".repeat(400)),
            Message::user().with_text("b".repeat(400)),
            format!("{}\n{}", "a".repeat(400), "b".repeat(399)),
            true,
        ),
        (
            Message::user().with_text("paint it"),
            Message::user()
                .with_text("use this goal")
                .with_image("IMAGE_SENTINEL", "image/png"),
            "paint it\nuse this goal".to_string(),
            true,
        ),
    ] {
        let decisions = FakeDecisions::new(Reply::Probability(0.9));
        let (pipeline, _api) = shadow_pipeline(decisions.clone(), 4, &old_output(), 1.0).await?;
        pipeline.seed([opening]).await?;
        pipeline.seed(tool_pair("current", "recent output")).await?;
        pipeline.steer(steer).await;

        pipeline.resume().await?;

        let request = decisions.last_request.lock().unwrap().clone().unwrap();
        assert_eq!(request.state["goal"], expected);
        assert_eq!(request.state["goal_truncated"], truncated);
        assert!(request.state["goal"].as_str().unwrap().chars().count() <= 800);
        assert!(!serde_json::to_string(&request)?.contains("SENTINEL"));
    }
    Ok(())
}

#[tokio::test]
async fn steering_after_an_observation_does_not_repeat_it_after_reconstruction() -> Result<()> {
    let decisions = FakeDecisions::new(Reply::Probability(0.9));
    let (pipeline, _api) = shadow_pipeline(decisions.clone(), 4, &old_output(), 1.0).await?;
    pipeline
        .seed([Message::user()
            .with_id("original-kickoff")
            .with_text("current goal")])
        .await?;
    let cancel = CancellationToken::new();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let emit = Emitter::new(tx, cancel.clone());
    let session = pipeline.session().await?;
    let operation =
        ContextRelevanceOperation::new(decisions.clone(), pipeline.context_limit(), 0.8);
    let OperationResult::Applied(mut observed) = operation
        .run(&session, session.conversation.as_ref().unwrap(), &emit)
        .await?
    else {
        panic!("the turn is observed before compaction")
    };
    pipeline
        .machine(cancel)
        .await
        .apply(
            pipeline.session_manager.as_ref(),
            &session,
            &mut observed.effects,
            &emit,
        )
        .await?;
    pipeline
        .seed([Message::user().with_text("also use blue").with_steer()])
        .await?;
    let pipeline = pipeline.reconstruct().await?;

    let run = pipeline.resume().await?;

    assert_eq!(decisions.calls(), 1);
    assert_eq!(turns(&run).len(), 1);
    assert_eq!(turns(&run)[0]["kickoff_id"], "original-kickoff");
    Ok(())
}

#[tokio::test]
async fn archived_kickoff_is_not_reobserved_as_a_compaction_replay() -> Result<()> {
    let decisions = FakeDecisions::new(Reply::Probability(0.9));
    let (pipeline, _api) = shadow_pipeline(decisions.clone(), 4, &old_output(), 1.0).await?;
    let first = pipeline.run(["current goal"]).await?;
    let original_id = turns(&first)[0]["kickoff_id"].clone();
    assert_eq!(first.history_replacements(), 1);
    for index in 0..5 {
        pipeline
            .seed(tool_pair(
                &format!("after-compaction-{index}"),
                &old_output(),
            ))
            .await?;
    }
    pipeline
        .set_total_tokens(pipeline.context_limit() as i32)
        .await;
    let pipeline = pipeline.reconstruct().await?;
    pipeline
        .steer(Message::user().with_text("continue this goal"))
        .await;

    let run = pipeline.resume().await?;

    assert_eq!(decisions.calls(), 1);
    assert_eq!(turns(&run).len(), 1);
    assert_eq!(turns(&run)[0]["kickoff_id"], original_id);
    Ok(())
}

#[tokio::test]
async fn oversized_request_is_recorded_without_a_decision_call() -> Result<()> {
    let decisions = FakeDecisions::new(Reply::Probability(0.1));
    // Each control character serializes as a six-byte JSON escape.
    let (pipeline, _api) =
        shadow_pipeline(decisions.clone(), 16, &"\u{1}".repeat(800), 1.0).await?;

    let run = pipeline.run(["current goal"]).await?;

    let note = &turns(&run)[0];
    assert_eq!(note["status"], "input_budget_exceeded");
    assert!(note.get("observations").is_none());
    assert_eq!(decisions.calls(), 0);
    Ok(())
}

#[tokio::test]
async fn failed_or_invalid_decisions_never_propose_omission() -> Result<()> {
    let (pipeline, _api) =
        shadow_pipeline(FakeDecisions::new(Reply::Fail), 4, &old_output(), 1.0).await?;
    let note = &turns(&pipeline.run(["current goal"]).await?)[0];
    assert_eq!(note["status"], "provider_error");
    let error = note["error"].as_str().unwrap();
    assert!(error.contains("synthetic failure"));
    assert_eq!(error.chars().count(), 500);
    assert!(note.get("observations").is_none());

    for probability in [f64::NAN, -0.1, 1.1] {
        let decisions = FakeDecisions::new(Reply::Probability(probability));
        let (pipeline, _api) = shadow_pipeline(decisions, 4, &old_output(), 1.0).await?;
        let note = &turns(&pipeline.run(["current goal"]).await?)[0];
        let observation = &note["observations"][0];
        assert_eq!(observation["valid"], false);
        assert_eq!(observation["would_drop"], false);
    }
    Ok(())
}

/// Runs the operation directly because a paused clock stalls the SQLite pool
/// that the pipeline depends on.
#[tokio::test(start_paused = true)]
async fn a_decision_that_never_answers_times_out() {
    let operation = ContextRelevanceOperation::new(FakeDecisions::new(Reply::Hang), 10, 0.3);
    let mut messages = vec![Message::user().with_text("old goal")];
    for index in 0..4 {
        messages.extend(tool_pair(&format!("call-{index}"), &old_output()));
    }
    messages.push(Message::user().with_id("kickoff").with_text("current goal"));
    let mut session = Session::default();
    session.usage.total_tokens = Some(4);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    let result = operation
        .run(
            &session,
            &Conversation::new_unvalidated(messages),
            &Emitter::new(tx, CancellationToken::new()),
        )
        .await
        .unwrap();

    let OperationResult::Applied(result) = result else {
        panic!("a timeout is recorded")
    };
    let [GooseEffect::SetExtensionState { value, .. }] = result.effects.as_slice() else {
        panic!("the shadow operation may only record its own state")
    };
    assert_eq!(value["turns"][0]["status"], "timeout");
}

#[tokio::test]
async fn cancelling_during_a_decision_records_nothing() -> Result<()> {
    let decisions = FakeDecisions::new(Reply::Hang);
    let (pipeline, _api) = shadow_pipeline(decisions.clone(), 4, &old_output(), 1.0).await?;
    let cancel = CancellationToken::new();
    let waiting = decisions.clone();
    let canceller = cancel.clone();
    tokio::spawn(async move {
        while waiting.calls() == 0 {
            tokio::task::yield_now().await;
        }
        canceller.cancel();
    });

    let run = pipeline.run_with_cancel("current goal", cancel).await?;

    assert_eq!(decisions.calls(), 1);
    assert!(turns(&run).is_empty());
    assert_eq!(run.history_replacements(), 0);
    Ok(())
}

#[test]
fn judges_only_complete_visible_single_results() {
    let sentinel = |name: &str| name.repeat(60);
    let [private_request, private_result] = tool_pair("private", &sentinel("PRIVATE"));
    let [hidden_request, hidden_result] = tool_pair("hidden-request", &sentinel("HIDDEN_PAIR"));
    let [_, orphan] = tool_pair("orphan", &sentinel("ORPHAN"));
    let external_request = Message::assistant().with_tool_request_with_metadata(
        "external",
        Ok(CallToolRequestParams::new("read")),
        None,
        Some(json!({"goose.external_dispatch": true})),
    );
    let [_, external_result] = tool_pair("external", &sentinel("EXTERNAL"));
    let bundle_request = Message::assistant()
        .with_tool_request("bundle-1", Ok(CallToolRequestParams::new("read")))
        .with_tool_request("bundle-2", Ok(CallToolRequestParams::new("read")));
    let bundle_result =
        ["bundle-1", "bundle-2"]
            .into_iter()
            .fold(Message::user(), |message, id| {
                message.with_tool_response(
                    id,
                    Ok(CallToolResult::success(vec![ContentBlock::text(sentinel(
                        "BUNDLE",
                    ))])),
                )
            });
    let [mixed_request, _] = tool_pair("mixed", "");
    let mixed_result = Message::user().with_tool_response(
        "mixed",
        Ok(CallToolResult::success(vec![
            ContentBlock::text("MIXED".repeat(100)),
            ContentBlock::image("aW1hZ2U=", "image/png"),
        ])),
    );
    let [recent_request, _] = tool_pair("recent", "");
    let recent_result = Message::user().with_tool_response(
        "recent",
        Ok(CallToolResult::success(vec![
            ContentBlock::text("RECENT_VISIBLE"),
            ContentBlock::Text(
                TextContent::new("RECENT_USER_ONLY")
                    .with_annotations(Annotations::default().with_audience(vec![Role::User])),
            ),
        ])),
    );
    let mut messages = vec![
        Message::user().with_text("old goal"),
        private_request.with_visibility(true, false),
        private_result.with_visibility(true, false),
        hidden_request.with_visibility(true, false),
        hidden_result,
        orphan,
        external_request,
        external_result,
        bundle_request,
        bundle_result,
        mixed_request,
        mixed_result,
    ];
    messages.extend(tool_pair("complete", &old_output()));
    let cutoff = messages.len();
    messages.extend([recent_request, recent_result]);
    let [_, recent_hidden] = tool_pair("recent-hidden", "RECENT_HIDDEN");
    messages.push(recent_hidden.with_visibility(true, false));
    messages.push(Message::user().with_text("current goal"));

    let request = ContextRelevanceOperation::request(
        &Conversation::new_unvalidated(messages),
        cutoff,
        "current goal".into(),
        false,
    );

    assert_eq!(request.questions.len(), 1);
    assert_eq!(
        request.state["candidates"][0]["tool_call_ids"],
        json!(["complete"])
    );
    let recent = request.state["recent_evidence_not_eligible_for_omission"]
        .as_array()
        .unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0]["excerpt"], "RECENT_VISIBLE");
    let serialized = serde_json::to_string(&request).unwrap();
    for excluded in [
        "PRIVATE",
        "HIDDEN_PAIR",
        "ORPHAN",
        "EXTERNAL",
        "BUNDLE",
        "MIXED",
        "RECENT_HIDDEN",
        "RECENT_USER_ONLY",
    ] {
        assert!(!serialized.contains(excluded), "{excluded} was sent");
    }
}

#[test]
fn recent_mixed_media_evidence_is_marked_incomplete() {
    let mut messages = vec![Message::user().with_text("old goal")];
    messages.extend(tool_pair("old", &old_output()));
    let cutoff = messages.len();
    messages.push(Message::user().with_tool_response(
        "recent",
        Ok(CallToolResult::success(vec![
            ContentBlock::text("recent text"),
            ContentBlock::image("IMAGE_SENTINEL", "image/png"),
        ])),
    ));
    messages.push(Message::user().with_text("current goal"));

    let request = ContextRelevanceOperation::request(
        &Conversation::new_unvalidated(messages),
        cutoff,
        "current goal".into(),
        false,
    );

    let recent = &request.state["recent_evidence_not_eligible_for_omission"][0];
    assert_eq!(recent["excerpt"], "recent text");
    assert_eq!(recent["truncated"], true);
    assert!(!serde_json::to_string(&request)
        .unwrap()
        .contains("IMAGE_SENTINEL"));
}

#[test]
fn request_preserves_tool_error_status_in_candidates_and_recent_evidence() {
    let mut messages = vec![Message::user().with_text("old goal")];
    for (id, is_error) in [("success", false), ("error", true)] {
        let [request, _] = tool_pair(id, "");
        let mut result = CallToolResult::success(vec![ContentBlock::text(old_output())]);
        result.is_error = Some(is_error);
        messages.extend([request, Message::user().with_tool_response(id, Ok(result))]);
    }
    let cutoff = messages.len();
    for (id, is_error) in [("recent-success", false), ("recent-error", true)] {
        let [request, _] = tool_pair(id, "");
        let mut result = CallToolResult::success(vec![ContentBlock::text(old_output())]);
        result.is_error = Some(is_error);
        messages.extend([request, Message::user().with_tool_response(id, Ok(result))]);
    }
    messages.push(Message::user().with_text("current goal"));

    let request = ContextRelevanceOperation::request(
        &Conversation::new_unvalidated(messages),
        cutoff,
        "current goal".into(),
        false,
    );

    let candidates = request.state["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0]["tool_call_ids"], json!(["error"]));
    assert_eq!(candidates[0]["is_error"], true);
    assert_eq!(candidates[1]["is_error"], false);
    let recent = request.state["recent_evidence_not_eligible_for_omission"]
        .as_array()
        .unwrap();
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0]["is_error"], false);
    assert_eq!(recent[1]["is_error"], true);
}

#[test]
fn request_keeps_the_newest_candidates_within_excerpt_limits() {
    let mut messages = vec![Message::user().with_text("old goal")];
    for index in 0..15 {
        messages.extend(tool_pair(&format!("old-{index}"), &"o".repeat(1_000)));
    }
    let cutoff = messages.len();
    for index in 0..10 {
        messages.extend(tool_pair(&format!("recent-{index}"), &"r".repeat(500)));
    }
    messages.push(Message::user().with_text("current goal"));

    let request = ContextRelevanceOperation::request(
        &Conversation::new_unvalidated(messages),
        cutoff,
        "current goal".into(),
        false,
    );

    let candidates = request.state["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 12);
    assert_eq!(candidates[0]["tool_call_ids"], json!(["old-14"]));
    assert_eq!(candidates[11]["tool_call_ids"], json!(["old-3"]));
    assert_eq!(candidates[0]["tool_name"], "read");
    assert_eq!(candidates[0]["arguments"], r#"{"path":"old-14"}"#);
    assert_eq!(candidates[0]["arguments_truncated"], false);
    let recent = request.state["recent_evidence_not_eligible_for_omission"]
        .as_array()
        .unwrap();
    assert_eq!(recent.len(), 8);
    let excerpt_chars = |entry: &Value| entry["excerpt"].as_str().unwrap().chars().count();
    assert!(candidates.iter().all(|entry| excerpt_chars(entry) == 800));
    assert!(recent.iter().all(|entry| excerpt_chars(entry) == 300));
    assert!(candidates
        .iter()
        .chain(recent)
        .all(|entry| entry["truncated"] == true));
    assert!(serde_json::to_vec(&request).unwrap().len() <= 48_000);
}

/// Panics if serialization runs to the end instead of stopping at the limit.
struct Unbounded;

impl serde::Serialize for Unbounded {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut items = serializer.serialize_seq(None)?;
        for _ in 0..10_000 {
            items.serialize_element("item")?;
        }
        panic!("serialization did not stop at the limit");
    }
}

#[test]
fn arguments_are_serialized_only_up_to_the_limit() {
    let long = json!({ "content": "é".repeat(1_000) });
    let expected: String = long.to_string().chars().take(300).collect();
    assert_eq!(truncated_json(&long, 300), (expected, true));

    let short = json!({ "path": "a" });
    assert_eq!(truncated_json(&short, 300), (short.to_string(), false));

    assert!(truncated_json(&Unbounded, 300).1);
}

#[tokio::test]
async fn opt_in_reports_setup_failures_and_skips_context_owning_providers() -> Result<()> {
    let _env = env_lock::lock_env([
        ("GOOSE_CONTEXT_RELEVANCE_SHADOW", None::<&str>),
        ("TYPESAFE_API_KEY", None),
    ]);
    let (compacting, _api) = test_pipeline().await?;
    let (context_owning, _api) = test_pipeline_with(ProviderFeatures {
        manages_own_context: true,
        ..ProviderFeatures::default()
    })
    .await?;
    let dir = tempfile::tempdir()?;
    let config = Config::new_with_file_secrets(
        dir.path().join("config.yaml"),
        dir.path().join("secrets.yaml"),
    )?;
    assert!(context_relevance_provider(&config, compacting.provider()).is_none());

    config.set_param("GOOSE_CONTEXT_RELEVANCE_SHADOW", true)?;
    let unavailable = context_relevance_provider(&config, compacting.provider()).unwrap();
    let request = ContextRelevanceOperation::request(
        &Conversation::new_unvalidated([Message::user().with_text("goal")]),
        0,
        "goal".into(),
        false,
    );
    let error = unavailable.create_decision(&request).await.unwrap_err();
    assert!(error.to_string().contains("TYPESAFE_API_KEY"));

    config.set_secret("TYPESAFE_API_KEY", &"test-key")?;
    assert!(context_relevance_provider(&config, compacting.provider()).is_some());
    assert!(context_relevance_provider(&config, context_owning.provider()).is_none());
    Ok(())
}
