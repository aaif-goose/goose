use goose_test_support::mcp::McpFixtureServer;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};

#[tokio::main]
async fn main() {
    let service = StreamableHttpService::new(
        || Ok(McpFixtureServer::new()),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    eprintln!("MCP fixture server running at http://{addr}/mcp");
    axum::serve(listener, router).await.unwrap();
}
