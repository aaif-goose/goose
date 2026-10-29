use futures::StreamExt;
use goose_provider_types::conversation::message::MessageContentBlock;
use goose_provider_types::errors::ProviderError;
use goose_provider_types::formats::anthropic::response_to_streaming_message;

const START: &str = r#"data: {"type":"message_start","message":{"id":"msg_capacity","model":"claude-test","usage":{"input_tokens":10,"output_tokens":0}}}"#;
const OVERLOAD: &str =
    r#"data: {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
const TEXT: &str = r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Working on it"}}"#;
const DELTA: &str = r#"data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}"#;
const STOP: &str = r#"data: {"type":"message_stop"}"#;

const TOOL_START: &str = r#"data: {"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"tool_1","name":"dangerous_action","input":{}}}"#;
const TOOL_ARGS: &str = r#"data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{}"}}"#;
const TOOL_STOP: &str = r#"data: {"type":"content_block_stop","index":2}"#;

async fn assert_stream_error(events: &[&str]) -> ProviderError {
    let lines: Vec<anyhow::Result<String>> =
        events.iter().map(|line| Ok(line.to_string())).collect();
    let results = response_to_streaming_message(futures::stream::iter(lines))
        .collect::<Vec<_>>()
        .await;
    let errors: Vec<_> = results
        .iter()
        .filter_map(|result| result.as_ref().err())
        .collect();
    assert!(
        errors.len() == 1,
        "an unsuccessful stream must surface exactly one error; parsed results: {results:?}"
    );
    assert!(
        results
            .iter()
            .filter_map(|r| r.as_ref().ok())
            .all(|(_, usage)| usage.is_none()),
        "an incomplete response must not emit final usage"
    );
    assert!(
        results.iter().filter_map(|result| result.as_ref().ok())
            .filter_map(|(message, _)| message.as_ref())
            .flat_map(|message| &message.content)
            .all(|block| !matches!(block, MessageContentBlock::ToolRequest(request) if request.tool_call.is_ok())),
        "an unsuccessful response must not emit executable tool requests: {results:?}"
    );
    errors[0]
        .downcast_ref::<ProviderError>()
        .expect("typed provider error")
        .clone()
}

#[tokio::test]
async fn overload_before_message_start_surfaces_error() {
    assert_eq!(
        assert_stream_error(&[OVERLOAD]).await,
        ProviderError::ServerError("overloaded_error: Overloaded".to_string())
    );
}

#[tokio::test]
async fn overload_after_message_start_surfaces_error() {
    assert_eq!(
        assert_stream_error(&[START, OVERLOAD]).await,
        ProviderError::ServerError("overloaded_error: Overloaded".to_string())
    );
}

#[tokio::test]
async fn overload_after_partial_text_surfaces_error() {
    assert_eq!(
        assert_stream_error(&[START, TEXT, OVERLOAD]).await,
        ProviderError::ServerError("overloaded_error: Overloaded".to_string())
    );
}

#[tokio::test]
async fn clean_eof_without_completion_surfaces_error() {
    assert!(
        matches!(assert_stream_error(&[START]).await, ProviderError::NetworkError(message) if message.contains("message_stop"))
    );
}

#[tokio::test]
async fn completed_response_control() {
    let lines: Vec<anyhow::Result<String>> = [START, TEXT, DELTA, STOP]
        .into_iter()
        .map(|line| Ok(line.to_string()))
        .collect();
    let results = response_to_streaming_message(futures::stream::iter(lines))
        .collect::<Vec<_>>()
        .await;
    assert!(results.iter().all(Result::is_ok));
    assert!(results.iter().any(|result| {
        result
            .as_ref()
            .unwrap()
            .1
            .as_ref()
            .is_some_and(|usage| usage.finish_reasons.as_deref() == Some(&["end_turn".to_string()]))
    }));
}

#[tokio::test]
async fn transport_error_control() {
    let lines = futures::stream::iter(vec![
        Ok(START.to_string()),
        Err(anyhow::anyhow!("connection reset")),
    ]);
    let results = response_to_streaming_message(lines)
        .collect::<Vec<_>>()
        .await;
    assert!(results.iter().any(Result::is_err));
}

#[tokio::test]
async fn invalid_request_is_not_a_retryable_server_error() {
    let error = r#"data: {"type":"error","error":{"type":"invalid_request_error","message":"Bad request"}}"#;
    assert_eq!(
        assert_stream_error(&[START, error, STOP]).await,
        ProviderError::RequestFailed("invalid_request_error: Bad request".to_string())
    );
}

#[tokio::test]
async fn error_types_preserve_retry_classification() {
    for (kind, expected) in [
        (
            "api_error",
            ProviderError::ServerError("api_error: failed".to_string()),
        ),
        (
            "rate_limit_error",
            ProviderError::RateLimitExceeded {
                details: "rate_limit_error: failed".to_string(),
                retry_delay: None,
            },
        ),
        (
            "authentication_error",
            ProviderError::Authentication("authentication_error: failed".to_string()),
        ),
        (
            "future_error",
            ProviderError::RequestFailed("future_error: failed".to_string()),
        ),
    ] {
        let event =
            format!(r#"data: {{"type":"error","error":{{"type":"{kind}","message":"failed"}}}}"#);
        assert_eq!(assert_stream_error(&[&event]).await, expected);
    }
}

#[tokio::test]
async fn missing_message_stop_never_finalizes_usage() {
    for events in [
        vec![],
        vec![START, TEXT],
        vec![START, DELTA],
        vec![START, DELTA, "data: [DONE]"],
    ] {
        assert!(
            matches!(assert_stream_error(&events).await, ProviderError::NetworkError(message) if message.contains("message_stop"))
        );
    }
}

#[tokio::test]
async fn malformed_json_is_not_swallowed_even_before_completion() {
    assert!(
        matches!(assert_stream_error(&[START, "data: {broken", STOP]).await,
        ProviderError::NetworkError(message) if message.contains("Stream decode error"))
    );
}

#[tokio::test]
async fn ping_and_unknown_events_remain_compatible() {
    let lines = [
        "event: ping",
        r#"data:{"type":"ping"}"#,
        r#"data: {"type":"future_event"}"#,
        START,
        DELTA,
        STOP,
    ];
    let results = response_to_streaming_message(futures::stream::iter(
        lines.into_iter().map(|line| Ok(line.to_string())),
    ))
    .collect::<Vec<_>>()
    .await;
    assert!(results.iter().all(Result::is_ok));
}

#[tokio::test]
async fn interrupted_open_tool_call_is_not_emitted() {
    let tool_start = r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tool_1","name":"dangerous_action","input":{}}}"#;
    let tool_args = r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{}"}}"#;
    for ending in [None, Some(OVERLOAD)] {
        let mut events = vec![START, tool_start, tool_args];
        events.extend(ending);
        let results = response_to_streaming_message(futures::stream::iter(
            events.into_iter().map(|line| Ok(line.to_string())),
        ))
        .collect::<Vec<_>>()
        .await;
        assert_eq!(results.len(), 1, "must not emit an unfinished tool call");
        assert!(results[0].is_err());
    }
}

#[tokio::test]
async fn closed_tool_followed_by_eof_is_not_executable() {
    assert!(matches!(
        assert_stream_error(&[START, TOOL_START, TOOL_ARGS, TOOL_STOP]).await,
        ProviderError::NetworkError(_)
    ));
}

#[tokio::test]
async fn closed_tool_followed_by_error_is_not_executable() {
    assert_eq!(
        assert_stream_error(&[START, TOOL_START, TOOL_ARGS, TOOL_STOP, OVERLOAD]).await,
        ProviderError::ServerError("overloaded_error: Overloaded".to_string())
    );
}

#[tokio::test]
async fn completed_tool_waits_for_message_stop_and_is_emitted_once() {
    let stream = response_to_streaming_message(futures::stream::iter(
        [START, TOOL_START, TOOL_ARGS, TOOL_STOP, TEXT, DELTA, STOP]
            .into_iter()
            .map(|line| Ok(line.to_string())),
    ));
    futures::pin_mut!(stream);
    let (message, usage) = stream.next().await.unwrap().unwrap();
    assert!(usage.is_none());
    assert!(
        matches!(&message.unwrap().content[0], MessageContentBlock::Text(text) if text.text == "Working on it")
    );
    let results = stream.collect::<Vec<_>>().await;
    assert!(results.iter().all(Result::is_ok));
    let requests: Vec<_> = results
        .iter()
        .filter_map(|result| result.as_ref().unwrap().0.as_ref())
        .flat_map(|message| &message.content)
        .filter_map(|block| match block {
            MessageContentBlock::ToolRequest(request) => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].provider_index(), Some(2));
    assert_eq!(requests[0].id, "tool_1");
    let call = requests[0].tool_call.as_ref().unwrap();
    assert_eq!(call.name, "dangerous_action");
    assert_eq!(call.arguments, Some(Default::default()));
}
