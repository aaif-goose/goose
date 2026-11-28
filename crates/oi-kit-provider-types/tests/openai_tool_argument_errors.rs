use futures::TryStreamExt;
use oi_kit_provider_types::conversation::message::MessageContentBlock;
use oi_kit_provider_types::formats::openai::response_to_streaming_message;
use rmcp::model::ErrorCode;
use serde_json::json;

#[tokio::test]
async fn streaming_tool_argument_error_uses_finish_reason() -> anyhow::Result<()> {
    let incomplete_arguments = r#"{"command": "echo hi""#;
    let tool_call = json!({
        "choices": [{
            "index": 0,
            "delta": {"tool_calls": [{
                "index": 0,
                "id": "call_1",
                "type": "function",
                "function": {"name": "shell", "arguments": incomplete_arguments}
            }]},
            "finish_reason": null
        }]
    });

    for (finish_reason, reached_limit) in [("tool_calls", false), ("length", true)] {
        let finish =
            json!({"choices": [{"index": 0, "delta": {}, "finish_reason": finish_reason}]});
        let lines = [
            format!("data: {tool_call}"),
            format!("data: {finish}"),
            "data: [DONE]".into(),
        ];
        let stream = futures::stream::iter(lines.into_iter().map(Ok));
        let results: Vec<_> = response_to_streaming_message(stream).try_collect().await?;
        let message = results
            .into_iter()
            .find_map(|(message, _)| message)
            .expect("expected a tool request message");

        let [MessageContentBlock::ToolRequest(request)] = message.content.as_slice() else {
            panic!("expected a single tool request");
        };
        let error = request.tool_call.as_ref().expect_err("invalid arguments");
        assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
        assert_eq!(message.metadata.output_token_limit_reached, reached_limit);
        assert_eq!(error.message.contains("output token limit"), reached_limit);
        assert_eq!(
            error.message.contains("malformed tool arguments"),
            !reached_limit
        );
    }

    Ok(())
}
