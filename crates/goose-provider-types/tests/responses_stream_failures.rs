use futures::StreamExt;
use goose_provider_types::errors::ProviderError;
use goose_provider_types::formats::openai_responses::responses_api_to_streaming_message;
use serde_json::{json, Value};

fn response(status: &str) -> Value {
    json!({"id":"resp_capacity","object":"response","created_at":1,"status":status,"model":"gpt-test","output":[]})
}

fn overload() -> Value {
    json!({"type":"service_unavailable_error","code":"server_is_overloaded","message":"Our servers are currently overloaded. Please try again later.","param":null})
}

async fn assert_failure(events: Vec<Value>, expected: &str) {
    let lines = events.into_iter().map(|event| Ok(format!("data: {event}")));
    let results = responses_api_to_streaming_message(futures::stream::iter(lines))
        .collect::<Vec<_>>()
        .await;
    let error = results.iter().find_map(|result| result.as_ref().err());
    assert!(
        error.is_some(),
        "failed stream must not succeed: {results:?}"
    );
    assert!(
        error.unwrap().to_string().contains(expected),
        "provider error details must survive: {error:?}"
    );
    if expected == "server_is_overloaded" {
        assert!(
            matches!(
                error.unwrap().downcast_ref::<ProviderError>(),
                Some(ProviderError::ServerError(_))
            ),
            "overload must remain a typed retryable server error, not a decode error: {error:?}"
        );
    }
}

#[tokio::test]
async fn nested_overload_event_control() {
    assert_failure(
        vec![json!({"type":"error","error":overload(),"sequence_number":2})],
        "server_is_overloaded",
    )
    .await;
}

#[tokio::test]
async fn gateway_overload_without_type_is_not_ignored() {
    assert_failure(
        vec![json!({"error":overload(),"sequence_number":2})],
        "server_is_overloaded",
    )
    .await;
}

#[tokio::test]
async fn official_flat_error_preserves_details() {
    // OpenAI's ResponseErrorEvent has top-level code/message, not a nested error.
    assert_failure(vec![json!({"type":"error","code":"server_is_overloaded","message":"Our servers are currently overloaded. Please try again later.","param":null,"sequence_number":2})], "server_is_overloaded").await;
}

#[tokio::test]
async fn official_failed_response_preserves_details() {
    // OpenAI's ResponseFailedEvent nests the error inside response.
    let mut failed = response("failed");
    failed["error"] = overload();
    assert_failure(
        vec![json!({"type":"response.failed","response":failed,"sequence_number":2})],
        "server_is_overloaded",
    )
    .await;
}

#[tokio::test]
async fn eof_without_terminal_event_is_not_success() {
    assert_failure(vec![json!({"type":"response.created","sequence_number":1,"response":response("in_progress")})], "completion").await;
}

#[tokio::test]
async fn eof_after_partial_text_is_not_success() {
    assert_failure(vec![
        json!({"type":"response.created","sequence_number":1,"response":response("in_progress")}),
        json!({"type":"response.output_text.delta","sequence_number":2,"item_id":"msg_1","output_index":0,"content_index":0,"delta":"Working on it"}),
    ], "completion").await;
}

#[tokio::test]
async fn done_without_terminal_event_is_not_success() {
    let results = responses_api_to_streaming_message(futures::stream::iter(vec![Ok(
        "data: [DONE]".to_string(),
    )]))
    .collect::<Vec<_>>()
    .await;
    assert!(
        results.iter().any(Result::is_err),
        "DONE alone is not a completed Responses response"
    );
}

#[tokio::test]
async fn complete_response_and_unknown_metadata_control() {
    let events = vec![
        json!({"gateway_metadata":true}),
        json!({"type":"response.completed","sequence_number":2,"response":response("completed")}),
    ];
    let lines = events.into_iter().map(|event| Ok(format!("data: {event}")));
    let results = responses_api_to_streaming_message(futures::stream::iter(lines))
        .collect::<Vec<_>>()
        .await;
    assert!(!results.is_empty());
    assert!(results.iter().all(Result::is_ok), "{results:?}");
}

#[tokio::test]
async fn transport_error_control() {
    let results = responses_api_to_streaming_message(futures::stream::iter(vec![Err(
        anyhow::anyhow!("connection reset"),
    )]))
    .collect::<Vec<_>>()
    .await;
    assert!(results.iter().any(|result| result
        .as_ref()
        .err()
        .is_some_and(|error| error.to_string().contains("connection reset"))));
}

#[tokio::test]
async fn invalid_request_is_not_a_retryable_server_error() {
    let event = json!({"type":"error","code":"invalid_request_error","message":"Invalid input","sequence_number":2});
    let results = responses_api_to_streaming_message(futures::stream::iter(vec![Ok(format!(
        "data: {event}"
    ))]))
    .collect::<Vec<_>>()
    .await;
    assert!(matches!(
        results[0]
            .as_ref()
            .unwrap_err()
            .downcast_ref::<ProviderError>(),
        Some(ProviderError::RequestFailed(_))
    ));
}

#[tokio::test]
async fn interrupted_response_does_not_emit_buffered_tool_calls() {
    let event = json!({"type":"response.output_item.done","sequence_number":2,"output_index":0,"item":{"type":"function_call","call_id":"call_incomplete","name":"write_file","arguments":"{}"}});
    let results = responses_api_to_streaming_message(futures::stream::iter(vec![Ok(format!(
        "data: {event}"
    ))]))
    .collect::<Vec<_>>()
    .await;
    assert_eq!(results.len(), 1);
    assert!(
        results[0].is_err(),
        "must not execute buffered tools from an incomplete response"
    );
}

#[tokio::test]
async fn context_overflow_stays_compactable() {
    let lines = vec![Ok(format!(
        "data: {}",
        json!({"type":"error","code":"context_length_exceeded","message":"Your input exceeds the context window of this model.","param":"input","sequence_number":2})
    ))];
    let results = responses_api_to_streaming_message(futures::stream::iter(lines))
        .collect::<Vec<_>>()
        .await;
    let error = results.iter().find_map(|result| result.as_ref().err());
    assert!(
        matches!(
            error.and_then(|error| error.downcast_ref::<ProviderError>()),
            Some(ProviderError::ContextLengthExceeded(_))
        ),
        "context overflow must stay typed so compaction can recover: {error:?}"
    );
}
