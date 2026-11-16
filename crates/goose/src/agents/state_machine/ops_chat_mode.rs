use std::collections::HashSet;

use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ContentBlock, Role};

use crate::agents::state_machine::ops_toolcalling::request_was_advertised;
use crate::agents::state_machine::{
    applied, messages_since_kickoff, not_applicable, Emitter, GooseEffect, Operation,
    OperationResult,
};
use crate::agents::tool_execution::CHAT_MODE_TOOL_SKIPPED_RESPONSE;
use crate::config::GooseMode;
use crate::conversation::message::{Message, MessageContent};
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
        session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        if session.goose_mode != GooseMode::Chat {
            return Ok(Vec::new());
        }
        Ok(vec![(
            "chat_mode".to_string(),
            "Right now you are in the chat only mode, no access to any tool use and system."
                .to_string(),
        )])
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        if session.goose_mode != GooseMode::Chat {
            return not_applicable();
        }
        let messages = messages_since_kickoff(conversation)?;
        let answered: HashSet<&str> = messages
            .iter()
            .flat_map(Message::get_tool_response_ids)
            .collect();

        let mut response = Message::user();
        for request in messages
            .iter()
            .filter(|message| message.role == Role::Assistant)
            .flat_map(|message| &message.content)
            .filter_map(MessageContent::as_tool_request)
            .filter(|request| {
                !answered.contains(request.id.as_str())
                    && !request.was_executed_externally()
                    && request_was_advertised(messages, request)
            })
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
