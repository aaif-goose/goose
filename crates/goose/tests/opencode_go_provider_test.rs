use goose::conversation::message::Message;
use goose_providers::model::ModelConfig;

async fn make_test_provider(
    server: &wiremock::MockServer,
) -> std::sync::Arc<dyn goose::providers::base::Provider> {
    let configs = goose_providers::declarative::fixed_provider_configs().unwrap();
    let mut config = configs
        .into_iter()
        .find(|config| config.name == "opencode_go")
        .expect("OpenCode Go config should exist");

    config.base_url = format!("{}/zen/go", server.uri());
    config.api_key_env.clear();
    config.auth = Some(goose_providers::declarative::AuthConfig {
        command: "echo".to_string(),
        args: vec!["test-token-123".to_string()],
        refresh_interval: 3600,
        timeout_seconds: None,
        cwd: None,
    });
    config.requires_auth = false;

    let mut registry = goose::providers::provider_registry::ProviderRegistry::new(None);
    goose::config::declarative_providers::register_declarative_provider(
        &mut registry,
        config,
        goose::providers::base::ProviderType::Declarative,
    );
    registry.create("opencode_go", vec![]).await.unwrap()
}

#[tokio::test]
async fn routes_models_to_expected_endpoints() {
    for (model_name, expected_path, auth_header, auth_value, expected_anthropic_version) in [
        (
            "grok-4.6",
            "/zen/go/v1/responses",
            "authorization",
            "Bearer test-token-123",
            None,
        ),
        (
            "grok-4.5",
            "/zen/go/v1/chat/completions",
            "authorization",
            "Bearer test-token-123",
            None,
        ),
        (
            "omen-alpha",
            "/zen/go/v1/chat/completions",
            "authorization",
            "Bearer test-token-123",
            None,
        ),
        (
            "kimi-k2.6",
            "/zen/go/v1/chat/completions",
            "authorization",
            "Bearer test-token-123",
            None,
        ),
        (
            "qwen3.6-plus",
            "/zen/go/v1/messages",
            "x-api-key",
            "test-token-123",
            Some(goose_providers::anthropic::ANTHROPIC_API_VERSION),
        ),
    ] {
        let server = wiremock::MockServer::start().await;
        let provider = make_test_provider(&server).await;

        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(expected_path))
            .and(wiremock::matchers::header(auth_header, auth_value))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string("data: [DONE]\n\n"),
            )
            .mount(&server)
            .await;

        let model = ModelConfig::new(model_name);
        let message = Message::user().with_text("hello");

        let tool = rmcp::model::Tool::new(
            "test-echo-tool",
            "Echo test",
            rmcp::object!({"type": "object", "properties": {}}),
        );
        let _stream = goose::session_context::with_session_id(
            Some("test-session-123".to_string()),
            provider.stream(&model, "system", &[message], &[tool]),
        )
        .await
        .unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "model: {model_name}");

        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["model"].as_str(), Some(model_name));
        assert!(
            body["tools"]
                .as_array()
                .is_some_and(|tool| !tool.is_empty()),
            "model: {model_name}"
        );
        assert_eq!(
            requests[0]
                .headers
                .get("x-opencode-session")
                .unwrap()
                .to_str()
                .unwrap(),
            "test-session-123"
        );
        if let Some(expected_version) = expected_anthropic_version {
            let actual = requests[0]
                .headers
                .get("anthropic-version")
                .unwrap()
                .to_str()
                .unwrap();
            assert_eq!(actual, expected_version, "model: {model_name}");
        }
    }
}

#[tokio::test]
async fn fetches_models_from_go_endpoint() {
    let server = wiremock::MockServer::start().await;
    let provider = make_test_provider(&server).await;

    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/zen/go/v1/models"))
        .and(wiremock::matchers::header(
            "authorization",
            "Bearer test-token-123",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({
                    "data": [
                        {"id": "catalog-only-model"},
                    ]
                })),
        )
        .mount(&server)
        .await;

    let models = provider.fetch_supported_models().await.unwrap();
    assert_eq!(models, vec!["catalog-only-model"]);
    assert_eq!(provider.get_context_limit("kimi-k2.6", None).await, 262_144,);
}
