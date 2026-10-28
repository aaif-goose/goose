use std::collections::{HashMap, HashSet};

use goose_provider_types::conversation::message::{
    ActionRequiredData, Message, MessageContent, ToolRequest, ToolResponse,
};
use goose_provider_types::permission::Permission;
use rmcp::model::{CallToolResult, ContentBlock};

use crate::operation::ConversationEffect;

pub const EXECUTABLE_KEY: &str = "goose.executable";
pub const APPROVED_KEY: &str = "goose.approved";

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ToolCallState<'a> {
    Responded(&'a ToolResponse),
    Unchecked,
    Approved,
    Executable,
    Blocked,
    AwaitingUser,
    UserAnswered(&'a Permission),
}

impl ToolCallState<'_> {
    pub(crate) fn is_open(&self) -> bool {
        !matches!(self, ToolCallState::Responded(_))
    }
}

pub(crate) fn tool_calls(messages: &[Message]) -> Vec<(&ToolRequest, ToolCallState<'_>)> {
    let mut request_ids = HashSet::new();
    let mut requests = Vec::new();
    let mut tool_responses_by_id = HashMap::new();
    let mut confirmation_requests = HashSet::new();
    let mut confirmations = HashMap::new();
    for content in messages.iter().flat_map(|message| &message.content) {
        match content {
            MessageContent::ToolRequest(request) => {
                if request_ids.insert(request.id.as_str()) {
                    requests.push(request);
                }
            }
            MessageContent::ToolResponse(response) => {
                tool_responses_by_id.insert(response.id.as_str(), response);
            }
            MessageContent::ActionRequired(action) => match &action.data {
                ActionRequiredData::ToolConfirmation { id, .. } => {
                    confirmation_requests.insert(id.as_str());
                }
                ActionRequiredData::ToolConfirmationResponse { id, permission } => {
                    confirmations.insert(id.as_str(), permission);
                }
                _ => {}
            },
            _ => {}
        }
    }

    requests
        .into_iter()
        .map(|request| {
            let id = request.id.as_str();
            let state = match (tool_responses_by_id.get(id), request_executable(request)) {
                (Some(response), _) => ToolCallState::Responded(response),
                (None, Some(true)) => ToolCallState::Executable,
                (None, _) if tool_meta_flag(request, APPROVED_KEY) == Some(true) => {
                    ToolCallState::Approved
                }
                (None, None) => ToolCallState::Unchecked,
                (None, Some(false)) if !confirmation_requests.contains(id) => {
                    ToolCallState::Blocked
                }
                (None, Some(false)) => match confirmations.get(id) {
                    Some(permission) => ToolCallState::UserAnswered(permission),
                    None => ToolCallState::AwaitingUser,
                },
            };
            (request, state)
        })
        .collect()
}

fn tool_meta_flag(request: &ToolRequest, key: &str) -> Option<bool> {
    request
        .tool_meta
        .as_ref()
        .and_then(|meta| meta.get(key))
        .and_then(serde_json::Value::as_bool)
}

pub fn request_executable(request: &ToolRequest) -> Option<bool> {
    tool_meta_flag(request, EXECUTABLE_KEY)
}

pub(crate) fn mark_executable(tool_call_id: &str, executable: bool) -> ConversationEffect {
    ConversationEffect::PatchToolRequestMeta {
        tool_call_id: tool_call_id.to_string(),
        patch: serde_json::json!({ EXECUTABLE_KEY: executable }),
    }
}

pub(crate) fn mark_approved(tool_call_id: &str) -> ConversationEffect {
    ConversationEffect::PatchToolRequestMeta {
        tool_call_id: tool_call_id.to_string(),
        patch: serde_json::json!({ APPROVED_KEY: true, EXECUTABLE_KEY: false }),
    }
}

pub(crate) fn add_error_response(tool_responses: &mut Message, request: &ToolRequest, text: &str) {
    tool_responses.add_tool_response_with_metadata(
        request.id.clone(),
        Ok(CallToolResult::error(vec![ContentBlock::text(text)])),
        request.metadata.as_ref(),
    );
}
