//! Compacts conversation history when it is too large for the configured context window.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tracing_futures::Instrument;

use crate::agents::state_machine::ops_llm::{chat_span, record_chat_usage};
use crate::agents::state_machine::{
    applied, last_effective_role, messages_since_kickoff, not_applicable, trailing_error,
    yielded_with, ConversationEffect, Emitter, GooseEffect, Operation, OperationResult,
    SlashCommand,
};
use crate::context_mgmt::{compact_messages, count_context_tokens};
use crate::conversation::message::{Message, MessageErrorKind, SystemNotificationType};
use crate::conversation::{Conversation, EffectiveRole};
use crate::providers::base::Provider;
use crate::session::Session;
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;

const COMPACTION_THINKING_TEXT: &str = "goose is compacting the conversation...";

/// A failed compaction may still have billed completed-but-rejected
/// summarization calls.
fn billed_compaction_usage(
    error: &anyhow::Error,
) -> Vec<goose_providers::conversation::token_usage::ProviderUsage> {
    error
        .downcast_ref::<goose_context_management::CompactionFailure>()
        .map(|failure| failure.billed_usage.clone())
        .unwrap_or_default()
}

pub(super) const MAX_CONTEXT_ERROR_COMPACTIONS: usize = 2;

/// Reported usage stops at the inference that requested the tools, so the
/// results that answered it are not counted until the next request.
async fn unreported_tool_tokens(conversation: &Conversation) -> Result<i32> {
    let messages = conversation.messages();
    if last_effective_role(messages)? != EffectiveRole::Tool {
        return Ok(0);
    }
    let after_request = messages
        .iter()
        .rposition(Message::is_tool_call)
        .map_or(0, |index| index + 1);
    count_context_tokens(&messages[after_request..]).await
}

pub struct CompactionOperation {
    provider: Arc<dyn Provider>,
    model_config: ModelConfig,
    context_limit: usize,
    threshold: f64,
    manages_own_context: bool,
}

impl CompactionOperation {
    pub fn new(
        provider: Arc<dyn Provider>,
        model_config: ModelConfig,
        context_limit: usize,
        threshold: f64,
    ) -> Self {
        let manages_own_context = provider.manages_own_context();
        Self {
            provider,
            model_config,
            context_limit,
            threshold,
            manages_own_context,
        }
    }

    fn over_threshold(&self, tokens: usize) -> bool {
        if self.threshold <= 0.0 || self.threshold >= 1.0 {
            return false;
        }
        (tokens as f64 / self.context_limit as f64) > self.threshold
    }

    async fn context_tokens(&self, session: &Session, conversation: &Conversation) -> Result<i32> {
        match session.usage.total_tokens {
            Some(tokens) => Ok(tokens + unreported_tool_tokens(conversation).await?),
            None => count_context_tokens(conversation.messages()).await,
        }
    }

    fn command_error(
        conversation: &Conversation,
        message: String,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let command = messages_since_kickoff(conversation)?
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("compact command conversation has no kickoff message"))?;
        let message_id = command
            .id
            .clone()
            .ok_or_else(|| anyhow!("Persisted slash command message has no id"))?;
        let command = command.with_visibility(true, false);
        let response = Message::assistant()
            .with_text(message)
            .with_visibility(true, false);
        emit.message(command);
        let response = emit.message(response);
        yielded_with([
            ConversationEffect::SetMessageVisibility {
                message_id,
                user_visible: true,
                agent_visible: false,
            }
            .into(),
            response.into(),
        ])
    }

    fn clear(conversation: &Conversation, emit: &Emitter) -> Result<OperationResult<GooseEffect>> {
        let command = messages_since_kickoff(conversation)?
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("clear command conversation has no kickoff message"))?
            .with_visibility(true, false);
        let response = Message::assistant()
            .with_text("Conversation cleared")
            .with_visibility(true, false);
        let command = emit.message(command);
        let response = emit.message(response);
        yielded_with([
            Conversation::default().into(),
            command.into(),
            response.into(),
        ])
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for CompactionOperation {
    fn name(&self) -> &'static str {
        "compaction"
    }

    async fn run_command(
        &self,
        command: &SlashCommand<'_>,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        match command.command {
            "clear" => return Self::clear(conversation, emit),
            "compact" => {}
            _ => return not_applicable(),
        }

        let span = chat_span(
            self.provider.as_ref(),
            &self.model_config,
            &session.id,
            "compaction",
        );
        let result = match compact_messages(
            self.provider.as_ref(),
            &self.model_config,
            &session.id,
            conversation,
            true,
        )
        .instrument(span.clone())
        .await
        {
            Ok(result) => result,
            Err(error) => {
                span.record("error.type", "compaction_error");
                let mut result = Self::command_error(conversation, error.to_string(), emit);
                if let Ok(OperationResult::Applied(step)) = &mut result {
                    step.effects.extend(
                        billed_compaction_usage(&error)
                            .into_iter()
                            .map(GooseEffect::RecordUsage),
                    );
                }
                return result;
            }
        };
        let compacted = result.conversation;
        let usage = result.usage;
        record_chat_usage(&span, &usage);

        let command = messages_since_kickoff(conversation)?
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("compact command conversation has no kickoff message"))?
            .with_visibility(true, false);
        let response = Message::assistant()
            .with_text("Compaction complete")
            .with_visibility(true, false);
        emit.message(command);
        let response = emit.message(response);
        yielded_with(
            result
                .rejected_usage
                .into_iter()
                .map(GooseEffect::RecordUsage)
                .chain([
                    GooseEffect::CompactConversation {
                        conversation: compacted,
                        usage: Some(usage),
                    },
                    response.into(),
                ]),
        )
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        if self.manages_own_context {
            return not_applicable();
        }

        let messages = messages_since_kickoff(conversation)?;
        let reactive_context_error = matches!(
            trailing_error(conversation),
            Some(MessageErrorKind::ContextLengthExceeded)
        );

        if reactive_context_error {
            let context_errors = messages
                .iter()
                .filter(|message| {
                    message.error_kind() == Some(MessageErrorKind::ContextLengthExceeded)
                        && !message.is_agent_visible()
                })
                .count();
            if context_errors > MAX_CONTEXT_ERROR_COMPACTIONS {
                return not_applicable();
            }
        } else {
            // Compact only ahead of an inference; an assistant tail ends the turn.
            if last_effective_role(messages)? == EffectiveRole::Assistant {
                return not_applicable();
            }
            let tokens = self.context_tokens(session, conversation).await?;
            if tokens <= 0 || !self.over_threshold(tokens as usize) {
                return not_applicable();
            }
        }

        let conversation_with_hidden_error;
        let conversation = if reactive_context_error {
            let mut messages = conversation.messages().to_vec();
            let Some(last) = messages.last_mut() else {
                return not_applicable();
            };
            last.metadata.agent_visible = false;
            conversation_with_hidden_error = Conversation::new_unvalidated(messages);
            &conversation_with_hidden_error
        } else {
            conversation
        };

        let threshold_k_tokens = (self.context_limit as f64 * self.threshold / 1000.0) as usize;
        emit.message(Message::assistant().with_system_notification(
            SystemNotificationType::InlineMessage,
            format!(
                "Exceeded auto-compact threshold of {threshold_k_tokens}k tokens. \
                     Performing auto-compaction..."
            ),
        ));
        emit.message(Message::assistant().with_system_notification(
            SystemNotificationType::ThinkingMessage,
            COMPACTION_THINKING_TEXT,
        ));

        let span = chat_span(
            self.provider.as_ref(),
            &self.model_config,
            &session.id,
            "compaction",
        );
        match compact_messages(
            self.provider.as_ref(),
            &self.model_config,
            &session.id,
            conversation,
            false,
        )
        .instrument(span.clone())
        .await
        {
            Ok(result) => {
                let compacted = result.conversation;
                let usage = result.usage;
                record_chat_usage(&span, &usage);
                emit.message(Message::assistant().with_system_notification(
                    SystemNotificationType::InlineMessage,
                    "Compaction complete",
                ));
                applied(
                    result
                        .rejected_usage
                        .into_iter()
                        .map(GooseEffect::RecordUsage)
                        .chain([GooseEffect::CompactConversation {
                            conversation: compacted,
                            usage: Some(usage),
                        }]),
                )
            }
            Err(e) => {
                span.record("error.type", "compaction_error");
                let message = emit.message(compaction_failure_message(&e));
                yielded_with(
                    std::iter::once(message.into()).chain(
                        billed_compaction_usage(&e)
                            .into_iter()
                            .map(GooseEffect::RecordUsage),
                    ),
                )
            }
        }
    }
}

fn compaction_failure_message(e: &anyhow::Error) -> Message {
    let provider_error = e.downcast_ref::<ProviderError>().or_else(|| {
        e.downcast_ref::<goose_context_management::CompactionFailure>()
            .and_then(|failure| failure.error.downcast_ref::<ProviderError>())
    });
    if let Some(err @ ProviderError::CreditsExhausted { .. }) = provider_error {
        return Message::from_provider_error(err);
    }
    Message::assistant().with_error(
        provider_error.map_or(MessageErrorKind::Other, MessageErrorKind::from),
        format!(
            "Ran into this error trying to compact: {e}.\n\n\
             Please try again or create a new session"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credits_failure_keeps_the_top_up_url() {
        let error = anyhow::Error::new(goose_context_management::CompactionFailure {
            error: ProviderError::CreditsExhausted {
                details: "no credits".to_string(),
                top_up_url: Some("https://example.com/billing".to_string()),
            }
            .into(),
            billed_usage: Vec::new(),
        });
        let message = compaction_failure_message(&error);
        let Some(crate::conversation::message::MessageContent::SystemNotification(notification)) =
            message.content.first()
        else {
            panic!("expected a credits notification, got {message:?}");
        };
        assert_eq!(
            notification.notification_type,
            SystemNotificationType::CreditsExhausted
        );
        assert_eq!(
            notification.data.as_ref().unwrap()["top_up_url"],
            "https://example.com/billing"
        );
    }
}
