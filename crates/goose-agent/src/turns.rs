//! Limits autonomous assistant turns using persisted conversation history.

use anyhow::Result;
use async_trait::async_trait;
use goose_provider_types::conversation::message::Message;
use goose_provider_types::conversation::Conversation;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};

use crate::operation::{
    assistant_turn_count, ends_turn, messages_since_kickoff, not_applicable, yielded, yielded_with,
    Emitter, Operation, OperationResult,
};

const OPERATION_NAME: &str = "max_turns";
const EXHAUSTED: &str = "exhausted";

#[cfg(not(target_arch = "wasm32"))]
type ExhaustionMessage = dyn Fn() -> Message + Send + Sync;

#[cfg(target_arch = "wasm32")]
type ExhaustionMessage = dyn Fn() -> Message;

/// Whether a message records turn-limit exhaustion, independently of its display text.
pub fn turn_limit_reached(message: &Message) -> bool {
    message
        .metadata
        .operation_note(OPERATION_NAME, EXHAUSTED)
        .is_some()
}

/// Yields when the conversation has used its autonomous turn budget.
///
/// Place this operation after tool handling and normal completion, before inference.
/// Turns are consecutive assistant-message blocks since the latest kickoff, so
/// counting follows the persisted history rather than individual provider calls.
pub struct MaxTurnsOperation {
    max_turns: u32,
    exhaustion_message: Option<Box<ExhaustionMessage>>,
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
    /// Records exhaustion with an empty, hidden message. A zero limit yields before inference.
    pub fn new(max_turns: u32) -> Self {
        Self {
            max_turns,
            exhaustion_message: None,
        }
    }

    /// Customizes the exhaustion message's content and visibility.
    /// The operation adds its exhaustion marker to the returned message.
    pub fn with_exhaustion_message(
        mut self,
        message: impl Fn() -> Message + MaybeSend + MaybeSync + 'static,
    ) -> Self {
        self.exhaustion_message = Some(Box::new(message));
        self
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
        // The default exhaustion marker is also an assistant message without tool
        // calls, so check it before ends_turn() treats it as a completed answer.
        // Inference ignores the hidden marker and could otherwise resume the
        // exhausted turn after reconstruction.
        if messages.last().is_some_and(turn_limit_reached) {
            return yielded();
        }
        if ends_turn(messages) || assistant_turn_count(messages) < self.max_turns {
            return not_applicable();
        }

        let mut message = self
            .exhaustion_message
            .as_ref()
            .map(|build| build())
            .unwrap_or_else(|| Message::assistant().with_visibility(false, false));
        message.metadata.set_operation_note(
            OPERATION_NAME,
            EXHAUSTED,
            serde_json::Value::Bool(true),
        );
        let message = emit.message(message);
        yielded_with([message.into()])
    }
}
