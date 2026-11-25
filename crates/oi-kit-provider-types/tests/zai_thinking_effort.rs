use oi_kit_provider_types::conversation::message::{Message, MessageContentBlock};
use oi_kit_provider_types::formats::anthropic::{create_request, AnthropicFormatOptions};
use oi_kit_provider_types::model::ModelConfig;
use oi_kit_provider_types::thinking::ThinkingEffort;
use serde_json::{json, Value};

fn zai_request(model: &str, effort: Option<ThinkingEffort>, messages: &[Message]) -> Value {
    let mut config = ModelConfig::new(model);
    if let Some(effort) = effort {
        config = config.with_thinking_effort(effort);
    }
    create_request(
        "zai",
        &config,
        "system",
        messages,
        &[],
        AnthropicFormatOptions {
            preserve_unsigned_thinking: true,
            preserve_thinking_context: true,
            emit_clear_thinking: true,
            ..Default::default()
        },
    )
    .unwrap()
}

fn hello() -> Vec<Message> {
    vec![Message::user().with_text("Hello")]
}

#[test]
fn glm_5_3_sends_its_effort_levels() {
    for (effort, expected) in [
        (None, "max"),
        (Some(ThinkingEffort::Off), "low"),
        (Some(ThinkingEffort::Low), "low"),
        (Some(ThinkingEffort::Medium), "high"),
        (Some(ThinkingEffort::High), "high"),
        (Some(ThinkingEffort::Max), "max"),
    ] {
        let payload = zai_request("glm-5.3", effort, &hello());
        assert_eq!(
            payload["thinking"],
            json!({"type": "enabled", "clear_thinking": false}),
            "{effort:?}"
        );
        assert_eq!(
            payload["output_config"],
            json!({"effort": expected}),
            "{effort:?}"
        );
    }
}

#[test]
fn glm_5_2_off_turns_thinking_off() {
    let payload = zai_request("glm-5.2", Some(ThinkingEffort::Off), &hello());
    assert!(payload.get("thinking").is_none());
    assert!(payload.get("output_config").is_none());
}

#[test]
fn glm_5_2_uses_its_lowest_level_for_low_settings() {
    for (effort, expected) in [
        (ThinkingEffort::Low, "high"),
        (ThinkingEffort::Medium, "high"),
        (ThinkingEffort::High, "high"),
        (ThinkingEffort::Max, "max"),
    ] {
        let payload = zai_request("glm-5.2", Some(effort), &hello());
        assert_eq!(
            payload["output_config"],
            json!({"effort": expected}),
            "{effort:?}"
        );
    }
}

#[test]
fn glm_5_3_off_keeps_prior_thinking() {
    let messages = vec![
        Message::assistant().with_content(MessageContentBlock::thinking("internal", "")),
        Message::user().with_text("Continue"),
    ];
    let payload = zai_request("glm-5.3", Some(ThinkingEffort::Off), &messages);
    assert_eq!(payload["messages"][0]["content"][0]["type"], "thinking");
    assert_eq!(payload["messages"][0]["content"][0]["thinking"], "internal");
}

#[test]
fn glm_4_7_keeps_its_token_budget() {
    let payload = zai_request("glm-4.7", Some(ThinkingEffort::High), &hello());
    assert_eq!(payload["thinking"]["type"], "enabled");
    assert!(payload["thinking"]["budget_tokens"].is_i64());
    assert!(payload.get("output_config").is_none());
}
