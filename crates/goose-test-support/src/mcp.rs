use rmcp::model::{
    Annotations, CallToolResult, ContentBlock, Implementation, InitializeResult, ProtocolVersion,
    Role, ServerCapabilities, ServerInfo, TextContent,
};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use tokio::task::JoinHandle;

pub const FAKE_CODE: &str = "test-uuid-12345-67890";

pub const TEST_IMAGE_B64: &str = include_str!("test_assets/test_image.b64").trim_ascii_end();

#[derive(Clone, Default)]
pub struct McpFixtureServer;

#[tool_router]
impl McpFixtureServer {
    pub fn new() -> Self {
        Self
    }

    #[tool(description = "Get the code", annotations(read_only_hint = true))]
    fn get_code(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![ContentBlock::text(FAKE_CODE)]))
    }

    #[tool(description = "Get an image")]
    fn get_image(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![ContentBlock::image(
            TEST_IMAGE_B64,
            "image/png",
        )]))
    }

    #[tool(
        description = "Get audience-scoped content",
        annotations(read_only_hint = true)
    )]
    fn get_audience_content(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![
            ContentBlock::text("visible"),
            ContentBlock::Text(
                TextContent::new("provider-only")
                    .with_annotations(Annotations::default().with_audience(vec![Role::Assistant])),
            ),
        ]))
    }
}

#[tool_handler]
impl ServerHandler for McpFixtureServer {
    fn get_info(&self) -> ServerInfo {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::LATEST)
            .with_server_info(Implementation::new("mcp-fixture", "1.0.0"))
            .with_instructions("Test server with code, image, and audience-scoped content tools.")
    }
}

pub struct McpFixture {
    pub url: String,
    handle: JoinHandle<()>,
}

impl Drop for McpFixture {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

impl McpFixture {
    pub async fn new() -> Self {
        let service_factory = || Ok::<_, std::io::Error>(McpFixtureServer::new());

        let service = StreamableHttpService::new(
            service_factory,
            LocalSessionManager::default().into(),
            StreamableHttpServerConfig::default(),
        );
        let router = axum::Router::new().nest_service("/mcp", service);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{addr}/mcp");

        let handle = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        Self { url, handle }
    }
}
