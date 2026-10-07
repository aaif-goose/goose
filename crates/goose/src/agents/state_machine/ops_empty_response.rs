//! Reveals the empty-response fallback when nothing else took over the turn.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use goose_agent::inference::is_empty_response_marker;

use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::{
    messages_since_kickoff, not_applicable, yielded_with, ConversationEffect, Emitter, Operation,
    OperationResult,
};
use crate::conversation::Conversation;
use crate::session::Session;

pub struct EmptyResponseOperation;

#[async_trait]
impl Operation<Session, GooseEffect> for EmptyResponseOperation {
    fn name(&self) -> &'static str {
        "empty_response"
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let Some(fallback) = messages_since_kickoff(conversation)?
            .last()
            .filter(|message| is_empty_response_marker(message))
        else {
            return not_applicable();
        };
        if session
            .recipe
            .as_ref()
            .is_some_and(|recipe| recipe.retry.is_some())
        {
            return not_applicable();
        }

        let message_id = fallback
            .id
            .clone()
            .ok_or_else(|| anyhow!("Persisted empty-response fallback has no id"))?;
        emit.message(fallback.clone().with_visibility(true, true))
            .await;
        yielded_with([ConversationEffect::SetMessageVisibility {
            message_id,
            user_visible: true,
            agent_visible: true,
        }
        .into()])
    }
}
