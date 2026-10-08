use anyhow::Result;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tracing::{debug, info, warn};

use crate::subprocess::SubprocessExt;

use crate::agents::types::SuccessCheck;

const MAX_COMMAND_STDERR_BYTES: usize = 8 * 1024;

pub async fn execute_success_checks_with_timeout(
    checks: &[SuccessCheck],
    timeout: Duration,
) -> Result<bool> {
    for check in checks {
        match check {
            SuccessCheck::Shell { command } => {
                let result = execute_shell_command(command, timeout).await?;
                if !result.status.success() {
                    warn!(
                        "Success check failed: command '{}' exited with status {}, stderr: {}",
                        command,
                        result.status,
                        String::from_utf8_lossy(&result.stderr)
                    );
                    return Ok(false);
                }
                info!(
                    "Success check passed: command '{}' completed successfully",
                    command
                );
            }
        }
    }
    Ok(true)
}

/// Execute a shell command with cross-platform compatibility and mandatory timeout
pub(crate) async fn execute_shell_command(
    command: &str,
    timeout: std::time::Duration,
) -> Result<std::process::Output> {
    debug!(
        "Executing shell command with timeout {:?}: {}",
        timeout, command
    );

    let future = async {
        let mut cmd = if cfg!(target_os = "windows") {
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", command]);
            cmd.env("GOOSE_TERMINAL", "1");
            cmd.env("AGENT", "goose");
            cmd
        } else {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", command]);
            cmd.env("GOOSE_TERMINAL", "1");
            cmd.env("AGENT", "goose");
            cmd
        };

        cmd.set_no_window();

        let mut child = cmd
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let stderr = child
            .stderr
            .take()
            .expect("stderr was configured to be piped");
        let (status, stderr) = tokio::try_join!(child.wait(), capture_tail(stderr))?;
        let output = std::process::Output {
            status,
            stdout: Vec::new(),
            stderr,
        };

        debug!(
            "Shell command completed with status: {}, stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );

        Ok(output)
    };

    match tokio::time::timeout(timeout, future).await {
        Ok(result) => result,
        Err(_) => {
            let error_msg = format!("Shell command timed out after {:?}: {}", timeout, command);
            warn!("{}", error_msg);
            Err(anyhow::anyhow!("{}", error_msg))
        }
    }
}

async fn capture_tail<R>(mut reader: R) -> std::io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut tail = Vec::with_capacity(MAX_COMMAND_STDERR_BYTES);
    let mut chunk = [0; 4096];

    loop {
        let count = reader.read(&mut chunk).await?;
        if count == 0 {
            return Ok(tail);
        }

        if count >= MAX_COMMAND_STDERR_BYTES {
            tail.clear();
            tail.extend_from_slice(&chunk[count - MAX_COMMAND_STDERR_BYTES..count]);
            continue;
        }

        let overflow = tail
            .len()
            .saturating_add(count)
            .saturating_sub(MAX_COMMAND_STDERR_BYTES);
        if overflow > 0 {
            tail.drain(..overflow);
        }
        tail.extend_from_slice(&chunk[..count]);
    }
}

pub async fn execute_on_failure_command_with_timeout(
    command: &str,
    timeout: Duration,
) -> Result<()> {
    info!(
        "Executing on_failure command with timeout {:?}: {}",
        timeout, command
    );

    let output = match execute_shell_command(command, timeout).await {
        Ok(output) => output,
        Err(e) => {
            if e.to_string().contains("timed out") {
                let error_msg = format!(
                    "On_failure command timed out after {:?}: {}",
                    timeout, command
                );
                warn!("{}", error_msg);
                return Err(anyhow::anyhow!(error_msg));
            } else {
                warn!("On_failure command execution error: {}", e);
                return Err(e);
            }
        }
    };

    if !output.status.success() {
        let error_msg = format!(
            "On_failure command failed: command '{}' exited with status {}, stderr: {}",
            command,
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        warn!("{}", error_msg);
        return Err(anyhow::anyhow!(error_msg));
    } else {
        info!("On_failure command completed successfully: {}", command);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_success_checks_all_pass() {
        let checks = [SuccessCheck::Shell {
            command: "echo success".to_string(),
        }];

        assert!(
            execute_success_checks_with_timeout(&checks, Duration::from_secs(30))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_execute_success_checks_one_fails() {
        let checks = [SuccessCheck::Shell {
            command: "false".to_string(),
        }];

        assert!(
            !execute_success_checks_with_timeout(&checks, Duration::from_secs(30))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_execute_on_failure_command_success() {
        execute_on_failure_command_with_timeout("echo recovered", Duration::from_secs(30))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_execute_on_failure_command_failure() {
        assert!(
            execute_on_failure_command_with_timeout("false", Duration::from_secs(30))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_execute_shell_command_success() {
        let result = execute_shell_command("echo 'hello world'", Duration::from_secs(30)).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.status.success());
        assert!(output.stdout.is_empty());
    }

    #[tokio::test]
    async fn test_execute_shell_command_failure() {
        let result = execute_shell_command("false", Duration::from_secs(30)).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.status.success());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_execute_shell_command_bounds_stderr_tail() {
        let command = "i=0; while [ $i -lt 10000 ]; do printf x >&2; i=$((i + 1)); done; printf diagnostic-tail >&2; exit 1";
        let output = execute_shell_command(command, Duration::from_secs(30))
            .await
            .unwrap();

        assert!(!output.status.success());
        assert_eq!(output.stderr.len(), MAX_COMMAND_STDERR_BYTES);
        assert!(output.stderr.ends_with(b"diagnostic-tail"));
    }

    #[tokio::test]
    async fn test_capture_tail_preserves_small_diagnostic() {
        use tokio::io::AsyncWriteExt;

        let diagnostic = b"useful diagnostic";
        let (mut writer, reader) = tokio::io::duplex(diagnostic.len());
        writer.write_all(diagnostic).await.unwrap();
        writer.shutdown().await.unwrap();

        assert_eq!(capture_tail(reader).await.unwrap(), diagnostic);
    }

    #[tokio::test]
    async fn test_shell_command_timeout() {
        let timeout = std::time::Duration::from_millis(100);
        let result = if cfg!(target_os = "windows") {
            execute_shell_command("timeout /t 1", timeout).await
        } else {
            execute_shell_command("sleep 1", timeout).await
        };

        assert!(result.is_err());
    }
}
