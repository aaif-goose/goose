use std::collections::HashSet;

use oi_kit_provider_types::conversation::message::{Message, ToolRequest};
use oi_kit_provider_types::conversation::unanswered_tool_requests;
use rmcp::model::{CallToolResult, ContentBlock};

pub const EXECUTABLE_KEY: &str = "goose.executable";

pub fn pending_tool_requests(messages: &[Message]) -> impl Iterator<Item = &ToolRequest> {
    let mut seen = HashSet::new();
    unanswered_tool_requests(messages).filter(move |request| seen.insert(request.id.as_str()))
}

pub fn request_executable(request: &ToolRequest) -> Option<bool> {
    request
        .tool_meta
        .as_ref()
        .and_then(|meta| meta.get(EXECUTABLE_KEY))
        .and_then(serde_json::Value::as_bool)
}

pub(crate) fn add_error_response(tool_responses: &mut Message, request: &ToolRequest, text: &str) {
    tool_responses.add_tool_response_with_metadata(
        request.id.clone(),
        Ok(CallToolResult::error(vec![ContentBlock::text(text)])),
        request.metadata.as_ref(),
    );
}
