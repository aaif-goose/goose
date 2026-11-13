//! Retries empty provider responses before exposing the failure.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use goose_provider_types::conversation::message::Message;
use goose_provider_types::conversation::Conversation;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};
use rmcp::model::Role;

use crate::inference::is_empty_response_marker;
use crate::operation::{
    applied, not_applicable, ConversationEffect, Emitter, Operation, OperationResult,
};

const MAX_RETRIES: usize = 3;
const RETRY_MESSAGE: &str = "The model returned an empty response. Try again.";

pub struct EmptyResponseOperation;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for EmptyResponseOperation
where
    S: MaybeSend + MaybeSync + 'static,
    E: From<Message> + From<ConversationEffect> + MaybeSend + 'static,
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
        let Some(fallback) = messages.last().filter(|message| {
            is_empty_response_marker(message)
                && message.is_user_visible()
                && !message.is_agent_visible()
        }) else {
            return not_applicable();
        };
        let message_id = fallback
            .id
            .clone()
            .ok_or_else(|| anyhow!("Persisted empty-response fallback has no id"))?;
        let empty_responses = messages
            .iter()
            .rev()
            .take_while(|message| {
                is_empty_response_marker(message)
                    || (message.role == Role::User && message.as_concat_text() == RETRY_MESSAGE)
            })
            .filter(|message| is_empty_response_marker(message))
            .count();

        if empty_responses <= MAX_RETRIES {
            return applied([
                ConversationEffect::SetMessageVisibility {
                    message_id,
                    user_visible: false,
                    agent_visible: false,
                }
                .into(),
                Message::user()
                    .with_text(RETRY_MESSAGE)
                    .with_visibility(false, true)
                    .into(),
            ]);
        }

        emit.message(fallback.clone().with_visibility(true, true));
        applied([ConversationEffect::SetMessageVisibility {
            message_id,
            user_visible: true,
            agent_visible: true,
        }
        .into()])
    }
}
