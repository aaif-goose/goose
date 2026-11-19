use std::future::Future;

use futures::Stream;
use rmcp::model::{CallToolResult, ErrorData, ServerNotification};
use tokio::sync::mpsc;

use oi_kit_provider_types::conversation::message::Message;

pub type ToolResult<T> = Result<T, ErrorData>;

#[derive(Clone)]
pub struct ToolCallNotificationEmitter {
    sender: mpsc::Sender<ServerNotification>,
}

impl ToolCallNotificationEmitter {
    pub fn new(sender: mpsc::Sender<ServerNotification>) -> Self {
        Self { sender }
    }

    pub fn emit_best_effort(&self, notification: ServerNotification) {
        // Do not let a slow notification consumer delay tool execution.
        let _ = self.sender.try_send(notification);
    }
}

/// The result of a tool call with streams of the notifications and
/// action-required messages it produces while it runs.
pub struct ToolCallResult {
    pub result: Box<dyn Future<Output = ToolResult<CallToolResult>> + Send + Unpin>,
    pub notification_stream: Option<Box<dyn Stream<Item = ServerNotification> + Send + Unpin>>,
    pub action_required_stream: Option<Box<dyn Stream<Item = Message> + Send + Unpin>>,
}

impl From<ToolResult<CallToolResult>> for ToolCallResult {
    fn from(result: ToolResult<CallToolResult>) -> Self {
        Self {
            result: Box::new(futures::future::ready(result)),
            notification_stream: None,
            action_required_stream: None,
        }
    }
}
