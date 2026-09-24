//! Recovers turns where the model produced only thinking and no answer.
//!
//! The thinking the model already did is kept, and an agent-only user message
//! prompts it to continue. If the model keeps thinking without answering, a
//! visible message ends the turn instead of silence.

use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::Role;
use tracing::warn;

use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::{
    applied, ends_turn, messages_since_kickoff, not_applicable, Emitter, Operation, OperationResult,
};
use crate::conversation::message::{Message, MessageContent};
use crate::conversation::Conversation;
use crate::session::Session;

pub(crate) const THINKING_ONLY_CONTINUATION_MESSAGE: &str =
    "You did some thinking but did not produce an answer. Please continue.";
pub(crate) const THINKING_ONLY_TURN_MESSAGE: &str =
    "The model finished thinking but did not produce an answer. Please resend your message to continue.";
const MAX_THINKING_RECOVERY_ATTEMPTS: u32 = 3;

const CONTINUED: &str = "continued";

pub(crate) fn is_answer_content(content: &MessageContent) -> bool {
    match content {
        MessageContent::Thinking(_) | MessageContent::RedactedThinking(_) => false,
        MessageContent::Text(text) => !text.text.is_empty(),
        MessageContent::Image(image) => !image.data.is_empty(),
        MessageContent::SystemNotification(notification) => !notification.msg.is_empty(),
        _ => true,
    }
}

pub(crate) fn is_thinking_content(content: &MessageContent) -> bool {
    match content {
        MessageContent::Thinking(thinking) => {
            !thinking.thinking.is_empty() || !thinking.signature.is_empty()
        }
        MessageContent::RedactedThinking(thinking) => !thinking.data.is_empty(),
        _ => false,
    }
}

fn is_thinking_only_turn(messages: &[Message]) -> bool {
    let mut produced_thinking = false;
    for content in messages
        .iter()
        .rev()
        .take_while(|message| message.role == Role::Assistant)
        .flat_map(|message| message.content.iter())
    {
        if is_answer_content(content) {
            return false;
        }
        produced_thinking |= is_thinking_content(content);
    }
    produced_thinking
}

pub struct ThinkingRecoveryOperation;

#[async_trait]
impl Operation<Session, GooseEffect> for ThinkingRecoveryOperation {
    fn name(&self) -> &'static str {
        "thinking_recovery"
    }

    async fn run(
        &self,
        _session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        if !ends_turn(messages) || !is_thinking_only_turn(messages) {
            return not_applicable();
        }

        let mut attempts = 0u32;
        for message in messages {
            if message.role == Role::Assistant && message.content.iter().any(is_answer_content) {
                attempts = 0;
            } else if self.message_meta(message, CONTINUED).is_some() {
                attempts += 1;
            }
        }

        if attempts < MAX_THINKING_RECOVERY_ATTEMPTS {
            warn!(
                "Provider returned a thinking-only response; prompting it to continue ({}/{})",
                attempts + 1,
                MAX_THINKING_RECOVERY_ATTEMPTS
            );
            let mut continuation = Message::user()
                .with_text(THINKING_ONLY_CONTINUATION_MESSAGE)
                .with_visibility(false, true);
            self.set_message_meta(&mut continuation, CONTINUED, serde_json::json!(true));
            applied([continuation.into()])
        } else {
            warn!("Provider returned a thinking-only response after retries; ending turn");
            let message = Message::assistant().with_text(THINKING_ONLY_TURN_MESSAGE);
            let message = emit.message(message).await;
            applied([message.into()])
        }
    }
}
