use anyhow::Result;
use async_trait::async_trait;
use goose_provider_types::conversation::Conversation;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};

use crate::operation::{
    not_applicable, trailing_error, yielded, Emitter, Operation, OperationResult,
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
        _emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        if trailing_error(conversation).is_none() {
            return not_applicable();
        }

        yielded()
    }
}
