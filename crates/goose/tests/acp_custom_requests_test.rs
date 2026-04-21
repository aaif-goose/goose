#[allow(dead_code)]
#[path = "acp_common_tests/mod.rs"]
mod common_tests;

use common_tests::fixtures::server::AcpServerConnection;
use common_tests::fixtures::{
    run_test, send_custom, Connection, PermissionDecision, Session, SessionData,
    TestConnectionConfig,
};
use goose::acp::server::AcpProviderFactory;
use goose::model::ModelConfig;
use goose::providers::base::{MessageStream, Provider};
use goose::providers::errors::ProviderError;
use goose_test_support::{EnforceSessionId, IgnoreSessionId};
use std::sync::{Arc, Mutex};

use common_tests::fixtures::OpenAiFixture;

struct MockProvider {
    name: String,
    model_config: ModelConfig,
    recommended_models: Vec<String>,
}

#[async_trait::async_trait]
impl Provider for MockProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    async fn stream(
        &self,
        _model_config: &ModelConfig,
        _session_id: &str,
        _system: &str,
        _messages: &[goose::conversation::message::Message],
        _tools: &[rmcp::model::Tool],
    ) -> Result<MessageStream, ProviderError> {
        unimplemented!()
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model_config.clone()
    }

    async fn fetch_recommended_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(self.recommended_models.clone())
    }
}

fn mock_provider_factory() -> AcpProviderFactory {
    Arc::new(|provider_name, model_config, _extensions| {
        Box::pin(async move {
            let recommended_models = match provider_name.as_str() {
                "anthropic" => vec![
                    "claude-3-7-sonnet-latest".to_string(),
                    "claude-3-5-haiku-latest".to_string(),
                ],
                _ => vec!["gpt-4o".to_string(), "o4-mini".to_string()],
            };
            Ok(Arc::new(MockProvider {
                name: provider_name,
                model_config,
                recommended_models,
            }) as Arc<dyn Provider>)
        })
    })
}

#[test]
fn test_custom_get_tools() {
    run_test(async {
        let openai = OpenAiFixture::new(vec![], Arc::new(EnforceSessionId::default())).await;
        let mut conn = AcpServerConnection::new(TestConnectionConfig::default(), openai).await;

        let SessionData { session, .. } = conn.new_session().await.unwrap();
        let session_id = session.session_id().0.clone();

        let result = send_custom(
            conn.cx(),
            "_goose/tools",
            serde_json::json!({ "sessionId": session_id }),
        )
        .await;
        assert!(result.is_ok(), "expected ok, got: {:?}", result);

        let response = result.unwrap();
        let tools = response.get("tools").expect("missing 'tools' field");
        assert!(tools.is_array(), "tools should be array");
    });
}

#[test]
fn test_custom_get_extensions() {
    run_test(async {
        let openai = OpenAiFixture::new(vec![], Arc::new(EnforceSessionId::default())).await;
        let conn = AcpServerConnection::new(TestConnectionConfig::default(), openai).await;

        let result =
            send_custom(conn.cx(), "_goose/config/extensions", serde_json::json!({})).await;
        assert!(result.is_ok(), "expected ok, got: {:?}", result);

        let response = result.unwrap();
        assert!(
            response.get("extensions").is_some(),
            "missing 'extensions' field"
        );
        assert!(
            response.get("warnings").is_some(),
            "missing 'warnings' field"
        );
    });
}

#[test]
fn test_custom_provider_inventory_includes_metadata() {
    run_test(async {
        let openai = OpenAiFixture::new(vec![], Arc::new(EnforceSessionId::default())).await;
        let conn = AcpServerConnection::new(TestConnectionConfig::default(), openai).await;

        let response = send_custom(conn.cx(), "_goose/providers/list", serde_json::json!({}))
            .await
            .expect("provider inventory should succeed");
        let providers = response
            .get("entries")
            .and_then(|value| value.as_array())
            .expect("missing entries array");
        let openai = providers
            .iter()
            .find(|provider| provider.get("providerId") == Some(&serde_json::json!("openai")))
            .expect("expected openai inventory entry");

        assert!(openai.get("providerName").is_some(), "missing providerName");
        assert!(openai.get("description").is_some(), "missing description");
        assert!(openai.get("defaultModel").is_some(), "missing defaultModel");
        assert!(openai.get("providerType").is_some(), "missing providerType");
        assert!(openai.get("configKeys").is_some(), "missing configKeys");
        assert!(openai.get("setupSteps").is_some(), "missing setupSteps");
    });
}

#[test]
fn test_custom_config_crud() {
    run_test(async {
        let openai = OpenAiFixture::new(vec![], Arc::new(EnforceSessionId::default())).await;
        let conn = AcpServerConnection::new(TestConnectionConfig::default(), openai).await;

        send_custom(
            conn.cx(),
            "_goose/config/upsert",
            serde_json::json!({
                "key": "GOOSE_PROVIDER",
                "value": "anthropic",
            }),
        )
        .await
        .expect("config upsert should succeed");

        let response = send_custom(
            conn.cx(),
            "_goose/config/read",
            serde_json::json!({
                "key": "GOOSE_PROVIDER",
            }),
        )
        .await
        .expect("config read should succeed");
        assert_eq!(response.get("value"), Some(&serde_json::json!("anthropic")));

        send_custom(
            conn.cx(),
            "_goose/config/remove",
            serde_json::json!({
                "key": "GOOSE_PROVIDER",
            }),
        )
        .await
        .expect("config remove should succeed");

        let response = send_custom(
            conn.cx(),
            "_goose/config/read",
            serde_json::json!({
                "key": "GOOSE_PROVIDER",
            }),
        )
        .await
        .expect("config read after remove should succeed");
        assert_eq!(response.get("value"), Some(&serde_json::Value::Null));
    });
}

#[test]
fn test_provider_switching_updates_session_state() {
    run_test(async {
        let openai = OpenAiFixture::new(vec![], Arc::new(EnforceSessionId::default())).await;
        let config = TestConnectionConfig {
            provider_factory: Some(mock_provider_factory()),
            current_model: "gpt-4o".to_string(),
            ..Default::default()
        };
        let mut conn = AcpServerConnection::new(config, openai).await;

        let SessionData { session, .. } = conn.new_session().await.unwrap();
        let session_id = session.session_id().0.clone();

        conn.set_config_option(&session_id, "provider", "anthropic")
            .await
            .expect("provider switch to anthropic should succeed");

        conn.set_config_option(&session_id, "provider", "openai")
            .await
            .expect("provider switch to openai should succeed");

        conn.set_config_option(&session_id, "provider", "goose")
            .await
            .expect("provider reset to goose should succeed");
    });
}

#[test]
fn test_custom_unknown_method() {
    run_test(async {
        let openai = OpenAiFixture::new(vec![], Arc::new(EnforceSessionId::default())).await;
        let conn = AcpServerConnection::new(TestConnectionConfig::default(), openai).await;

        let result = send_custom(conn.cx(), "_unknown/method", serde_json::json!({})).await;
        assert!(result.is_err(), "expected method_not_found error");
    });
}

#[test]
fn test_developer_fs_requests_use_acp_session_id() {
    run_test(async {
        let seen_session_id = Arc::new(Mutex::new(None::<String>));
        let seen_session_id_clone = Arc::clone(&seen_session_id);
        let openai = OpenAiFixture::new(
            vec![
                (
                    "Use the read tool to read /tmp/test_acp_read.txt and output only its contents."
                        .to_string(),
                    include_str!("acp_test_data/openai_fs_read_tool_call.txt"),
                ),
                (
                    r#""content":"test-read-content-12345""#.into(),
                    include_str!("acp_test_data/openai_fs_read_tool_result.txt"),
                ),
            ],
            Arc::new(IgnoreSessionId),
        )
        .await;
        let config = TestConnectionConfig {
            // gpt-5-nano routes to the Responses API; use a Chat Completions
            // model so the canned SSE fixtures are parsed correctly.
            current_model: "gpt-4.1".to_string(),
            read_text_file: Some(Arc::new(move |req| {
                *seen_session_id_clone.lock().unwrap() = Some(req.session_id.0.to_string());
                Ok(sacp::schema::ReadTextFileResponse::new(
                    "test-read-content-12345",
                ))
            })),
            ..Default::default()
        };
        let mut conn = AcpServerConnection::new(config, openai).await;

        let SessionData { mut session, .. } = conn.new_session().await.unwrap();
        let acp_session_id = session.session_id().0.to_string();

        let output = session
            .prompt(
                "Use the read tool to read /tmp/test_acp_read.txt and output only its contents.",
                PermissionDecision::Cancel,
            )
            .await
            .expect("prompt should succeed");

        assert_eq!(output.text, "test-read-content-12345");
        assert_eq!(
            seen_session_id.lock().unwrap().as_deref(),
            Some(acp_session_id.as_str()),
            "ACP read request should use the ACP session/thread ID",
        );
    });
}
