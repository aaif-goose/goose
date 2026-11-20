use std::sync::Arc;

use std::path::PathBuf;

use crate::agents::container::Container;
pub use crate::agents::extension_runtime::{ToolCallNotificationEmitter, ToolCallResult};

#[cfg(feature = "code-mode")]
#[async_trait::async_trait]
pub(crate) trait ToolAccess: Send + Sync {
    async fn tools(&self) -> Vec<rmcp::model::Tool>;
    async fn call(
        &self,
        call: rmcp::model::CallToolRequestParams,
        request: crate::agents::extension_manager::CallRequest,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Result<ToolCallResult, rmcp::model::ErrorData>;
}

/// Context passed through the tool call dispatch chain.
#[derive(Clone)]
pub struct ToolCallContext {
    pub session_id: String,
    pub working_dir: Option<PathBuf>,
    pub tool_call_request_id: Option<String>,
    container: Option<Container>,
    tool_snapshot: Option<Arc<ToolSnapshot>>,
    notification_emitter: Option<ToolCallNotificationEmitter>,
    services: Option<Arc<StateMachineServices>>,
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
            tool_snapshot: None,
            services: None,
            notification_emitter: None,
        }
    }

    pub(crate) fn with_container(mut self, container: Option<Container>) -> Self {
        self.container = container;
        self
    }

    pub(crate) fn container(&self) -> Option<&Container> {
        self.container.as_ref()
    }

    pub(crate) fn with_services(mut self, services: Arc<StateMachineServices>) -> Self {
        self.services = Some(services);
        self
    }

    pub(crate) fn with_tool_snapshot(mut self, tool_snapshot: Arc<ToolSnapshot>) -> Self {
        self.tool_snapshot = Some(tool_snapshot);
        self
    }

    pub(crate) fn tool_snapshot(&self) -> Option<&Arc<ToolSnapshot>> {
        self.tool_snapshot.as_ref()
    }

    #[cfg(feature = "code-mode")]
    pub(crate) fn tools(&self) -> &dyn ToolAccess {
        self
    }

    pub fn working_dir_str(&self) -> Option<&str> {
        self.working_dir.as_ref().and_then(|p| p.to_str())
    }

    pub(crate) fn with_notification_emitter(
        mut self,
        notification_emitter: ToolCallNotificationEmitter,
    ) -> Self {
        self.notification_emitter = Some(notification_emitter);
        self
    }

    pub(crate) fn notification_emitter(&self) -> Option<&ToolCallNotificationEmitter> {
        self.notification_emitter.as_ref()
    }
}

impl From<&ToolCallContext> for crate::agents::extension_manager::CallRequest {
    fn from(ctx: &ToolCallContext) -> Self {
        Self {
            tool_call_id: ctx.tool_call_request_id.clone(),
            notification_emitter: ctx.notification_emitter().cloned(),
            container: ctx.container().cloned(),
        }
    }
}

use crate::agents::tool_snapshot::ToolSnapshot;
use crate::agents::StateMachineServices;

pub const DECLINED_RESPONSE: &str = "The user has declined to run this tool. \
    DO NOT attempt to call this tool again. \
    If there are no alternative methods to proceed, clearly explain the situation and STOP.";

#[cfg(feature = "code-mode")]
#[async_trait::async_trait]
impl ToolAccess for ToolCallContext {
    async fn tools(&self) -> Vec<rmcp::model::Tool> {
        self.tool_snapshot
            .as_ref()
            .expect("platform calls carry a snapshot")
            .tools()
            .await
    }

    async fn call(
        &self,
        call: rmcp::model::CallToolRequestParams,
        request: crate::agents::extension_manager::CallRequest,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Result<ToolCallResult, rmcp::model::ErrorData> {
        let services = self
            .services
            .as_ref()
            .expect("platform calls carry services");
        self.tool_snapshot
            .as_ref()
            .expect("platform calls carry a snapshot")
            .call(call, request, cancellation, services)
            .await
    }
}
