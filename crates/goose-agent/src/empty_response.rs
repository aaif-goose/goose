//! Retries empty provider responses before exposing the failure.

use anyhow::Result;
use async_trait::async_trait;
use goose_provider_types::conversation::message::Message;
use goose_provider_types::conversation::Conversation;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};
use rmcp::model::Role;

use crate::inference::{ends_with_successful_tool_response, is_empty_response};
use crate::operation::{applied, not_applicable, Emitter, Operation, OperationResult};

const MAX_RETRIES: usize = 3;
const RETRY_MESSAGE: &str = "The model returned an empty response. Try again.";
pub const EMPTY_RESPONSE_MESSAGE: &str =
    "The model returned an empty response. Please resend your message to continue.";

pub struct EmptyResponseOperation;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for EmptyResponseOperation
where
    S: MaybeSend + MaybeSync + 'static,
    E: From<Message> + MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        "empty_response"
    }

    async fn run(
        &self,
        _session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let messages = conversation.messages();
        if !messages
            .last()
            .is_some_and(|message| message.role == Role::Assistant && is_empty_response(message))
        {
            return not_applicable();
        }
        if ends_with_successful_tool_response(&messages[..messages.len() - 1]) {
            return not_applicable();
        }

        let empty_responses = messages
            .iter()
            .rev()
            .take_while(|message| {
                (message.role == Role::Assistant && is_empty_response(message))
                    || (message.role == Role::User && message.as_concat_text() == RETRY_MESSAGE)
            })
            .filter(|message| message.role == Role::Assistant && is_empty_response(message))
            .count();

        if empty_responses <= MAX_RETRIES {
            let retry = Message::user()
                .with_text(RETRY_MESSAGE)
                .with_visibility(false, true);
            return applied([retry.into()]);
        }

        let fallback = emit.message(Message::assistant().with_text(EMPTY_RESPONSE_MESSAGE));
        applied([fallback.into()])
    }
}
