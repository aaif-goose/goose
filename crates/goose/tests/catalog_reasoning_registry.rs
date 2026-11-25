use goose::config::declarative_providers::{create_custom_provider, CreateCustomProviderParams};
use goose::conversation::message::Message;
use goose::model_config::model_config_from_user_config;
use goose::providers::{get_from_registry, refresh_custom_providers};
use oi_kit_providers::base::ModelInfo;
use serde_json::json;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn catalog_reasoning_keeps_openai_request_shape_for_renamed_provider() {
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().to_str().unwrap();
    let _guard = env_lock::lock_env([
        ("GOOSE_PATH_ROOT", Some(root_path)),
        ("GOOSE_TEMPERATURE", Some("0.5")),
        ("GOOSE_THINKING_EFFORT", None),
    ]);

    // Catalogued with reasoning only under fireworks-ai, and unknown to the
    // OpenAI reasoning classification.
    let model_name = "accounts/fireworks/models/kimi-k3";

    for endpoint in ["chat/completions", "responses"] {
        let server = MockServer::start().await;
        let response = if endpoint == "responses" {
            json!({
                "id": "response-1", "object": "response", "created_at": 0,
                "status": "completed", "model": model_name,
                "output": [{"type": "message", "role": "assistant", "content": [
                    {"type": "output_text", "text": "ok"}
                ]}]
            })
        } else {
            json!({
                "id": "response-1", "model": model_name,
                "choices": [{"message": {"role": "assistant", "content": "ok"},
                             "finish_reason": "stop"}]
            })
        };
        Mock::given(method("POST"))
            .and(path(format!("/v1/{endpoint}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount(&server)
            .await;

        let saved = create_custom_provider(CreateCustomProviderParams {
            engine: "openai".to_string(),
            display_name: "Catalog Reasoning Self Test".to_string(),
            api_url: server.uri(),
            api_key: None,
            models: vec![ModelInfo::new(model_name)],
            supports_streaming: Some(false),
            headers: None,
            requires_auth: false,
            catalog_provider_id: Some("fireworks-ai".to_string()),
            base_path: Some(format!("v1/{endpoint}")),
            toolshim: false,
            preserves_thinking: None,
            auth: None,
        })
        .unwrap();
        refresh_custom_providers().await.unwrap();
        let entry = get_from_registry(&saved.name).await.unwrap();
        let model = entry
            .normalize_model_config(model_config_from_user_config(&saved.name, model_name).unwrap())
            .unwrap();
        entry
            .create(vec![])
            .await
            .unwrap()
            .complete(&model, "system", &[Message::user().with_text("hi")], &[])
            .await
            .unwrap();

        let requests = server.received_requests().await.unwrap();
        let body: serde_json::Value = requests[0].body_json().unwrap();
        assert_eq!(body["temperature"], json!(0.5), "{endpoint}: {body}");
        if endpoint == "responses" {
            assert!(body.get("include").is_none(), "{endpoint}: {body}");
        } else {
            assert_eq!(body["messages"][0]["role"], "system", "{endpoint}: {body}");
        }
    }
}
