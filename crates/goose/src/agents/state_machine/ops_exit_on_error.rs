//! Ends the turn when an error remains at the end of the conversation.

use anyhow::Result;
use async_trait::async_trait;

use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::{not_applicable, yielded, Emitter, Operation, OperationResult};
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::session::Session;

pub struct ExitOnErrorOperation;

pub(crate) fn turn_failure(conversation: &Conversation) -> Option<&Message> {
    conversation
        .last()
        .filter(|message| message.error_kind().is_some())
}

#[async_trait]
impl Operation<Session, GooseEffect> for ExitOnErrorOperation {
    fn name(&self) -> &'static str {
        "exit_on_error"
    }

    async fn run(
        &self,
        _session: &Session,
        conversation: &Conversation,
        _emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        if turn_failure(conversation).is_none() {
            return not_applicable();
        }

        yielded()
    }
}
