use anyhow::Result;

use self::calculator_extension::{value, ADD};
use self::dummy_api::ProviderFeatures;
use self::pipeline::MessageKind::{Agent, ToolCall};
use self::pipeline::{test_pipeline, test_pipeline_with, MAX_TURNS};
use crate::agents::state_machine;
use crate::agents::state_machine::ops_retry::NUDGED;
use crate::agents::state_machine::Emitter;
use crate::session::extension_data::{EnabledExtensionsState, ExtensionData, ExtensionState};

mod agent_reply;
mod calculator_extension;
mod compaction_lifecycle;
mod dummy_api;
mod hooks_lifecycle;
mod pipeline;
mod prompt_skill_lifecycle;
mod provider_lifecycle;
mod recipe_scheduling_lifecycle;
mod reconstruction_isolation_lifecycle;
mod steering_lifecycle;
mod tool_lifecycle;

async fn capture_state_machine_trace_fields(
    capture_setting: Option<&'static str>,
) -> Result<serde_json::Map<String, serde_json::Value>> {
    use goose_test_support::otel::clear_otel_env;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;
    use tracing_futures::Instrument;

    use crate::agents::gen_ai_telemetry::{
        test_support::SpanFieldCapture, CAPTURE_MESSAGE_CONTENT_ENV,
    };
    use crate::conversation::message::Message;

    let _env = match capture_setting {
        Some(value) => clear_otel_env(&[(CAPTURE_MESSAGE_CONTENT_ENV, value)]),
        None => clear_otel_env(&[]),
    };
    let capture = SpanFieldCapture::new("state_machine_security_trace");
    let _subscriber = capture.clone().set_default();
    let (pipeline, api) = test_pipeline().await?;
    api.on("input-super-secret-token")
        .reply("output-super-secret-token");
    pipeline
        .session_manager
        .add_message(
            &pipeline.session_id,
            &Message::user().with_text("input-super-secret-token"),
        )
        .await?;

    let cancel = CancellationToken::new();
    let machine = pipeline.machine(cancel.clone());
    let (tx, _rx) = mpsc::channel(1024);
    let emit = Emitter::new(tx, cancel);
    let span = tracing::info_span!(
        "state_machine_security_trace",
        trace_input = tracing::field::Empty,
        trace_output = tracing::field::Empty,
        gen_ai.agent.name = tracing::field::Empty,
        gen_ai.output.messages = tracing::field::Empty,
    );
    super::session::run(
        &machine,
        pipeline.session_manager.as_ref(),
        &pipeline.session_id,
        &emit,
    )
    .instrument(span)
    .await?;

    Ok(capture.fields())
}

#[tokio::test]
async fn state_machine_trace_omits_content_without_capture() -> Result<()> {
    for capture_setting in [None, Some("false")] {
        let fields = capture_state_machine_trace_fields(capture_setting).await?;
        let recorded = serde_json::to_string(&fields)?;
        assert!(!recorded.contains("super-secret-token"));
        assert!(!fields.contains_key("trace_input"));
        assert!(!fields.contains_key("trace_output"));
        assert!(!fields.contains_key("gen_ai.output.messages"));
        assert_eq!(fields["gen_ai.agent.name"], "goose");
    }
    Ok(())
}

#[tokio::test]
async fn state_machine_trace_retains_content_with_capture() -> Result<()> {
    let fields = capture_state_machine_trace_fields(Some("true")).await?;
    assert_eq!(fields["trace_input"], "input-super-secret-token");
    assert_eq!(fields["trace_output"], "output-super-secret-token");
    assert!(fields["gen_ai.output.messages"]
        .as_str()
        .unwrap()
        .contains("output-super-secret-token"));
    Ok(())
}

#[tokio::test]
async fn bang_shell_requests_the_shell_tool() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;

    let result = pipeline.run(["!echo hello"]).await?;
    result.assert_message(1, ToolCall, r#"shell({"command":"echo hello"})"#);
    assert_eq!(api.call_count(), 0);

    Ok(())
}

#[tokio::test]
async fn doctor_refuses_without_developer_before_inference() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    let mut extension_data = ExtensionData::default();
    EnabledExtensionsState::new(Vec::new()).to_extension_data(&mut extension_data)?;
    pipeline
        .session_manager
        .update(&pipeline.session_id)
        .extension_data(extension_data)
        .apply()
        .await?;

    let result = pipeline.run(["/doctor"]).await?;

    result.assert_message(
        -1,
        Agent,
        crate::doctor::DEVELOPER_EXTENSION_REQUIRED_MESSAGE,
    );
    assert_eq!(api.call_count(), 0);

    Ok(())
}

#[tokio::test]
async fn max_turns_counts_inference_calls_and_injects_budget() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    api.on("keep going").call(ADD, value(1));

    let result = pipeline.run(["keep going"]).await?;
    let calls = api.calls();
    assert_eq!(calls.len(), MAX_TURNS as usize);
    assert_eq!(pipeline.calculator_total(), MAX_TURNS as i64);

    let first_budgeted_call = MAX_TURNS.div_ceil(2) as usize;
    assert!(!calls[first_budgeted_call - 1].input_contains("<turn-budget>"));
    assert!(calls[first_budgeted_call].input_contains("<turn-budget>"));
    result.assert_message(-1, Agent, state_machine::MAX_TURNS_MESSAGE);
    result.assert_emitted_message_matches_persisted(state_machine::MAX_TURNS_MESSAGE);

    Ok(())
}

#[tokio::test]
async fn auto_effort_is_scoped_to_each_turn_and_reused_across_inferences() -> Result<()> {
    use std::sync::Arc;

    use goose_providers::api_client::{ApiClient, AuthMethod};
    use goose_providers::model::ModelConfig;
    use goose_providers::thinking::{
        ThinkingEffort, ThinkingEffortCapability, ThinkingEffortOption, ThinkingEffortSupport,
    };
    use goose_providers::typesafe::TypeSafeProvider;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::agents::agent::available_auto_efforts;
    use crate::agents::state_machine::AutoEffortOperation;

    let (pipeline, api) = test_pipeline_with(ProviderFeatures {
        thinking_effort_options: true,
        ..ProviderFeatures::default()
    })
    .await?;
    let skill_dir = pipeline
        .working_dir()
        .join(".agents/skills/auto-effort-review");
    std::fs::create_dir_all(&skill_dir)?;
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: auto-effort-review\ndescription: Review helper\n---\nReview the expanded request carefully.\n",
    )?;
    let skill = crate::skills::list_installed_skills(Some(pipeline.working_dir()))
        .into_iter()
        .find(|skill| skill.name == "auto-effort-review")
        .expect("project skill should be installed");
    let expanded_skill_prompt = crate::skills::loaded_skill_context_with_args(&skill, None)?;

    let jev = MockServer::start().await;
    for (request, effort) in [
        ("add one", "high"),
        (expanded_skill_prompt.as_str(), "high"),
        ("hello", "off"),
    ] {
        let probabilities = std::collections::HashMap::from([(effort, 0.9)]);
        Mock::given(method("POST"))
            .and(path("/v1/systemone"))
            .and(header("authorization", "Bearer test-key"))
            .and(body_partial_json(json!({
                "model": "test-effort-model",
                "state": request
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "model": "jev-latest",
                "answers": {
                    "effort": {
                        "type": "choice",
                        "choice": effort,
                        "confidence": 0.9,
                        "probabilities": probabilities
                    }
                },
                "usage": {"input_tokens": 12, "output_tokens": 3}
            })))
            .expect(1)
            .mount(&jev)
            .await;
    }
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer test-key"))
        .and(body_partial_json(json!({
            "model": "test-effort-model",
            "state": "fallback"
        })))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&jev)
        .await;

    let decision_provider = TypeSafeProvider::new(ApiClient::new_with_tls(
        jev.uri(),
        AuthMethod::BearerToken("test-key".to_string()),
        None,
    )?);
    let efforts = available_auto_efforts(
        "claude-acp",
        &ModelConfig::new("current"),
        ThinkingEffortSupport::Options(ThinkingEffortCapability {
            option_id: "effort".to_string(),
            values: vec![
                ThinkingEffortOption {
                    value: "default".to_string(),
                    label: "Default".to_string(),
                },
                ThinkingEffortOption {
                    value: "high".to_string(),
                    label: "High".to_string(),
                },
            ],
            current: Some("default".to_string()),
        }),
        None,
    );
    assert_eq!(
        available_auto_efforts(
            "openai",
            &ModelConfig::new("gpt-5-pro"),
            ThinkingEffortSupport::Unspecified,
            None,
        ),
        vec![ThinkingEffort::High]
    );
    assert_eq!(
        available_auto_efforts(
            "anthropic",
            &ModelConfig::new("claude-opus-5-5").with_canonical_limits("anthropic"),
            ThinkingEffortSupport::Unspecified,
            Some("anthropic"),
        ),
        vec![
            ThinkingEffort::Low,
            ThinkingEffort::Medium,
            ThinkingEffort::High,
            ThinkingEffort::Max,
        ]
    );
    assert_eq!(
        available_auto_efforts(
            "google",
            &ModelConfig::new("gemini-3-pro-preview"),
            ThinkingEffortSupport::Unspecified,
            None,
        ),
        vec![ThinkingEffort::Low, ThinkingEffort::High]
    );
    let mut ollama_model = ModelConfig::new("gpt-oss:20b");
    ollama_model.reasoning = Some(true);
    assert_eq!(
        available_auto_efforts(
            "ollama",
            &ollama_model,
            ThinkingEffortSupport::Unspecified,
            None,
        ),
        vec![
            ThinkingEffort::Off,
            ThinkingEffort::Low,
            ThinkingEffort::Medium,
            ThinkingEffort::High,
        ]
    );
    let pipeline = pipeline
        .with_model_config(
            ModelConfig::new(goose_providers::openai::OPEN_AI_DEFAULT_MODEL)
                .with_canonical_limits("openai")
                .with_thinking_effort(ThinkingEffort::Low),
        )
        .await
        .record_model_configs()
        .with_operation(Arc::new(AutoEffortOperation::new(
            Arc::new(decision_provider),
            "test-effort-model".to_string(),
            efforts,
        )));
    api.on("add one").call(ADD, value(1));
    api.on("result: 1").reply("The total is 1");
    api.on("Review the expanded request carefully")
        .reply("reviewed");
    api.on("hello").reply("hi there!");
    api.on("fallback").reply("using configured effort");

    let result = pipeline
        .run(["add one", "/auto-effort-review", "hello", "fallback"])
        .await?;

    let requests = jev
        .received_requests()
        .await
        .expect("requests should be recorded");
    assert_eq!(requests.len(), 4);
    for request in requests {
        let body: serde_json::Value = serde_json::from_slice(&request.body)?;
        let criteria = body["questions"]["effort"]["criteria"]
            .as_object()
            .expect("effort criteria should be an object");
        assert_eq!(criteria.len(), 2);
        assert!(criteria.contains_key("off"));
        assert!(criteria.contains_key("high"));
    }

    let conversation = result.conversation();
    let events: Vec<_> = conversation
        .messages()
        .iter()
        .filter(|message| message.is_turn_context())
        .collect();
    assert_eq!(
        events.len(),
        4,
        "one turn-context event per turn; the turn's second inference reuses it"
    );
    assert!(events.iter().all(|event| !event.is_user_visible()));
    assert!(api
        .calls()
        .iter()
        .all(|call| call.input_contains("<turn-context>")));
    assert_eq!(
        pipeline.recorded_efforts(),
        vec![
            Some(ThinkingEffort::High),
            Some(ThinkingEffort::High),
            Some(ThinkingEffort::High),
            Some(ThinkingEffort::Off),
            Some(ThinkingEffort::Low),
        ]
    );
    assert_eq!(
        pipeline.recorded_automatic_efforts(),
        [true, true, true, true, false]
    );

    let decisions: Vec<_> = conversation
        .messages()
        .iter()
        .filter_map(|message| message.metadata.operation_note("auto_effort", "decision"))
        .collect();
    assert_eq!(decisions.len(), 4);
    assert_eq!(decisions[0]["effort"], "high");
    assert_eq!(decisions[0]["probabilities"]["high"], 0.9);
    assert_eq!(decisions[1]["effort"], "high");
    assert_eq!(decisions[2]["effort"], "off");
    assert!(decisions[3]["effort"].is_null());

    let logged_messages: Vec<_> = conversation
        .messages()
        .iter()
        .filter(|message| message.metadata.has_operation_logs())
        .collect();
    assert_eq!(logged_messages.len(), 3);
    assert_eq!(
        logged_messages[0].metadata.operation_logs(),
        ["ops_auto_effort: thinking high"]
    );
    assert_eq!(
        logged_messages[1].metadata.operation_logs(),
        ["ops_auto_effort: thinking high"]
    );
    assert_eq!(
        logged_messages[2].metadata.operation_logs(),
        ["ops_auto_effort: thinking off"]
    );
    assert!(logged_messages[0].is_tool_call());
    assert_eq!(logged_messages[1].as_concat_text(), "reviewed");
    assert_eq!(logged_messages[2].as_concat_text(), "hi there!");
    assert_eq!(
        result
            .session
            .model_config
            .as_ref()
            .and_then(|config| config.thinking_effort()),
        Some(ThinkingEffort::Low)
    );

    Ok(())
}

#[tokio::test]
async fn goal_starts_nudges_and_clears_when_met() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    api.on("Start working toward this goal now")
        .reply("did some work");
    api.on("fully met").reply("goal is met");

    let (pipeline, result, _) = pipeline
        .run_reconstructing_each_step("/goal finish the migration")
        .await?;

    assert_eq!(api.call_count(), 2);
    assert!(pipeline.get_goal().await.is_none());
    result.assert_message(-1, Agent, "goal is met");

    let command = result
        .conversation()
        .messages()
        .iter()
        .find(|message| message.as_concat_text() == "/goal finish the migration")
        .expect("persisted goal command");
    assert!(command.is_user_visible());
    assert!(!command.is_agent_visible());

    result
        .conversation()
        .messages()
        .iter()
        .find(|message| {
            message.as_concat_text().contains("finish the migration")
                && !message.is_user_visible()
                && message.is_agent_visible()
        })
        .expect("hidden goal kickoff");
    result
        .conversation()
        .messages()
        .iter()
        .find(|message| {
            message.as_concat_text().contains("fully met")
                && !message.is_user_visible()
                && message.is_agent_visible()
        })
        .expect("hidden goal nudge");
    assert_eq!(
        result
            .conversation()
            .messages()
            .iter()
            .filter(|message| message.metadata.operation_note("retry", NUDGED).is_some())
            .count(),
        1
    );

    Ok(())
}

#[tokio::test]
async fn grind_is_bounded_by_max_turns() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    api.on("go").reply("grinding");
    api.on("never done").reply("grinding");
    pipeline.set_grind(Some("never done".to_string())).await;

    let result = pipeline.run(["go"]).await?;

    assert_eq!(api.call_count(), MAX_TURNS as usize);
    result.assert_message(-1, Agent, state_machine::MAX_TURNS_MESSAGE);

    Ok(())
}

#[tokio::test]
async fn slash_commands_yield_or_fall_through_to_inference() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;

    let status = pipeline.run(["/status"]).await?;
    assert_eq!(api.call_count(), 0);
    status.assert_message(-1, Agent, "Provider:");
    assert!(status
        .conversation()
        .messages()
        .iter()
        .all(|message| message.is_user_visible() && !message.is_agent_visible()));

    api.on("/not-a-command").reply("saw it");
    let unknown = pipeline.run(["/not-a-command"]).await?;
    assert_eq!(api.call_count(), 1);
    unknown.assert_message(-1, Agent, "saw it");
    let command = unknown
        .conversation()
        .messages()
        .iter()
        .find(|message| message.as_concat_text() == "/not-a-command")
        .expect("persisted user message");
    assert!(command.is_user_visible());
    assert!(command.is_agent_visible());

    Ok(())
}
