#![cfg(not(target_arch = "wasm32"))]

use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Arc,
};

use oi_kit_agent::{
    action_required_manager::ActionRequiredManager,
    extension::{ExtensionConfig, ExtensionResult},
    mcp::{
        client::{ConnectContext, McpCallContext, McpClientTrait},
        CallRequest, ClientContext, Container, ExtensionHost, ExtensionLease, ExtensionManager,
        ExtensionSet, ExtensionSpec,
    },
};
use rmcp::{
    model::{
        CallToolRequestParams, CallToolResult, ContentBlock, InitializeResult, JsonObject,
        ListToolsResult, Tool,
    },
    ServiceError,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, PartialEq)]
struct HostConfig;

impl ExtensionSpec for HostConfig {
    fn name(&self) -> String {
        "host".to_string()
    }

    fn is_tool_available(&self, _tool: &str) -> bool {
        true
    }

    fn transport(&self) -> Option<&ExtensionConfig> {
        None
    }
}

#[derive(Default)]
struct Host {
    generation: AtomicU64,
    connections: AtomicUsize,
}

#[async_trait::async_trait]
impl ExtensionHost<HostConfig> for Host {
    fn config_generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    async fn connect(
        &self,
        _config: &HostConfig,
        _container: Option<&Container>,
        _ctx: ConnectContext,
    ) -> ExtensionResult<Box<dyn McpClientTrait>> {
        let id = self.connections.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Client { id }))
    }
}

struct Client {
    id: usize,
}

#[async_trait::async_trait]
impl McpClientTrait for Client {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Result<ListToolsResult, ServiceError> {
        Ok(ListToolsResult {
            tools: vec![Tool::new(
                "identity",
                "Connection identity",
                Arc::new(JsonObject::new()),
            )],
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        _ctx: &McpCallContext,
        _name: &str,
        _arguments: Option<JsonObject>,
        _cancel_token: CancellationToken,
    ) -> Result<CallToolResult, ServiceError> {
        Ok(CallToolResult::success(vec![ContentBlock::text(
            self.id.to_string(),
        )]))
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        None
    }
}

async fn connection_identity(lease: &ExtensionLease<HostConfig>) -> String {
    let tools = lease.tools().await;
    let call = lease
        .call(
            CallToolRequestParams::new(tools[0].name.clone()),
            CallRequest::default(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let result = call.result.await.unwrap();
    result.content[0].as_text().unwrap().text.clone()
}

#[tokio::test]
async fn generation_change_replaces_host_connection_without_invalidating_held_leases() {
    let host = Arc::new(Host::default());
    let manager = Arc::new(ExtensionManager::new(
        host.clone(),
        Arc::new(ActionRequiredManager::new()),
        ClientContext {
            name: "test".to_string(),
            version: "1".to_string(),
            capabilities: Default::default(),
        },
    ));
    let set = ExtensionSet::new("session", None, vec![HostConfig]).unwrap();

    let original = manager.resolve(&set).await;
    assert_eq!(connection_identity(&original).await, "0");

    let unchanged = manager.resolve(&set).await;
    assert_eq!(connection_identity(&unchanged).await, "0");
    assert_eq!(host.connections.load(Ordering::SeqCst), 1);

    host.generation.fetch_add(1, Ordering::SeqCst);
    let replacement = manager.resolve(&set).await;
    assert_eq!(host.connections.load(Ordering::SeqCst), 1);
    assert_eq!(connection_identity(&replacement).await, "1");
    assert_eq!(connection_identity(&original).await, "0");
    assert_eq!(connection_identity(&unchanged).await, "0");

    let reused = manager.resolve(&set).await;
    assert_eq!(connection_identity(&reused).await, "1");
    assert_eq!(host.connections.load(Ordering::SeqCst), 2);
}
