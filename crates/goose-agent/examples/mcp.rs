//! Two already-connected servers; the host supplies connections and session storage.
use anyhow::Result;
use async_trait::async_trait;
use goose_agent::{
    mcp::{Client, Lease, McpOperation, Registration, StartupPolicy},
    operation::{ConversationEffect, Operation},
};
use rmcp::model::{CallToolResult, ContentBlock, ListToolsResult, Tool};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

struct Server(&'static str);
#[async_trait]
impl Client for Server {
    fn get_info(&self) -> Option<&rmcp::model::InitializeResult> {
        None
    }
    async fn list_tools(
        &self,
        _scope: &str,
        _cursor: Option<String>,
        _cancel: CancellationToken,
    ) -> Result<ListToolsResult, rmcp::ServiceError> {
        let result = ListToolsResult {
            tools: vec![Tool::new(
                "hello",
                "Say hello",
                Arc::new(serde_json::Map::new()),
            )],
            ..Default::default()
        };
        Ok(result)
    }
    async fn call_tool(
        &self,
        _context: &goose_agent::mcp::CallContext,
        _name: &str,
        _arguments: Option<rmcp::model::JsonObject>,
        _cancel: CancellationToken,
    ) -> Result<CallToolResult, rmcp::ServiceError> {
        Ok(CallToolResult::success(vec![ContentBlock::text(self.0)]))
    }
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let lease = Lease::connect(
        "session",
        vec![
            Registration::attached("one", Arc::new(Server("one"))),
            Registration::attached("two", Arc::new(Server("two"))),
        ],
        StartupPolicy::Strict,
        CancellationToken::new(),
    )
    .await?;
    let operation = McpOperation::<()>::new(Arc::new(lease));
    for tool in
        <McpOperation<()> as Operation<(), ConversationEffect>>::inference_tools(&operation, &())
            .await?
    {
        println!("{}", tool.name);
    }
    Ok(())
}
