use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use goose::agents::{Agent, SessionConfig};
use goose::config::{Config, GooseMode};
use goose::context_mgmt::auto_compact_threshold;
use goose::conversation::message::Message;
use goose::conversation::Conversation;
use goose::providers::base::{stream_from_single_message, MessageStream, Provider};
use goose::session::SessionType;
use goose_providers::conversation::token_usage::{ProviderUsage, Usage};
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;
use rmcp::model::Tool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Default)]
struct CompactionProvider {
    compacted: AtomicBool,
}

#[async_trait]
impl Provider for CompactionProvider {
    fn get_name(&self) -> &str {
        "compaction-test"
    }

    async fn get_context_limit(&self, model: &str, override_limit: Option<usize>) -> usize {
        override_limit.unwrap_or_else(|| model.parse().unwrap())
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        _system: &str,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let is_compaction = messages.iter().any(|message| {
            message
                .as_concat_text()
                .to_lowercase()
                .contains("summarize")
        });
        if is_compaction {
            self.compacted.store(true, Ordering::SeqCst);
        }
        Ok(stream_from_single_message(
            Message::assistant().with_text(if is_compaction {
                "summary"
            } else {
                "continued"
            }),
            ProviderUsage::new(
                model_config.model_name.clone(),
                Usage::new(Some(100), Some(10), Some(110)),
            ),
        ))
    }
}

#[tokio::test]
async fn effective_trigger_and_agent_execution_follow_the_current_model() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let root = temp_dir.path().to_str().unwrap();
    let _env = env_lock::lock_env([
        ("GOOSE_PATH_ROOT", Some(root)),
        ("GOOSE_DISABLE_KEYRING", Some("1")),
        ("GOOSE_AUTO_COMPACT_THRESHOLD", None),
        ("GOOSE_AUTO_COMPACT_TOKEN_LIMIT", None),
        ("GOOSE_CONTEXT_LIMIT", None),
        ("GOOSE_TOOL_PAIR_SUMMARIZATION", Some("false")),
        ("GOOSE_DISABLE_SESSION_NAMING", Some("true")),
    ]);
    let config = Config::global();
    assert_eq!(auto_compact_threshold(1_000_000), 0.225);
    assert_eq!(auto_compact_threshold(200_000), 0.8);
    assert_eq!(auto_compact_threshold(0), 0.8);

    let agent = Agent::new();
    let session = agent
        .config
        .session_manager
        .create_session(
            temp_dir.path().to_path_buf(),
            "compaction threshold".to_string(),
            SessionType::Hidden,
            GooseMode::default(),
        )
        .await?;

    for (percentage, cap, context, trigger) in [
        (0.8_f64, 225_000, 1_000_000, 225_000),
        (0.8, 225_000, 200_000, 160_000),
        (0.8, 225_000, 1_000_000, 225_000),
        (0.2, 225_000, 1_000_000, 200_000),
        (0.8, 900_000, 1_000_000, 800_000),
    ] {
        config.set_param("GOOSE_AUTO_COMPACT_THRESHOLD", percentage)?;
        config.set_param("GOOSE_AUTO_COMPACT_TOKEN_LIMIT", cap)?;
        assert_eq!(
            auto_compact_threshold(context),
            percentage.min(cap as f64 / context as f64)
        );
        for (tokens, expected) in [(trigger - 1, false), (trigger, false), (trigger + 1, true)] {
            verify_reply(&agent, &session.id, context, tokens, expected).await?;
        }
    }

    for disabled in [0.0, -0.1, 1.0] {
        config.set_param("GOOSE_AUTO_COMPACT_THRESHOLD", disabled)?;
        config.set_param("GOOSE_AUTO_COMPACT_TOKEN_LIMIT", 225_000)?;
        for context in [1_000_000, 200_000] {
            assert_eq!(auto_compact_threshold(context), disabled);
            verify_reply(&agent, &session.id, context, 900_000, false).await?;
        }
    }
    Ok(())
}

async fn verify_reply(
    agent: &Agent,
    session_id: &str,
    context: usize,
    tokens: i32,
    expected_compaction: bool,
) -> Result<()> {
    let provider = Arc::new(CompactionProvider::default());
    agent
        .update_provider(
            provider.clone(),
            ModelConfig::new(context.to_string()),
            session_id,
        )
        .await?;
    agent
        .config
        .session_manager
        .replace_conversation(
            session_id,
            &Conversation::new_unvalidated([
                Message::user().with_text("previous work"),
                Message::assistant().with_text("previous response"),
            ]),
        )
        .await?;
    agent
        .config
        .session_manager
        .update(session_id)
        .usage(Usage::new(Some(tokens), Some(0), Some(tokens)))
        .apply()
        .await?;
    let stream = agent
        .reply(
            Message::user().with_text("continue"),
            SessionConfig {
                id: session_id.to_string(),
                schedule_id: None,
                max_turns: Some(2),
            },
            None,
        )
        .await?;
    tokio::pin!(stream);
    while let Some(event) = stream.next().await {
        event?;
    }
    assert_eq!(
        provider.compacted.load(Ordering::SeqCst),
        expected_compaction,
        "context={context}, tokens={tokens}"
    );
    Ok(())
}
