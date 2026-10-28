use anyhow::Result;
use async_trait::async_trait;
use goose_provider_types::conversation::message::{ToolRequest, ToolResponse};
use goose_provider_types::conversation::Conversation;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait PostToolCallHook<S>: MaybeSend + MaybeSync {
    async fn after_call(
        &self,
        session: &S,
        conversation: &Conversation,
        request: &ToolRequest,
        response: &ToolResponse,
    ) -> Result<()>;
}
