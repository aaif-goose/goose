use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use goose::agents::{
    AgentEvent, GoosePlatform, SessionConfig, StateMachineServices, StateMachineServicesConfig,
};
use goose::config::permission::PermissionManager;
use goose::config::GooseMode;
use goose::conversation::message::Message;
use goose::hooks::HookManager;
use goose::providers::base::{stream_from_single_message, MessageStream, Provider};
use goose::session::{SessionManager, SessionType};
use oi_kit_providers::conversation::token_usage::{ProviderUsage, Usage};
use oi_kit_providers::errors::ProviderError;
use oi_kit_providers::model::ModelConfig;
use rmcp::model::{Role, Tool};
use test_case::test_case;

const HISTORY: &str = "Earlier user message";
const KICKOFF: &str = "Reply normally";
const REPLY: &str = "A normal provider reply";

struct MessageProvider {
    message: Message,
    calls: AtomicUsize,
}

#[async_trait]
impl Provider for MessageProvider {
    fn get_name(&self) -> &str {
        "message-role-test"
    }

    async fn stream(
        &self,
        _model: &ModelConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) > 2 {
            return Err(ProviderError::ExecutionError(
                "unexpected extra inference".into(),
            ));
        }
        Ok(stream_from_single_message(
            self.message.clone(),
            ProviderUsage::new("test-model".into(), Usage::default()),
        ))
    }
}

async fn assert_provider_reply_role(provider_message: Message, toolshim: bool) -> Result<()> {
    let root = tempfile::tempdir()?;
    let _env = env_lock::lock_env([
        ("GOOSE_PATH_ROOT", Some(root.path().to_str().unwrap())),
        // Exercise toolshim accumulation and its fallback without invoking an interpreter.
        ("GOOSE_TOOLSHIM_BACKEND", Some("unavailable-test-backend")),
    ]);
    let sessions = Arc::new(SessionManager::new(root.path().join("sessions")));
    let session = sessions
        .create_session(
            root.path().to_path_buf(),
            "provider message roles".into(),
            SessionType::Hidden,
            GooseMode::Auto,
        )
        .await?;
    sessions
        .add_message(&session.id, &Message::user().with_text(HISTORY))
        .await?;
    sessions
        .add_message(
            &session.id,
            &Message::assistant().with_text("Earlier reply"),
        )
        .await?;

    let mut config = StateMachineServicesConfig::new(
        sessions.clone(),
        Arc::new(PermissionManager::new(root.path().join("permissions"))),
        None,
        true,
        GoosePlatform::GooseCli,
    );
    config.hooks = Some(HookManager::default());
    let services = StateMachineServices::with_config(config);
    let provider = Arc::new(MessageProvider {
        message: provider_message,
        calls: AtomicUsize::new(0),
    });
    services
        .update_provider(
            provider.clone(),
            ModelConfig::new("test-model")
                .with_context_limit(Some(128_000))
                .with_toolshim(toolshim),
            &session.id,
        )
        .await?;
    let mut stream = services
        .reply(
            Message::user().with_text(KICKOFF),
            SessionConfig {
                id: session.id.clone(),
                schedule_id: None,
                max_turns: Some(2),
            },
            None,
        )
        .await?;
    let emitted = tokio::time::timeout(Duration::from_secs(10), async {
        let mut messages = Vec::new();
        while let Some(event) = stream.next().await {
            if let AgentEvent::Message(message) = event? {
                messages.push(message);
            }
        }
        Ok::<_, anyhow::Error>(messages)
    })
    .await??;
    let persisted = sessions.get_session(&session.id, true).await?;
    let messages = persisted.conversation.as_ref().unwrap().messages();
    for user_text in [HISTORY, KICKOFF] {
        assert!(messages.iter().any(|message| {
            message.as_concat_text() == user_text && message.role == Role::User
        }));
    }
    for messages in [&emitted, messages] {
        let mut replies = messages
            .iter()
            .filter(|message| message.as_concat_text() == REPLY)
            .peekable();
        assert!(replies.peek().is_some(), "provider text must be preserved");
        assert!(replies.all(|message| message.role == Role::Assistant));
    }
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test_case(false; "normal")]
#[test_case(true; "toolshim")]
#[tokio::test]
async fn provider_user_text_is_assistant_visible(toolshim: bool) -> Result<()> {
    assert_provider_reply_role(Message::user().with_text(REPLY), toolshim).await
}

#[test_case(false; "normal")]
#[test_case(true; "toolshim")]
#[tokio::test]
async fn provider_assistant_text_keeps_its_role(toolshim: bool) -> Result<()> {
    assert_provider_reply_role(Message::assistant().with_text(REPLY), toolshim).await
}

#[cfg(feature = "aws-providers")]
#[test_case(false; "normal")]
#[test_case(true; "toolshim")]
#[tokio::test]
async fn bedrock_user_text_is_assistant_visible(toolshim: bool) -> Result<()> {
    use aws_sdk_bedrockruntime::types::{ContentBlock, ConversationRole};
    let wire_message = aws_sdk_bedrockruntime::types::Message::builder()
        .role(ConversationRole::User)
        .content(ContentBlock::Text(REPLY.into()))
        .build()?;
    let message = goose::providers::formats::bedrock::from_bedrock_message(&wire_message)?;
    assert_eq!(message.role, Role::User);
    assert_provider_reply_role(message, toolshim).await
}
