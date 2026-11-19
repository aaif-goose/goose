use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use futures::{FutureExt, Stream, StreamExt};
use oi_kit_provider_types::conversation::message::{Message, ProviderMetadata, ToolRequest};
use rmcp::model::{CallToolResult, ServerNotification};

use super::{ToolCallResult, ToolResult};
use crate::events::AgentEvent;
use crate::operation::Emitter;

/// Derives a notification for the client from a finished call's result.
pub type ResultNotification = fn(&CallToolResult) -> Option<ServerNotification>;

#[derive(Default)]
struct BatchState {
    actions: Vec<Message>,
    response: Option<Message>,
}

impl BatchState {
    fn record(
        &mut self,
        request_id: &str,
        result: ToolResult<CallToolResult>,
        metadata: Option<&ProviderMetadata>,
    ) {
        let response = self
            .response
            .get_or_insert_with(|| Message::user().with_generated_id_if_missing());
        if !response.get_tool_response_ids().contains(&request_id) {
            response.add_tool_response_with_metadata(request_id, result, metadata);
        }
    }
}

/// Runs one model turn's tool calls concurrently. Each result is recorded the
/// moment it finishes, so a cancelled turn can still persist what completed.
#[derive(Default)]
pub struct ToolCallBatch {
    state: Arc<Mutex<BatchState>>,
    result_notification: Option<ResultNotification>,
}

impl ToolCallBatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_result_notification(mut self, notification: ResultNotification) -> Self {
        self.result_notification = Some(notification);
        self
    }

    /// The action-required messages and responses recorded so far.
    pub fn take(&self) -> (Vec<Message>, Option<Message>) {
        let mut state = self.state.lock().unwrap();
        (std::mem::take(&mut state.actions), state.response.take())
    }

    /// Forwards MCP notifications and action-required messages as they arrive and
    /// returns the action-required messages plus one message with every response.
    pub async fn run(
        &self,
        calls: Vec<(ToolRequest, ToolCallResult)>,
        emit: &Emitter,
    ) -> Result<(Vec<Message>, Message)> {
        let streams = calls.into_iter().map(|(request, result)| {
            let id = request.id.clone();
            tool_stream(self.recording(&request, result)).map(move |item| (id.clone(), item))
        });
        let mut combined = futures::stream::select_all(streams);
        while let Some((request_id, item)) = combined.next().await {
            match item {
                ToolStreamItem::Result(Ok(result)) => {
                    if let Some(notification) = self.result_notification.and_then(|f| f(&result)) {
                        emit.emit(AgentEvent::McpNotification((request_id, notification)));
                    }
                }
                ToolStreamItem::Result(Err(_)) => {}
                ToolStreamItem::Notification(notification) => {
                    emit.emit(AgentEvent::McpNotification((request_id, notification)));
                }
                ToolStreamItem::ActionRequired(message) => {
                    let message = message.with_generated_id_if_missing();
                    self.state.lock().unwrap().actions.push(message.clone());
                    emit.message(message);
                }
            }
        }

        let (actions, response) = self.take();
        let response = response.ok_or_else(|| anyhow!("tool batch produced no responses"))?;
        Ok((actions, response))
    }

    fn recording(&self, request: &ToolRequest, result: ToolCallResult) -> ToolCallResult {
        let state = Arc::clone(&self.state);
        let tool_call_id = request.id.clone();
        let metadata = request.metadata.clone();
        let inner = result.result;
        ToolCallResult {
            result: Box::new(
                async move {
                    let result = inner.await;
                    state
                        .lock()
                        .unwrap()
                        .record(&tool_call_id, result.clone(), metadata.as_ref());
                    result
                }
                .boxed(),
            ),
            notification_stream: result.notification_stream,
            action_required_stream: result.action_required_stream,
        }
    }
}

enum ToolStreamItem {
    ActionRequired(Message),
    Notification(ServerNotification),
    Result(ToolResult<CallToolResult>),
}

type ToolStream = Pin<Box<dyn Stream<Item = ToolStreamItem> + Send>>;

fn tool_stream(result: ToolCallResult) -> ToolStream {
    let notifications = result
        .notification_stream
        .unwrap_or_else(|| Box::new(futures::stream::empty()));
    let actions = result
        .action_required_stream
        .unwrap_or_else(|| Box::new(futures::stream::empty()));
    let done: Pin<Box<dyn Future<Output = _> + Send>> = Box::pin(result.result);
    Box::pin(futures::stream::unfold(
        Some((notifications, actions, done)),
        |state| async move {
            let (mut notifications, mut actions, mut done) = state?;
            tokio::select! {
                Some(message) = actions.next() => {
                    Some((ToolStreamItem::ActionRequired(message), Some((notifications, actions, done))))
                }
                Some(notification) = notifications.next() => {
                    Some((ToolStreamItem::Notification(notification), Some((notifications, actions, done))))
                }
                result = &mut done => Some((ToolStreamItem::Result(result), None)),
            }
        },
    ))
}
