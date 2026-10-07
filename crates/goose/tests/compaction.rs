use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use goose::agents::{Agent, AgentEvent, SessionConfig};
use goose::config::GooseMode;
use goose::conversation::message::{Message, MessageContent};
use goose::conversation::Conversation;
use goose::providers::base::{
    stream_from_single_message, MessageStream, Provider, ProviderDef, ProviderMetadata,
};
use goose::session::session_manager::SessionType;
use goose::session::Session;
use goose_providers::conversation::token_usage::{ProviderUsage, Usage};
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;
use rmcp::model::Tool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

struct MockCompactionProvider {
    /// Tracks whether compaction has occurred (for context limit recovery case)
    has_compacted: Arc<AtomicBool>,
    manages_own_context: bool,
}

impl MockCompactionProvider {
    fn new() -> Self {
        Self {
            has_compacted: Arc::new(AtomicBool::new(false)),
            manages_own_context: false,
        }
    }

    /// Calculate input tokens based on system prompt and messages
    /// Simulates realistic token counts for different scenarios
    fn calculate_input_tokens(&self, system_prompt: &str, messages: &[Message]) -> i32 {
        // Check if this is a compaction call
        let is_compaction_call = messages.len() == 1
            && messages[0].content.iter().any(|c| {
                if let MessageContent::Text(text) = c {
                    text.text.to_lowercase().contains("summarize")
                } else {
                    false
                }
            });

        if is_compaction_call {
            // For compaction: system prompt length is a good proxy for conversation size
            // Base: 6000 (system) + conversation content embedded in prompt
            6000 + (system_prompt.len() as i32 / 4).max(400)
        } else {
            // Regular call: system prompt + messages
            let system_tokens = if system_prompt.is_empty() { 0 } else { 6000 };

            let message_tokens: i32 = messages
                .iter()
                .map(|msg| {
                    let mut tokens = 100;
                    for content in &msg.content {
                        if let MessageContent::Text(text) = content {
                            if text.text.contains("long_tool_call") {
                                tokens += 15000;
                            }
                        }
                    }
                    tokens
                })
                .sum();

            system_tokens + message_tokens
        }
    }

    /// Calculate output tokens based on response type
    fn calculate_output_tokens(&self, is_compaction: bool, messages: &[Message]) -> i32 {
        if is_compaction {
            // Compaction produces a compact summary
            200
        } else {
            // Regular responses vary by content
            let has_hello = messages.iter().any(|msg| {
                msg.content.iter().any(|c| {
                    if let MessageContent::Text(text) = c {
                        text.text.to_lowercase().contains("hello")
                    } else {
                        false
                    }
                })
            });

            if has_hello {
                50 // Simple greeting response
            } else {
                100 // Default response
            }
        }
    }
}

#[async_trait]
impl Provider for MockCompactionProvider {
    fn manages_own_context(&self) -> bool {
        self.manages_own_context
    }

    async fn stream(
        &self,
        _model_config: &ModelConfig,
        system_prompt: &str,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        // Check if this is a compaction call (message contains "summarize")
        let is_compaction = messages.iter().any(|msg| {
            msg.content.iter().any(|content| {
                if let MessageContent::Text(text) = content {
                    text.text.to_lowercase().contains("summarize")
                } else {
                    false
                }
            })
        });

        // Calculate realistic token counts based on actual content
        let input_tokens = self.calculate_input_tokens(system_prompt, messages);
        let output_tokens = self.calculate_output_tokens(is_compaction, messages);

        // Simulate context limit: if input > 20k tokens and we haven't compacted yet, fail
        const CONTEXT_LIMIT: i32 = 20000;
        if !is_compaction
            && input_tokens > CONTEXT_LIMIT
            && !self.has_compacted.load(Ordering::SeqCst)
        {
            return Err(ProviderError::ContextLengthExceeded(format!(
                "Context limit exceeded: {} > {}",
                input_tokens, CONTEXT_LIMIT
            )));
        }

        // If this is a compaction call, mark that we've compacted
        if is_compaction {
            self.has_compacted.store(true, Ordering::SeqCst);
        }

        // Generate response
        let message = if is_compaction {
            Message::assistant().with_text("<mock summary of conversation>")
        } else {
            let response_text = if messages.iter().any(|msg| {
                msg.content.iter().any(|c| {
                    if let MessageContent::Text(text) = c {
                        text.text.to_lowercase().contains("hello")
                    } else {
                        false
                    }
                })
            }) {
                "Hi there! How can I help you?"
            } else {
                "This is a mock response."
            };
            Message::assistant().with_text(response_text)
        };

        let usage = ProviderUsage::new(
            "mock-model".to_string(),
            Usage::new(
                Some(input_tokens),
                Some(output_tokens),
                Some(input_tokens + output_tokens),
            ),
        );

        Ok(stream_from_single_message(message, usage))
    }

    fn get_name(&self) -> &str {
        "mock-compaction"
    }
}

impl goose::providers::base::ProviderDescriptor for MockCompactionProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            name: "mock".to_string(),
            display_name: "Mock Compaction Provider".to_string(),
            description: "Mock provider for compaction testing".to_string(),
            default_model: "mock-model".to_string(),
            known_models: vec![],
            model_doc_link: "".to_string(),
            config_keys: vec![],
            setup_steps: vec![],
            setup: None,
            deprecated: None,
        }
    }
}

impl ProviderDef for MockCompactionProvider {
    type Provider = Self;

    fn from_env(
        _extensions: Vec<goose::config::ExtensionConfig>,
        _tls_config: Option<goose::providers::api_client::TlsConfig>,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<Self>> {
        Box::pin(async { Ok(Self::new()) })
    }
}

/// Helper: Set up a test session with initial messages and token counts
async fn setup_test_session(
    agent: &Agent,
    temp_dir: &TempDir,
    session_name: &str,
    messages: Vec<Message>,
) -> Result<Session> {
    let session = agent
        .config
        .session_manager
        .create_session(
            temp_dir.path().to_path_buf(),
            session_name.to_string(),
            SessionType::Hidden,
            GooseMode::default(),
        )
        .await?;

    let conversation = Conversation::new_unvalidated(messages);
    agent
        .config
        .session_manager
        .replace_conversation(&session.id, &conversation)
        .await?;

    // Set initial token counts
    agent
        .config
        .session_manager
        .update(&session.id)
        .usage(Usage::new(Some(600), Some(400), Some(1000)))
        .accumulated_usage(Usage::new(Some(600), Some(400), Some(1000)))
        .apply()
        .await?;

    Ok(session)
}

#[tokio::test]
async fn test_auto_compaction_during_reply() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let agent = Agent::new();

    // Setup session with many messages to have substantial context
    // 20 exchanges = 40 messages * 100 tokens = ~4000 tokens in conversation
    let mut messages = vec![];
    for i in 0..20 {
        messages.push(Message::user().with_text(format!("User message {}", i)));
        messages.push(Message::assistant().with_text(format!("Assistant response {}", i)));
    }

    let session = setup_test_session(&agent, &temp_dir, "auto-compact-test", messages).await?;

    // Capture initial context size before triggering reply
    // Should be: system (6000) + 40 messages (4000) = ~10000 tokens
    let initial_session = agent
        .config
        .session_manager
        .get_session(&session.id, true)
        .await?;
    let initial_input_tokens = initial_session.usage.input_tokens.unwrap_or(0);

    // Setup mock provider (no context limit enforcement)
    let provider = Arc::new(MockCompactionProvider::new());
    agent
        .update_provider(provider, ModelConfig::new("mock-model"), &session.id)
        .await?;

    // Trigger a reply
    // Expected tokens for reply:
    // - Input: system (6000) + 40 messages (4000) + new user message (100) = 10100 tokens
    // - Output: regular response (100 tokens)
    let user_message = Message::user().with_text("Tell me more about compaction");

    let session_config = SessionConfig {
        id: session.id.clone(),
        schedule_id: None,
        max_turns: None,
        session_start_emitted: false,
    };

    let reply_stream = agent.reply(user_message, session_config, None).await?;
    tokio::pin!(reply_stream);

    // Track compaction and context size changes
    let mut compaction_occurred = false;
    let mut input_tokens_after_compaction: Option<i32> = None;

    while let Some(event_result) = reply_stream.next().await {
        match event_result {
            Ok(AgentEvent::HistoryReplaced(_)) => {
                compaction_occurred = true;

                // Capture the input tokens immediately after compaction
                let session_after_compact = agent
                    .config
                    .session_manager
                    .get_session(&session.id, true)
                    .await?;
                input_tokens_after_compaction = session_after_compact.usage.input_tokens;
            }
            Ok(_) => {}
            Err(e) => return Err(e),
        }
    }

    let updated_session = agent
        .config
        .session_manager
        .get_session(&session.id, true)
        .await?;

    if compaction_occurred {
        // Verify that current input context decreased after compaction
        let tokens_after =
            input_tokens_after_compaction.expect("Should have captured tokens after compaction");

        // Before compaction: system (6000) + 40 messages (4000) = 10,000 tokens
        // After compaction: only the summary (200 tokens) - this becomes the new input
        assert!(
            tokens_after < initial_input_tokens,
            "Input tokens should decrease after compaction. Before: {}, After: {}",
            initial_input_tokens,
            tokens_after
        );

        // After compaction, input should be exactly the summary: 200 tokens
        assert_eq!(
            tokens_after, 200,
            "Input tokens after compaction should be exactly 200 (summary). Got: {}",
            tokens_after
        );

        // After the subsequent reply, the current window includes:
        // - system (6000) + summary (200) + new user message (100) + reply (100) = 6400
        let final_input = updated_session.usage.input_tokens.unwrap();
        let final_output = updated_session.usage.output_tokens.unwrap();
        let final_total = updated_session.usage.total_tokens.unwrap();

        assert!(
            final_input >= 6000,
            "Final input should include at least system prompt (6000). Got: {}",
            final_input
        );
        assert_eq!(
            final_output, 100,
            "Final output should be 100 tokens (default response). Got: {}",
            final_output
        );
        assert_eq!(
            final_total,
            final_input + final_output,
            "Final total should equal input + output"
        );

        // Accumulated tokens should include:
        // - Initial: 1000
        // - Compaction: ~10,400 input + 200 output = 10,600
        // - Reply: ~6,300 input + 100 output = 6,400
        // Total: 1000 + 10,600 + 6,400 = 18,000
        let accumulated = updated_session.accumulated_usage.total_tokens.unwrap();
        assert!(
            (17000..=19000).contains(&accumulated),
            "Accumulated should be ~18,000 (initial + compaction + reply). Got: {}",
            accumulated
        );
    } else {
        // If no compaction, accumulated should include reply cost
        // - Initial: 1000
        // - Reply: system (6000) + 40 messages (4000) + new message (100) = 10,100 input
        // - Reply output: 100
        // Total: 1000 + 10,100 + 100 = 11,200
        let accumulated = updated_session.accumulated_usage.total_tokens.unwrap();
        assert!(
            (11000..=11500).contains(&accumulated),
            "Accumulated should be ~11,200 (initial + reply). Got: {}",
            accumulated
        );

        // Current window should be: 10,100 input + 100 output = 10,200
        let final_input = updated_session.usage.input_tokens.unwrap();
        let final_output = updated_session.usage.output_tokens.unwrap();

        assert!(
            (10000..=10500).contains(&final_input),
            "Input should be ~10,100. Got: {}",
            final_input
        );
        assert_eq!(
            final_output, 100,
            "Output should be 100. Got: {}",
            final_output
        );
    }

    Ok(())
}
