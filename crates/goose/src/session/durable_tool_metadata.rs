use std::collections::HashSet;

use crate::conversation::message::{Message, MessageContent, MessageMetadata};

pub(super) const LLM_OPERATION_NAME: &str = "llm";
pub(super) const ADVERTISED_TOOLS_NOTE: &str = "advertised_tools";

/// The advertised-tool list is only read while a tool request on that assistant
/// message is still unanswered, including after a resume that is waiting on
/// approval. It is not durable history.
pub(super) fn clear_advertised_tools(metadata: &mut MessageMetadata) {
    let Some(operations) = metadata.operations.as_mut() else {
        return;
    };
    let Some(notes) = operations.get_mut(LLM_OPERATION_NAME) else {
        return;
    };
    notes.remove(ADVERTISED_TOOLS_NOTE);
    if notes.is_empty() {
        operations.remove(LLM_OPERATION_NAME);
    }
    if operations.is_empty() {
        metadata.operations = None;
    }
}

pub(super) fn answered_tool_ids(messages: &[Message]) -> HashSet<String> {
    messages
        .iter()
        .flat_map(|message| message.get_tool_response_ids())
        .map(str::to_string)
        .collect()
}

pub(super) fn retains_advertised_tools(message: &Message, answered: &HashSet<String>) -> bool {
    message.role == rmcp::model::Role::Assistant
        && message.content.iter().any(|content| {
            let MessageContent::ToolRequest(request) = content else {
                return false;
            };
            !request.was_executed_externally() && !answered.contains(request.id.as_str())
        })
}

pub(super) fn metadata_for_storage(
    message: &Message,
    answered: &HashSet<String>,
) -> MessageMetadata {
    let mut metadata = message.metadata.clone();
    if !retains_advertised_tools(message, answered) {
        clear_advertised_tools(&mut metadata);
    }
    metadata
}
