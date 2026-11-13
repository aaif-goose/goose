//! Compacts the conversation when it outgrows the context window.
//!
//! The in-band shape asks the model for a summary through the normal inference
//! step: the operation appends an agent-only request, the next inference writes
//! the summary on the live prefix, and the operation then harvests the reply
//! into the compacted layout. The standalone shape summarizes with a separate
//! request; it recovers from context-length errors, where the live prefix no
//! longer fits, and backs up the in-band shape when the reply is unusable.
//!
//! Place the operation before everything that reacts to an assistant reply,
//! so the summary is harvested before tool execution or turn-end logic sees it.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use goose_context_management::{
    compacted_conversation, latest_prompt, summarize, summary_message, CompactionModel, Templates,
    TokenEstimator,
};
use goose_provider_types::conversation::message::{
    Message, MessageContent, MessageErrorKind, MessageMetadata, SystemNotificationType,
};
use goose_provider_types::conversation::{Conversation, EffectiveRole};
use goose_provider_types::errors::ProviderError;
use rmcp::model::Role;

use crate::command::Command;
use crate::inference::InferenceEffect;
use crate::machine::MachineSession;
use crate::operation::{
    applied, last_effective_role, messages_since_kickoff, not_applicable, trailing_error,
    yielded_with, ConversationEffect, Emitter, Operation, OperationResult,
};

const NAME: &str = "compaction";
const REQUEST_NOTE: &str = "request";
const MANUAL_NOTE: &str = "manual";
const PROMPT_NOTE: &str = "prompt";
const COMPACTION_THINKING_TEXT: &str = "Compacting the conversation...";
const COMPACTION_COMPLETE_TEXT: &str = "Compaction complete";

/// Context-length errors recovered from within one turn before giving up.
pub const MAX_CONTEXT_ERROR_COMPACTIONS: usize = 2;

/// Effects compaction produces beyond those of inference.
pub trait CompactionEffect: InferenceEffect + From<ConversationEffect> {
    /// Replaces the conversation with its compacted layout. Hosts that store
    /// messages individually can override this to update them in place.
    fn compact(conversation: Conversation) -> Self {
        Self::from(ConversationEffect::ReplaceConversation(conversation))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionMode {
    /// Summarize through the next inference on the live conversation.
    InBand,
    /// Summarize with a separate request outside the conversation.
    Standalone,
}

pub struct CompactionOperation {
    model: Arc<dyn CompactionModel>,
    estimator: Arc<dyn TokenEstimator>,
    templates: Templates,
    context_limit: usize,
    threshold: f64,
    mode: CompactionMode,
    compact_command: bool,
}

fn is_compaction_request(message: &Message) -> bool {
    message
        .metadata
        .operation_note(NAME, REQUEST_NOTE)
        .is_some()
}

/// Whether the conversation ends with an in-band request the next inference
/// will answer, so its usage belongs to compaction.
pub fn awaits_compaction_reply(conversation: &Conversation) -> bool {
    conversation
        .messages()
        .iter()
        .rev()
        .find(|message| message.is_agent_visible())
        .is_some_and(is_compaction_request)
}

async fn agent_visible_tokens(estimator: &dyn TokenEstimator, messages: &[Message]) -> i64 {
    let visible = Conversation::new_unvalidated(messages.to_vec()).agent_visible_messages();
    estimator.count_chat_tokens("", &visible).await as i64
}

/// Reported usage stops at the inference that requested the tools, so the
/// results that answered it are not counted until the next request.
async fn unreported_tool_tokens(
    conversation: &Conversation,
    estimator: &dyn TokenEstimator,
) -> Result<i64> {
    let messages = conversation.messages();
    if last_effective_role(messages)? != EffectiveRole::Tool {
        return Ok(0);
    }
    let after_request = messages
        .iter()
        .rposition(Message::is_tool_call)
        .map_or(0, |index| index + 1);
    Ok(agent_visible_tokens(estimator, &messages[after_request..]).await)
}

/// The context size when proactive compaction should run before the next
/// inference, or `None` when it should not.
pub async fn proactive_compaction_tokens<S: MachineSession + ?Sized>(
    session: &S,
    conversation: &Conversation,
    estimator: &dyn TokenEstimator,
    context_limit: usize,
    threshold: f64,
) -> Result<Option<i64>> {
    if threshold <= 0.0 || threshold >= 1.0 {
        return Ok(None);
    }
    // An assistant tail ends the turn, so no inference follows.
    if last_effective_role(messages_since_kickoff(conversation)?)? == EffectiveRole::Assistant {
        return Ok(None);
    }
    let tokens = match session.reported_context_tokens() {
        Some(reported) => {
            reported.saturating_add(unreported_tool_tokens(conversation, estimator).await?)
        }
        None => agent_visible_tokens(estimator, conversation.messages()).await,
    };
    Ok((tokens > 0 && tokens as f64 / context_limit as f64 > threshold).then_some(tokens))
}

/// A summary that was never followed by a model reply cannot have grown, so
/// compacting again would only summarize the summary.
fn replied_since_last_summary(conversation: &Conversation) -> bool {
    let messages = conversation.messages();
    let Some(summary) = messages
        .iter()
        .rposition(|message| message.is_compaction_summary() && message.is_agent_visible())
    else {
        return true;
    };
    messages[summary + 1..]
        .iter()
        .any(|message| message.role == Role::Assistant && message.metadata.inference.is_some())
}

/// A prompt with nothing before it has no history to summarize; if it does not
/// fit, the context-length recovery handles it.
fn has_history_before_prompt(messages: &[Message]) -> bool {
    latest_prompt(messages).is_none_or(|prompt| {
        messages[..prompt]
            .iter()
            .any(|message| message.is_agent_visible() && !message.is_turn_context())
    })
}

/// The in-band request and its replies leave both audiences; errors and turn
/// context in the tail keep their own visibility.
fn is_archived(message: &Message) -> bool {
    !message.is_turn_context() && message.error_kind().is_none()
}

fn message_id(message: &Message) -> Result<String> {
    message
        .id
        .clone()
        .ok_or_else(|| anyhow!("persisted message has no id"))
}

fn hide(message: &Message) -> Result<ConversationEffect> {
    Ok(ConversationEffect::SetMessageVisibility {
        message_id: message_id(message)?,
        user_visible: false,
        agent_visible: false,
    })
}

fn agent_visibility(message: &Message, agent_visible: bool) -> Result<ConversationEffect> {
    Ok(ConversationEffect::SetMessageVisibility {
        message_id: message_id(message)?,
        user_visible: message.is_user_visible(),
        agent_visible,
    })
}

/// The prompt an in-band request hid while the model summarized: it sits right
/// before the request, since both leave the operation in one step.
fn hidden_prompt(messages: &[Message], request_index: usize) -> Option<usize> {
    request_index.checked_sub(1).filter(|&index| {
        let message = &messages[index];
        message.metadata.operation_note(NAME, PROMPT_NOTE).is_some() && !message.is_agent_visible()
    })
}

/// Tool calls in a compaction reply never run; closing them keeps tool
/// execution from picking them up.
fn unanswered_tool_calls(tail: &[Message]) -> Option<Message> {
    let mut responses = Message::user();
    for request in tail
        .iter()
        .flat_map(|message| message.content.iter())
        .filter_map(MessageContent::as_tool_request)
    {
        responses.add_tool_response_with_metadata(
            request.id.clone(),
            Ok(rmcp::model::CallToolResult::error(vec![
                rmcp::model::ContentBlock::text("Tool calls are not executed while compacting"),
            ])),
            request.metadata.as_ref(),
        );
    }
    (!responses.content.is_empty()).then(|| responses.with_visibility(false, false))
}

fn compaction_failure_message(error: &anyhow::Error) -> Message {
    let provider_error = error.downcast_ref::<ProviderError>();
    if let Some(err @ ProviderError::CreditsExhausted { .. }) = provider_error {
        return Message::from_provider_error(err);
    }
    Message::assistant().with_error(
        provider_error.map_or(MessageErrorKind::Other, MessageErrorKind::from),
        format!(
            "Ran into this error trying to compact: {error}.\n\n\
             Please try again or create a new session"
        ),
    )
}

fn complete_message() -> Message {
    Message::assistant()
        .with_text(COMPACTION_COMPLETE_TEXT)
        .with_visibility(true, false)
}

impl CompactionOperation {
    /// `estimator` sizes the context when the provider reports no usage and
    /// fills in missing usage of standalone summarization.
    pub fn new(
        model: Arc<dyn CompactionModel>,
        estimator: Arc<dyn TokenEstimator>,
        context_limit: usize,
        threshold: f64,
    ) -> Self {
        Self {
            model,
            estimator,
            templates: Templates::default(),
            context_limit,
            threshold,
            mode: CompactionMode::InBand,
            compact_command: false,
        }
    }

    pub fn with_templates(mut self, templates: Templates) -> Self {
        self.templates = templates;
        self
    }

    pub fn with_mode(mut self, mode: CompactionMode) -> Self {
        self.mode = mode;
        self
    }

    /// Handles `/compact` (a compaction the user asked for).
    pub fn with_compact_command(mut self) -> Self {
        self.compact_command = true;
        self
    }

    fn request_message(&self, manual: bool) -> Message {
        let mut request = Message::user()
            .with_text(&self.templates.request)
            .with_metadata(MessageMetadata::agent_only());
        request.metadata.hide_response = true;
        request
            .metadata
            .set_operation_note(NAME, REQUEST_NOTE, serde_json::Value::Bool(true));
        request
            .metadata
            .set_operation_note(NAME, MANUAL_NOTE, serde_json::Value::Bool(manual));
        request
    }

    /// Appends the in-band request. A prompt the model has not answered yet
    /// leaves its context first, so the model summarizes instead of answering
    /// it; the compacted layout re-issues it.
    fn request_effects<E: CompactionEffect>(
        &self,
        messages: &[Message],
        manual: bool,
    ) -> Result<Vec<E>> {
        let mut effects = Vec::new();
        if !manual && last_effective_role(messages)? == EffectiveRole::User {
            if let Some(prompt) = latest_prompt(messages).map(|index| &messages[index]) {
                effects.push(E::from(agent_visibility(prompt, false)?));
                effects.push(E::from(ConversationEffect::SetMessageOperationNote {
                    message_id: message_id(prompt)?,
                    operation: NAME.to_string(),
                    key: PROMPT_NOTE.to_string(),
                    value: serde_json::Value::Bool(true),
                }));
            }
        }
        effects.push(E::from(self.request_message(manual)));
        Ok(effects)
    }

    /// Undoes an in-band request that produced no summary: the request and its
    /// replies leave both audiences, and the prompt it hid returns to the agent.
    fn abandon<E: CompactionEffect>(messages: &[Message], request_index: usize) -> Result<Vec<E>> {
        let tail = &messages[request_index..];
        let mut effects = tail
            .iter()
            .filter(|message| is_archived(message))
            .map(|message| hide(message).map(E::from))
            .collect::<Result<Vec<_>>>()?;
        effects.extend(unanswered_tool_calls(tail).map(E::from));
        if let Some(prompt) = hidden_prompt(messages, request_index) {
            effects.push(E::from(agent_visibility(&messages[prompt], true)?));
        }
        Ok(effects)
    }

    fn announce(&self, emit: &Emitter) {
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
    }

    /// Summarizes `messages[..end]` with a standalone request. `request_index`
    /// marks an in-band request whose tail is archived; `before` leads the
    /// effects whether or not the summary succeeds.
    #[allow(clippy::too_many_arguments)]
    async fn standalone<E: CompactionEffect>(
        &self,
        messages: &[Message],
        end: usize,
        request_index: Option<usize>,
        prompt: Option<usize>,
        manual: bool,
        before: Vec<E>,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let summarizable = Conversation::new_unvalidated(
            messages[..end]
                .iter()
                .filter(|message| !message.is_turn_context())
                .cloned(),
        )
        .agent_visible_messages();
        let mut effects = before;
        match summarize(
            self.model.as_ref(),
            Some(self.estimator.as_ref()),
            &self.templates,
            &summarizable,
        )
        .await
        {
            Ok(summary) => {
                effects.push(E::record_usage(summary.usage));
                self.complete(
                    messages,
                    end,
                    request_index,
                    summary.message,
                    prompt,
                    manual,
                    effects,
                    emit,
                )
            }
            Err(error) => {
                if let Some(request_index) = request_index {
                    effects.extend(Self::abandon(messages, request_index)?);
                }
                effects.push(E::from(emit.message(compaction_failure_message(&error))));
                yielded_with(effects)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn complete<E: CompactionEffect>(
        &self,
        messages: &[Message],
        end: usize,
        request_index: Option<usize>,
        summary: Message,
        prompt: Option<usize>,
        manual: bool,
        mut effects: Vec<E>,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let mut compacted = compacted_conversation(messages, end, summary, prompt, manual);
        if let Some(request_index) = request_index {
            let tail = &messages[request_index..];
            for (offset, _) in tail
                .iter()
                .enumerate()
                .filter(|(_, message)| is_archived(message))
            {
                compacted.messages_mut()[request_index + offset]
                    .metadata
                    .user_visible = false;
            }
            compacted
                .messages_mut()
                .splice(messages.len()..messages.len(), unanswered_tool_calls(tail));
        }
        effects.push(E::compact(compacted));
        if manual {
            effects.push(E::from(emit.message(complete_message())));
            return yielded_with(effects);
        }
        emit.message(Message::assistant().with_system_notification(
            SystemNotificationType::InlineMessage,
            COMPACTION_COMPLETE_TEXT,
        ));
        applied(effects)
    }

    /// Turns the reply to an in-band request into the compacted conversation,
    /// or falls back to a standalone summary when the reply is unusable.
    async fn harvest<E: CompactionEffect>(
        &self,
        conversation: &Conversation,
        request_index: usize,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let messages = conversation.messages();
        let request = &messages[request_index];
        let manual = request
            .metadata
            .operation_note(NAME, MANUAL_NOTE)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let replies: Vec<&Message> = messages[request_index + 1..]
            .iter()
            .filter(|message| message.role == Role::Assistant && message.error_kind().is_none())
            .collect();

        if let Some(kind) = trailing_error(conversation) {
            if kind != MessageErrorKind::ContextLengthExceeded {
                let mut effects = Self::abandon(messages, request_index)?;
                if let Some(error) = conversation.last().and_then(Message::error) {
                    effects.push(E::from(emit.message(
                        Message::assistant().with_error(error.kind, error.message.clone()),
                    )));
                }
                return applied(effects);
            }
        } else if replies.is_empty() {
            return not_applicable();
        }
        let prompt = if manual {
            None
        } else {
            hidden_prompt(messages, request_index)
                .or_else(|| latest_prompt(&messages[..request_index]))
        };

        let text = replies
            .iter()
            .flat_map(|message| message.content.iter())
            .filter_map(MessageContent::as_text)
            .collect::<Vec<_>>()
            .join("\n");
        let unusable = trailing_error(conversation).is_some()
            || text.trim().is_empty()
            || replies.iter().any(|message| {
                message.is_tool_call() || message.metadata.output_token_limit_reached
            });
        if unusable {
            return self
                .standalone(
                    messages,
                    request_index,
                    Some(request_index),
                    prompt,
                    manual,
                    Vec::new(),
                    emit,
                )
                .await;
        }
        let summary = summary_message(&text, &self.templates.summary);
        self.complete(
            messages,
            request_index,
            Some(request_index),
            summary,
            prompt,
            manual,
            Vec::new(),
            emit,
        )
    }

    async fn compact<E: CompactionEffect>(
        &self,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let command_message = messages_since_kickoff(conversation)?
            .first()
            .ok_or_else(|| anyhow!("compact command conversation has no kickoff message"))?;
        let command_hidden = E::from(ConversationEffect::SetMessageVisibility {
            message_id: command_message
                .id
                .clone()
                .ok_or_else(|| anyhow!("persisted slash command message has no id"))?,
            user_visible: true,
            agent_visible: false,
        });
        match self.mode {
            CompactionMode::InBand => {
                applied([command_hidden, E::from(self.request_message(true))])
            }
            CompactionMode::Standalone => {
                let messages = conversation.messages();
                self.standalone(
                    messages,
                    messages.len() - 1,
                    None,
                    None,
                    true,
                    vec![command_hidden],
                    emit,
                )
                .await
            }
        }
    }

    /// A request left behind by an earlier turn (the user cancelled or the
    /// inference never ran) must not shape later requests.
    fn stale_request_effects<E: CompactionEffect>(
        conversation: &Conversation,
        request_index: usize,
    ) -> Result<Vec<E>> {
        let turn_start = conversation.len() - messages_since_kickoff(conversation)?.len();
        Self::abandon(&conversation.messages()[..turn_start], request_index)
    }
}

#[async_trait]
impl<S: MachineSession, E: CompactionEffect> Operation<S, E> for CompactionOperation {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn commands(&self, _session: &S) -> Result<Vec<Command<'_, S, E>>> {
        if !self.compact_command {
            return Ok(Vec::new());
        }
        let mut compact =
            Command::new("compact", "Compact the conversation history", move |call| {
                Box::pin(self.compact(call.conversation, call.emit))
            });
        compact.aliases.push("summarize".to_string());
        Ok(vec![compact])
    }

    async fn run(
        &self,
        session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let messages = conversation.messages();
        if let Some(request_index) = messages
            .iter()
            .rposition(|message| message.is_agent_visible() && is_compaction_request(message))
        {
            let turn_start = conversation.len() - messages_since_kickoff(conversation)?.len();
            if request_index < turn_start {
                return applied(Self::stale_request_effects(conversation, request_index)?);
            }
            return self.harvest(conversation, request_index, emit).await;
        }

        if trailing_error(conversation) == Some(MessageErrorKind::ContextLengthExceeded) {
            let context_errors = messages_since_kickoff(conversation)?
                .iter()
                .filter(|message| {
                    message.error_kind() == Some(MessageErrorKind::ContextLengthExceeded)
                })
                .count();
            if context_errors > MAX_CONTEXT_ERROR_COMPACTIONS {
                return not_applicable();
            }
            self.announce(emit);
            return self
                .standalone(
                    messages,
                    messages.len(),
                    None,
                    latest_prompt(messages),
                    false,
                    Vec::new(),
                    emit,
                )
                .await;
        }

        if proactive_compaction_tokens(
            session,
            conversation,
            self.estimator.as_ref(),
            self.context_limit,
            self.threshold,
        )
        .await?
        .is_none()
            || !replied_since_last_summary(conversation)
            || !has_history_before_prompt(messages)
        {
            return not_applicable();
        }
        self.announce(emit);
        match self.mode {
            CompactionMode::InBand => applied(self.request_effects(messages, false)?),
            CompactionMode::Standalone => {
                self.standalone(
                    messages,
                    messages.len(),
                    None,
                    latest_prompt(messages),
                    false,
                    Vec::new(),
                    emit,
                )
                .await
            }
        }
    }
}
