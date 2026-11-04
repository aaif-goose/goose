//! Protocol interface for hosts with request-scoped context and shared clients.
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};
use rmcp::model::{
    CallToolResult, GetPromptResult, InitializeResult, JsonObject, ListPromptsResult,
    ListResourcesResult, ListToolsResult, ReadResourceResult, ServerNotification,
};
use serde_json::Value;
use std::path::Path;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait ScopedClient<C: MaybeSync>: MaybeSend + MaybeSync {
    async fn list_tools(
        &self,
        session_id: &str,
        next_cursor: Option<String>,
        cancel_token: CancellationToken,
    ) -> Result<ListToolsResult, rmcp::ServiceError>;

    async fn call_tool(
        &self,
        ctx: &C,
        name: &str,
        arguments: Option<JsonObject>,
        cancel_token: CancellationToken,
    ) -> Result<CallToolResult, rmcp::ServiceError>;

    fn get_info(&self) -> Option<&InitializeResult>;

    async fn get_instructions(&self, _session_id: &str, _working_dir: &Path) -> Option<String> {
        self.get_info().and_then(|info| info.instructions.clone())
    }

    async fn list_resources(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Result<ListResourcesResult, rmcp::ServiceError> {
        Err(rmcp::ServiceError::TransportClosed)
    }

    async fn read_resource(
        &self,
        _session_id: &str,
        _uri: &str,
        _cancel_token: CancellationToken,
    ) -> Result<ReadResourceResult, rmcp::ServiceError> {
        Err(rmcp::ServiceError::TransportClosed)
    }

    async fn list_prompts(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Result<ListPromptsResult, rmcp::ServiceError> {
        Err(rmcp::ServiceError::TransportClosed)
    }

    async fn get_prompt(
        &self,
        _session_id: &str,
        _name: &str,
        _arguments: Value,
        _cancel_token: CancellationToken,
    ) -> Result<GetPromptResult, rmcp::ServiceError> {
        Err(rmcp::ServiceError::TransportClosed)
    }

    async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
        mpsc::channel(1).1
    }

    async fn get_moim(&self, _session_id: &str, _tools: &[rmcp::model::Tool]) -> Option<String> {
        None
    }
}
