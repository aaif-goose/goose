use goose_test_support::mcp::McpFixtureServer;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{transport::stdio, ServiceExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        Some("stdio") => {
            McpFixtureServer::new()
                .serve(stdio())
                .await?
                .waiting()
                .await?;
        }
        Some("http") | None => {
            let service = StreamableHttpService::new(
                || Ok(McpFixtureServer::new()),
                LocalSessionManager::default().into(),
                StreamableHttpServerConfig::default(),
            );
            let router = axum::Router::new().nest_service("/mcp", service);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            eprintln!(
                "MCP fixture server running at http://{}/mcp",
                listener.local_addr()?
            );
            axum::serve(listener, router).await?;
        }
        Some(other) => return Err(format!("unknown transport {other}; use stdio or http").into()),
    }
    Ok(())
}
