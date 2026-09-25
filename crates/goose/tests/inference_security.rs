use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use goose::agents::{Agent, AgentConfig, AgentEvent, GoosePlatform, SessionConfig};
use goose::config::{permission::PermissionManager, GooseMode};
use goose::session::{SessionManager, SessionType};
use goose_providers::base::{stream_from_single_message, MessageStream, Provider};
use goose_providers::conversation::message::{InferenceSecurity, Message};
use goose_providers::conversation::token_usage::{ProviderUsage, Usage};
use goose_providers::{errors::ProviderError, model::ModelConfig};
use rmcp::model::{CallToolRequestParams, Tool};

struct VerifiedThenOrdinary(AtomicUsize);

#[async_trait]
impl Provider for VerifiedThenOrdinary {
    fn get_name(&self) -> &str {
        "mock-confidential"
    }

    async fn stream(
        &self,
        _: &ModelConfig,
        _: &str,
        _: &[Message],
        _: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
            let mut text = Message::assistant()
                .with_generated_id()
                .with_text("verified response");
            text.metadata.inference_security = Some(InferenceSecurity::AttestedTee);
            let mut tool = Message::assistant()
                .with_id(text.id.as_ref().unwrap())
                .with_tool_request(
                    "call",
                    Ok(CallToolRequestParams::new("unadvertised_test_tool")),
                );
            tool.metadata.inference_security = Some(InferenceSecurity::AttestedTee);
            return Ok(Box::pin(futures::stream::iter([
                Ok((Some(text), None)),
                Ok((
                    Some(tool),
                    Some(ProviderUsage::new("test-model".into(), Usage::default())),
                )),
            ])));
        }
        Ok(stream_from_single_message(
            Message::assistant().with_text("ordinary response"),
            ProviderUsage::new("test-model".into(), Usage::default()),
        ))
    }
}

#[tokio::test]
async fn inference_security_survives_both_agent_loops_and_persistence() -> Result<()> {
    for state_machine in [false, true] {
        let temp = tempfile::tempdir()?;
        let sessions = Arc::new(SessionManager::new(temp.path().to_path_buf()));
        let agent = Agent::with_config(AgentConfig::new(
            sessions.clone(),
            PermissionManager::instance(),
            None,
            GooseMode::Auto,
            true,
            GoosePlatform::GooseCli,
        ));
        let session = sessions
            .create_session(
                temp.path().to_path_buf(),
                "attestation-test".into(),
                SessionType::Hidden,
                GooseMode::Auto,
            )
            .await?;
        agent
            .update_provider(
                Arc::new(VerifiedThenOrdinary(AtomicUsize::new(0))),
                ModelConfig::new("test-model").with_context_limit(Some(100_000)),
                &session.id,
            )
            .await?;
        let mut stream = agent
            .reply(
                Message::user().with_text("Use your tool, then reply"),
                SessionConfig {
                    id: session.id.clone(),
                    schedule_id: None,
                    max_turns: Some(3),
                    retry_config: None,
                },
                state_machine,
                None,
            )
            .await?;
        let mut live = Vec::new();
        while let Some(event) = stream.next().await {
            if let AgentEvent::Message(message) = event? {
                live.push(message);
            }
        }
        let saved = sessions
            .get_session(&session.id, true)
            .await?
            .conversation
            .unwrap();
        for messages in [&live, saved.messages()] {
            let verified = messages
                .iter()
                .find(|m| m.as_concat_text().contains("verified response"))
                .unwrap_or_else(|| {
                    panic!(
                        "missing verified response: state_machine={state_machine}, {messages:#?}"
                    )
                });
            assert_eq!(
                verified.metadata.inference_security,
                Some(InferenceSecurity::AttestedTee),
                "state_machine={state_machine}"
            );
            let tool = messages.iter().find(|m| m.is_tool_call()).unwrap();
            assert_eq!(
                tool.metadata.inference_security,
                Some(InferenceSecurity::AttestedTee),
                "state_machine={state_machine}"
            );
            let ordinary = messages
                .iter()
                .find(|m| m.as_concat_text() == "ordinary response")
                .unwrap();
            assert_eq!(ordinary.metadata.inference_security, None);
            for message in messages
                .iter()
                .filter(|m| m.role == rmcp::model::Role::User)
            {
                assert_eq!(message.metadata.inference_security, None);
            }
        }
    }
    Ok(())
}
