use goose_test_support::mcp::McpFixtureServer;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{transport::stdio, ServiceExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next().as_deref()) {
        (Some("stdio"), None) => {
            McpFixtureServer::new()
                .serve(stdio())
                .await?
                .waiting()
                .await?;
        }
        (Some("stdio"), Some("legacy")) => {
            McpFixtureServer::with_max_protocol_version(rmcp::model::ProtocolVersion::V_2025_11_25)
                .serve(stdio())
                .await?
                .waiting()
                .await?;
        }
        (Some("http") | None, None) => {
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
        (transport, mode) => {
            return Err(format!(
                "unknown fixture mode {transport:?} {mode:?}; use stdio [legacy] or http"
            )
            .into())
        }
    }
    Ok(())
}
