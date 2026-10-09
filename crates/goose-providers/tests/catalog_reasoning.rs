use goose_providers::{
    api_client::{ApiClient, AuthMethod},
    base::{ModelInfo, Provider},
    conversation::message::Message,
    declarative::{DeclarativeProviderConfig, KeyResolver},
    model::ModelConfig,
    openai::{from_declarative_config, OpenAiProviderBuilder},
    thinking::ThinkingEffort,
};
use rmcp::model::Tool;
use serde_json::{json, Value};
use std::{collections::HashMap, convert::Infallible, sync::Arc};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const CATALOG_MODEL: &str = "deepseek-v4.1-flash";
const SYSTEM: &str = "Use the supplied tools when needed";

fn model(name: &str, reasoning: Option<bool>, effort: ThinkingEffort) -> ModelConfig {
    let mut config = ModelConfig::new(name)
        .with_temperature(Some(0.5))
        .with_max_tokens(Some(1024))
        .with_thinking_effort(effort);
    config.reasoning = reasoning;
    config
}

async fn capture_request(
    server: &MockServer,
    provider: &dyn Provider,
    config: &ModelConfig,
    endpoint: &str,
) -> Value {
    let response = if endpoint.ends_with("responses") {
        json!({
            "id": "response-1", "object": "response", "created_at": 0,
            "status": "completed", "model": config.model_name,
            "output": [{"type": "message", "role": "assistant", "content": [
                {"type": "output_text", "text": "Done"}
            ]}]
        })
    } else {
        json!({
            "id": "response-1", "model": config.model_name,
            "choices": [{"message": {"role": "assistant", "content": "Done"},
                         "finish_reason": "stop"}]
        })
    };
    Mock::given(method("POST"))
        .and(path(format!("/{endpoint}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .expect(1)
        .mount(server)
        .await;

    let tool = Tool::new(
        "lookup",
        "Look up a value",
        Arc::new(
            json!({"type": "object", "properties": {"key": {"type": "string"}},
                   "required": ["key"]})
            .as_object()
            .unwrap()
            .clone(),
        ),
    );
    provider
        .complete(
            config,
            SYSTEM,
            &[Message::user().with_text("Look up the answer")],
            &[tool],
        )
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: Value = requests[0].body_json().unwrap();
    assert_eq!(body["model"], config.model_name);
    assert_eq!(body["tools"][0]["type"], "function");
    assert!(body.get("thinking_effort").is_none(), "{body}");
    if endpoint.ends_with("responses") {
        assert_eq!(body["tools"][0]["name"], "lookup");
        assert_eq!(body["input"][0]["role"], "system");
        assert_eq!(body["input"][0]["content"][0]["text"], SYSTEM);
        assert_eq!(body["max_output_tokens"], 1024);
        assert!(body.get("messages").is_none(), "{body}");
        assert!(body.get("max_tokens").is_none(), "{body}");
        assert!(body.get("max_completion_tokens").is_none(), "{body}");
        assert!(body.get("reasoning_effort").is_none(), "{body}");
    } else {
        assert_eq!(body["tools"][0]["function"]["name"], "lookup");
        assert_eq!(body["messages"][0]["content"], SYSTEM);
        assert!(body.get("input").is_none(), "{body}");
        assert!(body.get("max_output_tokens").is_none(), "{body}");
    }
    body
}

async fn request(
    config: ModelConfig,
    provider_name: &str,
    catalog_provider_id: Option<&str>,
    endpoint: &str,
    native_openai: bool,
    models: Vec<ModelInfo>,
) -> Value {
    let server = MockServer::start().await;
    let client = ApiClient::new_with_tls(server.uri(), AuthMethod::NoAuth, None)
        .unwrap()
        .with_loopback_http_only()
        .unwrap();
    let provider = OpenAiProviderBuilder::new(client)
        .name(provider_name)
        .catalog_provider_id(catalog_provider_id.map(str::to_string))
        .base_path(endpoint)
        .native_openai(native_openai)
        .custom_models(Some(models))
        .supports_streaming(false)
        .build();
    capture_request(&server, &provider, &config, endpoint).await
}

fn assert_compatible(body: &Value, endpoint: &str, effort: Option<&str>) {
    assert_eq!(body["temperature"], 0.5, "{body}");
    if endpoint.ends_with("responses") {
        assert_eq!(
            body.pointer("/reasoning/effort"),
            effort.map(json).as_ref(),
            "{body}"
        );
        if effort.is_none() {
            assert!(body.get("reasoning").is_none(), "{body}");
        }
    } else {
        assert_eq!(body["messages"][0]["role"], "system", "{body}");
        assert_eq!(body["max_tokens"], 1024, "{body}");
        assert!(body.get("max_completion_tokens").is_none(), "{body}");
        assert_eq!(
            body.get("reasoning_effort"),
            effort.map(json).as_ref(),
            "{body}"
        );
    }
}

fn json(value: &str) -> Value {
    Value::String(value.to_string())
}

#[tokio::test]
async fn compatible_catalog_efforts_are_clamped_without_changing_protocol_fields() {
    for endpoint in ["chat/completions", "v1/responses"] {
        for reasoning in [None, Some(true), Some(false)] {
            for (effort, expected) in [
                (ThinkingEffort::Off, "low"),
                (ThinkingEffort::Low, "low"),
                (ThinkingEffort::Medium, "high"),
                (ThinkingEffort::High, "high"),
                (ThinkingEffort::Max, "max"),
            ] {
                let body = request(
                    model(CATALOG_MODEL, reasoning, effort),
                    "ollama_cloud",
                    None,
                    endpoint,
                    false,
                    vec![],
                )
                .await;
                assert_compatible(
                    &body,
                    endpoint,
                    (reasoning != Some(false)).then_some(expected),
                );
            }
        }
    }
}

#[tokio::test]
async fn selected_catalog_id_overrides_provider_name_and_accepts_provider_aliases() {
    for endpoint in ["chat/completions", "v1/responses"] {
        for (name, catalog_id, expected) in [
            ("custom_gateway", Some("ollama_cloud"), Some("max")),
            ("custom_gateway", Some("ollama-cloud"), Some("max")),
            ("ollama_cloud", Some("xai"), None),
            ("ollama_cloud", Some("unknown_catalog"), None),
            ("custom_gateway", None, None),
        ] {
            let body = request(
                model(CATALOG_MODEL, Some(true), ThinkingEffort::Max),
                name,
                catalog_id,
                endpoint,
                false,
                vec![],
            )
            .await;
            assert_compatible(&body, endpoint, expected);
        }
    }
}

#[tokio::test]
async fn compatible_models_do_not_borrow_reasoning_controls_from_upstream_publishers() {
    for endpoint in ["chat/completions", "v1/responses"] {
        for name in ["gpt-5.4", "o3-mini", "grok-4.5", "unknown-reasoner"] {
            for reasoning in [None, Some(true), Some(false)] {
                let config =
                    model(name, reasoning, ThinkingEffort::High).with_canonical_limits("openai");
                let body = request(config, "custom_gateway", None, endpoint, false, vec![]).await;
                assert_compatible(&body, endpoint, None);
            }
        }
        let body = request(
            model("gpt-5.4", Some(true), ThinkingEffort::High),
            "ollama_cloud",
            None,
            endpoint,
            false,
            vec![],
        )
        .await;
        assert_compatible(&body, endpoint, None);
    }
}

#[tokio::test]
async fn catalog_models_without_effort_controls_and_case_mismatches_omit_generated_effort() {
    for endpoint in ["chat/completions", "v1/responses"] {
        for (provider, name) in [
            ("ollama_cloud", "kimi-k2.5"),
            ("ollama_cloud", "mistral-large-3:675b"),
            ("ollama_cloud", "DeepSeek-v4.1-flash"),
            ("ollama_cloud", "deepseek-v4.1-flash:custom"),
            ("xai", "grok-4.20-0309-reasoning"),
            ("xai", "grok-4.20-0309-non-reasoning"),
        ] {
            let body = request(
                model(name, Some(true), ThinkingEffort::Max),
                provider,
                None,
                endpoint,
                false,
                vec![],
            )
            .await;
            assert_compatible(&body, endpoint, None);
        }
    }
}

#[tokio::test]
async fn xai_preserves_effort_restrictions_and_temperature_handling() {
    for endpoint in ["chat/completions", "v1/responses"] {
        for (name, effort, expected) in [
            ("grok-4.3", ThinkingEffort::Off, "none"),
            ("grok-4.5", ThinkingEffort::Off, "low"),
            ("grok-4.5", ThinkingEffort::Medium, "medium"),
            ("grok-4.5", ThinkingEffort::Max, "high"),
            ("GROK-4.5", ThinkingEffort::Max, "high"),
        ] {
            let body = request(
                model(name, None, effort),
                "xai",
                None,
                endpoint,
                false,
                vec![],
            )
            .await;
            assert!(body.get("temperature").is_none(), "{body}");
            let mut protocol_fields = body.clone();
            protocol_fields["temperature"] = json!(0.5);
            assert_compatible(&protocol_fields, endpoint, Some(expected));
        }
    }
}

#[tokio::test]
async fn raw_reasoning_effort_overrides_mapping_even_when_reasoning_is_disabled_or_unknown() {
    for endpoint in ["chat/completions", "v1/responses"] {
        for name in [CATALOG_MODEL, "unknown-reasoner"] {
            for reasoning in [None, Some(true), Some(false)] {
                let config =
                    model(name, reasoning, ThinkingEffort::Max).with_merged_request_params(
                        HashMap::from([("reasoning_effort".to_string(), json!("vendor-custom"))]),
                    );
                let body = request(config, "ollama_cloud", None, endpoint, false, vec![]).await;
                assert_compatible(&body, endpoint, Some("vendor-custom"));
            }
        }
    }
}

#[tokio::test]
async fn declared_request_params_remain_final_chat_overrides() {
    let mut info = ModelInfo::new(CATALOG_MODEL);
    info.request_params = Some(HashMap::from([
        ("reasoning_effort".to_string(), json!("declared-effort")),
        ("temperature".to_string(), json!(0.25)),
        ("max_tokens".to_string(), json!(2048)),
        ("vendor_option".to_string(), json!(true)),
    ]));
    for reasoning in [Some(true), Some(false)] {
        let config =
            model(CATALOG_MODEL, reasoning, ThinkingEffort::Max).with_merged_request_params(
                HashMap::from([("reasoning_effort".to_string(), json!("raw-effort"))]),
            );
        let body = request(
            config,
            "ollama_cloud",
            None,
            "chat/completions",
            false,
            vec![info.clone()],
        )
        .await;
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["reasoning_effort"], "declared-effort");
        assert_eq!(body["temperature"], 0.25);
        assert_eq!(body["max_tokens"], 2048);
        assert_eq!(body["vendor_option"], true);
        assert!(body.get("max_completion_tokens").is_none(), "{body}");
    }
}

#[tokio::test]
async fn declared_model_request_params_match_exact_case() {
    let models: Vec<_> = [("Foo", "high"), ("foo", "low")]
        .into_iter()
        .map(|(name, effort)| {
            let mut info = ModelInfo::new(name);
            info.request_params = Some(HashMap::from([(
                "reasoning_effort".to_string(),
                json!(effort),
            )]));
            info
        })
        .collect();
    for (name, expected) in [("Foo", Some("high")), ("foo", Some("low")), ("FOO", None)] {
        let body = request(
            model(name, Some(true), ThinkingEffort::Max),
            "custom_gateway",
            None,
            "chat/completions",
            false,
            models.clone(),
        )
        .await;
        assert_compatible(&body, "chat/completions", expected);
    }
}

#[tokio::test]
async fn compatible_gpt_models_use_selected_catalog_without_native_protocol_rewrites() {
    for endpoint in ["chat/completions", "v1/responses"] {
        for reasoning in [None, Some(true), Some(false)] {
            for (effort, expected) in [
                (ThinkingEffort::Off, "none"),
                (ThinkingEffort::Max, "xhigh"),
            ] {
                let body = request(
                    model("gpt-5.4", reasoning, effort),
                    "custom_gateway",
                    Some("openai"),
                    endpoint,
                    false,
                    vec![],
                )
                .await;
                assert_compatible(
                    &body,
                    endpoint,
                    (reasoning != Some(false)).then_some(expected),
                );
            }
        }
    }
}

#[tokio::test]
async fn catalog_reasoning_does_not_generate_effort_without_an_explicit_selection() {
    for endpoint in ["v1/chat/completions", "v1/responses"] {
        for reasoning in [None, Some(true), Some(false)] {
            let mut config = ModelConfig::new(CATALOG_MODEL)
                .with_temperature(Some(0.5))
                .with_max_tokens(Some(1024));
            config.reasoning = reasoning;
            let body = request(config, "ollama_cloud", None, endpoint, false, vec![]).await;
            assert_compatible(&body, endpoint, None);
        }
    }
}

#[tokio::test]
async fn native_openai_retains_reasoning_role_token_and_temperature_behavior() {
    for endpoint in ["chat/completions", "v1/responses"] {
        for reasoning in [None, Some(true)] {
            let body = request(
                model("o3-mini", reasoning, ThinkingEffort::High),
                "openai",
                None,
                endpoint,
                true,
                vec![],
            )
            .await;
            assert!(body.get("temperature").is_none(), "{body}");
            if endpoint.ends_with("responses") {
                assert_eq!(body["reasoning"]["effort"], "high");
            } else {
                assert_eq!(body["messages"][0]["role"], "developer");
                assert_eq!(body["max_completion_tokens"], 1024);
                assert!(body.get("max_tokens").is_none(), "{body}");
                assert_eq!(body["reasoning_effort"], "high");
            }
        }
        for (name, reasoning) in [("gpt-4o", None), ("o3-mini", Some(false))] {
            let body = request(
                model(name, reasoning, ThinkingEffort::High),
                "openai",
                None,
                endpoint,
                true,
                vec![],
            )
            .await;
            assert_compatible(&body, endpoint, None);
        }
    }
}

struct NoKey;

impl KeyResolver for NoKey {
    type Error = Infallible;

    fn resolve_key(&self, _: &str) -> Result<String, Self::Error> {
        panic!("Unauthenticated test provider must not resolve a key")
    }
}

#[tokio::test]
async fn declarative_provider_passes_selected_catalog_id_to_request_generation() {
    for endpoint in ["chat/completions", "v1/responses"] {
        let server = MockServer::start().await;
        let config: DeclarativeProviderConfig = serde_json::from_value(json!({
            "name": "custom_gateway", "engine": "openai", "display_name": "Test Gateway",
            "base_url": server.uri(), "base_path": endpoint, "models": [],
            "requires_auth": false, "supports_streaming": false,
            "catalog_provider_id": "ollama_cloud"
        }))
        .unwrap();
        let provider = from_declarative_config(config, None, NoKey)
            .unwrap()
            .map_api_client(|client| client.with_loopback_http_only().unwrap())
            .build();
        let body = capture_request(
            &server,
            &provider,
            &model(CATALOG_MODEL, Some(true), ThinkingEffort::Medium),
            endpoint,
        )
        .await;
        assert_compatible(&body, endpoint, Some("high"));
    }
}

#[tokio::test]
async fn catalog_identity_does_not_change_provider_temperature_conventions() {
    for endpoint in ["chat/completions", "v1/responses"] {
        let body = request(
            model("grok-4.5", Some(true), ThinkingEffort::Max),
            "custom_gateway",
            Some("xai"),
            endpoint,
            false,
            vec![],
        )
        .await;
        assert_compatible(&body, endpoint, Some("high"));
    }
}

#[tokio::test]
async fn limited_catalog_efforts_never_discard_an_explicit_selection() {
    for endpoint in ["chat/completions", "v1/responses"] {
        for (provider, name, effort, expected) in [
            (
                "ollama_cloud",
                "deepseek-v4-flash",
                ThinkingEffort::Off,
                "high",
            ),
            ("opencode_go", "kimi-k3", ThinkingEffort::Low, "max"),
            ("opencode_go", "kimi-k3", ThinkingEffort::Medium, "max"),
            ("opencode_go", "kimi-k3", ThinkingEffort::High, "max"),
        ] {
            let body = request(
                model(name, Some(true), effort),
                provider,
                None,
                endpoint,
                false,
                vec![],
            )
            .await;
            assert_compatible(&body, endpoint, Some(expected));
        }
    }
}
