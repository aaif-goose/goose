//! Two already-connected servers; the host supplies connections and session storage.
use anyhow::Result;
use async_trait::async_trait;
use goose_agent::{
    mcp::{Client, Manager, McpOperation, Registration},
    operation::{ConversationEffect, Operation},
};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ErrorData, ListToolsResult, Tool,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

struct Server(&'static str);
#[async_trait]
impl Client for Server {
    async fn list_tools(
        &self,
        _cursor: Option<String>,
        _cancel: CancellationToken,
    ) -> Result<ListToolsResult> {
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
        _id: &str,
        _call: CallToolRequestParams,
        _cancel: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(CallToolResult::success(vec![ContentBlock::text(self.0)]))
    }
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let manager = Manager::new();
    let lease = manager
        .resolve(
            "session",
            vec![
                Registration::attached("one", 0, Arc::new(Server("one"))),
                Registration::attached("two", 0, Arc::new(Server("two"))),
            ],
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
