use goose_providers::{
    api_client::{ApiClient, AuthMethod},
    base::{OpenAiWireApi, Provider},
    conversation::message::Message,
    declarative::{deserialize_provider_config, KeyResolver},
    model::ModelConfig,
    openai::{from_declarative_config, OpenAiProvider, OpenAiProviderBuilder},
    thinking::ThinkingEffort,
    venice,
};
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, Tool};
use serde_json::{json, Value};
use std::{convert::Infallible, sync::Arc};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

struct NoCredentials;

impl KeyResolver for NoCredentials {
    type Error = Infallible;

    fn resolve_key(&self, key: &str) -> Result<String, Self::Error> {
        panic!("credential lookup must not occur: {key}")
    }
}

fn definition(base_url: &str) -> Value {
    json!({
        "name": "custom_wire_api", "display_name": "Wire API test",
        "engine": "openai", "base_url": base_url, "requires_auth": false,
        "supports_streaming": false, "models": []
    })
}

fn provider(document: &Value) -> OpenAiProvider {
    let config = deserialize_provider_config(&document.to_string()).unwrap();
    from_declarative_config(config, None, NoCredentials)
        .unwrap()
        .build()
}

async fn request(
    server: &MockServer,
    provider: &impl Provider,
    model: &ModelConfig,
    api: OpenAiWireApi,
    endpoint: &str,
    messages: &[Message],
) -> Value {
    let reply = match api {
        OpenAiWireApi::ChatCompletions => json!({
            "id": "reply-1", "model": model.model_name,
            "choices": [{"message": {"role": "assistant", "content": "Hello"},
                         "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 4, "total_tokens": 14}
        }),
        OpenAiWireApi::Responses => json!({
            "id": "reply-1", "object": "response", "created_at": 0,
            "status": "completed", "model": model.model_name,
            "output": [{"type": "message", "role": "assistant", "content": [
                {"type": "output_text", "text": "Hello"}
            ]}],
            "usage": {"input_tokens": 10, "output_tokens": 4, "total_tokens": 14}
        }),
    };
    Mock::given(method("POST"))
        .and(path(endpoint))
        .respond_with(ResponseTemplate::new(200).set_body_json(reply))
        .expect(1)
        .mount(server)
        .await;
    let tool = Tool::new(
        "write_file",
        "Write a file",
        Arc::new(
            json!({"type": "object", "properties": {"text": {"type": "string"}}})
                .as_object()
                .unwrap()
                .clone(),
        ),
    );
    let (reply, usage) = provider
        .complete(model, "Be helpful", messages, &[tool])
        .await
        .unwrap();
    assert_eq!(reply.as_concat_text(), "Hello");
    assert_eq!(usage.usage.total_tokens, Some(14));
    server.verify().await;
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].headers.get("authorization").is_none());
    let body: Value = requests[0].body_json().unwrap();
    assert_eq!(body["model"], model.model_name);
    assert_ne!(body["stream"], true);
    match api {
        OpenAiWireApi::ChatCompletions => {
            assert!(body["messages"].is_array());
            assert!(body.get("input").is_none());
            assert_eq!(body["tools"][0]["function"]["name"], "write_file");
        }
        OpenAiWireApi::Responses => {
            assert!(body["input"].is_array());
            assert!(body.get("messages").is_none());
            assert_eq!(body["tools"][0]["name"], "write_file");
        }
    }
    server.reset().await;
    body
}

#[test]
fn venice_keeps_its_full_endpoint_and_streaming_with_explicit_chat() {
    let config = deserialize_provider_config(venice::JSON).unwrap();
    assert_eq!(
        config.base_url,
        "https://api.venice.ai/api/v1/chat/completions"
    );
    assert_eq!(config.wire_api, Some(OpenAiWireApi::ChatCompletions));
    assert_eq!(config.catalog_provider_id.as_deref(), Some("venice"));
    assert_eq!(config.supports_streaming, Some(true));
}

#[tokio::test]
async fn exact_model_override_precedes_provider_then_legacy_routing() {
    use OpenAiWireApi::{ChatCompletions, Responses};
    let server = MockServer::start().await;
    let mut document = definition(&format!("{}/proxy/v2/responses", server.uri()));
    document["wire_api"] = json!("chat_completions");
    document["models"] = json!([{"name": "Llama", "wire_api": "responses"}]);
    let explicit = provider(&document);
    for (model, api, endpoint) in [
        ("Llama", Responses, "/proxy/v2/responses"),
        ("llama", ChatCompletions, "/proxy/v2/chat/completions"),
        ("gpt-5", ChatCompletions, "/proxy/v2/chat/completions"),
    ] {
        request(
            &server,
            &explicit,
            &ModelConfig::new(model),
            api,
            endpoint,
            &[],
        )
        .await;
    }
    document["base_url"] = json!(format!("{}/v1/chat/completions", server.uri()));
    let explicit = provider(&document);
    request(
        &server,
        &explicit,
        &ModelConfig::new("gpt-5"),
        ChatCompletions,
        "/v1/chat/completions",
        &[],
    )
    .await;
    document.as_object_mut().unwrap().remove("wire_api");
    let baseline = provider(&document);
    request(
        &server,
        &baseline,
        &ModelConfig::new("gpt-5"),
        Responses,
        "/v1/responses",
        &[],
    )
    .await;
}

#[tokio::test]
async fn native_default_and_custom_paths_retain_legacy_routing() {
    use OpenAiWireApi::{ChatCompletions, Responses};
    let server = MockServer::start().await;
    for (base_path, model, api, endpoint) in [
        (None, "llama", Responses, "/v1/responses"),
        (
            Some("v1/chat/completions"),
            "llama",
            Responses,
            "/v1/responses",
        ),
        (
            Some("proxy/v2/chat/completions"),
            "gpt-5",
            ChatCompletions,
            "/proxy/v2/chat/completions",
        ),
        (
            Some("proxy/v2/responses"),
            "llama",
            Responses,
            "/proxy/v2/responses",
        ),
    ] {
        let client = ApiClient::new_with_tls(server.uri(), AuthMethod::NoAuth, None).unwrap();
        let mut builder = OpenAiProviderBuilder::new(client)
            .native_openai(true)
            .supports_streaming(false);
        if let Some(base_path) = base_path {
            builder = builder.base_path(base_path);
        }
        request(
            &server,
            &builder.build(),
            &ModelConfig::new(model),
            api,
            endpoint,
            &[],
        )
        .await;
    }
}

#[tokio::test]
async fn venice_alias_off_and_tool_history_encode_on_both_wire_apis() {
    use OpenAiWireApi::{ChatCompletions, Responses};
    let server = MockServer::start().await;
    let mut document: Value = serde_json::from_str(venice::JSON).unwrap();
    document["base_url"] = json!(format!("{}/api/v1/chat/completions", server.uri()));
    document["requires_auth"] = json!(false);
    document["supports_streaming"] = json!(false);
    document.as_object_mut().unwrap().remove("api_key_env");
    let alias = "openai-gpt-56-luna";
    let model = ModelConfig::new(alias).with_thinking_effort(ThinkingEffort::Off);
    let arguments = json!({"text": "Hello"}).as_object().unwrap().clone();
    let messages = [
        Message::user().with_text("Write Hello"),
        Message::assistant().with_tool_request(
            "call-1",
            Ok(CallToolRequestParams::new("write_file").with_arguments(arguments)),
        ),
        Message::user().with_tool_response(
            "call-1",
            Ok(CallToolResult::success(vec![ContentBlock::text("Written")])),
        ),
    ];
    for (api, endpoint) in [
        (ChatCompletions, "/api/v1/chat/completions"),
        (Responses, "/api/v1/responses"),
    ] {
        if api == Responses {
            document["models"] = json!([{"name": alias, "wire_api": "responses"}]);
        }
        let body = request(
            &server,
            &provider(&document),
            &model,
            api,
            endpoint,
            &messages,
        )
        .await;
        match api {
            ChatCompletions => {
                assert_eq!(body["reasoning_effort"], "none");
                let history = body["messages"].as_array().unwrap();
                let call = history
                    .iter()
                    .find(|item| item["role"] == "assistant")
                    .unwrap();
                assert_eq!(call["tool_calls"][0]["id"], "call-1");
                assert_eq!(call["tool_calls"][0]["function"]["name"], "write_file");
                let result = history.iter().find(|item| item["role"] == "tool").unwrap();
                assert_eq!(result["tool_call_id"], "call-1");
                assert!(result["content"].to_string().contains("Written"));
            }
            Responses => {
                assert_eq!(body["reasoning"]["effort"], "none");
                let history = body["input"].as_array().unwrap();
                let call = history
                    .iter()
                    .find(|item| item["type"] == "function_call")
                    .unwrap();
                assert_eq!(call["call_id"], "call-1");
                assert_eq!(call["name"], "write_file");
                let result = history
                    .iter()
                    .find(|item| item["type"] == "function_call_output")
                    .unwrap();
                assert_eq!(result["call_id"], "call-1");
                assert!(result["output"].to_string().contains("Written"));
            }
        }
    }
}
