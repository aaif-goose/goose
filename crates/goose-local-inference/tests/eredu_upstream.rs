mod support;
use eredu::{
    api::{LoadedModel, PreparedChatGenerationRequest, PreparedChatInput},
    runtime::chat::{ChatTemplateRequest, ToolChoice},
};
use eredu_core::SemanticEvent;
use serde_json::json;

#[test]
#[ignore = "requires GOOSE_EREDU_TEST_MODEL pointing at the LFM2.5 checkpoint"]
fn lfm_native_tool_activation_succeeds_without_goose_conversion() {
    let path = std::env::var("GOOSE_EREDU_TEST_MODEL").unwrap();
    let dir = tempfile::tempdir().unwrap();
    support::write_artifact(dir.path());
    for name in [
        "tokenizer.json",
        "tokenizer_config.json",
        "chat_template.jinja",
    ] {
        std::fs::copy(
            std::path::Path::new(&path).join(name),
            dir.path().join(name),
        )
        .unwrap();
    }
    let tokenizer = tokenizers::Tokenizer::from_file(dir.path().join("tokenizer.json")).unwrap();
    let output = tokenizer
        .encode(
            "<|tool_call_start|>[lookup(code=7)]<|tool_call_end|>",
            false,
        )
        .unwrap()
        .get_ids()
        .to_vec();
    let backend = support::MockBackend::new(Default::default(), output);
    let mut model = LoadedModel::load(backend, dir.path(), ()).unwrap();
    model
        .prepare_chat(ChatTemplateRequest {
            messages: vec![json!({"role":"user","content":"hello"})],
            add_generation_prompt: true,
            ..Default::default()
        })
        .unwrap();
    let prepared = model.prepare_chat(ChatTemplateRequest {
        messages: vec![json!({"role":"user","content":"look up code 7"})],
        tools: vec![json!({"type":"function","function":{"name":"lookup","description":"Look up a code","parameters":{"type":"object","properties":{"code":{"type":"integer","minimum":1}},"required":["code"]}}})],
        tool_choice: ToolChoice::Auto,
        add_generation_prompt: true,
        ..Default::default()
    }).unwrap();
    let mut events = Vec::new();
    let result = model
        .generate_prepared_chat(PreparedChatGenerationRequest {
            input: PreparedChatInput::rendered_prompt(&prepared),
            settings: Default::default(),
            caller_stop_sequences: &[],
            cancellation: Default::default(),
            on_event: |event| events.push(event),
        })
        .unwrap();
    assert!(!result.token_ids.is_empty());
    assert!(
        matches!(events.first(), Some(SemanticEvent::ToolCallStart { name, .. }) if name == "lookup")
    );
    let arguments: String = events
        .iter()
        .filter_map(|event| match event {
            SemanticEvent::ToolArgumentsDelta { json_fragment, .. } => Some(json_fragment.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&arguments).unwrap(),
        json!({"code": 7})
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, SemanticEvent::ToolCallEnd))
            .count(),
        1
    );
}
