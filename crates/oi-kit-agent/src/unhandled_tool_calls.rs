use anyhow::Result;
use async_trait::async_trait;
use oi_kit_provider_types::conversation::message::{Message, ToolRequest};
use oi_kit_provider_types::conversation::Conversation;
use oi_kit_provider_types::maybe_send::{MaybeSend, MaybeSync};

use crate::operation::{
    applied, messages_since_kickoff, not_applicable, yielded, ConversationEffect, Emitter,
    Operation, OperationResult,
};
use crate::tool_call_state::{add_error_response, open_tool_calls, ToolCallState};

pub struct UnhandledToolCallsOperation;

impl UnhandledToolCallsOperation {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UnhandledToolCallsOperation {
    fn default() -> Self {
        Self::new()
    }
}

fn respond_to_unhandled_calls(
    calls: &[(&ToolRequest, ToolCallState<'_>)],
    emit: &Emitter,
) -> Option<Message> {
    let mut tool_responses = Message::user();
    for (request, state) in calls {
        if *state == ToolCallState::AwaitingUser {
            continue;
        }
        match &request.tool_call {
            Ok(tool_call) => {
                tracing::error!(tool_call_id = %request.id, tool_name = %tool_call.name, "tool call was not handled by any operation");
                add_error_response(
                    &mut tool_responses,
                    request,
                    &format!("Tool '{}' is not available.", tool_call.name),
                );
            }
            Err(error) => {
                tracing::error!(tool_call_id = %request.id, %error, "tool call was not handled by any operation");
                add_error_response(
                    &mut tool_responses,
                    request,
                    &format!(
                        "The tool call could not be parsed: {error}. Correct the arguments and try again."
                    ),
                );
            }
        }
    }
    (!tool_responses.content.is_empty()).then(|| emit.message(tool_responses))
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for UnhandledToolCallsOperation
where
    S: MaybeSend + MaybeSync + 'static,
    E: From<ConversationEffect> + MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        "unhandled_tool_calls"
    }

    async fn run(
        &self,
        _session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let open_calls: Vec<_> = open_tool_calls(messages_since_kickoff(conversation)?).collect();
        if let Some(tool_responses) = respond_to_unhandled_calls(&open_calls, emit) {
            return applied([E::from(ConversationEffect::AppendMessage(tool_responses))]);
        }
        if open_calls
            .iter()
            .any(|(_, state)| *state == ToolCallState::AwaitingUser)
        {
            return yielded();
        }
        not_applicable()
    }
}
