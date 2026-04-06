use anyhow::Result;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::subprocess::SubprocessExt;

use crate::agents::types::SessionConfig;
use crate::agents::types::{
    RetryConfig, SuccessCheck, DEFAULT_ON_FAILURE_TIMEOUT_SECONDS, DEFAULT_RETRY_TIMEOUT_SECONDS,
};
use crate::config::Config;
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::tool_monitor::RepetitionInspector;

/// Result of a retry logic evaluation
#[derive(Debug, Clone, PartialEq)]
pub enum RetryResult {
    /// No retry configuration or session available, retry logic skipped
    Skipped,
    /// Maximum retry attempts reached, cannot retry further
    MaxAttemptsReached,
    /// Success checks passed, no retry needed
    SuccessChecksPassed,
    /// Retry is needed and will be performed
    Retried,
}

/// Environment variable for configuring retry timeout globally
const GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS: &str = "GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS";

/// Environment variable for configuring on_failure timeout globally
const GOOSE_RECIPE_ON_FAILURE_TIMEOUT_SECONDS: &str = "GOOSE_RECIPE_ON_FAILURE_TIMEOUT_SECONDS";

/// Manages retry state and operations for agent execution
#[derive(Debug)]
pub struct RetryManager {
    /// Current number of retry attempts
    attempts: Arc<Mutex<u32>>,
    /// Optional repetition inspector for reset operations
    repetition_inspector: Option<Arc<Mutex<Option<RepetitionInspector>>>>,
}

impl Default for RetryManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RetryManager {
    /// Create a new retry manager
    pub fn new() -> Self {
        Self {
            attempts: Arc::new(Mutex::new(0)),
            repetition_inspector: None,
        }
    }

    /// Create a new retry manager with repetition inspector
    pub fn with_repetition_inspector(
        repetition_inspector: Arc<Mutex<Option<RepetitionInspector>>>,
    ) -> Self {
        Self {
            attempts: Arc::new(Mutex::new(0)),
            repetition_inspector: Some(repetition_inspector),
        }
    }

    /// Reset the retry attempts counter to 0
    pub async fn reset_attempts(&self) {
        let mut attempts = self.attempts.lock().await;
        *attempts = 0;

        // Reset repetition inspector if available
        if let Some(inspector) = &self.repetition_inspector {
            if let Some(inspector) = inspector.lock().await.as_mut() {
                inspector.reset();
            }
        }
    }

    /// Increment the retry attempts counter and return the new value
    pub async fn increment_attempts(&self) -> u32 {
        let mut attempts = self.attempts.lock().await;
        *attempts += 1;
        *attempts
    }

    /// Get the current retry attempts count
    pub async fn get_attempts(&self) -> u32 {
        *self.attempts.lock().await
    }

    /// Reset status for retry: clear message history and final output tool state
    async fn reset_status_for_retry(
        messages: &mut Conversation,
        initial_messages: &[Message],
        final_output_tool: &Arc<Mutex<Option<crate::agents::final_output_tool::FinalOutputTool>>>,
    ) {
        *messages = Conversation::new_unvalidated(initial_messages.to_vec());
        info!("Reset message history to initial state for retry");

        let mut guard = final_output_tool.lock().await;
        if let Some(fot) = guard.as_mut() {
            fot.final_output = None;
            info!("Cleared final output tool state for retry");
        }
    }

    pub async fn handle_retry_logic(
        &self,
        messages: &mut Conversation,
        session_config: &SessionConfig,
        initial_messages: &[Message],
        final_output_tool: &Arc<Mutex<Option<crate::agents::final_output_tool::FinalOutputTool>>>,
    ) -> Result<RetryResult> {
        let Some(retry_config) = &session_config.retry_config else {
            return Ok(RetryResult::Skipped);
        };

        let success = execute_success_checks(&retry_config.checks, retry_config).await?;

        if success {
            info!("All success checks passed, no retry needed");
            return Ok(RetryResult::SuccessChecksPassed);
        }

        let current_attempts = self.get_attempts().await;
        if current_attempts >= retry_config.max_retries {
            let error_msg = Message::assistant().with_text(format!(
                "Maximum retry attempts ({}) exceeded. Unable to complete the task successfully.",
                retry_config.max_retries
            ));
            messages.push(error_msg);
            warn!(
                "Maximum retry attempts ({}) exceeded",
                retry_config.max_retries
            );
            #[cfg(feature = "telemetry")]
            crate::posthog::emit_error(
                "retry_max_exceeded",
                &format!("Max retries ({}) exceeded", retry_config.max_retries),
            );
            return Ok(RetryResult::MaxAttemptsReached);
        }

        if let Some(on_failure_cmd) = &retry_config.on_failure {
            info!("Executing on_failure command: {}", on_failure_cmd);
            execute_on_failure_command(on_failure_cmd, retry_config).await?;
        }

        Self::reset_status_for_retry(messages, initial_messages, final_output_tool).await;

        let new_attempts = self.increment_attempts().await;
        info!("Incrementing retry attempts to {}", new_attempts);

        Ok(RetryResult::Retried)
    }
}

/// Get the configured timeout duration for retry operations
/// retry_config.timeout_seconds -> env var -> default
fn get_retry_timeout(retry_config: &RetryConfig) -> Duration {
    let timeout_seconds = retry_config
        .timeout_seconds
        .or_else(|| {
            let config = Config::global();
            config.get_param(GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS).ok()
        })
        .unwrap_or(DEFAULT_RETRY_TIMEOUT_SECONDS);

    Duration::from_secs(timeout_seconds)
}

/// Get the configured timeout duration for on_failure operations
/// retry_config.on_failure_timeout_seconds -> env var -> default
fn get_on_failure_timeout(retry_config: &RetryConfig) -> Duration {
    let timeout_seconds = retry_config
        .on_failure_timeout_seconds
        .or_else(|| {
            let config = Config::global();
            config
                .get_param(GOOSE_RECIPE_ON_FAILURE_TIMEOUT_SECONDS)
                .ok()
        })
        .unwrap_or(DEFAULT_ON_FAILURE_TIMEOUT_SECONDS);

    Duration::from_secs(timeout_seconds)
}

/// Execute all success checks and return true if all pass
pub async fn execute_success_checks(
    checks: &[SuccessCheck],
    retry_config: &RetryConfig,
) -> Result<bool> {
    let timeout = get_retry_timeout(retry_config);

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
pub async fn execute_shell_command(
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

        let output = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output()
            .await?;

        debug!(
            "Shell command completed with status: {}, stdout: {}, stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
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

/// Execute an on_failure command and return an error if it fails
pub async fn execute_on_failure_command(command: &str, retry_config: &RetryConfig) -> Result<()> {
    let timeout = get_on_failure_timeout(retry_config);
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
    use crate::agents::types::SuccessCheck;

    fn create_test_retry_config() -> RetryConfig {
        RetryConfig {
            max_retries: 3,
            checks: vec![],
            on_failure: None,
            timeout_seconds: Some(60),
            on_failure_timeout_seconds: Some(120),
        }
    }

    #[test]
    fn test_retry_result_enum() {
        assert_ne!(RetryResult::Skipped, RetryResult::MaxAttemptsReached);
        assert_ne!(RetryResult::Skipped, RetryResult::SuccessChecksPassed);
        assert_ne!(RetryResult::Skipped, RetryResult::Retried);
        assert_ne!(
            RetryResult::MaxAttemptsReached,
            RetryResult::SuccessChecksPassed
        );
        assert_ne!(RetryResult::MaxAttemptsReached, RetryResult::Retried);
        assert_ne!(RetryResult::SuccessChecksPassed, RetryResult::Retried);

        let result = RetryResult::Retried;
        let cloned = result.clone();
        assert_eq!(result, cloned);

        let debug_str = format!("{:?}", RetryResult::MaxAttemptsReached);
        assert!(debug_str.contains("MaxAttemptsReached"));
    }

    #[tokio::test]
    async fn test_execute_success_checks_all_pass() {
        let checks = vec![
            SuccessCheck::Shell {
                command: "echo 'test'".to_string(),
            },
            SuccessCheck::Shell {
                command: "true".to_string(),
            },
        ];
        let retry_config = create_test_retry_config();

        let result = execute_success_checks(&checks, &retry_config).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_execute_success_checks_one_fails() {
        let checks = vec![
            SuccessCheck::Shell {
                command: "echo 'test'".to_string(),
            },
            SuccessCheck::Shell {
                command: "false".to_string(),
            },
        ];
        let retry_config = create_test_retry_config();

        let result = execute_success_checks(&checks, &retry_config).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_execute_shell_command_success() {
        let result = execute_shell_command("echo 'hello world'", Duration::from_secs(30)).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("hello world"));
    }

    #[tokio::test]
    async fn test_execute_shell_command_failure() {
        let result = execute_shell_command("false", Duration::from_secs(30)).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.status.success());
    }

    #[tokio::test]
    async fn test_execute_on_failure_command_success() {
        let retry_config = create_test_retry_config();
        let result = execute_on_failure_command("echo 'cleanup'", &retry_config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_on_failure_command_failure() {
        let retry_config = create_test_retry_config();
        let result = execute_on_failure_command("false", &retry_config).await;
        assert!(result.is_err());
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

    #[tokio::test]
    async fn test_get_retry_timeout_uses_config_default() {
        let retry_config = RetryConfig {
            max_retries: 1,
            checks: vec![],
            on_failure: None,
            timeout_seconds: None,
            on_failure_timeout_seconds: None,
        };

        let timeout = get_retry_timeout(&retry_config);
        assert_eq!(timeout, Duration::from_secs(DEFAULT_RETRY_TIMEOUT_SECONDS));
    }

    #[tokio::test]
    async fn test_get_retry_timeout_uses_retry_config() {
        let retry_config = RetryConfig {
            max_retries: 1,
            checks: vec![],
            on_failure: None,
            timeout_seconds: Some(120),
            on_failure_timeout_seconds: None,
        };

        let timeout = get_retry_timeout(&retry_config);
        assert_eq!(timeout, Duration::from_secs(120));
    }

    #[tokio::test]
    async fn test_get_on_failure_timeout_uses_config_default() {
        let retry_config = RetryConfig {
            max_retries: 1,
            checks: vec![],
            on_failure: None,
            timeout_seconds: None,
            on_failure_timeout_seconds: None,
        };

        let timeout = get_on_failure_timeout(&retry_config);
        assert_eq!(
            timeout,
            Duration::from_secs(DEFAULT_ON_FAILURE_TIMEOUT_SECONDS)
        );
    }

    #[tokio::test]
    async fn test_get_on_failure_timeout_uses_retry_config() {
        let retry_config = RetryConfig {
            max_retries: 1,
            checks: vec![],
            on_failure: None,
            timeout_seconds: None,
            on_failure_timeout_seconds: Some(900),
        };

        let timeout = get_on_failure_timeout(&retry_config);
        assert_eq!(timeout, Duration::from_secs(900));
    }

    #[tokio::test]
    async fn test_on_failure_timeout_different_from_retry_timeout() {
        let retry_config = RetryConfig {
            max_retries: 1,
            checks: vec![],
            on_failure: None,
            timeout_seconds: Some(60),
            on_failure_timeout_seconds: Some(300),
        };

        let retry_timeout = get_retry_timeout(&retry_config);
        let on_failure_timeout = get_on_failure_timeout(&retry_config);

        assert_eq!(retry_timeout, Duration::from_secs(60));
        assert_eq!(on_failure_timeout, Duration::from_secs(300));
        assert_ne!(retry_timeout, on_failure_timeout);
    }
}
