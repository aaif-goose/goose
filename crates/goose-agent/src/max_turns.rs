//! Limits autonomous assistant turns using persisted conversation history.

use anyhow::Result;
use async_trait::async_trait;
use goose_provider_types::conversation::message::{Message, MessageErrorKind};
use goose_provider_types::conversation::Conversation;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};

use crate::operation::{
    assistant_turn_count, ends_turn, messages_since_kickoff, not_applicable, yielded, yielded_with,
    Emitter, Operation, OperationResult,
};

const OPERATION_NAME: &str = "max_turns";

/// The error shown when the autonomous turn budget is exhausted.
pub const MAX_TURNS_MESSAGE: &str = "I've reached the maximum number of actions I can do without user input. Would you like me to continue?";

/// Yields when the conversation has used its autonomous turn budget.
///
/// Place this operation after tool handling and normal completion, before inference.
/// Turns are consecutive assistant-message blocks since the latest kickoff, so
/// counting follows the persisted history rather than individual provider calls.
pub struct MaxTurnsOperation {
    max_turns: u32,
}

fn turn_budget_part(turns_taken: u32, max_turns: u32) -> Option<String> {
    if max_turns == 0 || turns_taken.saturating_mul(2) < max_turns {
        return None;
    }

    Some(format!(
        "<turn-budget>{turns_taken}/{max_turns} used</turn-budget>"
    ))
}

impl MaxTurnsOperation {
    /// A zero limit yields before inference.
    pub fn new(max_turns: u32) -> Self {
        Self { max_turns }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for MaxTurnsOperation
where
    S: MaybeSend + MaybeSync + 'static,
    E: From<Message> + MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        OPERATION_NAME
    }

    async fn moim_parts(&self, _session: &S, conversation: &Conversation) -> Result<Vec<String>> {
        let turns_taken = assistant_turn_count(messages_since_kickoff(conversation)?);
        Ok(turn_budget_part(turns_taken, self.max_turns)
            .into_iter()
            .collect())
    }

    async fn run(
        &self,
        _session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let messages = messages_since_kickoff(conversation)?;
        if messages.last().and_then(Message::error_kind)
            == Some(MessageErrorKind::ResourceExhausted)
        {
            return yielded();
        }
        if ends_turn(messages) || assistant_turn_count(messages) < self.max_turns {
            return not_applicable();
        }

        let message =
            Message::assistant().with_error(MessageErrorKind::ResourceExhausted, MAX_TURNS_MESSAGE);
        let message = emit.message(message);
        yielded_with([message.into()])
    }
}
