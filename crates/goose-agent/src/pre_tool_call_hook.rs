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
use crate::tool_call_state::{add_error_response, mark_executable, tool_calls, ToolCallState};

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
    hook: Option<Arc<dyn PreToolCallHook<S> + 'a>>,
}

impl<'a, S> PreToolCallHookOperation<'a, S> {
    pub fn new() -> Self {
        Self { hook: None }
    }

    pub fn with_hook(mut self, hook: Arc<dyn PreToolCallHook<S> + 'a>) -> Self {
        self.hook = Some(hook);
        self
    }
}

impl<S> Default for PreToolCallHookOperation<'_, S> {
    fn default() -> Self {
        Self::new()
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
        let mut approved = tool_calls(messages_since_kickoff(conversation)?)
            .into_iter()
            .filter(|(_, state)| *state == ToolCallState::Approved)
            .map(|(request, _)| request);

        let Some(hook) = &self.hook else {
            let effects: Vec<_> = approved
                .map(|request| E::from(mark_executable(&request.id, true)))
                .collect();
            if effects.is_empty() {
                return not_applicable();
            }
            return applied(effects);
        };

        let Some(request) = approved.next() else {
            return not_applicable();
        };
        match hook.before_call(session, conversation, request).await? {
            PreToolCallDecision::Allow => applied([E::from(mark_executable(&request.id, true))]),
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
