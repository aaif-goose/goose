use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use goose::agents::{Agent, AgentConfig, AgentEvent, GoosePlatform, SessionConfig};
use goose::config::{GooseMode, PermissionManager};
use goose::conversation::message::Message;
use goose::providers::base::{MessageStream, Provider};
use goose::session::{SessionManager, SessionType};
use goose_providers::conversation::token_usage::Usage;
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;
use rmcp::model::Tool;

struct StatusProvider;

#[async_trait]
impl Provider for StatusProvider {
    fn get_name(&self) -> &str {
        "status-test-provider"
    }

    async fn stream(
        &self,
        _model_config: &ModelConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        panic!("/status must not call model inference");
    }

    async fn get_context_limit(&self, _model: &str, _override_limit: Option<usize>) -> usize {
        1_000
    }
}

#[test_case::test_case(false; "legacy_loop")]
#[test_case::test_case(true; "state_machine")]
#[tokio::test]
async fn status_identifies_the_requested_session_for_each_platform(
    use_state_machine: bool,
) -> Result<()> {
    let config_root = tempfile::tempdir()?;
    let _env = env_lock::lock_env([
        ("GOOSE_PATH_ROOT", config_root.path().to_str()),
        ("GOOSE_SLASH_COMMANDS_ENABLED", Some("true")),
        ("GOOSE_CONTEXT_LIMIT", Some("1000")),
    ]);
    for platform in [GoosePlatform::GooseCli, GoosePlatform::GooseDesktop] {
        let temp_dir = tempfile::tempdir()?;
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let agent = Agent::with_config(AgentConfig::new(
            session_manager.clone(),
            Arc::new(PermissionManager::new(temp_dir.path().join("config"))),
            None,
            GooseMode::Auto,
            true,
            platform,
        ));
        let mut session_ids = Vec::new();
        for name in ["first", "second"] {
            let session = session_manager
                .create_session(
                    temp_dir.path().to_path_buf(),
                    name.to_string(),
                    SessionType::Hidden,
                    GooseMode::Auto,
                )
                .await?;
            agent
                .update_provider(
                    Arc::new(StatusProvider),
                    ModelConfig::new(name),
                    &session.id,
                )
                .await?;
            session_manager
                .update(&session.id)
                .usage(Usage::new(Some(100), Some(25), Some(125)))
                .accumulated_usage(Usage::new(Some(400), Some(100), Some(500)))
                .apply()
                .await?;
            session_ids.push(session.id);
        }
        assert_ne!(session_ids[0], session_ids[1]);

        for (index, session_id) in session_ids.iter().enumerate() {
            let reply_stream = agent
                .reply(
                    Message::user().with_text("/status"),
                    SessionConfig {
                        id: session_id.clone(),
                        schedule_id: None,
                        max_turns: Some(1),
                        retry_config: None,
                    },
                    use_state_machine,
                    None,
                )
                .await?;
            tokio::pin!(reply_stream);
            let mut responses = Vec::new();
            while let Some(event) = reply_stream.next().await {
                if let AgentEvent::Message(message) = event? {
                    if message.role == rmcp::model::Role::Assistant {
                        responses.push(message);
                    }
                }
            }
            assert_eq!(responses.len(), 1);
            let response = responses.first().expect("/status should return a response");
            assert!(response.is_user_visible());
            assert!(!response.is_agent_visible());
            assert_eq!(response.role, rmcp::model::Role::Assistant);

            let text = response.as_concat_text();
            assert!(text.contains(&format!("- Session ID: `{session_id}`")));
            assert!(!text.contains(&session_ids[1 - index]));
            let model = ["first", "second"][index];
            assert!(text.contains(&format!("- Model: {model}")));
            assert!(text.contains("- Provider: status-test-provider"));
            assert!(text.contains("- Mode: auto"));
            assert!(text.contains("- Tokens (lifetime): 500"));
            assert!(text.contains("- Context: 125 / 1000 tokens (13%)"));
        }
    }
    Ok(())
}
