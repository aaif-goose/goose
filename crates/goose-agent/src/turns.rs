//! Limits autonomous assistant turns using persisted conversation history.

use anyhow::Result;
use async_trait::async_trait;
use goose_provider_types::conversation::message::Message;
use goose_provider_types::conversation::Conversation;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};

use crate::operation::{
    assistant_turn_count, messages_since_kickoff, not_applicable, yielded_with, Emitter, Operation,
    OperationResult,
};

const OPERATION_NAME: &str = "max_turns";

/// Yields when the conversation has used its autonomous turn budget.
///
/// Place this operation after tool handling and normal completion, before inference.
/// Turns are consecutive assistant-message blocks since the latest kickoff, so
/// counting follows the persisted history rather than individual provider calls.
pub struct MaxTurnsOperation {
    max_turns: u32,
    exhaustion_message: String,
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
    /// Uses the host's exhaustion text. A zero limit yields before inference.
    pub fn new(max_turns: u32, exhaustion_message: impl Into<String>) -> Self {
        Self {
            max_turns,
            exhaustion_message: exhaustion_message.into(),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: MaybeSync, E: From<Message> + MaybeSend + 'static> Operation<S, E> for MaxTurnsOperation {
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
        if assistant_turn_count(messages) < self.max_turns {
            return not_applicable();
        }

        let message = Message::assistant().with_text(&self.exhaustion_message);
        let message = emit.message(message);
        yielded_with([message.into()])
    }
}
