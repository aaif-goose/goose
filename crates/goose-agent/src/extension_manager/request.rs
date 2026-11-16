use goose_provider_types::conversation::message::{
    ActionRequiredData, Message, MessageContent, ToolRequest,
};
use rmcp::model::Role;

#[derive(Clone, Eq, PartialEq)]
/// A locally executable request or a malformed request requiring an error response.
pub enum ToolDisposition {
    Execute,
    ParseError(String),
}

/// Classify unanswered requests using the shared approval and external-dispatch state.
pub fn pending_tool_requests(messages: &[Message]) -> Vec<(ToolRequest, ToolDisposition)> {
    crate::tool_call_state::pending_tool_requests(messages)
        .into_iter()
        .map(|request| {
            let disposition = match &request.tool_call {
                Ok(_) => ToolDisposition::Execute,
                Err(error) => ToolDisposition::ParseError(error.to_string()),
            };
            (request.clone(), disposition)
        })
        .collect()
}

/// Validate a request against the tool names persisted on its inference message.
/// Operation-generated messages have no inference metadata and are accepted.
pub fn request_was_advertised(
    messages: &[Message],
    request: &ToolRequest,
    operation: &str,
    advertised_tools_note: &str,
) -> bool {
    let Some(tool_call) = request.tool_call.as_ref().ok() else {
        return true;
    };
    let Some(message) = messages.iter().find(|message| {
        message.role == Role::Assistant
            && message.content.iter().any(|content| {
                content
                    .as_tool_request()
                    .is_some_and(|item| item.id == request.id)
            })
    }) else {
        return false;
    };
    if message.metadata.inference.is_none() {
        return true;
    }
    message
        .metadata
        .operation_note(operation, advertised_tools_note)
        .and_then(serde_json::Value::as_array)
        .is_some_and(|tools| {
            tools
                .iter()
                .any(|tool| tool.as_str() == Some(tool_call.name.as_ref()))
        })
}

/// Only operation-generated requests with no approval history may acquire a lease
/// without an inference snapshot. An empty request set is never eligible.
pub fn requests_can_resolve_a_lease<'a>(
    messages: &[Message],
    requests: impl IntoIterator<Item = &'a ToolRequest>,
) -> bool {
    let mut requests = requests.into_iter().peekable();
    requests.peek().is_some()
        && requests.all(|request| {
            request_was_generated_by_operation(messages, request)
                && !request_has_approval_history(messages, request)
        })
}

fn request_was_generated_by_operation(messages: &[Message], request: &ToolRequest) -> bool {
    messages.iter().any(|message| {
        message.role == Role::Assistant
            && message.metadata.inference.is_none()
            && message.content.iter().any(|content| {
                content
                    .as_tool_request()
                    .is_some_and(|item| item.id == request.id)
            })
    })
}

fn request_has_approval_history(messages: &[Message], request: &ToolRequest) -> bool {
    messages.iter().any(|message| {
        message.content.iter().any(|content| match content {
            MessageContent::ActionRequired(action) => matches!(
                &action.data,
                ActionRequiredData::ToolConfirmation { id, .. }
                    | ActionRequiredData::ToolConfirmationResponse { id, .. }
                    if id == &request.id
            ),
            _ => false,
        })
    })
}
