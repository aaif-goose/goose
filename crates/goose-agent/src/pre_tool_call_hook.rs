use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use goose_provider_types::conversation::message::{Message, ToolRequest};
use goose_provider_types::conversation::Conversation;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};

use crate::operation::{
    applied, messages_since_kickoff, not_applicable, ConversationEffect, Emitter, Operation,
    OperationResult,
};
use crate::tool_call_state::{add_error_response, tool_calls, tool_meta_flag, ToolCallState};

pub const PRE_HOOK_ALLOWED_KEY: &str = "goose.pre_hook_allowed";

#[derive(Debug)]
pub enum PreToolCallDecision {
    Allow,
    Deny { reason: String },
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait PreToolCallHook<S>: MaybeSend + MaybeSync {
    async fn before_call(
        &self,
        session: &S,
        conversation: &Conversation,
        request: &ToolRequest,
    ) -> Result<PreToolCallDecision>;
}

pub struct PreToolCallHookOperation<'a, S> {
    hook: Arc<dyn PreToolCallHook<S> + 'a>,
}

impl<'a, S> PreToolCallHookOperation<'a, S> {
    pub fn new(hook: Arc<dyn PreToolCallHook<S> + 'a>) -> Self {
        Self { hook }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for PreToolCallHookOperation<'_, S>
where
    S: MaybeSend + MaybeSync + 'static,
    E: From<ConversationEffect> + MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        "pre_tool_call_hook"
    }

    async fn run(
        &self,
        session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let Some(request) = tool_calls(messages_since_kickoff(conversation)?)
            .into_iter()
            .find(|(request, state)| {
                *state == ToolCallState::Executable
                    && request.tool_call.is_ok()
                    && !request.was_executed_externally()
                    && tool_meta_flag(request, PRE_HOOK_ALLOWED_KEY) != Some(true)
            })
            .map(|(request, _)| request)
        else {
            return not_applicable();
        };
        match self
            .hook
            .before_call(session, conversation, request)
            .await?
        {
            PreToolCallDecision::Allow => {
                applied([E::from(ConversationEffect::PatchToolRequestMeta {
                    tool_call_id: request.id.clone(),
                    patch: serde_json::json!({ PRE_HOOK_ALLOWED_KEY: true }),
                })])
            }
            PreToolCallDecision::Deny { reason } => {
                let mut tool_responses = Message::user();
                add_error_response(&mut tool_responses, request, &reason);
                applied([E::from(ConversationEffect::from(
                    emit.message(tool_responses),
                ))])
            }
        }
    }
}
