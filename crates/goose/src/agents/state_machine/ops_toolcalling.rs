//! Exposes extension capabilities and executes requests that belong to them.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use futures::FutureExt;
use goose_agent::extension_manager::{PendingCall, ToolCallBatch};
use goose_agent::tool_call_state::pending_tool_requests as executable_tool_requests;
use rmcp::model::{CallToolResult, ContentBlock, Role};

use crate::agents::extension::ExtensionInfo;
use crate::agents::state_machine::ops_llm::{ADVERTISED_TOOLS_NOTE, LLM_OPERATION_NAME};
use crate::agents::state_machine::{
    applied, messages_since_kickoff, Emitter, GooseEffect, OperationResult,
};
use crate::agents::tool_execution::ToolCallResult;
use crate::agents::tool_snapshot::ToolSnapshot;
use crate::agents::AgentEvent;
use crate::conversation::message::{ActionRequiredData, Message, MessageContent, ToolRequest};
use crate::conversation::Conversation;
use crate::session::Session;

pub(super) const EXPIRED_APPROVAL_RESPONSE: &str =
    "Tool approval expired because the advertised tools are no longer available. Request the tool again.";
use std::sync::{Arc, Mutex as StdMutex};
use tracing_futures::Instrument;

fn platform_notification(result: &CallToolResult) -> Option<rmcp::model::ServerNotification> {
    let notification = result.meta.as_ref()?.0.get("platform_notification")?;
    let method = notification.get("method")?.as_str()?;
    Some(rmcp::model::ServerNotification::CustomNotification(
        rmcp::model::CustomNotification::new(
            method.to_string(),
            notification.get("params").cloned(),
        ),
    ))
}

pub(super) fn tool_span(tool_name: &str, tool_call_id: &str, session_id: &str) -> tracing::Span {
    tracing::info_span!(
        target: "goose::state_machine",
        "execute_tool",
        "gen_ai.operation.name" = "execute_tool",
        "gen_ai.tool.name" = %tool_name,
        "gen_ai.tool.call.id" = %tool_call_id,
        "gen_ai.conversation.id" = %session_id,
        "gen_ai.tool.call.arguments" = tracing::field::Empty,
        "gen_ai.tool.call.result" = tracing::field::Empty,
        "error.type" = tracing::field::Empty,
        session.id = %session_id,
    )
}

fn with_recording(
    batch: &ToolCallBatch,
    result: ToolCallResult,
    span: tracing::Span,
    request: &ToolRequest,
) -> ToolCallResult {
    let batch = batch.clone();
    let tool_call_id = request.id.clone();
    let metadata = request.metadata.clone();
    let future = async move {
        let result =
            crate::agents::large_response_handler::process_tool_response(result.result.await);
        batch.record(&tool_call_id, result.clone(), metadata.as_ref());
        crate::agents::gen_ai_telemetry::record_tool_result(&tracing::Span::current(), &result);
        match &result {
            Ok(result) if result.is_error == Some(true) => {
                tracing::Span::current().record("error.type", "tool_error");
            }
            Err(_) => {
                tracing::Span::current().record("error.type", "tool_execution_error");
            }
            _ => {}
        }
        result
    }
    .instrument(span);
    ToolCallResult {
        notification_stream: result.notification_stream,
        action_required_stream: result.action_required_stream,
        result: Box::new(future.boxed()),
    }
}

pub(super) fn advertised_working_dir(
    tool_snapshot: &StdMutex<Option<Arc<ToolSnapshot>>>,
    session: &Session,
) -> PathBuf {
    tool_snapshot
        .lock()
        .expect("tool snapshot unavailable")
        .as_ref()
        .filter(|tools| tools.scope_id() == session.id)
        .and_then(|tools| tools.working_dir())
        .map_or_else(|| session.working_dir.clone(), Path::to_path_buf)
}

fn advertised_session(tools: &ToolSnapshot, session: &Session) -> Session {
    let mut session = session.clone();
    if let Some(working_dir) = tools.working_dir() {
        session.working_dir = working_dir.to_path_buf();
    }
    session
}

pub(super) fn pending_tool_requests(messages: &[Message]) -> Vec<(ToolRequest, ToolDisposition)> {
    executable_tool_requests(messages)
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

pub(super) fn pending_advertised_tool_requests(
    messages: &[Message],
) -> Vec<(ToolRequest, ToolDisposition)> {
    pending_tool_requests(messages)
        .into_iter()
        .filter(|(request, _)| request_was_advertised(messages, request))
        .collect()
}

pub(super) fn request_was_advertised(messages: &[Message], request: &ToolRequest) -> bool {
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
        .operation_note(LLM_OPERATION_NAME, ADVERTISED_TOOLS_NOTE)
        .and_then(serde_json::Value::as_array)
        .is_some_and(|tools| {
            tools
                .iter()
                .any(|tool| tool.as_str() == Some(tool_call.name.as_ref()))
        })
}

pub(super) fn requests_can_resolve_tools<'a>(
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

#[derive(Clone, Eq, PartialEq)]
pub(super) enum ToolDisposition {
    Execute,
    ParseError(String),
}

/// The extension block, as a header part plus one part per extension. Prompt
/// extras are joined with a blank line, so this renders the same text as one
/// combined part while keeping each extension's tokens attributable.
pub(super) fn extension_prompt_parts(
    extensions: &[ExtensionInfo],
    defer_tools: bool,
) -> Vec<(String, String)> {
    let mut parts = vec![(
        "extensions".to_string(),
        "# Extensions\n\n\
         Extensions provide additional tools and context from different data sources and applications.\n\
         You can dynamically enable or disable extensions as needed to help complete tasks.\n\n\
         Because you dynamically load extensions, your conversation history may refer to interactions with extensions that are not currently active. The currently active extensions are below."
            .to_string(),
    )];
    if !defer_tools {
        parts[0].1.push_str(
            " Each of these extensions provides tools that are in your tool specification.",
        );
    }
    for extension in extensions {
        let mut lines = vec![format!("## {}", extension.name)];
        if extension.has_resources {
            lines.push(format!("{} supports resources.", extension.name));
        }
        if !extension.instructions.is_empty() {
            lines.push(format!("### Instructions\n{}", extension.instructions));
        }
        parts.push((format!("extension:{}", extension.name), lines.join("\n\n")));
    }
    parts
}

pub(super) struct ToolExecution {
    batch: ToolCallBatch,
}

impl ToolExecution {
    pub(super) fn new() -> Self {
        Self {
            batch: ToolCallBatch::default(),
        }
    }

    fn take_batch(&self) -> (Vec<Message>, Option<Message>) {
        self.batch.take()
    }

    pub(super) fn finalize_cancellation(&self, emit: &Emitter) -> Vec<GooseEffect> {
        let (actions, response) = self.take_batch();
        if let Some(response) = &response {
            emit.emit(AgentEvent::Message(response.user_visible_content()));
        }
        actions
            .into_iter()
            .chain(response)
            .map(GooseEffect::from)
            .collect()
    }

    pub(super) async fn run(
        &self,
        calls: Vec<(ToolRequest, ToolCallResult)>,
        session: &Session,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let mut pending = Vec::new();
        for (request, result) in calls {
            let tool_call = request
                .tool_call
                .as_ref()
                .expect("selected calls are parsed");
            let span = tool_span(&tool_call.name, &request.id, &session.id);
            crate::agents::gen_ai_telemetry::record_tool_arguments(&span, tool_call);
            let result = with_recording(&self.batch, result, span, &request);

            pending.push(PendingCall {
                request_id: request.id,
                metadata: request.metadata,
                call: result,
            });
        }
        self.batch
            .run(pending, emit, &|request_id, output| {
                if let Some(notification) = output.as_ref().ok().and_then(platform_notification) {
                    emit.emit(AgentEvent::McpNotification((
                        request_id.to_string(),
                        notification,
                    )));
                }
            })
            .await;

        let (actions, response) = self.take_batch();
        let response = response.ok_or_else(|| anyhow!("tool batch produced no responses"))?;
        emit.emit(AgentEvent::Message(response.user_visible_content()));
        applied(actions.into_iter().chain([response]).map(GooseEffect::from))
    }
}
