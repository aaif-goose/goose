use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use rmcp::model::{ErrorCode, ErrorData, Role, Tool};
use tracing_futures::Instrument;

use crate::agents::final_output_tool::{
    structured_output_unsupported_message, FinalOutputTool, FINAL_OUTPUT_CONTINUATION_MESSAGE,
    FINAL_OUTPUT_NOT_ALONE_MESSAGE, FINAL_OUTPUT_TOOL_NAME,
};
use crate::agents::state_machine::extension_operation_host::request_was_advertised;
use crate::agents::state_machine::ops_recipe::command_error;
use crate::agents::state_machine::tool_hook_context::tool_span;
use crate::agents::state_machine::{
    applied, ends_turn, messages_since_kickoff, not_applicable, Emitter, GooseEffect, Operation,
    OperationResult,
};
use crate::conversation::message::{Message, MessageContent, ToolRequest};
use crate::conversation::Conversation;
use crate::providers::base::Provider;
use crate::session::Session;

fn final_output_tool(session: &Session) -> Result<Option<FinalOutputTool>> {
    session
        .recipe
        .as_ref()
        .and_then(|recipe| recipe.response.clone())
        .map(FinalOutputTool::try_new)
        .transpose()
        .map_err(|error| anyhow!(error))
}

fn pending_final_output_request(messages: &[Message]) -> Option<ToolRequest> {
    let answered: HashSet<&str> = messages
        .iter()
        .flat_map(Message::get_tool_response_ids)
        .collect();
    messages
        .iter()
        .filter(|message| message.role == Role::Assistant)
        .flat_map(|message| &message.content)
        .filter_map(MessageContent::as_tool_request)
        .find(|request| {
            !answered.contains(request.id.as_str())
                && !request.was_executed_externally()
                && request
                    .tool_call
                    .as_ref()
                    .is_ok_and(|tool_call| tool_call.name == FINAL_OUTPUT_TOOL_NAME)
                && request_was_advertised(messages, request)
        })
        .cloned()
}

fn assistant_block_bounds(messages: &[Message], message_index: usize) -> (usize, usize) {
    let start = (0..message_index)
        .rev()
        .take_while(|index| messages[*index].role == Role::Assistant)
        .last()
        .unwrap_or(message_index);
    let end = (message_index + 1..messages.len())
        .take_while(|index| messages[*index].role == Role::Assistant)
        .last()
        .map_or(message_index + 1, |index| index + 1);
    (start, end)
}

fn has_sibling_tool_calls(messages: &[Message], request_id: &str) -> bool {
    let Some(message_index) = messages.iter().position(|message| {
        message
            .content
            .iter()
            .filter_map(MessageContent::as_tool_request)
            .any(|request| request.id == request_id)
    }) else {
        return false;
    };
    let (start, end) = assistant_block_bounds(messages, message_index);
    messages[start..end]
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(MessageContent::as_tool_request)
        .any(|request| request.id != request_id && !request.was_executed_externally())
}

pub struct FinalOutputOperation {
    provider: Arc<dyn Provider>,
}

impl FinalOutputOperation {
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for FinalOutputOperation {
    fn name(&self) -> &'static str {
        "final_output"
    }

    async fn inference_tools(&self, session: &Session) -> Result<Vec<Tool>> {
        Ok(final_output_tool(session)?
            .as_ref()
            .map(FinalOutputTool::tool)
            .into_iter()
            .collect())
    }

    async fn prompt_parts(
        &self,
        session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        Ok(final_output_tool(session)?
            .map(|tool| ("final_output".to_string(), tool.system_prompt()))
            .into_iter()
            .collect())
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let Some(mut final_output) = final_output_tool(session)? else {
            return not_applicable();
        };

        if !self.provider.supports_builtin_tools() {
            return command_error(
                conversation,
                structured_output_unsupported_message(self.provider.get_name()),
                emit,
            );
        }

        let messages = messages_since_kickoff(conversation)?;
        if let Some(request) = pending_final_output_request(messages) {
            let mut response = Message::user();
            if has_sibling_tool_calls(messages, &request.id) {
                response.add_tool_response_with_metadata(
                    request.id,
                    Err(ErrorData::new(
                        ErrorCode::INVALID_REQUEST,
                        FINAL_OUTPUT_NOT_ALONE_MESSAGE,
                        None,
                    )),
                    request.metadata.as_ref(),
                );
                let response = emit.message(response);
                return applied([response.into()]);
            }

            let tool_call = request
                .tool_call
                .clone()
                .map_err(|error| anyhow!("final output tool call could not be parsed: {error}"))?;
            let span = tool_span(&tool_call.name, &request.id, &session.id);
            let result = final_output
                .execute_tool_call(tool_call)
                .instrument(span.clone())
                .await;
            let output = result.result.instrument(span.clone()).await;
            match &output {
                Ok(result) if result.is_error == Some(true) => {
                    span.record("error.type", "tool_error");
                }
                Err(_) => {
                    span.record("error.type", "tool_execution_error");
                }
                _ => {}
            }
            response.add_tool_response_with_metadata(request.id, output, request.metadata.as_ref());
            let response = emit.message(response);
            let Some(accepted) = final_output.final_output.take() else {
                return applied([response.into()]);
            };
            let accepted = emit.message(Message::assistant().with_text(accepted));
            return applied([response.into(), accepted.into()]);
        }

        if FinalOutputTool::successful_output(messages).is_some() {
            return not_applicable();
        }

        if ends_turn(messages) {
            let message = Message::user()
                .with_text(FINAL_OUTPUT_CONTINUATION_MESSAGE)
                .agent_only();
            let message = emit.message(message);
            return applied([message.into()]);
        }

        not_applicable()
    }
}
