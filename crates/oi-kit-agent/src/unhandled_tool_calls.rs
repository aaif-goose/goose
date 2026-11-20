use anyhow::Result;
use async_trait::async_trait;
use oi_kit_provider_types::conversation::message::{Message, ToolRequest};
use oi_kit_provider_types::conversation::Conversation;
use oi_kit_provider_types::maybe_send::{MaybeSend, MaybeSync};

use crate::operation::{
    applied, messages_since_kickoff, not_applicable, Emitter, Operation, OperationResult,
};
use crate::tool_call_state::{add_error_response, pending_tool_requests};

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

fn respond_to_unhandled_calls<'a>(
    calls: impl Iterator<Item = &'a ToolRequest>,
    emit: &Emitter,
) -> Option<Message> {
    let mut tool_responses = Message::user();
    for request in calls {
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
    E: From<Message> + MaybeSend + 'static,
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
        let open_calls = pending_tool_requests(messages_since_kickoff(conversation)?);
        if let Some(tool_responses) = respond_to_unhandled_calls(open_calls, emit) {
            return applied([E::from(tool_responses)]);
        }
        not_applicable()
    }
}
