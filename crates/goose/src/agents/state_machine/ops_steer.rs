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
use crate::hooks::{HookContext, HookEvent, HookManager};
use crate::session::Session;

pub(crate) type SteerQueue = Arc<Mutex<VecDeque<Message>>>;

pub struct SteerOperation {
    queue: SteerQueue,
    hook_manager: HookManager,
}

impl SteerOperation {
    pub(crate) fn new(queue: SteerQueue, hook_manager: HookManager) -> Self {
        Self {
            queue,
            hook_manager,
        }
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for SteerOperation {
    fn name(&self) -> &'static str {
        "steer"
    }

    async fn run(
        &self,
        session: &Session,
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

        let mut effects = Vec::with_capacity(pending.len());
        for message in pending {
            let context = HookContext::new(HookEvent::UserPromptSubmit, &session.id)
                .with_message(message.as_concat_text());
            self.hook_manager
                .emit(HookEvent::UserPromptSubmit, context)
                .await;
            let message = emit.message(message).await;
            effects.push(message.into());
        }
        applied(effects)
    }
}
