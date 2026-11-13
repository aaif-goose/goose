use anyhow::Result;
use rmcp::{transport::stdio, ServerHandler, ServiceExt};

pub struct PluginRuntime;

impl PluginRuntime {
    pub async fn run<S>(server: S) -> Result<()>
    where
        S: ServerHandler + Send + 'static,
    {
        let service = server.serve(stdio()).await?;
        service.waiting().await?;
        Ok(())
    }
}
