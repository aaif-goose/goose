//! Exposes extension capabilities and executes requests that belong to them.

use anyhow::{anyhow, Result};
use futures::FutureExt;
use goose_agent::extension_manager::{PendingCall, ToolCallBatch};
use rmcp::model::{CallToolResult, ContentBlock};

use crate::agents::state_machine::{applied, Emitter, GooseEffect, OperationResult};
use crate::agents::tool_execution::ToolCallResult;
use crate::agents::AgentEvent;
use crate::conversation::message::{Message, ToolRequest};
use crate::session::Session;

use super::tool_hook_context::tool_span;
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
