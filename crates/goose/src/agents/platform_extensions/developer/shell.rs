#[cfg(windows)]
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;
use std::time::Duration;

use rmcp::model::{CallToolResult, Content};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_stream::{wrappers::SplitStream, StreamExt};

use crate::subprocess::SubprocessExt;

/// Check if the current process is running inside a Flatpak sandbox.
///
/// When inside Flatpak, shell commands must be wrapped with `flatpak-spawn --host`
/// to execute on the host system rather than inside the sandbox.
#[cfg(not(windows))]
fn is_flatpak() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

#[cfg(not(windows))]
const FLATPAK_HOST_ARGS: [&str; 2] = ["--host", "--watch-bus"];

#[cfg(not(windows))]
fn flatpak_spawn_command() -> tokio::process::Command {
    let mut command = tokio::process::Command::new("flatpak-spawn");
    command.args(FLATPAK_HOST_ARGS);
    command
}

#[cfg(not(windows))]
fn flatpak_spawn_process() -> std::process::Command {
    let mut command = std::process::Command::new("flatpak-spawn");
    command.args(FLATPAK_HOST_ARGS);
    command
}

/// Resolve the preferred Unix shell, respecting GOOSE_SHELL.
///
/// Returns `(shell_path, is_user_configured)` — the boolean is true when
/// `GOOSE_SHELL` was explicitly set, which matters in Flatpak mode: an
/// explicit path is passed through as-is (the user likely intends a host
/// path), whereas auto-detected defaults are reduced to their basename so
/// the host's PATH resolves the correct binary.
///
/// In Flatpak mode without an explicit GOOSE_SHELL, we skip sandbox
/// filesystem checks (e.g. `/bin/bash` existing inside the sandbox tells
/// us nothing about the host) and default to `"bash"` directly.
#[cfg(not(windows))]
fn unix_shell() -> (String, bool) {
    match std::env::var("GOOSE_SHELL") {
        Ok(shell) => (shell, true),
        Err(_) => {
            let shell = if is_flatpak() {
                // Don't inspect sandbox paths — they don't reflect the host.
                // Default to "bash" (ubiquitous on Flatpak-capable Linux hosts).
                "bash".to_string()
            } else if PathBuf::from("/bin/bash").is_file() {
                "/bin/bash".to_string()
            } else {
                std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
            };
            (shell, false)
        }
    }
}

/// Return the shell reference to pass to `flatpak-spawn --host`.
///
/// If the user explicitly configured GOOSE_SHELL, honour the full path
/// (it likely refers to a host binary, e.g. a Nix-profile shell).
/// Otherwise strip to basename so the host's default PATH resolves it.
#[cfg(not(windows))]
fn flatpak_shell_arg(shell: &str, is_user_configured: bool) -> &str {
    if is_user_configured {
        shell
    } else {
        std::path::Path::new(shell)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("bash")
    }
}

const OUTPUT_LIMIT_LINES: usize = 2000;
pub const OUTPUT_LIMIT_BYTES: usize = 50_000;
const OUTPUT_PREVIEW_LINES: usize = 50;

const OUTPUT_SLOTS: usize = 8;

/// Result of truncating command output.
struct TruncateResult {
    /// The (possibly truncated) text to display.
    text: String,
    /// When output was truncated, the path where the full output was saved
    /// and a human-readable reason for the truncation.
    truncation: Option<TruncationInfo>,
}

struct TruncationInfo {
    path: PathBuf,
    reason: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShellParams {
    pub command: String,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ShellOutput {
    pub stdout: String,
    pub stderr: String,
    /// Process exit code. 0 indicates success, non-zero indicates failure.
    /// Absent if the process was killed (e.g. timeout).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// True if the command was killed because it exceeded the timeout.
    #[serde(default)]
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub timed_out: bool,
    /// True if output collection was cut short after the shell exited.
    #[serde(default)]
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub output_truncated: bool,
    /// Error reported by output collection after process exit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_collection_error: Option<String>,
}

/// Resolve the user's full PATH by running a login shell.
///
/// When goosed is launched from a desktop app (e.g. Electron), it may inherit
/// a minimal PATH like `/usr/bin:/bin`. This function spawns a login shell to
/// source the user's profile and recover the full PATH.
#[cfg(not(windows))]
fn resolve_login_shell_path() -> Option<String> {
    let (shell, is_user_configured) = unix_shell();

    let mut child = if is_flatpak() {
        flatpak_spawn_process()
            .args([
                flatpak_shell_arg(&shell, is_user_configured),
                "-l",
                "-i",
                "-c",
                "echo $PATH",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?
    } else {
        std::process::Command::new(&shell)
            .args(["-l", "-i", "-c", "echo $PATH"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?
    };

    let mut stdout = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        use std::io::Read;
        if stdout.read_to_end(&mut buf).is_ok() {
            let _ = tx.send(buf);
        }
    });

    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(buf) if child.wait().is_ok_and(|s| s.success()) => {
            // Take the last non-empty line — interactive shells may emit
            // extra output from profile scripts before our echo.
            String::from_utf8_lossy(&buf)
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .map(|line| line.trim().to_string())
                .filter(|path| !path.is_empty())
        }
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            None
        }
    }
}

#[cfg(not(windows))]
static LOGIN_PATH: LazyLock<Option<String>> = LazyLock::new(resolve_login_shell_path);

pub struct ShellTool {
    output_dir: tempfile::TempDir,
    call_index: AtomicUsize,
    #[cfg(not(windows))]
    login_path: Option<String>,
}

impl ShellTool {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            output_dir: tempfile::tempdir()?,
            call_index: AtomicUsize::new(0),
            #[cfg(not(windows))]
            login_path: LOGIN_PATH.clone(),
        })
    }

    #[cfg(test)]
    pub fn new_for_test() -> std::io::Result<Self> {
        Ok(Self {
            output_dir: tempfile::tempdir()?,
            call_index: AtomicUsize::new(0),
            #[cfg(not(windows))]
            login_path: None,
        })
    }

    pub async fn shell(&self, params: ShellParams) -> CallToolResult {
        self.shell_with_cwd(params, None).await
    }

    pub async fn shell_with_cwd(
        &self,
        params: ShellParams,
        working_dir: Option<&std::path::Path>,
    ) -> CallToolResult {
        if params.command.trim().is_empty() {
            return Self::error_result("Command cannot be empty.", None);
        }

        #[cfg(not(windows))]
        let login_path = self.login_path.as_deref();
        #[cfg(windows)]
        let login_path: Option<&str> = None;

        let execution = match run_command(
            &params.command,
            params.timeout_secs,
            working_dir,
            login_path,
        )
        .await
        {
            Ok(execution) => execution,
            Err(error) => return Self::error_result(&error, None),
        };

        // Derive stdout, stderr, and interleaved display from the single tagged-line buffer
        let (raw_stdout, raw_stderr, interleaved) = split_lines(&execution.lines);

        let output_dir = self.output_dir.path();
        let slot = self.call_index.fetch_add(1, Ordering::Relaxed) % OUTPUT_SLOTS;
        let stdout_result = if raw_stdout.is_empty() {
            TruncateResult {
                text: String::new(),
                truncation: None,
            }
        } else {
            match truncate_output(&raw_stdout, &format!("stdout-{slot}"), output_dir) {
                Ok(r) => r,
                Err(error) => return Self::error_result(&error, None),
            }
        };
        let stderr_result = if raw_stderr.is_empty() {
            TruncateResult {
                text: String::new(),
                truncation: None,
            }
        } else {
            match truncate_output(&raw_stderr, &format!("stderr-{slot}"), output_dir) {
                Ok(r) => r,
                Err(error) => return Self::error_result(&error, None),
            }
        };

        let shell_output = ShellOutput {
            stdout: stdout_result.text,
            stderr: stderr_result.text,
            exit_code: execution.exit_code,
            timed_out: execution.timed_out,
            output_truncated: execution.output_truncated,
            output_collection_error: execution.output_collection_error.clone(),
        };
        let structured_content = serde_json::to_value(&shell_output).ok();
        let render_result = match render_output(&interleaved, &format!("output-{slot}"), output_dir)
        {
            Ok(r) => r,
            Err(error) => return Self::error_result(&error, None),
        };
        let mut rendered = render_result.text;

        // Collect truncation notices from stdout, stderr, and interleaved output.
        // These are delivered as a separate Content block so the model sees them as
        // instructions rather than part of the command's data output.
        let truncation_notices: Vec<String> = [
            &stdout_result.truncation,
            &stderr_result.truncation,
            &render_result.truncation,
        ]
        .iter()
        .filter_map(|t| t.as_ref().map(truncation_notice))
        .collect();

        let is_error = if execution.timed_out {
            if let Some(timeout_secs) = params.timeout_secs {
                rendered.push_str(&format!(
                    "\n\nCommand timed out after {} seconds",
                    timeout_secs
                ));
            } else {
                rendered.push_str("\n\nCommand timed out");
            }
            true
        } else {
            execution.exit_code.unwrap_or(1) != 0
        };

        if execution.output_truncated {
            rendered.push_str(
                "\n\nOutput may be incomplete because stream draining timed out after process exit.",
            );
        }
        if let Some(error) = &execution.output_collection_error {
            rendered.push_str(&format!(
                "\n\nOutput collection error occurred; output may be incomplete: {error}"
            ));
        }

        let is_error = is_error || execution.output_collection_error.is_some();

        if is_error {
            if let Some(code) = execution.exit_code.filter(|c| *c != 0) {
                rendered.push_str(&format!("\n\nCommand exited with code {code}"));
            }
            let mut error_blocks = vec![Content::text(rendered).with_priority(0.0)];
            if !truncation_notices.is_empty() {
                error_blocks.push(Content::text(truncation_notices.join("\n")).with_priority(0.0));
            }
            let mut result = CallToolResult::error(error_blocks);
            result.structured_content = structured_content;
            return result;
        }

        let mut content_blocks = vec![Content::text(rendered).with_priority(0.0)];
        if !truncation_notices.is_empty() {
            content_blocks.push(Content::text(truncation_notices.join("\n")).with_priority(0.0));
        }
        let mut result = CallToolResult::success(content_blocks);
        result.structured_content = structured_content;
        result
    }

    pub fn error_result(message: &str, exit_code: Option<i32>) -> CallToolResult {
        let shell_output = ShellOutput {
            stdout: String::new(),
            stderr: message.to_string(),
            exit_code,
            timed_out: false,
            output_truncated: false,
            output_collection_error: None,
        };
        let mut result = CallToolResult::error(vec![Content::text(message).with_priority(0.0)]);
        result.structured_content = serde_json::to_value(&shell_output).ok();
        result
    }
}

struct ExecutionOutput {
    /// Lines in arrival order, tagged by source: (is_stderr, text)
    lines: Vec<(bool, String)>,
    exit_code: Option<i32>,
    timed_out: bool,
    output_truncated: bool,
    output_collection_error: Option<String>,
}

async fn run_command(
    command_line: &str,
    timeout_secs: Option<u64>,
    working_dir: Option<&std::path::Path>,
    login_path: Option<&str>,
) -> Result<ExecutionOutput, String> {
    let mut command = build_shell_command(command_line, working_dir, login_path);

    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.stdin(Stdio::null());

    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to spawn shell command: {}", error))?;

    let child_stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture stdout".to_string())?;
    let child_stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture stderr".to_string())?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let output_task = tokio::spawn(collect_tagged_lines(child_stdout, child_stderr, tx));
    let abort_handle = output_task.abort_handle();

    let mut timed_out = false;
    let exit_code = if let Some(timeout_secs) = timeout_secs.filter(|value| *value > 0) {
        match tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait()).await {
            Ok(wait_result) => wait_result
                .map_err(|error| format!("Failed waiting on shell command: {}", error))?
                .code(),
            Err(_) => {
                timed_out = true;
                let _ = child.start_kill();
                let _ = child.wait().await;
                None
            }
        }
    } else {
        child
            .wait()
            .await
            .map_err(|error| format!("Failed waiting on shell command: {}", error))?
            .code()
    };

    const OUTPUT_DRAIN_TIMEOUT_MILLIS: u64 = 500;
    let mut output_collection_error = None;
    let output_truncated = match tokio::time::timeout(
        Duration::from_millis(OUTPUT_DRAIN_TIMEOUT_MILLIS),
        output_task,
    )
    .await
    {
        Ok(Ok(Ok(()))) => false,
        Ok(Ok(Err(e))) => {
            output_collection_error = Some(format!("Failed to collect shell output: {}", e));
            false
        }
        Ok(Err(e)) => {
            output_collection_error = Some(format!("Failed to collect shell output: {}", e));
            false
        }
        Err(_) => {
            tracing::debug!(
                    "output drain timed out after {OUTPUT_DRAIN_TIMEOUT_MILLIS}ms (backgrounded process?)"
                );
            abort_handle.abort();
            true
        }
    };

    rx.close();
    let mut lines = Vec::new();
    while let Some(item) = rx.recv().await {
        lines.push(item);
    }

    Ok(ExecutionOutput {
        lines,
        exit_code,
        timed_out,
        output_truncated,
        output_collection_error,
    })
}

fn build_shell_command(
    command_line: &str,
    working_dir: Option<&std::path::Path>,
    login_path: Option<&str>,
) -> tokio::process::Command {
    #[cfg(windows)]
    let mut command = {
        let shell = std::env::var("GOOSE_SHELL").unwrap_or_else(|_| "cmd".to_string());
        let shell_stem = Path::new(&shell)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("cmd")
            .to_lowercase();
        let mut command = tokio::process::Command::new(&shell);
        match shell_stem.as_str() {
            "pwsh" | "powershell" => {
                command.args(["-NoProfile", "-NonInteractive", "-Command", command_line]);
            }
            "cmd" => {
                command.arg("/C").raw_arg(command_line);
            }
            // POSIX-like shells (bash, zsh, etc.) on Windows (e.g. Cygwin/MSYS2)
            _ => {
                command.args(["-c", command_line]);
            }
        }
        if let Some(path) = working_dir {
            command.current_dir(path);
        }
        if let Some(path) = login_path {
            command.env("PATH", path);
        }
        command
    };

    #[cfg(not(windows))]
    let mut command = {
        let (shell, is_user_configured) = unix_shell();

        if is_flatpak() {
            let mut command = flatpak_spawn_command();
            if let Some(dir) = working_dir {
                command.arg(format!("--directory={}", dir.display()));
            }
            if let Some(path) = login_path {
                command.arg(format!("--env=PATH={}", path));
            }
            // If GOOSE_SHELL was explicitly set, honour the full path (likely a host
            // binary). Otherwise use basename so the host's PATH resolves it.
            command
                .arg(flatpak_shell_arg(&shell, is_user_configured))
                .arg("-c")
                .arg(command_line);
            command
        } else {
            let mut command = tokio::process::Command::new(shell);
            command.arg("-c").arg(command_line);
            if let Some(path) = working_dir {
                command.current_dir(path);
            }
            if let Some(path) = login_path {
                command.env("PATH", path);
            }
            command
        }
    };

    command.set_no_window();
    command
}

/// Split tagged lines into (stdout, stderr, interleaved) strings.
fn split_lines(lines: &[(bool, String)]) -> (String, String, String) {
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut interleaved = String::new();
    let mut stdout_started = false;
    let mut stderr_started = false;
    for (i, (is_stderr, text)) in lines.iter().enumerate() {
        if i > 0 {
            interleaved.push('\n');
        }
        interleaved.push_str(text);
        let (target, started) = if *is_stderr {
            (&mut stderr, &mut stderr_started)
        } else {
            (&mut stdout, &mut stdout_started)
        };
        if *started {
            target.push('\n');
        }
        *started = true;
        target.push_str(text);
    }
    (stdout, stderr, interleaved)
}

/// Collect lines from stdout and stderr and send `(is_stderr, line)` tuples to `tx`.
async fn collect_tagged_lines(
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
    tx: tokio::sync::mpsc::UnboundedSender<(bool, String)>,
) -> Result<(), std::io::Error> {
    let stdout_lines = SplitStream::new(BufReader::new(stdout).split(b'\n')).map(|l| (false, l));
    let stderr_lines = SplitStream::new(BufReader::new(stderr).split(b'\n')).map(|l| (true, l));
    let mut merged = stdout_lines.merge(stderr_lines);

    while let Some((is_stderr, line)) = merged.next().await {
        let line = line?;
        let _ = tx.send((is_stderr, String::from_utf8_lossy(&line).into_owned()));
    }
    Ok(())
}

/// Build a human-readable truncation notice with platform-appropriate commands.
fn truncation_notice(info: &TruncationInfo) -> String {
    let path = info.path.display();
    let commands = if cfg!(windows) {
        "PowerShell commands like `Get-Content -TotalCount 200`, `Select-String`, or \
         `Get-Content | Select-Object -Skip 100 -First 100`"
    } else {
        "shell commands like `head`, `tail`, or `sed -n '100,200p'`"
    };
    format!(
        "[{reason} Full output saved to {path}. \
         Read it with {commands} up to {limit} lines at a time.]",
        reason = info.reason,
        limit = OUTPUT_LIMIT_LINES,
    )
}

fn render_output(
    full_output: &str,
    label: &str,
    output_dir: &std::path::Path,
) -> Result<TruncateResult, String> {
    if full_output.is_empty() {
        return Ok(TruncateResult {
            text: "(no output)".to_string(),
            truncation: None,
        });
    }
    truncate_output(full_output, label, output_dir)
}

fn truncate_output(
    full_output: &str,
    label: &str,
    output_dir: &std::path::Path,
) -> Result<TruncateResult, String> {
    let lines: Vec<&str> = full_output.split('\n').collect();
    let total_lines = lines.len();
    let total_bytes = full_output.len();

    let exceeded_lines = total_lines > OUTPUT_LIMIT_LINES;
    let exceeded_bytes = total_bytes > OUTPUT_LIMIT_BYTES;

    if !exceeded_lines && !exceeded_bytes {
        return Ok(TruncateResult {
            text: full_output.to_string(),
            truncation: None,
        });
    }

    let output_path = save_full_output(full_output, label, output_dir)?;

    let preview_start = total_lines.saturating_sub(OUTPUT_PREVIEW_LINES);
    let preview = lines[preview_start..].join("\n");

    let reason = if exceeded_lines {
        format!("Output exceeded {OUTPUT_LIMIT_LINES} line limit ({total_lines} lines total).")
    } else {
        format!(
            "Output exceeded {} byte limit ({total_bytes} bytes total).",
            OUTPUT_LIMIT_BYTES
        )
    };

    Ok(TruncateResult {
        text: preview,
        truncation: Some(TruncationInfo {
            path: output_path,
            reason,
        }),
    })
}

fn save_full_output(
    output: &str,
    label: &str,
    output_dir: &std::path::Path,
) -> Result<PathBuf, String> {
    let path = output_dir.join(label);
    std::fs::write(&path, output).map_err(|e| format!("Failed to write output buffer: {e}"))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::RawContent;

    fn extract_text(result: &CallToolResult) -> &str {
        match &result.content[0].raw {
            RawContent::Text(text) => &text.text,
            _ => panic!("expected text"),
        }
    }

    fn extract_shell_output(result: &CallToolResult) -> ShellOutput {
        let value = result
            .structured_content
            .clone()
            .expect("expected structured content");
        serde_json::from_value(value).expect("expected shell output structured content")
    }

    #[tokio::test]
    async fn shell_executes_command() {
        let tool = ShellTool::new_for_test().unwrap();
        let result = tool
            .shell(ShellParams {
                command: "echo hello".to_string(),
                timeout_secs: None,
            })
            .await;

        assert_eq!(result.is_error, Some(false));
        assert!(extract_text(&result).contains("hello"));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn shell_returns_error_for_non_zero_exit() {
        let tool = ShellTool::new_for_test().unwrap();
        let result = tool
            .shell(ShellParams {
                command: "echo fail && exit 7".to_string(),
                timeout_secs: None,
            })
            .await;

        assert_eq!(result.is_error, Some(true));
        assert!(extract_text(&result).contains("Command exited with code 7"));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn shell_uses_working_dir_for_relative_execution() {
        let dir = tempfile::tempdir().unwrap();
        let tool = ShellTool::new_for_test().unwrap();
        let result = tool
            .shell_with_cwd(
                ShellParams {
                    command: "pwd".to_string(),
                    timeout_secs: None,
                },
                Some(dir.path()),
            )
            .await;

        assert_eq!(result.is_error, Some(false));
        let observed = std::fs::canonicalize(extract_text(&result)).unwrap();
        let expected = std::fs::canonicalize(dir.path()).unwrap();
        assert_eq!(observed, expected);
    }

    #[test]
    fn render_output_returns_full_output_when_under_limit() {
        let dir = tempfile::tempdir().unwrap();
        let input = (0..100)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");

        let result = render_output(&input, "test", dir.path()).unwrap();
        assert_eq!(result.text, input);
        assert!(result.truncation.is_none());
    }

    #[test]
    fn render_output_shows_empty_message() {
        let dir = tempfile::tempdir().unwrap();
        let result = render_output("", "test", dir.path()).unwrap();
        assert_eq!(result.text, "(no output)");
        assert!(result.truncation.is_none());
    }

    #[test]
    fn render_output_truncates_when_lines_exceeded() {
        let dir = tempfile::tempdir().unwrap();
        let input = (0..2500)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");

        let result = render_output(&input, "test_lines", dir.path()).unwrap();
        let preview = &result.text;

        assert_eq!(preview.lines().count(), OUTPUT_PREVIEW_LINES);
        assert!(preview.starts_with("line 2450"));
        assert!(preview.contains("line 2499"));

        let info = result
            .truncation
            .as_ref()
            .expect("expected truncation info");
        assert!(info.reason.contains("2000 line limit"));
        assert!(info.reason.contains("2500 lines total"));

        let notice = truncation_notice(info);
        assert!(notice.contains("Full output saved to"));
    }

    #[test]
    fn render_output_truncates_when_bytes_exceeded() {
        let dir = tempfile::tempdir().unwrap();
        let long_line = "x".repeat(1000);
        let input = (0..100)
            .map(|_| long_line.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(input.len() > OUTPUT_LIMIT_BYTES);
        assert!(input.lines().count() <= OUTPUT_LIMIT_LINES);

        let result = render_output(&input, "test_bytes", dir.path()).unwrap();
        let info = result
            .truncation
            .as_ref()
            .expect("expected truncation info");
        assert!(info.reason.contains("byte limit"));
        assert!(info.reason.contains("bytes total"));

        let notice = truncation_notice(info);
        assert!(notice.contains("Full output saved to"));
    }

    #[test]
    fn save_full_output_reuses_same_path() {
        let dir = tempfile::tempdir().unwrap();
        let path1 = save_full_output("first", "test_reuse", dir.path()).unwrap();
        let path2 = save_full_output("second", "test_reuse", dir.path()).unwrap();
        assert_eq!(path1, path2);
        // Note: we intentionally don't assert file content here because
        // parallel tests (render_output_truncates_*) share the same static
        // temp file and can overwrite the content between our write and read.
    }

    #[test]
    fn save_full_output_uses_separate_files_per_label() {
        let dir = tempfile::tempdir().unwrap();
        let path_a = save_full_output("aaa", "label_a", dir.path()).unwrap();
        let path_b = save_full_output("bbb", "label_b", dir.path()).unwrap();
        assert_ne!(path_a, path_b);
        assert_eq!(std::fs::read_to_string(&path_a).unwrap(), "aaa");
        assert_eq!(std::fs::read_to_string(&path_b).unwrap(), "bbb");
    }

    #[test]
    fn call_index_cycles_through_slots() {
        let tool = ShellTool::new_for_test().unwrap();
        for _cycle in 0..3 {
            for expected in 0..OUTPUT_SLOTS {
                let slot = tool.call_index.fetch_add(1, Ordering::Relaxed) % OUTPUT_SLOTS;
                assert_eq!(slot, expected);
            }
        }
    }

    #[test]
    fn concurrent_calls_get_distinct_slots() {
        let tool = ShellTool::new_for_test().unwrap();
        let mut slots: Vec<usize> = (0..OUTPUT_SLOTS)
            .map(|_| tool.call_index.fetch_add(1, Ordering::Relaxed) % OUTPUT_SLOTS)
            .collect();
        slots.sort();
        let expected: Vec<usize> = (0..OUTPUT_SLOTS).collect();
        assert_eq!(slots, expected);
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn shell_does_not_hang_on_backgrounded_process() {
        struct KillOnDrop(String);
        impl Drop for KillOnDrop {
            fn drop(&mut self) {
                let _ = std::process::Command::new("kill")
                    .args(["-9", &self.0])
                    .status();
            }
        }

        let tool = ShellTool::new_for_test().unwrap();
        let start = std::time::Instant::now();
        let result = tool
            .shell(ShellParams {
                command: "echo before && sleep 300 & echo bgpid:$! && echo after".to_string(),
                timeout_secs: None,
            })
            .await;

        assert!(
            start.elapsed().as_secs() < 10,
            "shell tool should return quickly, not wait for backgrounded sleep"
        );
        assert_eq!(result.is_error, Some(false));
        let text = extract_text(&result);
        let shell_output = extract_shell_output(&result);
        let background_pid = text
            .lines()
            .find_map(|line| line.strip_prefix("bgpid:"))
            .map(str::trim)
            .expect("expected bgpid in output");
        let _cleanup = KillOnDrop(background_pid.to_string());
        assert!(
            shell_output.output_truncated,
            "backgrounded process should set output_truncated"
        );
        assert!(
            shell_output.output_collection_error.is_none(),
            "timeout-based truncation should not set output collection error"
        );
        assert!(
            text.contains("before"),
            "should capture output before background cmd"
        );
        assert!(
            text.contains("after"),
            "should capture output after background cmd"
        );
    }
}
