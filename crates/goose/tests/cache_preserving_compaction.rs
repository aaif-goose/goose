use goose::context_mgmt::{compact_messages_with_context, CompactionRequestContext};
use goose::conversation::Conversation;
use goose_providers::base::Provider;
use goose_providers::conversation::message::{Message, MessageContent};
use goose_providers::databricks_auth::DatabricksAuth;
use goose_providers::databricks_v2::DatabricksV2Provider;
use goose_providers::model::ModelConfig;
use goose_providers::retry::RetryConfig;
use goose_providers::thinking::ThinkingEffort;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, Tool};
use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn provider(host: String) -> DatabricksV2Provider {
    DatabricksV2Provider::new(
        host,
        DatabricksAuth::token("synthetic-token".into()),
        RetryConfig::new(0, 0, 1.0, 0),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap()
}

fn response(anthropic: bool, text: &str, finish: &str) -> String {
    if anthropic {
        [
            json!({"type":"message_start","message":{"id":"synthetic","role":"assistant","content":[],"usage":{"input_tokens":10,"cache_read_input_tokens":256,"cache_creation_input_tokens":12,"output_tokens":0}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{"stop_reason":finish},"usage":{"output_tokens":20}}),
            json!({"type":"message_stop"}),
        ].iter().map(|event| format!("data: {event}\n\n")).collect()
    } else {
        let output = json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":text}]}]);
        [
            json!({"type":"response.output_text.delta","sequence_number":1,"item_id":"msg_synthetic","delta":text,"output_index":0,"content_index":0}),
            json!({"type":"response.completed","sequence_number":2,"response":{"id":"synthetic","object":"response","created_at":1,"status":finish,"model":"gpt-6.1-sol","output":output,"usage":{"input_tokens":278,"output_tokens":20,"total_tokens":298,"input_tokens_details":{"cached_tokens":256}}}}),
        ].iter().map(|event| format!("data: {event}\n\n")).collect()
    }
}

fn remove_cache_controls(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("cache_control");
            object.values_mut().for_each(remove_cache_controls);
        }
        Value::Array(array) => array.iter_mut().for_each(remove_cache_controls),
        _ => {}
    }
}

fn assert_wire_prefix(normal: &[Value], summary: &[Value]) {
    assert!(summary.len() >= normal.len());
    for (index, original) in normal.iter().enumerate() {
        let actual = &summary[index];
        if index + 1 < normal.len() || actual == original {
            assert_eq!(actual, original);
            continue;
        }
        // Request normalization can extend the last user message. Existing
        // content must still be its exact prefix.
        assert_eq!(actual["role"], original["role"]);
        match (&original["content"], &actual["content"]) {
            (Value::String(before), Value::String(after)) => assert!(after.starts_with(before)),
            (Value::Array(before), Value::Array(after)) => {
                assert_eq!(&after[..before.len()], before)
            }
            _ => panic!("Unexpected wire message content"),
        }
    }
}

fn history() -> Conversation {
    Conversation::new_unvalidated(vec![
        Message::user().with_text("Implement a synthetic feature; keep the current API"),
        Message::assistant()
            .with_thinking("Synthetic reasoning", "synthetic-signature")
            .with_tool_request("call-a", Ok(CallToolRequestParams::new("synthetic_read")))
            .with_tool_request("call-b", Ok(CallToolRequestParams::new("synthetic_read"))),
        Message::user()
            .with_tool_response(
                "call-a",
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    "synthetic result A",
                )])),
            )
            .with_tool_response(
                "call-b",
                Ok(CallToolResult::success(vec![ContentBlock::text(
                    "synthetic result B",
                )])),
            ),
        Message::assistant().with_text("The synthetic change is ready for tests"),
        Message::user().with_text("Run the focused tests next"),
    ])
}

#[tokio::test]
async fn databricks_native_summary_preserves_wire_prefix_and_usage() {
    for (model_name, anthropic) in [
        ("catalog.schema.goose-claude-sonnet-4-6", true),
        ("catalog.schema.goose-gpt-6-1-sol", false),
    ] {
        let server = MockServer::start().await;
        let route = if anthropic {
            "/ai-gateway/anthropic/v1/messages"
        } else {
            "/ai-gateway/openai/v1/responses"
        };
        Mock::given(method("POST")).and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_string(response(anthropic, r#"```json
{"user_intent":["Implement the synthetic feature"],"pending_tasks":["Run focused tests"],"current_work":"synthetic change ready"}
```"#, if anthropic { "end_turn" } else { "completed" })).append_header("content-type", "text/event-stream"))
            .expect(2).mount(&server).await;
        let provider = provider(server.uri());
        let model = ModelConfig::new(model_name)
            .with_thinking_effort(ThinkingEffort::High)
            .with_cache_ttl("1h");
        let context = CompactionRequestContext {
            system: "Original synthetic system".into(),
            tools: vec![Tool::new(
                "synthetic_read",
                "Read synthetic data",
                json!({"type":"object","properties":{}})
                    .as_object()
                    .unwrap()
                    .clone(),
            )],
        };
        let history = history();
        provider
            .complete(
                &model,
                &context.system,
                &goose::conversation::merge_consecutive_messages_for_request(
                    goose::conversation::fix_conversation(history.clone())
                        .0
                        .agent_visible_messages(),
                ),
                &context.tools,
            )
            .await
            .unwrap();
        let result = compact_messages_with_context(
            &provider,
            &model,
            "synthetic-session",
            &history,
            false,
            Some(&context),
        )
        .await
        .unwrap();
        let requests = server.received_requests().await.unwrap();
        let mut normal: Value = serde_json::from_slice(&requests[0].body).unwrap();
        let mut summary: Value = serde_json::from_slice(&requests[1].body).unwrap();
        remove_cache_controls(&mut normal);
        remove_cache_controls(&mut summary);
        let messages_key = if anthropic { "messages" } else { "input" };
        let normal_messages = normal[messages_key].as_array().unwrap();
        let summary_messages = summary[messages_key].as_array().unwrap();
        assert_wire_prefix(normal_messages, summary_messages);
        normal.as_object_mut().unwrap().remove(messages_key);
        summary.as_object_mut().unwrap().remove(messages_key);
        assert_eq!(
            normal, summary,
            "model/system/tools/thinking/effort must be unchanged"
        );
        assert_eq!(result.usage.usage.cache_read_input_tokens, Some(256));
        assert_eq!(result.usage.usage.input_tokens, Some(278));
        assert_eq!(result.usage.usage.output_tokens, Some(20));
        let retained = result.conversation.agent_visible_messages();
        assert_eq!(
            retained.last().unwrap().as_concat_text(),
            "Run the focused tests next"
        );
        assert!(retained
            .iter()
            .all(|message| message.content.iter().all(|content| !matches!(
                content,
                MessageContent::ToolRequest(_) | MessageContent::ToolResponse(_)
            ))));
        assert!(retained
            .iter()
            .any(|message| message.as_concat_text().contains("Run focused tests")));
    }
}

#[tokio::test]
async fn truncated_summary_and_transport_failure_keep_original_history() {
    for status in [200, 500] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_string(response(true, "partial summary", "max_tokens"))
                    .append_header("content-type", "text/event-stream"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let provider = provider(server.uri());
        let history = history();
        let before = serde_json::to_value(&history).unwrap();
        let context = CompactionRequestContext {
            system: "synthetic system".into(),
            tools: vec![],
        };
        let result = compact_messages_with_context(
            &provider,
            &ModelConfig::new("claude-sonnet-4-6"),
            "synthetic",
            &history,
            false,
            Some(&context),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(serde_json::to_value(&history).unwrap(), before);
    }
}

#[tokio::test]
async fn cancellation_keeps_original_history() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(response(true, "summary", "end_turn"))
                .set_delay(std::time::Duration::from_secs(1)),
        )
        .mount(&server)
        .await;
    let provider = provider(server.uri());
    let history = history();
    let before = serde_json::to_value(&history).unwrap();
    let context = CompactionRequestContext {
        system: "synthetic system".into(),
        tools: vec![],
    };
    let model = ModelConfig::new("claude-sonnet-4-6");
    assert!(tokio::time::timeout(
        std::time::Duration::from_millis(50),
        compact_messages_with_context(
            &provider,
            &model,
            "synthetic",
            &history,
            false,
            Some(&context)
        )
    )
    .await
    .is_err());
    assert_eq!(serde_json::to_value(&history).unwrap(), before);
}

#[tokio::test]
async fn provider_side_tool_overrides_use_safe_serialized_fallback() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(response(true, "safe serialized summary", "end_turn"))
                .append_header("content-type", "text/event-stream"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let provider = provider(server.uri());
    let history = history();
    let model = ModelConfig::new("claude-sonnet-4-6").with_merged_request_params(
        std::collections::HashMap::from([("tools".into(), json!([{"type":"web_search"}]))]),
    );
    let context = CompactionRequestContext {
        system: "synthetic system".into(),
        tools: vec![],
    };
    assert!(compact_messages_with_context(
        &provider,
        &model,
        "synthetic",
        &history,
        false,
        Some(&context)
    )
    .await
    .is_ok());
    let requests = server.received_requests().await.unwrap();
    let payload: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert!(payload.get("tools").is_none());
    assert!(payload["system"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Conversation History"));
}

#[tokio::test]
async fn native_context_overflow_uses_existing_serialized_fallback() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &wiremock::Request| {
            let payload: Value = serde_json::from_slice(&request.body).unwrap();
            if payload["system"][0]["text"] == "synthetic system" {
                ResponseTemplate::new(400).set_body_json(
                    json!({"error":{"message":"This request exceeds maximum context length"}}),
                )
            } else {
                ResponseTemplate::new(200)
                    .set_body_string(response(true, "fallback synthetic summary", "end_turn"))
                    .append_header("content-type", "text/event-stream")
            }
        })
        .expect(2)
        .mount(&server)
        .await;
    let provider = provider(server.uri());
    let model = ModelConfig::new("claude-sonnet-4-6").with_thinking_effort(ThinkingEffort::High);
    let context = CompactionRequestContext {
        system: "synthetic system".into(),
        tools: vec![],
    };
    let result = compact_messages_with_context(
        &provider,
        &model,
        "synthetic",
        &history(),
        false,
        Some(&context),
    )
    .await
    .unwrap();
    assert!(result.conversation.agent_visible_messages()[0]
        .as_concat_text()
        .contains("fallback synthetic summary"));
    let requests = server.received_requests().await.unwrap();
    let payload: Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert!(payload.get("tools").is_none());
    assert!(payload["system"][0].get("cache_control").is_none());
    assert_eq!(payload["thinking"]["type"], "disabled");
}

#[tokio::test]
#[ignore = "Requires authorized OpenAI credentials; sends only synthetic prompts"]
async fn live_openai_synthetic_cache_smoke() {
    use goose_providers::api_client::{ApiClient, AuthMethod};
    use goose_providers::openai::OpenAiProviderBuilder;
    let key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY required");
    let client = ApiClient::new_with_tls(
        "https://api.openai.com".into(),
        AuthMethod::BearerToken(key),
        None,
    )
    .unwrap();
    let provider = OpenAiProviderBuilder::new(client).build();
    let prefix = (0..220).map(|index| format!("Synthetic inventory row {index}: blue widget; status ready; no real people or production data.\n")).collect::<String>();
    let context = CompactionRequestContext { system: format!("Synthetic compaction smoke test. Summarize only when asked. Otherwise answer READY.\n{prefix}"), tools: vec![] };
    let model = ModelConfig::new("gpt-4o").with_max_tokens(Some(2000));
    let history = Conversation::new_unvalidated(vec![
        Message::user().with_text("Remember the synthetic inventory. Reply READY.")
    ]);
    let (ready, normal_usage) = provider
        .complete(
            &model,
            &context.system,
            &history.agent_visible_messages(),
            &context.tools,
        )
        .await
        .unwrap_or_else(|error| {
            use goose_providers::errors::ProviderError;
            let category = match &error {
                ProviderError::Authentication(_) => "authentication",
                ProviderError::NetworkError(_) => "network_or_response_decode",
                ProviderError::RateLimitExceeded { .. } => "rate_limit",
                ProviderError::CreditsExhausted { .. } => "credits_exhausted",
                ProviderError::EndpointNotFound(_) => "route_not_found",
                ProviderError::RequestFailed(message) => [
                    "insufficient_quota",
                    "model_not_found",
                    "unsupported_parameter",
                    "invalid_api_key",
                    "Resource not found",
                ]
                .into_iter()
                .find(|marker| message.contains(marker))
                .unwrap_or("request_rejected"),
                _ => "other_provider_error",
            };
            panic!("Live provider request failed: {category}")
        });
    let mut messages = history.messages().clone();
    messages.push(ready);
    let history = Conversation::new_unvalidated(messages);
    let summary = compact_messages_with_context(
        &provider,
        &model,
        "synthetic-live-smoke",
        &history,
        true,
        Some(&context),
    )
    .await
    .unwrap();
    println!(
        "synthetic_live_smoke model={} normal={:?} summary={:?}",
        model.model_name, normal_usage.usage, summary.usage.usage
    );
    assert!(!summary.conversation.agent_visible_messages().is_empty());
}

#[tokio::test]
async fn forced_tool_choice_and_disabled_cache_use_serialized_fallback() {
    for model in [
        ModelConfig::new("claude-sonnet-4-6").with_merged_request_params(
            std::collections::HashMap::from([("tool_choice".into(), json!({"type":"any"}))]),
        ),
        ModelConfig::new("claude-sonnet-4-6").with_prompt_cache_disabled(),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(response(true, "serialized synthetic summary", "end_turn"))
                    .append_header("content-type", "text/event-stream"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let provider = provider(server.uri());
        let context = CompactionRequestContext {
            system: "original synthetic system".into(),
            tools: vec![],
        };
        compact_messages_with_context(
            &provider,
            &model,
            "synthetic",
            &history(),
            false,
            Some(&context),
        )
        .await
        .unwrap();
        let requests = server.received_requests().await.unwrap();
        let payload: Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert!(payload["system"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Conversation History"));
        assert!(payload.get("tool_choice").is_none());
        assert!(payload.get("tools").is_none());
    }
}

#[tokio::test]
async fn provider_declared_server_tools_fail_before_request() {
    use goose_providers::api_client::{ApiClient, AuthMethod};
    use goose_providers::base::ModelInfo;
    use goose_providers::openai::OpenAiProviderBuilder;
    let server = MockServer::start().await;
    let client = ApiClient::new_with_tls(server.uri(), AuthMethod::NoAuth, None).unwrap();
    let mut declared = ModelInfo::new("gpt-4o");
    declared.request_params = Some(std::collections::HashMap::from([(
        "tools".into(),
        json!([{"type":"web_search"}]),
    )]));
    let provider = OpenAiProviderBuilder::new(client)
        .custom_models(Some(vec![declared]))
        .build();
    let context = CompactionRequestContext {
        system: "synthetic".into(),
        tools: vec![],
    };
    for context in [Some(&context), None] {
        assert!(compact_messages_with_context(
            &provider,
            &ModelConfig::new("gpt-4o"),
            "synthetic",
            &history(),
            false,
            context
        )
        .await
        .is_err());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn openai_chat_native_prefix_keeps_client_tools_as_functions() {
    use goose_providers::api_client::{ApiClient, AuthMethod};
    use goose_providers::openai::OpenAiProviderBuilder;
    let server = MockServer::start().await;
    let body = format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        json!({"id":"synthetic","model":"gpt-4o","choices":[{"index":0,"delta":{"content":"synthetic summary"},"finish_reason":null}]}),
        json!({"id":"synthetic","model":"gpt-4o","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":278,"completion_tokens":20,"total_tokens":298,"prompt_tokens_details":{"cached_tokens":256}}})
    );
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(body)
                .append_header("content-type", "text/event-stream"),
        )
        .expect(2)
        .mount(&server)
        .await;
    let client = ApiClient::new_with_tls(server.uri(), AuthMethod::NoAuth, None).unwrap();
    let provider = OpenAiProviderBuilder::new(client).build();
    let context = CompactionRequestContext {
        system: "original synthetic system".into(),
        tools: vec![Tool::new(
            "web_search_preview",
            "Synthetic MCP tool, never an OpenAI server tool",
            json!({"type":"object"}).as_object().unwrap().clone(),
        )],
    };
    let model = ModelConfig::new("gpt-4o");
    let history = Conversation::new_unvalidated(vec![
        Message::user().with_text("Synthetic task"),
        Message::assistant().with_text("Synthetic ready"),
    ]);
    provider
        .complete(
            &model,
            &context.system,
            &history.agent_visible_messages(),
            &context.tools,
        )
        .await
        .unwrap();
    let result = compact_messages_with_context(
        &provider,
        &model,
        "synthetic",
        &history,
        false,
        Some(&context),
    )
    .await
    .unwrap();
    let requests = server.received_requests().await.unwrap();
    let mut normal: Value = serde_json::from_slice(&requests[0].body).unwrap();
    let mut summary: Value = serde_json::from_slice(&requests[1].body).unwrap();
    let normal_messages = normal["messages"].as_array().unwrap();
    assert_eq!(
        &summary["messages"].as_array().unwrap()[..normal_messages.len()],
        normal_messages
    );
    assert_eq!(summary["tools"][0]["type"], "function");
    assert_eq!(
        summary["tools"][0]["function"]["name"],
        "web_search_preview"
    );
    normal.as_object_mut().unwrap().remove("messages");
    summary.as_object_mut().unwrap().remove("messages");
    assert_eq!(normal, summary);
    assert_eq!(result.usage.usage.cache_read_input_tokens, Some(256));
}
