use futures::Stream;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::mpsc;

use std::path::PathBuf;

use crate::container::Container;
use crate::extension_manager::ExtensionLease;
use goose_provider_types::conversation::message::Message;
use rmcp::model::ServerNotification;

type ToolResult<T> = Result<T, rmcp::model::ErrorData>;

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

/// Context passed through the tool call dispatch chain.
#[derive(Clone)]
pub struct ToolCallContext {
    pub session_id: String,
    pub working_dir: Option<PathBuf>,
    pub tool_call_request_id: Option<String>,
    container: Option<Container>,
    extension_lease: Option<Arc<ExtensionLease>>,
    notification_emitter: Option<ToolCallNotificationEmitter>,
}

impl ToolCallContext {
    pub fn new(
        session_id: String,
        working_dir: Option<PathBuf>,
        tool_call_request_id: Option<String>,
    ) -> Self {
        Self {
            session_id,
            working_dir,
            tool_call_request_id,
            container: None,
            extension_lease: None,
            notification_emitter: None,
        }
    }

    pub fn with_container(mut self, container: Option<Container>) -> Self {
        self.container = container;
        self
    }

    pub fn container(&self) -> Option<&Container> {
        self.container.as_ref()
    }

    pub fn with_extension_lease(mut self, extension_lease: Arc<ExtensionLease>) -> Self {
        self.extension_lease = Some(extension_lease);
        self
    }

    pub fn extension_lease(&self) -> Option<&Arc<ExtensionLease>> {
        self.extension_lease.as_ref()
    }

    pub fn working_dir_str(&self) -> Option<&str> {
        self.working_dir.as_ref().and_then(|p| p.to_str())
    }

    pub fn with_notification_emitter(
        mut self,
        notification_emitter: ToolCallNotificationEmitter,
    ) -> Self {
        self.notification_emitter = Some(notification_emitter);
        self
    }

    pub fn notification_emitter(&self) -> Option<&ToolCallNotificationEmitter> {
        self.notification_emitter.as_ref()
    }
}

// ToolCallResult combines the result of a tool call with an optional notification stream that
// can be used to receive notifications from the tool.
pub struct ToolCallResult {
    pub result: Box<dyn Future<Output = ToolResult<rmcp::model::CallToolResult>> + Send + Unpin>,
    pub notification_stream: Option<Box<dyn Stream<Item = ServerNotification> + Send + Unpin>>,
    pub action_required_stream: Option<Box<dyn Stream<Item = Message> + Send + Unpin>>,
}

impl From<ToolResult<rmcp::model::CallToolResult>> for ToolCallResult {
    fn from(result: ToolResult<rmcp::model::CallToolResult>) -> Self {
        Self {
            result: Box::new(futures::future::ready(result)),
            notification_stream: None,
            action_required_stream: None,
        }
    }
}
