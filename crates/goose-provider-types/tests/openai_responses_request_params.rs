use goose_provider_types::formats::openai_responses::{
    create_responses_request, create_responses_request_for_model,
};
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
    for (model_name, effort) in [
        ("gpt-5", "minimal"),
        ("gpt-5", "low"),
        ("gpt-5", "medium"),
        ("gpt-5", "high"),
        ("gpt-5-mini", "minimal"),
        ("gpt-5-nano", "minimal"),
        ("gpt-5-2025-08-07", "minimal"),
        ("gpt-5-pro", "high"),
        ("gpt-5.1", "none"),
        ("gpt-5.1-codex-max", "xhigh"),
        ("gpt-5.2", "none"),
        ("gpt-5.2-pro", "medium"),
        ("gpt-5.2-pro", "xhigh"),
        ("gpt-5.4", "xhigh"),
        ("gpt-6-sol", "max"),
    ] {
        let mut model = config(json!({"reasoning_effort": effort}));
        model.model_name = model_name.to_string();
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

#[test]
fn rejects_efforts_unsupported_by_the_selected_model() {
    for (model_name, effort) in [
        ("gpt-5", "max"),
        ("gpt-5", "none"),
        ("gpt-5", "xhigh"),
        ("gpt-5-mini", "max"),
        ("gpt-5-pro", "minimal"),
        ("gpt-5.1", "minimal"),
        ("gpt-5.1-codex", "none"),
        ("gpt-5.2-pro", "low"),
        ("gpt-5.4", "minimal"),
        ("gpt-5.4", "max"),
        ("gpt-6-astra", "none"),
        ("o3", "minimal"),
    ] {
        let mut model = config(json!({"reasoning_effort": effort}));
        model.model_name = model_name.to_string();
        let error = create_responses_request(&model, "", &[], &[]).unwrap_err();
        assert!(error.to_string().contains("reasoning_effort"), "{error}");
    }
}

#[test]
fn validates_capability_model_when_wire_name_is_an_alias() {
    let mut model = config(json!({"reasoning_effort": "max"}));
    model.model_name = "deployment-alias".to_string();
    let result =
        create_responses_request_for_model(&model, "deployment-alias", "gpt-5", "", &[], &[]);
    assert!(result.is_err());

    let model = model.with_merged_request_params(HashMap::from([(
        "reasoning_effort".to_string(),
        json!("minimal"),
    )]));
    let request =
        create_responses_request_for_model(&model, "deployment-alias", "gpt-5", "", &[], &[])
            .unwrap();
    assert_eq!(request["model"], "deployment-alias");
    assert_eq!(request["reasoning"]["effort"], "minimal");
}
