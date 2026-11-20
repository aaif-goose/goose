//! Exposes schedule management when Goose has a scheduler configured.

use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, InitializeResult, JsonObject, ServerCapabilities,
    Tool,
};
use tokio_util::sync::CancellationToken;

use crate::agents::platform_extensions::PlatformExtension;
use crate::agents::platform_tools::{manage_schedule_tool, MANAGE_SCHEDULE_TOOL_NAME};
use crate::agents::schedule_tool::ScheduleTool;
use crate::agents::tool_execution::ToolCallContext;
use oi_kit_agent::mcp::client::Error;

use super::PlatformExtensionContext;

pub const EXTENSION_NAME: &str = "scheduler";
pub const MANAGE_SCHEDULE_TOOL_NAME_COMPLETE: &str = "scheduler__manage_schedule";

pub struct SchedulerClient {
    info: InitializeResult,
    schedule_tool: ScheduleTool,
}

impl SchedulerClient {
    pub fn new(context: PlatformExtensionContext) -> Option<Self> {
        let scheduler = context.scheduler?;
        let info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(EXTENSION_NAME, "1.0.0").with_title("Scheduler"))
            .with_instructions(
                "Create, list, update, pause, resume, and remove scheduled recipe runs, \
                 and inspect the sessions they produced.",
            );
        Some(Self {
            info,
            schedule_tool: ScheduleTool::new(scheduler, context.session_manager),
        })
    }
}

#[async_trait::async_trait]
impl PlatformExtension for SchedulerClient {
    async fn list_tools(&self, _session_id: &str) -> Result<Vec<Tool>, Error> {
        Ok(vec![manage_schedule_tool()])
    }

    async fn call_tool(
        &self,
        _ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        if name != MANAGE_SCHEDULE_TOOL_NAME {
            return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                "Unknown tool: {name}"
            ))]));
        }
        let arguments = serde_json::Value::Object(arguments.unwrap_or_default());
        Ok(match self.schedule_tool.execute(arguments).await {
            Ok(content) => CallToolResult::success(content),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error.to_string())]),
        })
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }
}
