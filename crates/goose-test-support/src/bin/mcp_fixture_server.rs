use goose_test_support::mcp::McpFixtureServer;
use rmcp::model::ProtocolVersion;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{transport::stdio, ServiceExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next().as_deref()) {
        (Some("stdio"), max_version) => {
            let max_version = match max_version {
                Some(version) => ProtocolVersion::KNOWN_VERSIONS
                    .iter()
                    .find(|known| known.as_str() == version)
                    .cloned()
                    .ok_or_else(|| format!("unknown protocol version {version}"))?,
                None => ProtocolVersion::V_2026_07_28,
            };
            McpFixtureServer::with_max_protocol_version(max_version)
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
                "unknown fixture mode {transport:?} {mode:?}; use stdio [<max protocol version>] or http"
            )
            .into())
        }
    }
    Ok(())
}
