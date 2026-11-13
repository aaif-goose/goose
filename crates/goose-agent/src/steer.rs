//! Adds queued user guidance when the agent is between model and tool turns.

use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use goose_provider_types::conversation::message::Message;
use goose_provider_types::conversation::{Conversation, EffectiveRole};
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};
use tokio::sync::Mutex;

use crate::operation::{
    applied, ends_turn, last_effective_role, messages_since_kickoff, not_applicable, Emitter,
    Operation, OperationResult,
};

pub type SteerQueue = Arc<Mutex<VecDeque<Message>>>;

pub struct SteerOperation {
    queue: SteerQueue,
    drained: std::sync::Mutex<Vec<Message>>,
}

impl SteerOperation {
    pub fn new(queue: SteerQueue) -> Self {
        Self {
            queue,
            drained: std::sync::Mutex::default(),
        }
    }

    fn take_drained<E>(&self) -> Vec<E>
    where
        E: From<Message>,
    {
        std::mem::take(&mut *self.drained.lock().unwrap())
            .into_iter()
            .map(E::from)
            .collect()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for SteerOperation
where
    S: MaybeSend + MaybeSync + 'static,
    E: From<Message> + MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        "steer"
    }

    async fn finalize_cancellation(
        &self,
        _session: &S,
        _conversation: &Conversation,
        _emit: &Emitter,
    ) -> Vec<E> {
        self.take_drained()
    }

    async fn run(
        &self,
        _session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let messages = messages_since_kickoff(conversation)?;
        let between_turns =
            ends_turn(messages) || last_effective_role(messages)? == EffectiveRole::Tool;
        if !between_turns {
            return not_applicable();
        }

        let pending: Vec<_> = self
            .queue
            .lock()
            .await
            .drain(..)
            .map(Message::with_steer)
            .collect();
        if pending.is_empty() {
            return not_applicable();
        }

        let mut drained = Vec::with_capacity(pending.len());
        for message in pending {
            drained.push(emit.message(message));
        }
        *self.drained.lock().unwrap() = drained;
        applied(self.take_drained())
    }
}
