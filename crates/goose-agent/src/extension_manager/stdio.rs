use std::collections::HashMap;

use rmcp::transport::ConfigureCommandExt;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::host::ExtensionHost;
use crate::container::Container;
use crate::extension::{ExtensionError, ExtensionResult, ProcessExit};
use crate::mcp_client::{ConnectContext, McpClient};
use crate::subprocess::spawn_long_lived_mcp_subprocess;

pub(super) async fn connect(
    cmd: &str,
    args: &[String],
    envs: HashMap<String, String>,
    container: Option<&Container>,
    mut ctx: ConnectContext,
    host: &dyn ExtensionHost,
) -> ExtensionResult<McpClient> {
    crate::extension_malware_check::deny_if_malicious_cmd_args(cmd, args).await?;

    let command = match container {
        Some(container) => {
            ctx.docker_container = Some(container.id().to_string());
            tracing::info!(
                container = %container.id(),
                cmd = %cmd,
                "Starting stdio extension inside Docker container"
            );
            docker_exec(container, envs, cmd, args)
        }
        None => Command::new(host.resolve_command(cmd)).configure(|command| {
            command.args(args).envs(envs);
        }),
    };
    spawn(command, ctx, host).await
}

pub(super) fn docker_exec(
    container: &Container,
    envs: HashMap<String, String>,
    program: &str,
    args: &[String],
) -> Command {
    Command::new("docker").configure(|command| {
        command.arg("exec").arg("-i");
        for (key, value) in envs {
            command.arg("-e").arg(format!("{}={}", key, value));
        }
        command.arg(container.id()).arg(program).args(args);
    })
}

pub(super) async fn spawn(
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
