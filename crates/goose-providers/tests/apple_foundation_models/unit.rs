use super::*;
use rmcp::model::{CallToolResult, ContentBlock};
use serde_json::json;
use std::sync::Arc;

#[test]
fn desktop_setup_catalog_includes_provider_without_loading_native_model() {
    let catalog = crate::canonical::catalog::get_setup_catalog_entries([
        AppleFoundationModelsProvider::metadata(),
    ]);
    let entry = catalog
        .iter()
        .find(|entry| entry.provider_id == PROVIDER_NAME)
        .unwrap();
    assert_eq!(entry.category, ProviderSetupCategory::Model);
    assert_eq!(entry.setup_method, ProviderSetupMethod::None);
}

#[test]
fn request_accepts_shell_timeout_format_annotation() {
    let tool = Tool::new(
        "developer__shell",
        "Run a shell command",
        Arc::new(
            json!({
                "type":"object",
                "properties": {
                    "command":{"type":"string"},
                    "timeout_secs":{
                        "type":["integer","null"], "format":"uint64", "minimum":0,
                        "description":"Maximum time in seconds to allow the command to run."
                    }
                },
                "required":["command"]
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    let request = create_request(
        &ModelConfig::new(MODEL_NAME),
        "",
        &[Message::user().with_text("Hello")],
        &[tool],
    )
    .unwrap();
    let schema: Value = serde_json::from_str(&request.tools[0].schema).unwrap();
    assert_eq!(schema["required"], json!(["command"]));
    let timeout = &schema["properties"]["timeout_secs"]["anyOf"][0];
    assert_eq!(timeout["type"], "integer");
    assert_eq!(timeout["minimum"], 0);
    assert!(timeout["description"]
        .as_str()
        .unwrap()
        .contains("Format: uint64."));
    assert!(timeout.get("format").is_none());
}

fn tool() -> Tool {
    Tool::new(
        "developer__read",
        "Read a file",
        Arc::new(
            json!({
                "type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    )
}

fn map_tool(additional: Value) -> Tool {
    Tool::new(
        "summon__delegate",
        "Run a task with parameters",
        Arc::new(
            json!({
                "type":"object",
                "properties": {
                    "source":{"type":"string"},
                    "parameters":{"type":"object", "additionalProperties":additional}
                },
                "required":["source","parameters"]
            })
            .as_object()
            .unwrap()
            .clone(),
        ),
    )
}

fn wrapped_arguments(value: Value) -> String {
    json!({"arguments_json":value.to_string()}).to_string()
}

#[test]
fn open_maps_round_trip_through_goose_tool_calls_and_replayed_history() {
    let tools = [map_tool(json!(true)), tool()];
    let arguments = json!({"source":"review", "parameters":{
        "arbitrary/key": {"nested":[true, null, 42, "a \"quoted\" string"]},
        "arguments_json":"This is an original argument, not an envelope"
    }});
    let request = create_request(
        &ModelConfig::new(MODEL_NAME),
        "",
        &[Message::user().with_text("Review")],
        &tools,
    )
    .unwrap();
    let schema: Value = serde_json::from_str(&request.tools[0].schema).unwrap();
    assert_eq!(schema["properties"]["arguments_json"]["type"], "string");
    assert!(schema["properties"]["arguments_json"]["description"]
        .as_str()
        .unwrap()
        .contains("additionalProperties"));
    let encoded = wrapped_arguments(arguments.clone());
    let (message, _) = convert_response(
        response(vec![afm::Entry::ToolCalls {
            calls: vec![
                afm::ToolCall {
                    id: "map".into(),
                    name: tools[0].name.to_string(),
                    arguments: encoded.clone(),
                },
                afm::ToolCall {
                    id: "read".into(),
                    name: tools[1].name.to_string(),
                    arguments: r#"{"path":"file"}"#.into(),
                },
            ],
        }]),
        &tools,
    )
    .unwrap();
    let MessageContentBlock::ToolRequest(call) = &message.content[0] else {
        panic!("Expected a tool request")
    };
    assert_eq!(call.id, "map");
    assert_eq!(
        call.tool_call.as_ref().unwrap().arguments.as_ref().unwrap(),
        arguments.as_object().unwrap()
    );
    let resumed = create_request(
        &ModelConfig::new(MODEL_NAME),
        "",
        &[
            Message::user().with_text("Review"),
            message,
            Message::user()
                .with_tool_response(
                    "map",
                    Ok(CallToolResult::error(vec![ContentBlock::text("Denied")])),
                )
                .with_tool_response(
                    "read",
                    Ok(CallToolResult::success(vec![ContentBlock::text("Read")])),
                ),
        ],
        &tools,
    )
    .unwrap();
    let afm::Entry::ToolCalls { calls } = &resumed.history[1] else {
        panic!("Expected native calls")
    };
    assert_eq!(calls[0].arguments, encoded);
    assert_eq!(calls[1].arguments, r#"{"path":"file"}"#);
    assert!(
        matches!(&resumed.history[2], afm::Entry::ToolOutput {id, text, ..} if id == "map" && text.contains("Denied"))
    );
}

#[test]
fn fallback_rejects_malformed_json_and_arguments_that_violate_the_original_schema() {
    let tools = [map_tool(json!({"type":"integer", "minimum":1}))];
    for arguments in [
        r#"{"arguments_json":"not json"}"#.to_owned(),
        wrapped_arguments(json!([])),
        wrapped_arguments(json!({"source":"review"})),
        wrapped_arguments(json!({"source":"review","parameters":{"count":"wrong type"}})),
        wrapped_arguments(json!({"source":"review","parameters":{"count":0}})),
        json!({"arguments_json":"{}","unexpected":true}).to_string(),
    ] {
        let error = convert_response(
            response(vec![afm::Entry::ToolCalls {
                calls: vec![afm::ToolCall {
                    id: "map".into(),
                    name: tools[0].name.to_string(),
                    arguments,
                }],
            }]),
            &tools,
        )
        .unwrap_err();
        assert!(error.to_string().contains("summon__delegate"));
    }
    assert!(PreparedTool::new(&tools[0])
        .unwrap()
        .decode_arguments(&wrapped_arguments(json!({
            "source":"review", "parameters":{"count":2}
        })))
        .is_ok());
}

#[test]
fn fallback_handles_constraints_and_local_references_and_reports_invalid_schemas() {
    let make = |schema: Value| {
        Tool::new(
            "constrained",
            "A constrained tool",
            Arc::new(schema.as_object().unwrap().clone()),
        )
    };
    let prepared = PreparedTool::new(&make(json!({
        "type":"object", "$defs":{"Label":{"type":"string", "minLength":3}},
        "properties":{"name":{"$ref":"#/$defs/Label"}}, "required":["name"]
    })))
    .unwrap();
    assert!(prepared
        .decode_arguments(&wrapped_arguments(json!({"name":"long"})))
        .is_ok());
    assert!(prepared
        .decode_arguments(&wrapped_arguments(json!({"name":"x"})))
        .is_err());
    assert!(PreparedTool::new(&make(json!({"type":"object", "additionalProperties":42}))).is_err());
    assert!(PreparedTool::new(&make(
        json!({"type":"object", "properties":{"x":{"$ref":"https://invalid.example/schema"}}})
    ))
    .is_err());
    assert!(matches!(
        PreparedTool::new(&make(json!({"type":"object"})))
            .unwrap()
            .encoding,
        ArgumentEncoding::JsonText(_)
    ));
}

fn response(entries: Vec<afm::Entry>) -> afm::Response {
    afm::Response {
        entries,
        input_tokens: 30,
        output_tokens: 10,
        cached_tokens: 5,
    }
}

#[test]
fn returns_complete_tool_batch_and_replays_results_without_executing_tools() {
    let calls = vec![
        afm::ToolCall {
            id: "a".into(),
            name: "developer__read".into(),
            arguments: r#"{"path":"one.txt"}"#.into(),
        },
        afm::ToolCall {
            id: "b".into(),
            name: "developer__read".into(),
            arguments: r#"{"path":"two.txt"}"#.into(),
        },
    ];
    let (message, usage) = convert_response(
        response(vec![afm::Entry::ToolCalls {
            calls: calls.clone(),
        }]),
        &[tool()],
    )
    .unwrap();
    assert_eq!(message.content.len(), 2);
    assert_eq!(usage.usage.total_tokens, Some(40));
    let messages = vec![
        Message::user().with_text("Read both files"),
        message,
        Message::user()
            .with_tool_response(
                "b",
                Ok(CallToolResult::error(vec![ContentBlock::text(
                    "Permission denied",
                )])),
            )
            .with_tool_response(
                "a",
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    "file contents",
                )])),
            ),
    ];
    let request = create_request(
        &ModelConfig::new(MODEL_NAME),
        "instructions",
        &messages,
        &[tool()],
    )
    .unwrap();
    assert_eq!(request.instructions, "instructions");
    assert_eq!(request.history.len(), 4);
    assert_eq!(request.history[1], afm::Entry::ToolCalls { calls });
    assert!(
        matches!(&request.history[2], afm::Entry::ToolOutput { id, name, text } if id == "b" && name == "developer__read" && text.contains("Permission denied"))
    );
    assert!(
        matches!(&request.history[3], afm::Entry::ToolOutput { id, text, .. } if id == "a" && text.contains("file contents"))
    );
    // No synthetic user continuation or framework-produced tool result was added.
}

#[test]
fn incomplete_and_orphan_tool_results_are_rejected() {
    let call = Message::assistant()
        .with_tool_request("a", Ok(CallToolRequestParams::new("developer__read")));
    assert!(create_request(&ModelConfig::new(MODEL_NAME), "", &[call], &[]).is_err());
    let orphan = Message::user().with_tool_response("a", Ok(CallToolResult::success(vec![])));
    assert!(create_request(&ModelConfig::new(MODEL_NAME), "", &[orphan], &[]).is_err());
}

#[test]
fn visibility_and_roles_survive_replay() {
    let messages = vec![
        Message::user().with_text("question"),
        Message::assistant().with_text("answer"),
        Message::user()
            .with_text("secret UI state")
            .with_visibility(true, false),
        Message::user().with_text("follow up"),
    ];
    let request = create_request(&ModelConfig::new(MODEL_NAME), "system", &messages, &[]).unwrap();
    assert_eq!(
        request.history,
        vec![
            afm::Entry::User {
                text: "question".into()
            },
            afm::Entry::Assistant {
                text: "answer".into()
            },
            afm::Entry::User {
                text: "follow up".into()
            }
        ]
    );
}

#[test]
fn model_cannot_invent_tools_or_return_non_object_arguments() {
    for (name, arguments) in [
        ("shell", "{}"),
        ("developer__read", "[]"),
        ("developer__read", "not json"),
    ] {
        assert!(convert_response(
            response(vec![afm::Entry::ToolCalls {
                calls: vec![afm::ToolCall {
                    id: "a".into(),
                    name: name.into(),
                    arguments: arguments.into()
                }]
            }]),
            &[tool()]
        )
        .is_err());
    }
}

#[test]
fn rejects_unsupported_models_and_media() {
    assert!(create_request(
        &ModelConfig::new("pcc"),
        "",
        &[Message::user().with_text("hello")],
        &[]
    )
    .is_err());
    let image = Message::user().with_content(MessageContentBlock::image("AAAA", "image/png"));
    assert!(matches!(
        create_request(&ModelConfig::new(MODEL_NAME), "", &[image], &[]),
        Err(ProviderError::NotImplemented(_))
    ));
}

#[test]
fn context_and_refusal_errors_keep_their_provider_classification() {
    assert!(matches!(
        provider_error(afm::Error {
            kind: afm::ErrorKind::ContextLengthExceeded,
            message: "full".into()
        }),
        ProviderError::ContextLengthExceeded(_)
    ));
    assert!(matches!(
        provider_error(afm::Error {
            kind: afm::ErrorKind::Refusal,
            message: "refused".into()
        }),
        ProviderError::Refusal { .. }
    ));
}

#[tokio::test]
async fn unavailable_native_runtime_reports_errors_across_the_async_ffi() {
    if let Err(availability) = afm::model_info() {
        assert_eq!(availability.kind, afm::ErrorKind::Unavailable);
        let request = create_request(
            &ModelConfig::new(MODEL_NAME),
            "",
            &[Message::user().with_text("Hello")],
            &[],
        )
        .unwrap();
        let error = afm::generate(request).await.unwrap_err();
        assert_eq!(error.kind, afm::ErrorKind::Unavailable);
    }
}
