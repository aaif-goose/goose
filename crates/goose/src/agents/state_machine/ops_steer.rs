//! Adds queued user guidance when the agent is between model and tool turns.

use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::{
    applied, ends_turn, last_effective_role, messages_since_kickoff, not_applicable, Emitter,
    Operation, OperationResult,
};
use crate::conversation::message::Message;
use crate::conversation::{Conversation, EffectiveRole};
use crate::session::Session;

pub(crate) type SteerQueue = Arc<Mutex<VecDeque<Message>>>;

pub struct SteerOperation {
    queue: SteerQueue,
    drained: std::sync::Mutex<Vec<Message>>,
}

impl SteerOperation {
    pub(crate) fn new(queue: SteerQueue) -> Self {
        Self {
            queue,
            drained: std::sync::Mutex::default(),
        }
    }

    fn take_drained(&self) -> Vec<GooseEffect> {
        std::mem::take(&mut *self.drained.lock().unwrap())
            .into_iter()
            .map(GooseEffect::from)
            .collect()
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for SteerOperation {
    fn name(&self) -> &'static str {
        "steer"
    }

    async fn finalize_cancellation(
        &self,
        _session: &Session,
        _conversation: &Conversation,
        _emit: &Emitter,
    ) -> Vec<GooseEffect> {
        self.take_drained()
    }

    async fn run(
        &self,
        _session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
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
