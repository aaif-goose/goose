mod support;

use eredu::api::LoadedModel;
use goose_local_inference::{
    eredu_adapter::generation::{prepare, Request},
    model::ModelSettings,
};
use goose_provider_types::{
    conversation::message::Message, model::ModelConfig, thinking::ThinkingEffort,
};
use serde_json::json;

fn model(
    template: &str,
    tokens: &[&str],
) -> (tempfile::TempDir, LoadedModel<support::MockBackend>) {
    let dir = tempfile::tempdir().unwrap();
    support::write_artifact(dir.path());
    let mut tokenizer = support::tokenizer();
    tokenizer
        .add_special_tokens(
            tokens
                .iter()
                .map(|token| tokenizers::AddedToken::from(*token, true)),
        )
        .unwrap();
    tokenizer
        .save(dir.path().join("tokenizer.json"), false)
        .unwrap();
    std::fs::write(dir.path().join("chat_template.jinja"), template).unwrap();
    let model = LoadedModel::load(
        support::MockBackend::new(Default::default(), vec![1]),
        dir.path(),
        (),
    )
    .unwrap();
    (dir, model)
}

fn request(effort: ThinkingEffort) -> Request {
    Request {
        model: ModelConfig::new("fixture").with_thinking_effort(effort),
        settings: ModelSettings::default(),
        system: "You are Goose".into(),
        messages: vec![Message::user().with_text("Look up code 7")],
        tools: vec![],
        message_id: "response-1".into(),
        model_load_ms: None,
    }
}

#[test]
fn lfm_native_tools_ignore_generic_thinking_but_validate_explicit_controls() {
    let (_dir, mut model) = model(
        include_str!("support/lfm2.jinja"),
        &["<|tool_call_start|>", "<|tool_call_end|>"],
    );
    let mut req = request(ThinkingEffort::High);
    req.tools = vec![rmcp::model::Tool::new(
        "lookup",
        "Look up a code",
        json!({"type":"object","properties":{"code":{"type":"integer"}},"required":["code"]})
            .as_object()
            .unwrap()
            .clone(),
    )];
    for effort in [
        ThinkingEffort::Off,
        ThinkingEffort::Low,
        ThinkingEffort::Medium,
        ThinkingEffort::High,
        ThinkingEffort::Max,
    ] {
        req.model = req.model.clone().with_thinking_effort(effort);
        let (prepared, emulated) = prepare(&mut model, &req).unwrap();
        assert!(!emulated);
        assert!(prepared.native_tool_support().is_supported());
        assert!(prepared.rendered_prompt().contains("Look up code 7"));
    }
    req.model
        .request_params
        .as_mut()
        .unwrap()
        .insert("reasoning_effort".into(), json!("low"));
    assert!(prepare(&mut model, &req).is_err());
    req.model
        .request_params
        .as_mut()
        .unwrap()
        .remove("reasoning_effort");
    req.settings.enable_thinking = Some(true);
    assert!(prepare(&mut model, &req).is_err());
}

#[test]
fn supported_effort_changes_the_prompt_and_explicit_controls_keep_precedence() {
    let (_dir, mut model) = model(
        include_str!("support/muse-reasoning.jinja"),
        &["<|start|>", "<|message|>", "<|eom|>", "<|eot|>"],
    );
    for (effort, expected) in [
        (ThinkingEffort::Low, "low"),
        (ThinkingEffort::Max, "xhigh"),
        (ThinkingEffort::Off, "high"),
    ] {
        let (prepared, _) = prepare(&mut model, &request(effort)).unwrap();
        assert!(prepared.capabilities().reasoning_parser.is_supported());
        assert!(prepared
            .rendered_prompt()
            .contains(&format!("Reasoning strength: {expected}.")));
    }
    let mut req = request(ThinkingEffort::Off);
    req.model
        .request_params
        .as_mut()
        .unwrap()
        .insert("reasoning_effort".into(), json!("medium"));
    let (prepared, _) = prepare(&mut model, &req).unwrap();
    assert!(prepared
        .rendered_prompt()
        .contains("Reasoning strength: medium."));
    req.model
        .request_params
        .as_mut()
        .unwrap()
        .insert("reasoning_effort".into(), json!("invalid"));
    assert!(prepare(&mut model, &req).is_err());
    let mut req = request(ThinkingEffort::Low);
    req.model.request_params.as_mut().unwrap().insert(
        "chat_template_kwargs".into(),
        json!({"reasoning_strength":"medium"}),
    );
    let (prepared, _) = prepare(&mut model, &req).unwrap();
    assert!(prepared
        .rendered_prompt()
        .contains("Reasoning strength: medium."));
}
