use crate::agents::tool_execution::ToolCallContext;
use oi_kit_agent::mcp::client::Error;
use rmcp::model::*;
use std::path::Path;
use tokio_util::sync::CancellationToken;

#[async_trait::async_trait]
pub trait PlatformExtension: Send + Sync {
    async fn list_tools(&self, session_id: &str) -> Result<Vec<Tool>, Error>;

    async fn call_tool(
        &self,
        ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        cancel_token: CancellationToken,
    ) -> Result<CallToolResult, Error>;

    fn get_info(&self) -> Option<&InitializeResult>;

    async fn get_instructions(&self, _session_id: &str, _working_dir: &Path) -> Option<String> {
        self.get_info().and_then(|info| info.instructions.clone())
    }

    async fn list_resources(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Result<ListResourcesResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn read_resource(
        &self,
        _session_id: &str,
        _uri: &str,
        _cancel_token: CancellationToken,
    ) -> Result<ReadResourceResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn get_moim(&self, _session_id: &str) -> Option<String> {
        None
    }
}
