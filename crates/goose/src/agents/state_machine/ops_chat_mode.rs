use anyhow::Result;
use async_trait::async_trait;
use goose_agent::tool_call_state::open_tool_requests;
use rmcp::model::{CallToolResult, ContentBlock};

use crate::agents::state_machine::ops_toolcalling::request_was_advertised;
use crate::agents::state_machine::{
    applied, messages_since_kickoff, not_applicable, Emitter, GooseEffect, Operation,
    OperationResult,
};
use crate::agents::tool_execution::CHAT_MODE_TOOL_SKIPPED_RESPONSE;
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::session::Session;

pub struct ChatModeOperation;

#[async_trait]
impl Operation<Session, GooseEffect> for ChatModeOperation {
    fn name(&self) -> &'static str {
        "chat_mode"
    }

    async fn prompt_parts(
        &self,
        _session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        Ok(vec![(
            "chat_mode".to_string(),
            "Right now you are in the chat only mode, no access to any tool use and system."
                .to_string(),
        )])
    }

    async fn run(
        &self,
        _session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        let mut response = Message::user();
        for request in open_tool_requests(messages)
            .into_iter()
            .filter(|request| request_was_advertised(messages, request))
        {
            let result = match &request.tool_call {
                Ok(_) => CallToolResult::success(vec![ContentBlock::text(
                    CHAT_MODE_TOOL_SKIPPED_RESPONSE,
                )]),
                Err(parse_error) => CallToolResult::error(vec![ContentBlock::text(format!(
                    "The tool call could not be parsed: {parse_error}."
                ))]),
            };
            response.add_tool_response_with_metadata(
                request.id.clone(),
                Ok(result),
                request.metadata.as_ref(),
            );
        }
        if response.content.is_empty() {
            return not_applicable();
        }
        let response = emit.message(response);
        applied([response.into()])
    }
}
