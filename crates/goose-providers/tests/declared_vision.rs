use goose_providers::{
    api_client::{ApiClient, AuthMethod},
    base::{ModelInfo, Provider},
    conversation::message::Message,
    model::ModelConfig,
    openai::OpenAiProviderBuilder,
};
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};
use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

async fn image_request(models: Vec<ModelInfo>, model: ModelConfig, endpoint: &str) -> Value {
    let server = MockServer::start().await;
    let response = if endpoint == "responses" {
        json!({
            "id": "response-1", "object": "response", "created_at": 0,
            "status": "completed", "model": model.model_name,
            "output": [{"type": "message", "role": "assistant", "content": [
                {"type": "output_text", "text": "Seen"}
            ]}]
        })
    } else {
        json!({
            "id": "response-1", "model": model.model_name,
            "choices": [{"message": {"role": "assistant", "content": "Seen"},
                         "finish_reason": "stop"}]
        })
    };
    Mock::given(method("POST"))
        .and(path(format!("/v1/{endpoint}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .expect(1)
        .mount(&server)
        .await;

    let client = ApiClient::new_with_tls(server.uri(), AuthMethod::NoAuth, None)
        .unwrap()
        .with_loopback_http_only()
        .unwrap();
    let provider = OpenAiProviderBuilder::new(client)
        .name("custom_vision")
        .base_path(format!("v1/{endpoint}"))
        .custom_models(Some(models))
        .supports_streaming(false)
        .build();
    let messages = [
        Message::user()
            .with_text("Describe these images")
            .with_image("dXNlcg==", "image/png"),
        Message::assistant()
            .with_tool_request("tool-1", Ok(CallToolRequestParams::new("screenshot"))),
        Message::user().with_tool_response(
            "tool-1",
            Ok(CallToolResult::success(vec![ContentBlock::image(
                "dG9vbA==",
                "image/png",
            )])),
        ),
    ];
    provider
        .complete(&model, "Describe images", &messages, &[])
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: Value = requests[0].body_json().unwrap();
    assert_eq!(body["model"], model.model_name);
    body
}

fn assert_images(body: &Value, expected: bool) {
    let request = body.to_string();
    if expected {
        assert!(request.contains("data:image/png;base64,dXNlcg=="), "{body}");
        assert!(request.contains("data:image/png;base64,dG9vbA=="), "{body}");
    } else {
        assert!(!request.contains("data:image"), "{body}");
        assert!(request.contains("model does not support vision"), "{body}");
    }
}

#[tokio::test]
async fn declared_vision_controls_user_and_tool_images_on_both_endpoints() {
    for endpoint in ["chat/completions", "responses"] {
        for (declared, configured, expected) in [
            (Some(true), Some(false), true),
            (Some(false), Some(true), false),
            (Some(true), None, true),
            (Some(false), None, false),
            (None, Some(true), true),
            (None, Some(false), false),
            (None, None, false),
        ] {
            let mut info = ModelInfo::new("local-vlm");
            info.supports_vision = declared;
            let mut model = ModelConfig::new("local-vlm");
            model.supports_vision = configured;
            let body = image_request(vec![info], model, endpoint).await;
            assert_images(&body, expected);
        }
    }
}

#[tokio::test]
async fn model_matching_never_borrows_vision_from_a_case_collision() {
    for endpoint in ["chat/completions", "responses"] {
        let models = vec![
            ModelInfo::new("Foo").with_vision_support(false),
            ModelInfo::new("foo").with_vision_support(true),
        ];
        for (name, expected) in [("Foo", false), ("foo", true), ("FOO", false)] {
            let body = image_request(models.clone(), ModelConfig::new(name), endpoint).await;
            assert_images(&body, expected);
        }

        let body = image_request(
            vec![ModelInfo::new("Foo").with_vision_support(true)],
            ModelConfig::new("foo").with_vision_support(false),
            endpoint,
        )
        .await;
        assert_images(&body, false);
    }
}
