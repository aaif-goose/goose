use std::collections::{HashMap, HashSet};

use goose_provider_types::conversation::message::{
    ActionRequiredData, Message, MessageContent, ToolRequest, ToolResponse,
};
use goose_provider_types::permission::Permission;
use rmcp::model::{CallToolResult, ContentBlock};

use crate::operation::ConversationEffect;

pub const EXECUTABLE_KEY: &str = "goose.executable";

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ToolCallState<'a> {
    Responded(&'a ToolResponse),
    Unchecked,
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

/// Unanswered, locally executed requests permitted to run. Parsed calls require
/// approval; unchecked malformed calls are returned so executors can report parse
/// errors. Denied calls and calls awaiting confirmation are never returned.
pub fn pending_tool_requests(messages: &[Message]) -> Vec<&ToolRequest> {
    tool_calls(messages)
        .into_iter()
        .filter(|(request, state)| {
            !request.was_executed_externally()
                && (*state == ToolCallState::Executable
                    || (*state == ToolCallState::Unchecked && request.tool_call.is_err()))
        })
        .map(|(request, _)| request)
        .collect()
}

pub fn request_executable(request: &ToolRequest) -> Option<bool> {
    request
        .tool_meta
        .as_ref()
        .and_then(|meta| meta.get(EXECUTABLE_KEY))
        .and_then(serde_json::Value::as_bool)
}

pub(crate) fn mark_executable(tool_call_id: &str, executable: bool) -> ConversationEffect {
    ConversationEffect::PatchToolRequestMeta {
        tool_call_id: tool_call_id.to_string(),
        patch: serde_json::json!({ EXECUTABLE_KEY: executable }),
    }
}

pub(crate) fn add_error_response(tool_responses: &mut Message, request: &ToolRequest, text: &str) {
    tool_responses.add_tool_response_with_metadata(
        request.id.clone(),
        Ok(CallToolResult::error(vec![ContentBlock::text(text)])),
        request.metadata.as_ref(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose_provider_types::conversation::message::TOOL_META_EXTERNAL_DISPATCH_KEY;
    use rmcp::model::{CallToolRequestParams, ErrorData};
    use serde_json::json;

    #[test]
    fn pending_execution_respects_approval_responses_and_external_dispatch() {
        let mut calls = Message::assistant();
        for (id, executable) in [
            ("allowed", Some(true)),
            ("unchecked", None),
            ("blocked", Some(false)),
            ("waiting", Some(false)),
            ("user-answered", Some(false)),
            ("responded", Some(true)),
            ("external", Some(true)),
            ("malformed", None),
            ("blocked-malformed", Some(false)),
        ] {
            let call = if id.contains("malformed") {
                Err(ErrorData::invalid_params("bad arguments", None))
            } else {
                Ok(CallToolRequestParams::new("tool"))
            };
            let mut meta = json!({});
            if let Some(executable) = executable {
                meta[EXECUTABLE_KEY] = json!(executable);
            }
            if id == "external" {
                meta[TOOL_META_EXTERNAL_DISPATCH_KEY] = json!(true);
            }
            calls = calls.with_tool_request_with_metadata(id, call, None, Some(meta));
        }
        let messages = [
            calls,
            Message::assistant().with_action_required(
                "waiting",
                "tool".into(),
                Default::default(),
                None,
            ),
            Message::assistant().with_action_required(
                "user-answered",
                "tool".into(),
                Default::default(),
                None,
            ),
            crate::tool_approval::confirmation_response_message(
                "user-answered",
                Permission::AllowOnce,
            ),
            Message::user().with_tool_response("responded", Ok(CallToolResult::success(vec![]))),
        ];
        let pending = pending_tool_requests(&messages);
        assert_eq!(
            pending.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["allowed", "malformed"]
        );
    }

    #[test]
    fn duplicate_request_ids_execute_once() {
        let request = Message::assistant().with_tool_request_with_metadata(
            "call",
            Ok(CallToolRequestParams::new("tool")),
            None,
            Some(json!({EXECUTABLE_KEY: true})),
        );
        assert_eq!(pending_tool_requests(&[request.clone(), request]).len(), 1);
    }
}
