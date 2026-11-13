use std::collections::HashMap;

use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::host::ExtensionHost;
use crate::agents::mcp_client::{ConnectContext, McpClient};
use crate::subprocess::spawn_long_lived_mcp_subprocess;
use goose_agent::extension::{ExtensionError, ExtensionResult, ProcessExit};

pub(super) async fn connect(
    cmd: &str,
    args: &[String],
    envs: HashMap<String, String>,
    placement: Option<&dyn super::registration::StdioCommand>,
    ctx: ConnectContext,
    host: &dyn ExtensionHost,
) -> ExtensionResult<McpClient> {
    crate::agents::extension_malware_check::deny_if_malicious_cmd_args(cmd, args).await?;

    let command = match placement {
        Some(placement) => placement.command(cmd, args, envs)?,
        None => {
            let mut command = Command::new(host.resolve_command(cmd));
            command.args(args).envs(envs);
            command
        }
    };
    spawn(command, ctx, host).await
}

pub async fn spawn(
    mut command: Command,
    ctx: ConnectContext,
    host: &dyn ExtensionHost,
) -> ExtensionResult<McpClient> {
    if let Ok(path) = host.search_path() {
        command.env("PATH", path);
    }

    if ctx.working_dir.is_dir() {
        tracing::info!(
            "Setting MCP process working directory: {:?}",
            ctx.working_dir
        );
        command.current_dir(&ctx.working_dir);
    } else {
        tracing::warn!(
            "Working directory doesn't exist or isn't a directory: {:?}",
            ctx.working_dir
        );
    }

    let (transport, mut stderr) = spawn_long_lived_mcp_subprocess(command).await?;
    let mut stderr = stderr.take().ok_or_else(|| {
        ExtensionError::SetupError("failed to attach child process stderr".to_owned())
    })?;

    let stderr_task = tokio::spawn(async move {
        let mut all_stderr = Vec::new();
        stderr.read_to_end(&mut all_stderr).await?;
        Ok::<String, std::io::Error>(String::from_utf8_lossy(&all_stderr).into())
    });

    match McpClient::connect(transport, ctx).await {
        Ok(client) => Ok(client),
        Err(error) => Err(match stderr_task.await? {
            Ok(stderr_content) => ProcessExit::new(stderr_content, error).into(),
            Err(e) => e.into(),
        }),
    }
}
