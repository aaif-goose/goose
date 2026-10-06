use futures::StreamExt;
use goose_provider_types::conversation::token_usage::Usage;
use goose_provider_types::formats::openai_responses::{
    get_responses_usage, responses_api_to_streaming_message, ResponsesApiResponse,
};
use serde_json::json;

async fn final_stream_usage(lines: Vec<String>) -> Usage {
    let messages =
        responses_api_to_streaming_message(tokio_stream::iter(lines.into_iter().map(Ok)));
    futures::pin_mut!(messages);

    let mut usage = None;
    while let Some(item) = messages.next().await {
        let (_, maybe_usage) = item.expect("stream item should parse");
        if maybe_usage.is_some() {
            usage = maybe_usage;
        }
    }
    usage.expect("stream should report usage").usage
}

#[tokio::test]
async fn stream_completed_reports_cache_write_tokens() {
    let usage = final_stream_usage(vec![
        r#"data: {"type":"response.created","sequence_number":1,"response":{"id":"resp_1","object":"response","created_at":1737368310,"status":"in_progress","model":"gpt-6-luna","output":[]}}"#.to_string(),
        r#"data: {"type":"response.completed","sequence_number":2,"response":{"id":"resp_1","object":"response","created_at":1737368310,"status":"completed","model":"gpt-6-luna","output":[],"usage":{"input_tokens":5457,"input_tokens_details":{"cache_write_tokens":5454,"cached_tokens":0},"output_tokens":5,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":5462}}}"#.to_string(),
        "data: [DONE]".to_string(),
    ])
    .await;

    assert_eq!(usage.input_tokens, Some(5457));
    assert_eq!(usage.total_tokens, Some(5462));
    assert_eq!(usage.cache_read_input_tokens, Some(0));
    assert_eq!(usage.cache_write_input_tokens, Some(5454));
}

#[tokio::test]
async fn stream_incomplete_reports_cache_write_tokens() {
    let usage = final_stream_usage(vec![
        r#"data: {"type":"response.created","sequence_number":1,"response":{"id":"resp_1","object":"response","created_at":1737368310,"status":"in_progress","model":"gpt-6-luna","output":[]}}"#.to_string(),
        r#"data: {"type":"response.incomplete","sequence_number":2,"response":{"id":"resp_1","object":"response","created_at":1737368310,"status":"incomplete","model":"gpt-6-luna","output":[],"incomplete_details":{"reason":"max_output_tokens"},"usage":{"input_tokens":10,"input_tokens_details":{"cache_write_tokens":6,"cached_tokens":3},"output_tokens":5,"total_tokens":15}}}"#.to_string(),
        "data: [DONE]".to_string(),
    ])
    .await;

    assert_eq!(usage.input_tokens, Some(10));
    assert_eq!(usage.cache_read_input_tokens, Some(3));
    assert_eq!(usage.cache_write_input_tokens, Some(6));
}

#[test]
fn non_streaming_usage_reports_cache_read_and_write_tokens() {
    let response: ResponsesApiResponse = serde_json::from_value(json!({
        "id": "resp_1",
        "object": "response",
        "created_at": 1737368310,
        "status": "completed",
        "model": "gpt-6-luna",
        "output": [],
        "usage": {
            "input_tokens": 2176,
            "input_tokens_details": { "cache_write_tokens": 9, "cached_tokens": 2164 },
            "output_tokens": 7,
            "output_tokens_details": { "reasoning_tokens": 0 },
            "total_tokens": 2183
        }
    }))
    .unwrap();

    let usage = get_responses_usage(&response);

    assert_eq!(usage.input_tokens, Some(2176));
    assert_eq!(usage.cache_read_input_tokens, Some(2164));
    assert_eq!(usage.cache_write_input_tokens, Some(9));
}
