//! Compacts the conversation when it outgrows the context window.
//!
//! In-band compaction asks for the summary through the normal inference step on
//! the live prefix and harvests the reply afterwards. Standalone compaction
//! summarizes with a separate request; it recovers from context-length errors,
//! where the live prefix no longer fits, and backs up the in-band shape when the
//! reply is unusable. The operation must run before anything that reacts to an
//! assistant reply, so the summary is harvested before tool execution sees it.

mod layout;

use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use oi_kit_context_management::{
    summarize, summary_message, CompactionModel, Templates, TokenEstimator,
};
use oi_kit_provider_types::conversation::message::{
    Message, MessageContent, MessageErrorKind, MessageMetadata, SystemNotificationType,
};
use oi_kit_provider_types::conversation::{Conversation, EffectiveRole};
use oi_kit_provider_types::errors::ProviderError;
use rmcp::model::Role;

use crate::command::Command;
use crate::inference::InferenceEffect;
use crate::machine::MachineSession;
use crate::operation::{
    applied, last_effective_role, messages_since_kickoff, not_applicable, trailing_error,
    yielded_with, ConversationEffect, Emitter, Operation, OperationResult,
};
use crate::tool_call_state::open_tool_calls;
use layout::{latest_prompt, message_id, set_visibility, Plan};
pub use layout::{
    CONVERSATION_CONTINUATION_TEXT, MANUAL_COMPACT_CONTINUATION_TEXT, TOOL_LOOP_CONTINUATION_TEXT,
};

const NAME: &str = "compaction";
const REQUEST_NOTE: &str = "request";
const MANUAL_NOTE: &str = "manual";
const PROMPT_NOTE: &str = "prompt";
const COMPACTION_THINKING_TEXT: &str = "Compacting the conversation...";
const COMPACTION_COMPLETE_TEXT: &str = "Compaction complete";

pub const MAX_CONTEXT_ERROR_COMPACTIONS: usize = 2;

pub trait CompactionEffect: InferenceEffect + From<ConversationEffect> {
    /// Hosts that store messages individually can override this to update them in place.
    fn compact(conversation: Conversation) -> Self {
        Self::from(ConversationEffect::ReplaceConversation(conversation))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionMode {
    InBand,
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

/// Reported usage stops at the inference that requested the tools; their results
/// are not counted until the next request.
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

/// Compacting again before the model replied would only summarize the summary.
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

/// Operations ahead of tool execution may answer part of a tool batch; archiving
/// the rest of the requests would orphan their results.
fn has_outstanding_tool_calls(messages: &[Message]) -> bool {
    open_tool_calls(messages).next().is_some()
}

fn has_history_beyond_prompt(messages: &[Message]) -> bool {
    let prompt = latest_prompt(messages);
    messages.iter().enumerate().any(|(index, message)| {
        Some(index) != prompt && message.is_agent_visible() && !message.is_turn_context()
    })
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

    fn request_effects<E: CompactionEffect>(
        &self,
        messages: &[Message],
        manual: bool,
    ) -> Result<Vec<E>> {
        let mut effects = Vec::new();
        if !manual && last_effective_role(messages)? == EffectiveRole::User {
            if let Some(prompt) = latest_prompt(messages).map(|index| &messages[index]) {
                effects.push(E::from(set_visibility(
                    prompt,
                    prompt.is_user_visible(),
                    false,
                )?));
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

    async fn standalone<E: CompactionEffect>(
        &self,
        plan: &Plan<'_>,
        before: Vec<E>,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let mut effects = before;
        match summarize(
            self.model.as_ref(),
            Some(self.estimator.as_ref()),
            &self.templates,
            &plan.summarizable(),
        )
        .await
        {
            Ok(summary) => {
                effects.push(E::record_usage(summary.usage));
                self.complete(plan, summary.message, effects, emit)
            }
            Err(error) => {
                effects.extend(plan.abandon()?);
                effects.push(E::from(emit.message(compaction_failure_message(&error))));
                yielded_with(effects)
            }
        }
    }

    fn complete<E: CompactionEffect>(
        &self,
        plan: &Plan<'_>,
        summary: Message,
        mut effects: Vec<E>,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        effects.push(E::compact(plan.compacted(summary)));
        if plan.manual {
            effects.push(E::from(emit.message(complete_message())));
            return yielded_with(effects);
        }
        emit.message(Message::assistant().with_system_notification(
            SystemNotificationType::InlineMessage,
            COMPACTION_COMPLETE_TEXT,
        ));
        applied(effects)
    }

    async fn harvest<E: CompactionEffect>(
        &self,
        conversation: &Conversation,
        request_index: usize,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let messages = conversation.messages();
        let plan = Plan::in_band(messages, request_index);
        let replies: Vec<&Message> = messages[request_index + 1..]
            .iter()
            .filter(|message| message.role == Role::Assistant && message.error_kind().is_none())
            .collect();

        if let Some(kind) = trailing_error(conversation) {
            if kind != MessageErrorKind::ContextLengthExceeded {
                let mut effects = plan.abandon()?;
                if let Some(error) = conversation.last() {
                    let mut shown = error.clone();
                    shown.id = None;
                    shown.metadata.user_visible = true;
                    effects.push(E::from(emit.message(shown)));
                }
                return applied(effects);
            }
        } else if replies.is_empty() {
            return not_applicable();
        }

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
            return self.standalone(&plan, Vec::new(), emit).await;
        }
        let summary = summary_message(&text, &self.templates.summary);
        self.complete(&plan, summary, Vec::new(), emit)
    }

    async fn compact<E: CompactionEffect>(
        &self,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let command_message = messages_since_kickoff(conversation)?
            .first()
            .ok_or_else(|| anyhow!("compact command conversation has no kickoff message"))?;
        let command_hidden = E::from(set_visibility(command_message, true, false)?);
        match self.mode {
            CompactionMode::InBand => {
                applied([command_hidden, E::from(self.request_message(true))])
            }
            CompactionMode::Standalone => {
                self.standalone(
                    &Plan::command(conversation.messages()),
                    vec![command_hidden],
                    emit,
                )
                .await
            }
        }
    }

    fn stale_request_effects<E: CompactionEffect>(
        conversation: &Conversation,
        request_index: usize,
    ) -> Result<Vec<E>> {
        let turn_start = conversation.len() - messages_since_kickoff(conversation)?.len();
        layout::abandon(&conversation.messages()[..turn_start], request_index)
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
                .standalone(&Plan::proactive(messages), Vec::new(), emit)
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
            || !has_history_beyond_prompt(messages)
            || has_outstanding_tool_calls(messages_since_kickoff(conversation)?)
        {
            return not_applicable();
        }
        self.announce(emit);
        match self.mode {
            CompactionMode::InBand => applied(self.request_effects(messages, false)?),
            CompactionMode::Standalone => {
                self.standalone(&Plan::proactive(messages), Vec::new(), emit)
                    .await
            }
        }
    }
}
