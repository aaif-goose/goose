use goose::config::declarative_providers::{create_custom_provider, CreateCustomProviderParams};
use goose::conversation::message::Message;
use goose::providers::{get_from_registry, refresh_custom_providers};
use goose_providers::{base::ModelInfo, model::ModelConfig};
use serde_json::json;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn persisted_vision_declarations_control_registry_requests() {
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().to_str().unwrap();
    let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path))]);

    for endpoint in ["chat/completions", "responses"] {
        for (declared, configured, expected) in [
            (Some(true), false, true),
            (Some(false), true, false),
            (None, true, true),
            (None, false, false),
        ] {
            let server = MockServer::start().await;
            let response = if endpoint == "responses" {
                json!({
                    "id": "response-1", "object": "response", "created_at": 0,
                    "status": "completed", "model": "local-vlm",
                    "output": [{"type": "message", "role": "assistant", "content": [
                        {"type": "output_text", "text": "Seen"}
                    ]}]
                })
            } else {
                json!({
                    "id": "response-1", "model": "local-vlm",
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

            let mut info = ModelInfo::new("local-vlm");
            info.supports_vision = declared;
            let saved = create_custom_provider(CreateCustomProviderParams {
                engine: "openai".to_string(),
                display_name: "Declared Vision Self Test".to_string(),
                api_url: server.uri(),
                api_key: None,
                models: vec![info],
                supports_streaming: Some(false),
                headers: None,
                requires_auth: false,
                catalog_provider_id: None,
                base_path: Some(format!("v1/{endpoint}")),
                toolshim: false,
                preserves_thinking: None,
                auth: None,
            })
            .unwrap();
            refresh_custom_providers().await.unwrap();
            let entry = get_from_registry(&saved.name).await.unwrap();
            let model = entry
                .normalize_model_config(
                    ModelConfig::new("local-vlm").with_vision_support(configured),
                )
                .unwrap();
            let provider = entry.create(vec![]).await.unwrap();
            provider
                .complete(
                    &model,
                    "Describe images",
                    &[Message::user()
                        .with_text("Describe this image")
                        .with_image("dXNlcg==", "image/png")],
                    &[],
                )
                .await
                .unwrap();
            let requests = server.received_requests().await.unwrap();
            assert_eq!(requests.len(), 1);
            let body: serde_json::Value = requests[0].body_json().unwrap();
            assert_eq!(body["model"], "local-vlm");
            assert_eq!(
                body.to_string().contains("data:image/png;base64,dXNlcg=="),
                expected,
                "{body}"
            );
        }
    }
}
