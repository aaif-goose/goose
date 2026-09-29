use goose_provider_types::formats::openai_responses::create_responses_request;
use goose_provider_types::model::ModelConfig;
use goose_provider_types::thinking::ThinkingEffort;
use serde_json::{json, Value};
use std::collections::HashMap;

fn config(params: Value) -> ModelConfig {
    ModelConfig::new("gpt-5").with_merged_request_params(
        serde_json::from_value::<HashMap<String, Value>>(params).unwrap(),
    )
}

#[test]
fn maps_chat_completions_parameters_to_responses_fields() {
    let model = config(json!({"reasoning_effort": "minimal", "verbosity": "low"}));
    let request = create_responses_request(&model, "Be helpful.", &[], &[]).unwrap();

    assert_eq!(request["model"], "gpt-5");
    assert_eq!(request["reasoning"]["effort"], "minimal");
    assert_eq!(request["text"]["verbosity"], "low");
    assert!(request.get("reasoning_effort").is_none());
    assert!(request.get("verbosity").is_none());
}

#[test]
fn explicit_effort_overrides_suffix_and_generic_thinking_effort() {
    let mut model =
        config(json!({"reasoning_effort": "minimal"})).with_thinking_effort(ThinkingEffort::High);
    model.model_name = "gpt-5-high".to_string();
    let request = create_responses_request(&model, "", &[], &[]).unwrap();

    assert_eq!(request["model"], "gpt-5");
    assert_eq!(request["reasoning"]["effort"], "minimal");
}

#[test]
fn preserves_native_parameter_values() {
    for effort in ["none", "minimal", "low", "medium", "high", "xhigh", "max"] {
        let model = config(json!({"reasoning_effort": effort}));
        let request = create_responses_request(&model, "", &[], &[]).unwrap();
        assert_eq!(request["reasoning"]["effort"], effort);
    }
    for verbosity in ["low", "medium", "high"] {
        let model = config(json!({"verbosity": verbosity}));
        let request = create_responses_request(&model, "", &[], &[]).unwrap();
        assert_eq!(request["text"]["verbosity"], verbosity);
        assert!(request.get("reasoning").is_none());
    }
}

#[test]
fn absent_or_null_parameters_preserve_existing_defaults() {
    for params in [
        json!({}),
        json!({"reasoning_effort": null, "verbosity": null}),
    ] {
        let model = config(params);
        let request = create_responses_request(&model, "", &[], &[]).unwrap();
        assert!(request.get("reasoning").is_none());
        assert!(request.get("text").is_none());

        let model = model.with_thinking_effort(ThinkingEffort::Low);
        let request = create_responses_request(&model, "", &[], &[]).unwrap();
        assert_eq!(request["reasoning"]["effort"], "low");
    }
}

#[test]
fn rejects_invalid_explicit_parameters_instead_of_silently_using_defaults() {
    for (key, value) in [
        ("reasoning_effort", json!("minmal")),
        ("reasoning_effort", json!(1)),
        ("verbosity", json!("quiet")),
        ("verbosity", json!(false)),
    ] {
        let model = config(json!({key: value}));
        let error = create_responses_request(&model, "", &[], &[]).unwrap_err();
        assert!(error.to_string().contains(key), "{error}");
    }
}
