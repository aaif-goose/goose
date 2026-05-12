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
    run_test(async move {
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
    run_test(async move {
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
fn test_custom_list_builtin_skill_sources() {
    run_test(async move {
        let openai = OpenAiFixture::new(vec![], Arc::new(EnforceSessionId::default())).await;
        let conn = AcpServerConnection::new(TestConnectionConfig::default(), openai).await;

        let response = send_custom(
            conn.cx(),
            "_goose/sources/list",
            serde_json::json!({ "type": "builtinSkill" }),
        )
        .await
        .expect("builtin skill sources list should succeed");
        let sources = response
            .get("sources")
            .and_then(|value| value.as_array())
            .expect("missing sources array");
        let builtin = sources
            .iter()
            .find(|source| source.get("name") == Some(&serde_json::json!("goose-doc-guide")))
            .expect("expected goose-doc-guide builtin skill");

        assert_eq!(
            builtin.get("type"),
            Some(&serde_json::json!("builtinSkill"))
        );
        assert_eq!(builtin.get("global"), Some(&serde_json::json!(true)));
        assert_eq!(
            builtin.get("path"),
            Some(&serde_json::json!("builtin://skills/goose-doc-guide"))
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
fn test_custom_preferences_read_save_remove() {
    run_test(async {
        let data_root = tempfile::tempdir().unwrap();
        std::fs::write(
            data_root
                .path()
                .join(goose::config::base::CONFIG_YAML_NAME),
            "GOOSE_MODEL: gpt-4o\nGOOSE_PROVIDER: openai\nGOOSE_AUTO_COMPACT_THRESHOLD: 0.7\nVOICE_AUTO_SUBMIT_PHRASES: send it\n",
        )
        .unwrap();
        let openai = OpenAiFixture::new(vec![], Arc::new(EnforceSessionId::default())).await;
        let config = TestConnectionConfig {
            data_root: data_root.path().to_path_buf(),
            ..Default::default()
        };
        let conn = AcpServerConnection::new(config, openai).await;

        let response = send_custom(
            conn.cx(),
            "_goose/preferences/read",
            serde_json::json!({
                "keys": [
                    "autoCompactThreshold",
                    "voiceAutoSubmitPhrases",
                    "voiceDictationPreferredMic"
                ],
            }),
        )
        .await
        .expect("preferences read should succeed");
        assert_eq!(
            response.get("values"),
            Some(&serde_json::json!([
                { "key": "autoCompactThreshold", "value": 0.7 },
                { "key": "voiceAutoSubmitPhrases", "value": "send it" },
                { "key": "voiceDictationPreferredMic", "value": null },
            ]))
        );

        send_custom(
            conn.cx(),
            "_goose/preferences/save",
            serde_json::json!({
                "values": [
                    { "key": "voiceDictationProvider", "value": "__disabled__" },
                    { "key": "voiceDictationPreferredMic", "value": "mic-1" }
                ],
            }),
        )
        .await
        .expect("preferences save should succeed");

        send_custom(
            conn.cx(),
            "_goose/preferences/remove",
            serde_json::json!({
                "keys": ["voiceDictationProvider"],
            }),
        )
        .await
        .expect("preferences remove should succeed");

        let response = send_custom(
            conn.cx(),
            "_goose/preferences/read",
            serde_json::json!({
                "keys": ["voiceDictationProvider", "voiceDictationPreferredMic"],
            }),
        )
        .await
        .expect("preferences read after remove should succeed");
        assert_eq!(
            response.get("values"),
            Some(&serde_json::json!([
                { "key": "voiceDictationProvider", "value": null },
                { "key": "voiceDictationPreferredMic", "value": "mic-1" },
            ]))
        );
    });
}

#[test]
fn test_custom_preferences_save_rejects_invalid_values() {
    run_test(async {
        let openai = OpenAiFixture::new(vec![], Arc::new(EnforceSessionId::default())).await;
        let conn = AcpServerConnection::new(TestConnectionConfig::default(), openai).await;

        let invalid_payloads = [
            serde_json::json!({
                "values": [{ "key": "autoCompactThreshold", "value": 0 }],
            }),
            serde_json::json!({
                "values": [{ "key": "autoCompactThreshold", "value": 1.1 }],
            }),
            serde_json::json!({
                "values": [{ "key": "voiceAutoSubmitPhrases", "value": ["send"] }],
            }),
            serde_json::json!({
                "values": [{ "key": "voiceDictationProvider", "value": "bogus" }],
            }),
            serde_json::json!({
                "values": [{ "key": "voiceDictationPreferredMic", "value": "" }],
            }),
        ];

        for payload in invalid_payloads {
            let result = send_custom(conn.cx(), "_goose/preferences/save", payload).await;
            assert!(result.is_err(), "expected invalid params error");
        }

        let result = send_custom(
            conn.cx(),
            "_goose/preferences/save",
            serde_json::json!({
                "values": [
                    { "key": "voiceDictationPreferredMic", "value": "mic-1" },
                    { "key": "voiceDictationProvider", "value": "bogus" }
                ],
            }),
        )
        .await;
        assert!(result.is_err(), "expected invalid params error");

        let response = send_custom(
            conn.cx(),
            "_goose/preferences/read",
            serde_json::json!({
                "keys": ["voiceDictationPreferredMic"],
            }),
        )
        .await
        .expect("preferences read should succeed");
        assert_eq!(
            response.get("values"),
            Some(&serde_json::json!([
                { "key": "voiceDictationPreferredMic", "value": null },
            ]))
        );
    });
}

#[test]
fn test_custom_defaults_read() {
    run_test(async {
        let data_root = tempfile::tempdir().unwrap();
        std::fs::write(
            data_root.path().join(goose::config::base::CONFIG_YAML_NAME),
            "GOOSE_MODEL: claude-3-5-haiku-latest\nGOOSE_PROVIDER: anthropic\n",
        )
        .unwrap();
        let openai = OpenAiFixture::new(vec![], Arc::new(EnforceSessionId::default())).await;
        let config = TestConnectionConfig {
            data_root: data_root.path().to_path_buf(),
            ..Default::default()
        };
        let conn = AcpServerConnection::new(config, openai).await;

        let response = send_custom(conn.cx(), "_goose/defaults/read", serde_json::json!({}))
            .await
            .expect("defaults read should succeed");
        assert_eq!(
            response,
            serde_json::json!({
                "providerId": "anthropic",
                "modelId": "claude-3-5-haiku-latest",
            })
        );
    });
}

#[test]
fn test_custom_dictation_secret_save_delete() {
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().to_string_lossy().to_string();
    let _env = env_lock::lock_env([
        ("GOOSE_PATH_ROOT", Some(root_path.as_str())),
        ("GOOSE_DISABLE_KEYRING", Some("1")),
        ("GROQ_API_KEY", None::<&str>),
    ]);
    let config_dir = goose::config::paths::Paths::config_dir();
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join(goose::config::base::CONFIG_YAML_NAME),
        "GOOSE_MODEL: gpt-4o\nGOOSE_PROVIDER: openai\nGOOSE_DISABLE_KEYRING: true\n",
    )
    .unwrap();

    run_test(async move {
        let openai = OpenAiFixture::new(vec![], Arc::new(EnforceSessionId::default())).await;
        let config = TestConnectionConfig {
            data_root: config_dir.clone(),
            ..Default::default()
        };
        let conn = AcpServerConnection::new(config, openai).await;

        send_custom(
            conn.cx(),
            "_goose/dictation/secret/save",
            serde_json::json!({
                "provider": "groq",
                "value": "groq-key",
            }),
        )
        .await
        .expect("dictation secret save should succeed");

        let config = send_custom(conn.cx(), "_goose/dictation/config", serde_json::json!({}))
            .await
            .expect("dictation config should succeed");
        assert_eq!(
            config
                .pointer("/providers/groq/configured")
                .and_then(|value| value.as_bool()),
            Some(true)
        );

        let provider_config_result = send_custom(
            conn.cx(),
            "_goose/dictation/secret/save",
            serde_json::json!({
                "provider": "openai",
                "value": "openai-key",
            }),
        )
        .await;
        assert!(
            provider_config_result.is_err(),
            "provider-config dictation providers should be rejected"
        );

        let unknown_result = send_custom(
            conn.cx(),
            "_goose/dictation/secret/save",
            serde_json::json!({
                "provider": "unknown",
                "value": "key",
            }),
        )
        .await;
        assert!(
            unknown_result.is_err(),
            "unknown provider should be rejected"
        );

        send_custom(
            conn.cx(),
            "_goose/dictation/secret/delete",
            serde_json::json!({
                "provider": "groq",
            }),
        )
        .await
        .expect("dictation secret delete should succeed");

        let config = send_custom(conn.cx(), "_goose/dictation/config", serde_json::json!({}))
            .await
            .expect("dictation config should succeed");
        assert_eq!(
            config
                .pointer("/providers/groq/configured")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
    });
}

#[test]
fn test_raw_config_and_secret_methods_are_removed() {
    run_test(async {
        let openai = OpenAiFixture::new(vec![], Arc::new(EnforceSessionId::default())).await;
        let conn = AcpServerConnection::new(TestConnectionConfig::default(), openai).await;

        for method in [
            "_goose/config/read",
            "_goose/config/upsert",
            "_goose/config/remove",
            "_goose/secret/check",
            "_goose/secret/upsert",
            "_goose/secret/remove",
        ] {
            let result = send_custom(conn.cx(), method, serde_json::json!({})).await;
            assert!(result.is_err(), "{method} should be removed");
        }
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
                Ok(agent_client_protocol::schema::ReadTextFileResponse::new(
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
