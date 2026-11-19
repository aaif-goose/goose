use anyhow::{anyhow, Result};
use oi_kit_provider_types::conversation::message::{Message, MessageContent, MessageMetadata};
use oi_kit_provider_types::conversation::{merge_consecutive_messages, Conversation};
use rmcp::model::Role;

use super::{CompactionEffect, MANUAL_NOTE, NAME, PROMPT_NOTE};
use crate::operation::ConversationEffect;

pub const CONVERSATION_CONTINUATION_TEXT: &str =
    "Your context was compacted. The previous message contains a summary of the conversation so far.
Do not mention that you read a summary or that conversation summarization occurred.
Just continue the conversation naturally based on the summarized context.";

pub const TOOL_LOOP_CONTINUATION_TEXT: &str =
    "Your context was compacted. The previous message contains a summary of the conversation so far.
Do not mention that you read a summary or that conversation summarization occurred.
Continue calling tools as necessary to complete the task.";

pub const MANUAL_COMPACT_CONTINUATION_TEXT: &str =
    "Your context was compacted at the user's request. The previous message contains a summary of the conversation so far.
Do not mention that you read a summary or that conversation summarization occurred.
Just continue the conversation naturally based on the summarized context.";

fn is_prompt(message: &Message) -> bool {
    let content = message.agent_visible_content().content;
    !content.is_empty()
        && !content.iter().any(|content| {
            matches!(
                content,
                MessageContent::ToolRequest(_) | MessageContent::ToolResponse(_)
            )
        })
}

pub(super) fn latest_prompt(messages: &[Message]) -> Option<usize> {
    messages.iter().rposition(|message| {
        message.is_agent_visible()
            && !message.is_turn_context()
            && !message.metadata.hide_response
            && message.role == Role::User
            && is_prompt(message)
    })
}

fn reissued(prompt: &Message) -> Message {
    prompt.agent_visible_content().content.into_iter().fold(
        Message::user().with_metadata(MessageMetadata::agent_only()),
        Message::with_content,
    )
}

fn is_archived(message: &Message) -> bool {
    !message.is_turn_context() && message.error_kind().is_none()
}

pub(super) fn message_id(message: &Message) -> Result<String> {
    message
        .id
        .clone()
        .ok_or_else(|| anyhow!("persisted message has no id"))
}

pub(super) fn set_visibility(
    message: &Message,
    user_visible: bool,
    agent_visible: bool,
) -> Result<ConversationEffect> {
    Ok(ConversationEffect::SetMessageVisibility {
        message_id: message_id(message)?,
        user_visible,
        agent_visible,
    })
}

fn hidden_prompt(messages: &[Message], request: usize) -> Option<usize> {
    messages[..request]
        .iter()
        .rposition(|message| !message.is_turn_context())
        .filter(|&index| {
            let message = &messages[index];
            message.metadata.operation_note(NAME, PROMPT_NOTE).is_some()
                && !message.is_agent_visible()
        })
}

pub(super) fn abandon<E: CompactionEffect>(messages: &[Message], request: usize) -> Result<Vec<E>> {
    let mut effects = messages[request..]
        .iter()
        .filter(|message| is_archived(message))
        .map(|message| set_visibility(message, false, false).map(E::from))
        .collect::<Result<Vec<_>>>()?;
    if let Some(prompt) = hidden_prompt(messages, request).map(|index| &messages[index]) {
        effects.push(E::from(set_visibility(
            prompt,
            prompt.is_user_visible(),
            true,
        )?));
    }
    Ok(effects)
}

pub(super) struct Plan<'a> {
    messages: &'a [Message],
    end: usize,
    request: Option<usize>,
    prompt: Option<usize>,
    pub(super) manual: bool,
}

impl<'a> Plan<'a> {
    pub(super) fn proactive(messages: &'a [Message]) -> Self {
        Self {
            messages,
            end: messages.len(),
            request: None,
            prompt: latest_prompt(messages),
            manual: false,
        }
    }

    /// The `/compact` command message itself is the last message and stays out of
    /// the summary.
    pub(super) fn command(messages: &'a [Message]) -> Self {
        Self {
            messages,
            end: messages.len() - 1,
            request: None,
            prompt: None,
            manual: true,
        }
    }

    pub(super) fn in_band(messages: &'a [Message], request: usize) -> Self {
        let manual = messages[request]
            .metadata
            .operation_note(NAME, MANUAL_NOTE)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let prompt = if manual {
            None
        } else {
            hidden_prompt(messages, request).or_else(|| latest_prompt(&messages[..request]))
        };
        Self {
            messages,
            end: request,
            request: Some(request),
            prompt,
            manual,
        }
    }

    pub(super) fn summarizable(&self) -> Vec<Message> {
        Conversation::new_unvalidated(
            self.messages[..self.end]
                .iter()
                .filter(|message| !message.is_turn_context())
                .cloned(),
        )
        .agent_visible_messages()
    }

    pub(super) fn abandon<E: CompactionEffect>(&self) -> Result<Vec<E>> {
        self.request
            .map_or(Ok(Vec::new()), |request| abandon(self.messages, request))
    }

    pub(super) fn compacted(&self, summary: Message) -> Conversation {
        let Self {
            messages,
            end,
            request,
            prompt,
            manual,
        } = *self;
        let prompt_is_latest = prompt.is_some_and(|index| {
            messages[index + 1..end]
                .iter()
                .all(Message::is_turn_context)
        });

        let mut compacted: Vec<Message> = messages
            .iter()
            .map(|message| {
                message
                    .clone()
                    .with_metadata(message.metadata.clone().with_agent_invisible())
            })
            .collect();
        if let Some(request) = request {
            for message in compacted[request..]
                .iter_mut()
                .filter(|message| is_archived(message))
            {
                message.metadata.user_visible = false;
            }
        }

        let continuation_text = if manual {
            MANUAL_COMPACT_CONTINUATION_TEXT
        } else if prompt_is_latest {
            CONVERSATION_CONTINUATION_TEXT
        } else {
            TOOL_LOOP_CONTINUATION_TEXT
        };
        let continuation = Message::assistant()
            .with_text(continuation_text)
            .with_metadata(MessageMetadata::agent_only());
        let continuation_created = continuation.created;
        let (merged, _issues) = merge_consecutive_messages(vec![summary, continuation]);
        compacted.extend(merged);

        if let Some(index) = prompt {
            let mut reissued = reissued(&messages[index]);
            reissued.created = continuation_created;
            compacted.push(reissued);

            // Only the turn's own context event follows the prompt; earlier ones belong
            // to previous turns.
            if let Some(turn_context) = messages[index + 1..]
                .iter()
                .rev()
                .find(|message| message.is_turn_context() && message.is_agent_visible())
            {
                let mut carried = turn_context.clone();
                carried.id = None;
                // Storage reloads order by created timestamp, so the copy must sort
                // after the continuation rather than at the original event's time.
                if let Some(latest) = compacted.iter().map(|message| message.created).max() {
                    carried.created = carried.created.max(latest);
                }
                compacted.push(carried);
            }
        }

        Conversation::new_unvalidated(compacted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn_context(text: &str) -> Message {
        Message::user()
            .with_text(text)
            .with_metadata(MessageMetadata::agent_only().with_turn_context())
    }

    #[test]
    fn preserves_the_prompt_and_carries_only_its_own_turn_context() {
        let messages = [
            Message::user().with_text("earlier request"),
            turn_context("<turn-context>stale</turn-context>"),
            Message::assistant().with_text("earlier response"),
            Message::user().with_text("the real current request"),
            turn_context("<turn-context>frozen</turn-context>"),
        ];
        let summary = Message::user()
            .with_text("summary")
            .with_metadata(MessageMetadata::agent_only().with_compaction_summary());
        let compacted = Plan::proactive(&messages).compacted(summary);

        let agent_texts: Vec<String> = compacted
            .agent_visible_messages()
            .iter()
            .map(Message::as_concat_text)
            .collect();
        assert_eq!(
            agent_texts,
            [
                "summary",
                CONVERSATION_CONTINUATION_TEXT,
                "the real current request",
                "<turn-context>frozen</turn-context>",
            ]
        );
        assert!(compacted
            .messages()
            .iter()
            .filter(|message| !message.is_agent_visible())
            .all(|message| message.is_turn_context() || message.is_user_visible()));
    }
}
