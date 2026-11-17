use goose_provider_types::conversation::message::{Message, MessageContentBlock};
use goose_provider_types::formats::{databricks, openai};
use goose_provider_types::images::ImageFormat;
use goose_provider_types::model::ModelConfig;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};
use serde_json::Value;
use std::collections::BTreeSet;
use test_case::test_case;

fn calls(ids: &[&str]) -> Message {
    ids.iter().fold(Message::assistant(), |message, id| {
        message.with_tool_request(*id, Ok(CallToolRequestParams::new("read_image")))
    })
}

fn response(id: &str, images: &[&str]) -> Message {
    let mut content = vec![ContentBlock::text(format!("result for {id}"))];
    content.extend(
        images
            .iter()
            .map(|data| ContentBlock::image(*data, "image/png")),
    );
    Message::user().with_tool_response(id, Ok(CallToolResult::success(content)))
}

fn format(provider: &str, messages: &[Message], vision: bool) -> Vec<Value> {
    let output = match provider {
        "openai" => openai::format_messages_with_options(
            messages,
            &ImageFormat::OpenAi,
            openai::OpenAiFormatOptions {
                preserve_thinking_context: true,
                supports_vision: vision,
                ..Default::default()
            },
        ),
        "databricks" => {
            let payload = databricks::create_request_for_provider(
                "databricks",
                &ModelConfig::new("gpt-4o").with_vision_support(vision),
                "system",
                messages,
                &[],
                &ImageFormat::OpenAi,
            )
            .unwrap();
            payload["messages"].as_array().unwrap()[1..].to_vec()
        }
        _ => unreachable!(),
    };
    eprintln!("{provider}: {}", serde_json::to_string(&output).unwrap());
    output
}

fn roles(messages: &[Value]) -> Vec<&str> {
    messages
        .iter()
        .map(|message| message["role"].as_str().unwrap())
        .collect()
}

fn images(messages: &[Value]) -> Vec<&str> {
    messages
        .iter()
        .filter_map(|message| message["content"].as_array())
        .flatten()
        .filter_map(|part| part["image_url"]["url"].as_str())
        .collect()
}

fn assert_complete_batches(messages: &[Value]) {
    let mut pending = BTreeSet::new();
    for (index, message) in messages.iter().enumerate() {
        if message["role"] == "tool" {
            let id = message["tool_call_id"].as_str().unwrap();
            assert!(pending.remove(id), "orphan or duplicate result: {id}");
        } else {
            assert!(pending.is_empty(), "message {index} interrupts {pending:?}");
        }
        if let Some(calls) = message["tool_calls"].as_array() {
            for call in calls {
                assert!(pending.insert(call["id"].as_str().unwrap()));
            }
        }
    }
    assert!(pending.is_empty(), "missing results: {pending:?}");
}

#[test_case("openai"; "openai")]
#[test_case("databricks"; "databricks")]
fn persisted_parallel_results_keep_images_after_the_batch(provider: &str) {
    let messages = vec![
        calls(&["a", "b", "c"]),
        response("a", &["YQ==", "YTI="]),
        response("b", &[]),
        response("c", &["Yw=="]),
    ];
    let output = format(provider, &messages, true);
    assert_eq!(
        roles(&output),
        ["assistant", "tool", "tool", "tool", "user", "user", "user"]
    );
    assert_complete_batches(&output);
    for (index, id) in ["a", "b", "c"].iter().enumerate() {
        assert_eq!(output[index + 1]["tool_call_id"], *id);
        assert!(output[index + 1]["content"]
            .as_str()
            .unwrap()
            .starts_with(&format!("result for {id}")));
    }
    assert_eq!(
        images(&output),
        [
            "data:image/png;base64,YQ==",
            "data:image/png;base64,YTI=",
            "data:image/png;base64,Yw=="
        ]
    );
}

#[test]
fn split_reasoning_calls_keep_images_after_all_merged_results() {
    let messages = vec![
        calls(&["a"]).with_content(MessageContentBlock::thinking("read both", "")),
        response("a", &["YQ=="]),
        calls(&["b"]).with_content(MessageContentBlock::thinking("read both", "")),
        response("b", &["Yg=="]),
    ];
    let output = format("openai", &messages, true);
    assert_eq!(
        roles(&output),
        ["assistant", "tool", "tool", "user", "user"]
    );
    assert_eq!(output[0]["tool_calls"].as_array().unwrap().len(), 2);
    assert_complete_batches(&output);
    assert_eq!(
        images(&output),
        ["data:image/png;base64,YQ==", "data:image/png;base64,Yg=="]
    );
}

#[test_case("openai"; "openai")]
#[test_case("databricks"; "databricks")]
fn real_user_image_separates_batches_even_with_identical_reasoning(provider: &str) {
    let messages = vec![
        calls(&["a"]).with_content(MessageContentBlock::thinking("read images", "")),
        response("a", &["YQ=="]),
        Message::user().with_image("cmVhbA==", "image/png"),
        calls(&["b"]).with_content(MessageContentBlock::thinking("read images", "")),
        response("b", &["Yg=="]),
    ];
    let output = format(provider, &messages, true);
    assert_eq!(
        roles(&output),
        [
            "assistant",
            "tool",
            "user",
            "user",
            "assistant",
            "tool",
            "user"
        ]
    );
    assert_eq!(output[0]["tool_calls"].as_array().unwrap().len(), 1);
    assert_eq!(output[4]["tool_calls"].as_array().unwrap().len(), 1);
    assert_complete_batches(&output);
    assert_eq!(
        images(&output),
        [
            "data:image/png;base64,YQ==",
            "data:image/png;base64,cmVhbA==",
            "data:image/png;base64,Yg=="
        ]
    );
}

#[test_case("openai"; "openai")]
#[test_case("databricks"; "databricks")]
fn non_vision_replay_has_only_tool_results(provider: &str) {
    let messages = vec![
        calls(&["a", "b"]),
        response("a", &["YQ=="]),
        response("b", &["Yg=="]),
    ];
    let output = format(provider, &messages, false);
    assert_eq!(roles(&output), ["assistant", "tool", "tool"]);
    assert_complete_batches(&output);
    assert!(images(&output).is_empty());
}

#[test_case("openai"; "openai")]
#[test_case("databricks"; "databricks")]
fn empty_source_message_does_not_end_a_tool_batch(provider: &str) {
    let messages = vec![
        calls(&["a", "b"]),
        response("a", &["YQ=="]),
        Message::user().with_text(""),
        response("b", &["Yg=="]),
    ];
    let output = format(provider, &messages, true);
    assert_eq!(
        roles(&output),
        ["assistant", "tool", "tool", "user", "user"]
    );
    assert_complete_batches(&output);
    assert_eq!(
        images(&output),
        ["data:image/png;base64,YQ==", "data:image/png;base64,Yg=="]
    );
}
