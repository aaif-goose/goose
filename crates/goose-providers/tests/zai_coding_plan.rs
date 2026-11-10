use futures::StreamExt;
use goose_providers::{
    base::Provider,
    conversation::message::{Message, MessageContentBlock},
    declarative::{deserialize_provider_config, KeyResolver},
    model::ModelConfig,
    openai::from_declarative_config,
    zai_coding_plan,
};
use rmcp::model::{CallToolResult, ContentBlock, Tool};
use serde_json::{json, Value};
use std::{convert::Infallible, sync::Arc, time::Duration};
use wiremock::{
    matchers::{body_partial_json, header, method, path},
    Mock, MockServer, ResponseTemplate,
};

struct TestKey;

impl KeyResolver for TestKey {
    type Error = Infallible;

    fn resolve_key(&self, key: &str) -> Result<String, Self::Error> {
        assert_eq!(key, "ZAI_CODING_PLAN_API_KEY");
        Ok("test-key".into())
    }
}

fn provider(server: &MockServer) -> impl Provider {
    let mut config = deserialize_provider_config(zai_coding_plan::JSON).unwrap();
    assert_eq!(config.base_url, "https://api.z.ai/api/coding/paas/v4");
    config.base_url = format!("{}/api/coding/paas/v4", server.uri());
    from_declarative_config(config, None, TestKey)
        .unwrap()
        .build()
}

fn tool() -> Tool {
    Tool::new(
        "write_file",
        "Write a file",
        Arc::new(
            json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"]})
                .as_object()
                .unwrap()
                .clone(),
        ),
    )
}

fn sse(deltas: Vec<Value>, finish: &str) -> String {
    let mut body = String::new();
    for delta in deltas {
        body.push_str(&format!(
            "data: {}\n\n",
            json!({"id":"response-1","model":"glm-5.3","choices":[{"index":0,"delta":delta,"finish_reason":null}]})
        ));
    }
    body.push_str(&format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"id":"response-1","model":"glm-5.3","choices":[{"index":0,"delta":{},"finish_reason":finish}],
            "usage":{"prompt_tokens":20,"completion_tokens":10,"total_tokens":30}})
    ));
    body
}

#[tokio::test]
async fn streams_fragmented_tools_and_replays_reasoning_with_tool_results() {
    let server = MockServer::start().await;
    let provider = provider(&server);
    Mock::given(method("GET"))
        .and(path("/api/coding/paas/v4/models"))
        .and(header("authorization", "Bearer test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{"id":"glm-5.3"}, {"id":"glm-5.3-flash"}, {"id":"glm-future"}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        provider.fetch_supported_models().await.unwrap(),
        ["glm-5.3", "glm-5.3-flash", "glm-future"]
    );
    server.verify().await;
    server.reset().await;

    for model in ["glm-5.3", "glm-5.3-flash", "glm-future"] {
        let response = sse(
            vec![
                json!({"role":"assistant","reasoning_content":"Inspect first. "}),
                json!({"reasoning_content":"Then write."}),
                json!({"tool_calls":[{"index":0,"id":"call-1","type":"function","function":{"name":"write_file","arguments":""}}]}),
                json!({"tool_calls":[{"index":0,"function":{"arguments":"{\"text\":\"Hello"}}]}),
                json!({"tool_calls":[{"index":0,"function":{"arguments":" \\u4e16\\u754c\\n\"}"}}]}),
            ],
            "tool_calls",
        );
        Mock::given(method("POST"))
            .and(path("/api/coding/paas/v4/chat/completions"))
            .and(header("authorization", "Bearer test-key"))
            .and(body_partial_json(
                json!({"model":model,"stream":true,"tool_stream":true}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_raw(response, "text/event-stream"))
            .expect(1)
            .mount(&server)
            .await;

        let mut messages = vec![Message::user().with_text("Write the greeting")];
        let (reply, usage) = provider
            .complete(
                &ModelConfig::new(model),
                "You are a coding assistant",
                &messages,
                &[tool()],
            )
            .await
            .unwrap();
        assert_eq!(usage.usage.total_tokens, Some(30));
        let calls: Vec<_> = reply
            .content
            .iter()
            .filter_map(|block| match block {
                MessageContentBlock::ToolRequest(request) => Some(request),
                _ => None,
            })
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call-1");
        let call = calls[0].tool_call.as_ref().unwrap();
        assert_eq!(call.name, "write_file");
        assert_eq!(
            call.arguments,
            Some(json!({"text":"Hello 世界\n"}).as_object().unwrap().clone())
        );
        messages.push(reply);
        messages.push(Message::user().with_tool_response(
            "call-1",
            Ok(CallToolResult::success(vec![ContentBlock::text("Written")])),
        ));
        server.verify().await;
        server.reset().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                sse(vec![json!({"content":"Done"})], "stop"),
                "text/event-stream",
            ))
            .expect(1)
            .mount(&server)
            .await;
        let (reply, _) = provider
            .complete(
                &ModelConfig::new(model),
                "You are a coding assistant",
                &messages,
                &[tool()],
            )
            .await
            .unwrap();
        assert_eq!(reply.as_concat_text(), "Done");
        let requests = server.received_requests().await.unwrap();
        let body: Value = requests[0].body_json().unwrap();
        let history = body["messages"].as_array().unwrap();
        let assistant = history.iter().find(|m| m["role"] == "assistant").unwrap();
        assert_eq!(assistant["reasoning_content"], "Inspect first. Then write.");
        assert_eq!(assistant["tool_calls"][0]["id"], "call-1");
        let result = history.iter().find(|m| m["role"] == "tool").unwrap();
        assert_eq!(result["tool_call_id"], "call-1");
        assert_eq!(body["tool_stream"], true);
        server.verify().await;
        server.reset().await;
    }
}

#[tokio::test]
async fn cancelled_request_does_not_contaminate_next_turn() {
    let server = MockServer::start().await;
    let provider = provider(&server);
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(10)))
        .mount(&server)
        .await;
    let model = ModelConfig::new("glm-5.3");
    assert!(tokio::time::timeout(
        Duration::from_millis(100),
        provider.stream(
            &model,
            "system",
            &[Message::user().with_text("Old task")],
            &[tool()]
        ),
    )
    .await
    .is_err());
    server.reset().await;
    Mock::given(method("POST"))
        .and(body_partial_json(json!({"stream":true,"tool_stream":true})))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            sse(vec![json!({"content":"New task"})], "stop"),
            "text/event-stream",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let mut stream = provider
        .stream(
            &model,
            "system",
            &[Message::user().with_text("New task")],
            &[],
        )
        .await
        .unwrap();
    let mut text = String::new();
    while let Some(chunk) = stream.next().await {
        if let (Some(message), _) = chunk.unwrap() {
            text.push_str(&message.as_concat_text());
        }
    }
    assert_eq!(text, "New task");
}
