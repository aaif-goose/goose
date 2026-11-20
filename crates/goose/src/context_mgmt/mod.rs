pub use oi_kit_context_management::structured;

use std::sync::Arc;

use crate::conversation::message::{Message, MessageContent};
use crate::conversation::Conversation;
use crate::providers::base::Provider;
use crate::{config::Config, token_counter::create_token_counter};
use anyhow::Result;
use oi_kit_agent::compaction::{
    CONVERSATION_CONTINUATION_TEXT, MANUAL_COMPACT_CONTINUATION_TEXT, TOOL_LOOP_CONTINUATION_TEXT,
};
use oi_kit_providers::conversation::token_usage::ProviderUsage;
use oi_kit_providers::errors::ProviderError;
use oi_kit_providers::model::ModelConfig;
use tracing::log::warn;
use tracing::Instrument;

pub use oi_kit_context_management::DEFAULT_COMPACTION_THRESHOLD;

/// Starts compaction with headroom below OpenAI's long-context price tier
/// (above 272k input tokens). This is not a maximum request size.
pub const DEFAULT_AUTO_COMPACT_TOKEN_LIMIT: usize = 225_000;

/// The configured threshold, lowered so compaction starts by the token limit.
pub fn auto_compact_threshold(context_limit: usize) -> f64 {
    let config = Config::global();
    capped_compaction_threshold(
        config
            .get_param::<f64>("GOOSE_AUTO_COMPACT_THRESHOLD")
            .unwrap_or(DEFAULT_COMPACTION_THRESHOLD),
        config
            .get_param::<usize>("GOOSE_AUTO_COMPACT_TOKEN_LIMIT")
            .unwrap_or(DEFAULT_AUTO_COMPACT_TOKEN_LIMIT),
        context_limit,
    )
}

/// `threshold` of the context, but never more than `token_limit` tokens.
pub fn capped_compaction_threshold(
    threshold: f64,
    token_limit: usize,
    context_limit: usize,
) -> f64 {
    if threshold <= 0.0 || threshold >= 1.0 || context_limit == 0 {
        return threshold;
    }
    threshold.min(token_limit as f64 / context_limit as f64)
}

pub(crate) fn tool_pair_summarization_enabled() -> bool {
    Config::global()
        .get_param::<bool>("GOOSE_TOOL_PAIR_SUMMARIZATION")
        .unwrap_or(false)
}

/// Summaries persisted before the `compaction_summary` flag existed are known
/// by the continuation text compaction merges into them. Conversation repair
/// may have concatenated it onto the summary text, so match the tail.
pub fn is_compaction_summary(message: &Message) -> bool {
    message.is_compaction_summary()
        || (message.is_agent_visible()
            && !message.metadata.user_visible
            && message.content.iter().any(|content| match content {
                MessageContent::Text(text) => [
                    CONVERSATION_CONTINUATION_TEXT,
                    TOOL_LOOP_CONTINUATION_TEXT,
                    MANUAL_COMPACT_CONTINUATION_TEXT,
                ]
                .iter()
                .any(|continuation| text.text.ends_with(continuation)),
                _ => false,
            }))
}

/// Estimate the tokens of the agent-visible messages, counted the same way
/// as the fallback estimation in `check_if_compaction_needed`.
pub(crate) async fn count_context_tokens(messages: &[Message]) -> Result<i64> {
    let counter = create_token_counter()
        .await
        .map_err(|error| anyhow::anyhow!("Failed to create token counter: {error}"))?;
    let total: usize = messages
        .iter()
        .filter(|message| message.is_agent_visible())
        .map(|message| counter.count_message_tokens(message))
        .sum();
    Ok(total.try_into()?)
}

/// Check if messages exceed the auto-compaction threshold
pub async fn check_if_compaction_needed(
    provider: &dyn Provider,
    conversation: &Conversation,
    threshold_override: Option<f64>,
    session: &crate::session::Session,
) -> Result<bool> {
    if provider.manages_own_context() {
        return Ok(false);
    }

    let messages = conversation.messages();

    let model_config = session
        .model_config
        .clone()
        .unwrap_or_else(|| ModelConfig::new("unknown"));
    let context_limit =
        crate::context_limit::get_context_limit(provider, &model_config.model_name).await?;
    let threshold = threshold_override.unwrap_or_else(|| auto_compact_threshold(context_limit));

    let (current_tokens, _token_source) = match session.usage.total_tokens {
        Some(tokens) => (tokens as usize, "session metadata"),
        None => {
            let token_counter = create_token_counter()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create token counter: {}", e))?;

            let estimated: usize = messages
                .iter()
                .filter(|message| message.is_agent_visible())
                .map(|message| token_counter.count_message_tokens(message))
                .sum();

            (estimated, "estimated")
        }
    };

    let usage_ratio = current_tokens as f64 / context_limit as f64;

    let needs_compaction = if threshold <= 0.0 || threshold >= 1.0 {
        false // Auto-compact is disabled.
    } else {
        usage_ratio > threshold
    };
    Ok(needs_compaction)
}

pub(crate) struct GooseCompactionModel {
    provider: Arc<dyn Provider>,
    model_config: ModelConfig,
    operation_name: &'static str,
}

impl GooseCompactionModel {
    pub(crate) fn new(provider: Arc<dyn Provider>, model_config: ModelConfig) -> Self {
        Self {
            provider,
            model_config,
            operation_name: "compaction",
        }
    }

    pub(crate) fn with_operation_name(mut self, operation_name: &'static str) -> Self {
        self.operation_name = operation_name;
        self
    }
}

#[async_trait::async_trait]
impl oi_kit_context_management::CompactionModel for GooseCompactionModel {
    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        let session_id = crate::session_context::current_session_id().unwrap_or_default();
        let span = oi_kit_agent::inference::chat_span(
            self.provider.as_ref(),
            &self.model_config,
            &session_id,
            self.operation_name,
        );
        let result = crate::model_config::complete_one_shot(
            self.provider.as_ref(),
            &self.model_config,
            &session_id,
            system,
            messages,
            &[],
        )
        .instrument(span.clone())
        .await;
        match &result {
            Ok((_, usage)) => oi_kit_agent::inference::record_chat_usage(&span, usage),
            Err(error) => {
                span.record("error.type", error.telemetry_type());
            }
        }
        result
    }
}

pub(crate) struct GooseTokenEstimator;

#[async_trait::async_trait]
impl oi_kit_context_management::TokenEstimator for GooseTokenEstimator {
    async fn count_chat_tokens(&self, system: &str, messages: &[Message]) -> usize {
        match create_token_counter().await {
            Ok(counter) => counter.count_chat_tokens(system, messages, &[]),
            Err(error) => {
                warn!("Failed to create token counter: {error}");
                0
            }
        }
    }

    async fn count_text_tokens(&self, text: &str) -> usize {
        match create_token_counter().await {
            Ok(counter) => counter.count_tokens(text),
            Err(error) => {
                warn!("Failed to create token counter: {error}");
                0
            }
        }
    }
}

pub(crate) fn compaction_templates() -> Result<oi_kit_context_management::Templates> {
    Ok(oi_kit_context_management::Templates {
        compaction: crate::prompt_template::template_source("compaction.md")?,
        summary: crate::prompt_template::template_source("compaction_summary.md")?,
        request: crate::prompt_template::template_source("compaction_request.md")?,
    })
}

pub use oi_kit_context_management::format_message_for_compacting;

pub fn compute_tool_call_cutoff(context_limit: usize, compaction_threshold: f64) -> usize {
    let threshold = if compaction_threshold > 0.0 && compaction_threshold <= 1.0 {
        compaction_threshold
    } else {
        DEFAULT_COMPACTION_THRESHOLD
    };
    let effective_limit = (context_limit as f64 * threshold) as usize;
    (3 * effective_limit / 20_000).clamp(10, 500)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::MessageMetadata;

    #[test]
    fn summaries_written_before_the_flag_are_still_recognized() {
        let legacy = Message::assistant()
            .with_text("# Conversation Summary\nWe were adding a test.")
            .with_text(CONVERSATION_CONTINUATION_TEXT)
            .with_metadata(MessageMetadata::agent_only());
        let ordinary = Message::assistant()
            .with_text("Continuing.")
            .with_metadata(MessageMetadata::agent_only());
        let quoted = Message::assistant().with_text(CONVERSATION_CONTINUATION_TEXT);

        assert!(is_compaction_summary(&legacy));
        assert!(!is_compaction_summary(&ordinary));
        assert!(!is_compaction_summary(&quoted));

        // The report repairs the conversation first, which concatenates the
        // summary's text blocks.
        let (repaired, _) =
            crate::conversation::fix_conversation(Conversation::new_unvalidated(vec![
                Message::user().with_text("hi"),
                legacy,
                Message::user().with_text("next"),
            ]));
        assert_eq!(repaired.messages()[1].content.len(), 1);
        assert!(is_compaction_summary(&repaired.messages()[1]));
    }

    #[test]
    fn test_compute_tool_call_cutoff_scales_with_context() {
        // Default threshold (0.8)
        assert_eq!(compute_tool_call_cutoff(128_000, 0.8), 15); // 102K effective
        assert_eq!(compute_tool_call_cutoff(200_000, 0.8), 24); // 160K effective
        assert_eq!(compute_tool_call_cutoff(1_000_000, 0.8), 120); // 800K effective
                                                                   // Clamp at minimum
        assert_eq!(compute_tool_call_cutoff(50_000, 0.8), 10);
        assert_eq!(compute_tool_call_cutoff(10_000, 0.8), 10);
        // Clamp at maximum (500)
        assert_eq!(compute_tool_call_cutoff(10_000_000, 0.8), 500);
        // Lower compaction threshold means earlier summarization
        assert_eq!(compute_tool_call_cutoff(200_000, 0.3), 10); // 60K effective
        assert_eq!(compute_tool_call_cutoff(1_000_000, 0.5), 75); // 500K effective
                                                                  // Invalid threshold falls back to default 0.8
        assert_eq!(compute_tool_call_cutoff(200_000, 0.0), 24); // falls back to 0.8
        assert_eq!(compute_tool_call_cutoff(200_000, -1.0), 24); // falls back to 0.8
    }
}
