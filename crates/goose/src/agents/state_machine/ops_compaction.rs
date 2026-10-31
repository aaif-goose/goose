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
use crate::context_mgmt::{
    compact_messages, compact_messages_from_summary, compaction_instruction, count_context_tokens,
    prepare_compaction_summary,
};
use crate::conversation::message::{Message, MessageErrorKind, SystemNotificationType};
use crate::conversation::{Conversation, EffectiveRole};
use crate::providers::base::Provider;
use crate::session::Session;
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;

const COMPACTION_THINKING_TEXT: &str = "goose is compacting the conversation...";

pub(super) const MAX_CONTEXT_ERROR_COMPACTIONS: usize = 2;
pub(super) const CACHE_FRIENDLY_COMPACTION_MIN_HEADROOM: usize = 5_000;
const COMPACTION_REQUEST: &str = "request";

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

pub(crate) fn has_pending_compaction(messages: &[Message]) -> bool {
    let latest_user = messages
        .iter()
        .rposition(|message| message.role == rmcp::model::Role::User && message.is_user_visible());
    messages.iter().enumerate().rev().any(|(index, message)| {
        message.is_agent_visible()
            && message
                .metadata
                .operation_note("compaction", COMPACTION_REQUEST)
                .is_some()
            && latest_user.map_or(true, |latest_user| index > latest_user)
    })
}

struct PendingCompaction {
    instruction_index: usize,
    manual: bool,
}

pub struct CompactionOperation {
    provider: Arc<dyn Provider>,
    model_config: ModelConfig,
    context_limit: usize,
    threshold: f64,
    manages_own_context: bool,
    cache_friendly: bool,
}

impl CompactionOperation {
    pub fn new(
        provider: Arc<dyn Provider>,
        model_config: ModelConfig,
        context_limit: usize,
        threshold: f64,
        cache_friendly: bool,
    ) -> Self {
        let manages_own_context = provider.manages_own_context();
        Self {
            provider,
            model_config,
            context_limit,
            threshold,
            manages_own_context,
            cache_friendly,
        }
    }

    fn pending_compaction(&self, conversation: &Conversation) -> Option<PendingCompaction> {
        if !has_pending_compaction(conversation.messages()) {
            return None;
        }
        conversation
            .messages()
            .iter()
            .enumerate()
            .rev()
            .find_map(|(instruction_index, message)| {
                let request = self
                    .message_meta(message, COMPACTION_REQUEST)?
                    .as_object()?;
                Some(PendingCompaction {
                    instruction_index,
                    manual: request.get("manual")?.as_bool()?,
                })
            })
    }

    async fn should_append(&self, session: &Session, conversation: &Conversation) -> Result<bool> {
        if !self.cache_friendly {
            return Ok(false);
        }

        let used_tokens = self.context_tokens(session, conversation).await?.max(0) as usize;
        if self.context_limit.saturating_sub(used_tokens) < CACHE_FRIENDLY_COMPACTION_MIN_HEADROOM {
            return Ok(false);
        }
        Ok(last_effective_role(messages_since_kickoff(conversation)?)? == EffectiveRole::User)
    }

    fn start_append_compaction(
        &self,
        conversation: &Conversation,
        manual: bool,
    ) -> Result<OperationResult<GooseEffect>> {
        let trigger = messages_since_kickoff(conversation)?
            .first()
            .ok_or_else(|| anyhow!("compaction conversation has no kickoff message"))?;
        let trigger_id = trigger
            .id
            .clone()
            .ok_or_else(|| anyhow!("Persisted compaction message has no id"))?;
        let mut instruction = Message::user()
            .with_text(compaction_instruction()?)
            .with_visibility(false, true);
        self.set_message_meta(
            &mut instruction,
            COMPACTION_REQUEST,
            serde_json::json!({ "manual": manual }),
        );
        instruction.metadata.set_operation_note(
            "inference",
            "hide_response",
            serde_json::Value::Bool(true),
        );
        applied([
            ConversationEffect::SetMessageVisibility {
                message_id: trigger_id,
                user_visible: true,
                agent_visible: false,
            }
            .into(),
            instruction.into(),
        ])
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

    fn pending_conversation(
        &self,
        conversation: &Conversation,
        pending: &PendingCompaction,
    ) -> Result<Conversation> {
        let mut messages = conversation.messages().to_vec();
        let trigger_id = messages_since_kickoff(conversation)?
            .first()
            .and_then(|message| message.id.as_deref())
            .ok_or_else(|| anyhow!("compaction trigger message is missing"))?;
        let trigger = messages
            .iter_mut()
            .find(|message| message.id.as_deref() == Some(trigger_id))
            .ok_or_else(|| anyhow!("compaction trigger message is missing"))?;
        trigger.metadata.agent_visible = !pending.manual;
        for message in &mut messages[pending.instruction_index..] {
            message.metadata.user_visible = false;
            message.metadata.agent_visible = false;
        }
        Ok(Conversation::new_unvalidated(messages))
    }

    fn pending_response(
        &self,
        conversation: &Conversation,
        pending: &PendingCompaction,
    ) -> Option<Message> {
        let responses = conversation.messages()[pending.instruction_index + 1..]
            .iter()
            .filter(|message| {
                message.is_agent_visible() && message.role == rmcp::model::Role::Assistant
            })
            .collect::<Vec<_>>();
        let last = responses.last()?;
        let mut response = Message::assistant();
        response.metadata = last.metadata.clone();
        response.id = None;
        response.content = responses
            .into_iter()
            .flat_map(|message| message.content.iter().cloned())
            .collect();
        Some(response)
    }

    fn pending_error(
        &self,
        conversation: &Conversation,
        pending: &PendingCompaction,
        mut response: Message,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let trigger_id = messages_since_kickoff(conversation)?
            .first()
            .and_then(|message| message.id.clone())
            .ok_or_else(|| anyhow!("compaction trigger message is missing"))?;
        let mut effects = vec![ConversationEffect::SetMessageVisibility {
            message_id: trigger_id,
            user_visible: true,
            agent_visible: !pending.manual,
        }
        .into()];
        effects.extend(
            conversation.messages()[pending.instruction_index..]
                .iter()
                .filter_map(|message| message.id.clone())
                .map(|message_id| {
                    ConversationEffect::SetMessageVisibility {
                        message_id,
                        user_visible: false,
                        agent_visible: false,
                    }
                    .into()
                }),
        );
        response.id = None;
        response.metadata.user_visible = true;
        response.metadata.agent_visible = false;
        effects.push(emit.message(response).into());
        yielded_with(effects)
    }

    async fn finish_pending_compaction(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let Some(pending) = self.pending_compaction(conversation) else {
            return not_applicable();
        };
        let Some(response) = self.pending_response(conversation, &pending) else {
            return not_applicable();
        };

        let context_error = response.error_kind() == Some(MessageErrorKind::ContextLengthExceeded);
        if response.error_kind().is_some() && !context_error {
            return self.pending_error(conversation, &pending, response, emit);
        }

        let pending_conversation = self.pending_conversation(conversation, &pending)?;
        let summary = if context_error {
            None
        } else {
            prepare_compaction_summary(response).ok()
        };
        let (compacted, usage) = if let Some(summary) = summary {
            (
                compact_messages_from_summary(&pending_conversation, pending.manual, summary)
                    .await?,
                None,
            )
        } else {
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
                &pending_conversation,
                pending.manual,
            )
            .instrument(span.clone())
            .await
            {
                Ok(result) => result,
                Err(error) => {
                    span.record("error.type", "compaction_error");
                    return self.pending_error(
                        conversation,
                        &pending,
                        compaction_failure_message(&error),
                        emit,
                    );
                }
            };
            record_chat_usage(&span, &result.usage);
            (result.conversation, Some(result.usage))
        };

        emit.message(Message::assistant().with_system_notification(
            SystemNotificationType::InlineMessage,
            "Compaction complete",
        ));
        let effect = GooseEffect::CompactConversation {
            conversation: compacted,
            usage,
        };
        if !pending.manual {
            return applied([effect]);
        }
        let response = emit.message(
            Message::assistant()
                .with_text("Compaction complete")
                .with_visibility(true, false),
        );
        yielded_with([effect, response.into()])
    }

    fn command_error(
        conversation: &Conversation,
        error: &anyhow::Error,
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
        let response = compaction_failure_message(error).with_visibility(true, false);
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

pub struct CompactionResponseOperation {
    compaction: Arc<CompactionOperation>,
}

impl CompactionResponseOperation {
    pub fn new(compaction: Arc<CompactionOperation>) -> Self {
        Self { compaction }
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for CompactionResponseOperation {
    fn name(&self) -> &'static str {
        "compaction_response"
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        self.compaction
            .finish_pending_compaction(session, conversation, emit)
            .await
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

        if self.should_append(session, conversation).await? {
            return self.start_append_compaction(conversation, true);
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
                return Self::command_error(conversation, &error, emit);
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
        yielded_with([
            GooseEffect::CompactConversation {
                conversation: compacted,
                usage: Some(usage),
            },
            response.into(),
        ])
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
        if self.pending_compaction(conversation).is_some() {
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

        if !reactive_context_error && self.should_append(session, conversation).await? {
            return self.start_append_compaction(conversation, false);
        }

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
                applied([GooseEffect::CompactConversation {
                    conversation: compacted,
                    usage: Some(usage),
                }])
            }
            Err(e) => {
                span.record("error.type", "compaction_error");
                let message = emit.message(compaction_failure_message(&e));
                yielded_with([message.into()])
            }
        }
    }
}

fn compaction_failure_message(e: &anyhow::Error) -> Message {
    let provider_error = e.downcast_ref::<ProviderError>();
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
        let error = anyhow::Error::from(ProviderError::CreditsExhausted {
            details: "no credits".to_string(),
            top_up_url: Some("https://example.com/billing".to_string()),
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
