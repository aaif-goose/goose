#[allow(dead_code)]
mod common_tests;

use common_tests::fixtures::server::AcpServerConnection;
use common_tests::fixtures::{
    run_test, send_custom, Connection, Session, SessionData, TestConnectionConfig,
};
use goose::model::ModelConfig;
use goose::providers::base::{MessageStream, Provider};
use goose::providers::errors::ProviderError;
use goose_acp::server::AcpProviderFactory;
use goose_test_support::EnforceSessionId;
use std::sync::Arc;

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
fn test_custom_session_get() {
    run_test(async {
        let openai = OpenAiFixture::new(vec![], Arc::new(EnforceSessionId::default())).await;
        let mut conn = AcpServerConnection::new(TestConnectionConfig::default(), openai).await;

        let SessionData { session, .. } = conn.new_session().await.unwrap();
        let session_id = session.session_id().0.clone();

        let result = send_custom(
            conn.cx(),
            "session/get",
            serde_json::json!({
                "sessionId": session_id,
            }),
        )
        .await;
        assert!(result.is_ok(), "expected ok, got: {:?}", result);

        let response = result.unwrap();
        let returned_session = response.get("session").expect("missing 'session' field");
        assert_eq!(
            returned_session.get("id").and_then(|v| v.as_str()),
            Some(session_id.as_ref())
        );
    });
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
fn test_custom_list_providers() {
    run_test(async {
        let openai = OpenAiFixture::new(vec![], Arc::new(EnforceSessionId::default())).await;
        let conn = AcpServerConnection::new(TestConnectionConfig::default(), openai).await;

        let response = send_custom(conn.cx(), "_goose/providers/list", serde_json::json!({}))
            .await
            .expect("provider list should succeed");
        let providers = response
            .get("providers")
            .and_then(|value| value.as_array())
            .expect("missing providers array");

        assert!(
            providers.iter().any(|provider| {
                provider.get("id") == Some(&serde_json::json!("goose"))
                    && provider.get("label") == Some(&serde_json::json!("Goose (Default)"))
            }),
            "expected Goose default provider sentinel"
        );
        assert!(
            providers
                .iter()
                .any(|provider| provider.get("id") == Some(&serde_json::json!("openai"))),
            "expected at least one concrete provider from the goose registry"
        );
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
            .expect("provider config option should succeed");

        let response = send_custom(
            conn.cx(),
            "session/get",
            serde_json::json!({
                "sessionId": session_id,
            }),
        )
        .await
        .expect("session/get should succeed");
        let session_value = response.get("session").expect("missing session");
        assert_eq!(
            session_value.get("provider_name"),
            Some(&serde_json::json!("anthropic"))
        );
        assert_eq!(
            session_value
                .get("model_config")
                .and_then(|value| value.get("model_name")),
            Some(&serde_json::json!("current"))
        );

        let response = send_custom(
            conn.cx(),
            "_goose/session/provider/update",
            serde_json::json!({
                "sessionId": session_id,
                "provider": "openai",
                "model": "o4-mini",
            }),
        )
        .await
        .expect("provider update should succeed");
        let config_options = response
            .get("configOptions")
            .and_then(|value| value.as_array())
            .expect("missing config options");
        assert!(
            !config_options.is_empty(),
            "expected refreshed config options"
        );

        let response = send_custom(
            conn.cx(),
            "session/get",
            serde_json::json!({
                "sessionId": session_id,
            }),
        )
        .await
        .expect("session/get after provider update should succeed");
        let session_value = response.get("session").expect("missing session");
        assert_eq!(
            session_value.get("provider_name"),
            Some(&serde_json::json!("openai"))
        );
        assert_eq!(
            session_value
                .get("model_config")
                .and_then(|value| value.get("model_name")),
            Some(&serde_json::json!("o4-mini"))
        );

        let response = send_custom(
            conn.cx(),
            "_goose/session/provider/update",
            serde_json::json!({
                "sessionId": session_id,
                "provider": "goose",
            }),
        )
        .await
        .expect("provider reset to goose should succeed");
        let config_options = response
            .get("configOptions")
            .and_then(|value| value.as_array())
            .expect("missing config options after reset");
        assert!(
            config_options
                .iter()
                .any(|option| option.get("id") == Some(&serde_json::json!("provider"))),
            "missing provider config option after reset"
        );

        let response = send_custom(
            conn.cx(),
            "session/get",
            serde_json::json!({
                "sessionId": session_id,
            }),
        )
        .await
        .expect("session/get after provider reset should succeed");
        let session_value = response.get("session").expect("missing session");
        assert_eq!(
            session_value.get("provider_name"),
            Some(&serde_json::json!("goose"))
        );
        assert_eq!(
            session_value.get("model_config"),
            Some(&serde_json::Value::Null)
        );
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
