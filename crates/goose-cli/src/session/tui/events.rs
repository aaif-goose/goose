use crate::session::tui::model_picker::ModelSelection;
use goose::conversation::message::Message;
use rmcp::model::Role;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::session) enum ContextUsageSource {
    ProviderReported,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::session) struct ContextSnapshot {
    pub current_tokens: Option<u64>,
    pub accumulated_tokens: Option<u64>,
    pub context_limit: Option<u64>,
    pub usage_source: ContextUsageSource,
}

impl ContextSnapshot {
    pub(in crate::session) fn from_values(
        current_tokens: Option<i32>,
        accumulated_tokens: Option<i32>,
        context_limit: Option<usize>,
    ) -> Self {
        let current_tokens = current_tokens.and_then(|value| u64::try_from(value).ok());
        let accumulated_tokens = accumulated_tokens.and_then(|value| u64::try_from(value).ok());
        let context_limit = context_limit
            .filter(|value| *value > 0)
            .map(|value| value as u64);
        let usage_source = if current_tokens.is_some() || accumulated_tokens.is_some() {
            ContextUsageSource::ProviderReported
        } else {
            ContextUsageSource::Unavailable
        };

        Self {
            current_tokens,
            accumulated_tokens,
            context_limit,
            usage_source,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::session) struct ToolActivityEvent {
    pub tool_name: String,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(in crate::session) enum SessionEvent {
    AssistantDelta(String),
    AssistantAnswer(String),
    ToolActivity(ToolActivityEvent),
    Status(String),
    ResumeUi,
    ModelChanged {
        provider: String,
        model: String,
        is_local: bool,
    },
    Error(String),
    TurnFinished,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(super) enum UiAction {
    Submit(String),
    OpenModelPicker,
    BrowseCatalog(String),
    DownloadModel(ModelSelection),
    SelectModel(ModelSelection),
    Cancel,
    Resize,
    Quit,
}

pub(super) fn assistant_message_event(message: &Message) -> Option<SessionEvent> {
    if message.role != Role::Assistant || !message.is_user_visible() {
        return None;
    }

    let text = message.user_visible_content().as_concat_text();
    (!text.trim().is_empty()).then_some(SessionEvent::AssistantAnswer(text))
}

pub(in crate::session) fn message_events(message: &Message) -> Vec<SessionEvent> {
    use goose::conversation::message::MessageContent;

    if !message.is_user_visible() {
        return Vec::new();
    }

    let assistant_visible = message.role == Role::Assistant;
    let mut events = Vec::new();
    for content in message.user_visible_content().content {
        match content {
            MessageContent::Text(text) if assistant_visible && !text.text.trim().is_empty() => {
                events.push(SessionEvent::AssistantDelta(text.text));
            }
            MessageContent::ToolRequest(request) if assistant_visible => {
                let tool_name = request
                    .tool_call
                    .as_ref()
                    .map(|call| call.name.to_string())
                    .unwrap_or_else(|_| "tool".into());
                // Per plan: show concise activity without sensitive tool arguments.
                let detail = None;
                events.push(SessionEvent::ToolActivity(ToolActivityEvent {
                    tool_name,
                    status: "running".into(),
                    detail,
                }));
            }
            MessageContent::ToolResponse(response) => {
                let status = if response.tool_result.is_ok() {
                    "completed"
                } else {
                    "failed"
                };
                events.push(SessionEvent::ToolActivity(ToolActivityEvent {
                    tool_name: "tool".into(),
                    status: status.into(),
                    detail: None,
                }));
            }
            _ => {}
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose::conversation::message::Message;
    use goose::conversation::message::MessageContent;
    use rmcp::model::CallToolRequestParams;

    #[test]
    fn visible_assistant_message_becomes_answer_event() {
        let message = Message::assistant().with_text("The result is 42.");

        assert_eq!(
            assistant_message_event(&message),
            Some(SessionEvent::AssistantAnswer("The result is 42.".into()))
        );
    }

    #[test]
    fn hidden_assistant_message_does_not_become_an_answer_event() {
        let message = Message::assistant()
            .with_text("private reasoning")
            .with_visibility(false, true);

        assert_eq!(assistant_message_event(&message), None);
    }

    #[test]
    fn activity_and_status_use_secondary_event_variants() {
        let activity = ToolActivityEvent {
            tool_name: "shell".into(),
            status: "completed".into(),
            detail: Some("cargo test".into()),
        };

        assert!(matches!(
            SessionEvent::ToolActivity(activity),
            SessionEvent::ToolActivity(_)
        ));
        assert!(matches!(
            SessionEvent::Status("Working".into()),
            SessionEvent::Status(_)
        ));
    }

    #[test]
    fn visible_assistant_chunk_becomes_one_delta_event() {
        let message = Message::assistant().with_text("A streamed chunk.");

        assert_eq!(
            message_events(&message),
            vec![SessionEvent::AssistantDelta("A streamed chunk.".into())]
        );
    }

    #[test]
    fn hidden_assistant_chunk_produces_no_live_event() {
        let message = Message::assistant()
            .with_text("private reasoning")
            .with_visibility(false, true);

        assert!(message_events(&message).is_empty());
    }

    #[test]
    fn tool_request_and_response_become_activity_events() {
        let request = Message::assistant().with_content(MessageContent::tool_request(
            "call-1",
            Ok(CallToolRequestParams::new("shell")),
        ));
        let response = Message::user().with_content(MessageContent::tool_response(
            "call-1",
            Err(rmcp::model::ErrorData::new(
                rmcp::model::ErrorCode::INTERNAL_ERROR,
                "tool failed",
                None,
            )),
        ));

        assert_eq!(
            message_events(&request),
            vec![SessionEvent::ToolActivity(ToolActivityEvent {
                tool_name: "shell".into(),
                status: "running".into(),
                detail: None,
            })]
        );
        assert_eq!(
            message_events(&response),
            vec![SessionEvent::ToolActivity(ToolActivityEvent {
                tool_name: "tool".into(),
                status: "failed".into(),
                detail: None,
            })]
        );
    }

    #[test]
    fn tool_request_arguments_are_not_exposed_in_detail() {
        let mut arguments = serde_json::Map::new();
        arguments.insert(
            "command".to_string(),
            serde_json::json!("cat ~/.ssh/id_rsa"),
        );
        let message = Message::assistant().with_content(MessageContent::tool_request(
            "call-1",
            Ok(CallToolRequestParams::new("shell").with_arguments(arguments)),
        ));

        let events = message_events(&message);
        assert_eq!(events.len(), 1);
        match &events[0] {
            SessionEvent::ToolActivity(activity) => {
                // Regression: detail must be None so arguments are never exposed in TUI.
                assert!(activity.detail.is_none());
            }
            _ => panic!("Expected ToolActivity event, got {:?}", events[0]),
        }
    }

    #[test]
    fn context_snapshot_keeps_reported_usage_and_unknown_limits_distinct() {
        let reported = ContextSnapshot::from_values(Some(128), Some(512), Some(4096));
        assert_eq!(reported.current_tokens, Some(128));
        assert_eq!(reported.accumulated_tokens, Some(512));
        assert_eq!(reported.context_limit, Some(4096));
        assert_eq!(reported.usage_source, ContextUsageSource::ProviderReported);

        let partial = ContextSnapshot::from_values(Some(128), None, None);
        assert_eq!(partial.current_tokens, Some(128));
        assert_eq!(partial.accumulated_tokens, None);
        assert_eq!(partial.context_limit, None);
        assert_eq!(partial.usage_source, ContextUsageSource::ProviderReported);

        let unavailable = ContextSnapshot::from_values(None, None, None);
        assert_eq!(unavailable.current_tokens, None);
        assert_eq!(unavailable.usage_source, ContextUsageSource::Unavailable);
    }
}
