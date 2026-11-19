use anyhow::Result;
use async_trait::async_trait;
use oi_kit_provider_types::conversation::message::{Message, MessageContent};
use oi_kit_provider_types::conversation::Conversation;
use oi_kit_provider_types::maybe_send::{MaybeSend, MaybeSync};
use rmcp::model::Role;

use crate::inference::{ends_with_successful_tool_response, is_empty_response};
use crate::operation::{
    applied, messages_since_kickoff, not_applicable, Emitter, Operation, OperationResult,
};

const MAX_RETRIES: usize = 3;
const OPERATION: &str = "empty_response";
const RETRY_MESSAGE: &str = "The model returned an empty response. Try again.";
pub const RESPONSE_RECOVERY_OPERATION: &str = "response_recovery";
pub const RESPONSE_RECOVERY_RETRIED: &str = "retried";
pub const EMPTY_RESPONSE_MESSAGE: &str =
    "The model returned an empty response. Please resend your message to continue.";

pub struct EmptyResponseOperation;

fn is_hidden_note(message: &Message) -> bool {
    message.role == Role::User && !message.is_user_visible() && !message.is_tool_response()
}

pub fn is_answer_content(content: &MessageContent) -> bool {
    match content {
        MessageContent::Thinking(_) | MessageContent::RedactedThinking(_) => false,
        MessageContent::Text(text) => !text.text.trim().is_empty(),
        MessageContent::Image(image) => !image.data.is_empty(),
        MessageContent::SystemNotification(notification) => !notification.msg.is_empty(),
        _ => true,
    }
}

pub fn response_recovery_attempts(conversation: &Conversation) -> usize {
    let mut attempts = 0;
    for message in conversation.messages().iter().rev() {
        if message
            .metadata
            .operation_note(RESPONSE_RECOVERY_OPERATION, RESPONSE_RECOVERY_RETRIED)
            .is_some()
        {
            attempts += 1;
        } else if (message.role == Role::User
            && !message.is_tool_response()
            && message.is_user_visible())
            || (message.role == Role::Assistant && message.content.iter().any(is_answer_content))
        {
            break;
        }
    }
    attempts
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for EmptyResponseOperation
where
    S: MaybeSend + MaybeSync + 'static,
    E: From<Message> + MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        OPERATION
    }

    async fn run(
        &self,
        _session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let messages = messages_since_kickoff(conversation)?;
        if !messages
            .last()
            .is_some_and(|message| message.role == Role::Assistant && is_empty_response(message))
        {
            return not_applicable();
        }
        let before_reply = &messages[..messages.len() - 1];
        let end = before_reply
            .iter()
            .rposition(|message| !is_hidden_note(message))
            .map_or(0, |index| index + 1);
        if ends_with_successful_tool_response(&before_reply[..end]) {
            return not_applicable();
        }

        if response_recovery_attempts(conversation) < MAX_RETRIES {
            let mut retry = Message::user().with_text(RETRY_MESSAGE).agent_only();
            retry.metadata.set_operation_note(
                RESPONSE_RECOVERY_OPERATION,
                RESPONSE_RECOVERY_RETRIED,
                serde_json::json!(true),
            );
            return applied([retry.into()]);
        }

        let fallback = emit.message(Message::assistant().with_text(EMPTY_RESPONSE_MESSAGE));
        applied([fallback.into()])
    }
}
