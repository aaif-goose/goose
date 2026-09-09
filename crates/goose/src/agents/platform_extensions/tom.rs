use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::agents::tool_execution::ToolCallContext;
use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ServerCapabilities,
};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

pub static EXTENSION_NAME: &str = "tom";

const MAX_BYTES: usize = 65_536;

pub struct TomClient {
    info: InitializeResult,
}

impl TomClient {
    pub fn new(_context: PlatformExtensionContext) -> Result<Self> {
        Ok(Self {
            info: InitializeResult::new(ServerCapabilities::builder().build()).with_server_info(
                Implementation::new(EXTENSION_NAME.to_string(), "1.0.0".to_string())
                    .with_title("Top Of Mind"),
            ),
        })
    }
}

#[async_trait]
impl McpClientTrait for TomClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult {
            tools: vec![],
            next_cursor: None,
            meta: None,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        _ctx: &ToolCallContext,
        name: &str,
        _arguments: Option<JsonObject>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        Ok(CallToolResult::error(vec![ContentBlock::text(format!(
            "tom has no tools (called: {name})"
        ))]))
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }

    async fn get_moim(&self, _session_id: &str) -> Option<String> {
        let mut parts = Vec::new();

        if let Ok(text) = std::env::var("GOOSE_MOIM_MESSAGE_TEXT") {
            if !text.trim().is_empty() {
                parts.push(truncate_utf8(text));
            }
        }

        if let Ok(path) = std::env::var("GOOSE_MOIM_MESSAGE_FILE") {
            let expanded = shellexpand::tilde(&path);
            if let Some(content) = read_bounded(&expanded).await {
                if !content.trim().is_empty() {
                    parts.push(content);
                }
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n"))
        }
    }
}

async fn read_bounded(path: &str) -> Option<String> {
    let mut file = tokio::fs::File::open(path).await.ok()?;
    let mut buf = vec![0u8; MAX_BYTES];
    let mut total = 0;
    loop {
        let n = file.read(&mut buf[total..]).await.ok()?;
        if n == 0 {
            break;
        }
        total += n;
        if total >= MAX_BYTES {
            break;
        }
    }
    buf.truncate(total);
    let s = String::from_utf8_lossy(&buf).into_owned();
    Some(truncate_utf8(s))
}

fn truncate_utf8(s: String) -> String {
    if s.len() <= MAX_BYTES {
        return s;
    }
    s.char_indices()
        .take_while(|(i, c)| i + c.len_utf8() <= MAX_BYTES)
        .map(|(_, c)| c)
        .collect()
}
