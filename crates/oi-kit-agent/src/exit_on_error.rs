//! Ends the turn when an error remains at the end of the conversation.

use anyhow::Result;
use async_trait::async_trait;
use oi_kit_provider_types::conversation::Conversation;
use oi_kit_provider_types::maybe_send::{MaybeSend, MaybeSync};

use crate::operation::{
    not_applicable, trailing_error_message, yielded, Emitter, Operation, OperationResult,
};

pub struct ExitOnErrorOperation;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for ExitOnErrorOperation
where
    S: MaybeSend + MaybeSync + 'static,
    E: MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        "exit_on_error"
    }

    async fn run(
        &self,
        _session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let Some(message) = trailing_error_message(conversation) else {
            return not_applicable();
        };
        emit.record_failure(message);
        yielded()
    }
}
