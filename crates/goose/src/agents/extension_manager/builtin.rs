use std::collections::HashMap;

use crate::agents::container::Container;
use crate::agents::extension::{name_to_key, ExtensionError, ExtensionResult};
use crate::agents::extension_runtime::{docker_exec, spawn_stdio};
use crate::builtin_extension::get_builtin_extension;
use crate::config::search_path::SearchPaths;
use oi_kit_agent::mcp::client::{ConnectContext, McpClient, McpClientTrait};

pub(super) async fn connect(
    name: &str,
    container: Option<&Container>,
    mut ctx: ConnectContext,
) -> ExtensionResult<Box<dyn McpClientTrait>> {
    let key = name_to_key(name);
    let extension_fn = get_builtin_extension(&key)
        .ok_or_else(|| ExtensionError::ConfigError(format!("Unknown extension: {}", name)))?;

    if let Some(container) = container {
        ctx.docker_container = Some(container.id().to_string());
        tracing::info!(
            container = %container.id(),
            builtin = %name,
            "Starting builtin extension inside Docker container"
        );
        let mut command = docker_exec(
            container,
            HashMap::new(),
            "goose",
            &["mcp".to_string(), key],
        );
        if let Ok(path) = SearchPaths::builder().path() {
            command.env("PATH", path);
        }
        return Ok(Box::new(spawn_stdio(command, ctx).await?));
    }

    let (server_read, client_write) = tokio::io::duplex(65536);
    let (client_read, server_write) = tokio::io::duplex(65536);
    extension_fn(server_read, server_write);
    Ok(Box::new(
        McpClient::connect((client_read, client_write), ctx).await?,
    ))
}
