use std::collections::VecDeque;
#[cfg(windows)]
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::Stream;
use rmcp::model::{Annotations, CallToolResult, ContentBlock, TextContent};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;

use crate::agents::tool_execution::ToolCallNotificationEmitter;
use crate::subprocess::SubprocessExt;

pub use super::shell_output_streaming::{
    parse_shell_output_notification, ShellOutputNotificationChunk, ShellOutputNotificationParams,
    ShellOutputStream, DEVELOPER_SHELL_OUTPUT_NOTIFICATION_METHOD,
};
use super::shell_output_streaming::{ShellOutputBatcher, SHELL_LIVE_OUTPUT_FLUSH_INTERVAL};
use super::shell_process::ShellProcess;

/// Check if the current process is running inside a Flatpak sandbox.
///
/// When inside Flatpak, shell commands must be wrapped with `flatpak-spawn --host`
/// to execute on the host system rather than inside the sandbox.
#[cfg(not(windows))]
pub(crate) fn is_flatpak() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

#[cfg(not(windows))]
const FLATPAK_HOST_ARGS: [&str; 2] = ["--host", "--watch-bus"];

/// `flatpak-spawn --host` runs commands with the session helper's environment, not
/// ours, and nothing outside the sandbox can resolve the host's login-shell PATH
/// for us, so pass it along explicitly.
#[cfg(not(windows))]
pub(crate) async fn flatpak_spawn_command() -> tokio::process::Command {
    static HOST_LOGIN_PATH: tokio::sync::OnceCell<Option<String>> =
        tokio::sync::OnceCell::const_new();
    let host_path = HOST_LOGIN_PATH.get_or_init(resolve_host_login_path).await;

    let mut command = tokio::process::Command::new("flatpak-spawn");
    command.args(FLATPAK_HOST_ARGS);
    if let Some(path) = host_path {
        command.arg(format!("--env=PATH={path}"));
    }
    command
}

/// Flatpak sets `SHELL=/bin/sh` inside the sandbox, so the login shell has to come
/// from the host's account database.
#[cfg(not(windows))]
async fn resolve_host_login_path() -> Option<String> {
    let passwd = run_on_host(&["sh", "-c", r#"getent passwd "$(id -un)""#]).await;
    let shell = passwd
        .as_deref()
        .and_then(|entry| entry.split(':').nth(6))
        .filter(|shell| !shell.is_empty())
        .unwrap_or("bash");
    run_on_host(&[shell, "-l", "-i", "-c", "printenv PATH"]).await
}

#[cfg(not(windows))]
async fn run_on_host(args: &[&str]) -> Option<String> {
    let mut command = tokio::process::Command::new("flatpak-spawn");
    command
        .args(FLATPAK_HOST_ARGS)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(5), command.output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map(str::to_string)
}

#[cfg(not(windows))]
fn unix_shell_command_args(command_line: &str) -> [&str; 2] {
    ["-c", command_line]
}

/// Resolve the shell used to run Developer extension commands on Windows,
/// respecting `GOOSE_SHELL`.
///
/// Defaults to `cmd` when `GOOSE_SHELL` is unset. The invocation flags are
/// chosen automatically from the executable name in `build_shell_command`,
/// so callers only ever provide a bare executable path or name — see that
/// function for the flag mapping.
#[cfg(windows)]
fn windows_shell() -> String {
    std::env::var("GOOSE_SHELL").unwrap_or_else(|_| "cmd".to_string())
}

/// Short, human-readable name of a shell path (the file stem), used both to
/// pick the right argument style on Windows and to tell the LLM which
/// dialect to write in the tool description.
#[cfg(windows)]
fn shell_basename(shell: &str) -> String {
    Path::new(shell)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("cmd")
        .to_lowercase()
}

#[cfg(not(windows))]
fn shell_basename(shell: &str) -> String {
    std::path::Path::new(shell)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(shell)
        .to_string()
}

/// Basename of the shell the `shell` tool will invoke, for use in the tool
/// description so the LLM knows which dialect to write.
#[cfg(windows)]
pub fn shell_display_name() -> String {
    shell_basename(&windows_shell())
}

#[cfg(not(windows))]
pub fn shell_display_name() -> String {
    shell_basename(&unix_shell())
}

/// The shell tool runs commands with `-c "..."`, and LLMs routinely emit
/// POSIX-style patterns such as heredocs (`cat <<EOF > file`), `$VAR`
/// expansion, and `2>&1` redirection. Non-POSIX shells (fish, csh, tcsh,
/// nu, ...) reject or mis-interpret these, so we don't auto-select based
/// on `$SHELL`: we check whether `bash` is on PATH and otherwise fall back
/// to `sh`. Users who really want their login shell can opt in via
/// `GOOSE_SHELL`.
#[cfg(not(windows))]
fn unix_shell() -> String {
    if let Ok(shell) = std::env::var("GOOSE_SHELL") {
        return shell;
    }
    if which::which("bash").is_ok() {
        "bash".to_string()
    } else {
        "sh".to_string()
    }
}

const OUTPUT_LIMIT_LINES: usize = 2000;
const OUTPUT_LIMIT_BYTES: usize = 50_000;
const OUTPUT_PREVIEW_LINES: usize = 50;
const OUTPUT_PREVIEW_BYTES: usize = 10_000;

const OUTPUT_SLOTS: usize = 8;
const DEFAULT_CAPTURE_LIMIT_BYTES: usize = 10 * 1024 * 1024;
/// Large enough that the biggest possible record always fits in half the budget.
const MIN_CAPTURE_LIMIT_BYTES: usize = 1024;
const MAX_SEGMENT_BYTES: usize = 64 * 1024;
/// Counted against the capture limit so output made of many tiny lines is bounded too.
const RECORD_OVERHEAD_BYTES: usize = 32;
const REPLACEMENT_CHAR_LEN_UTF8: usize = '\u{FFFD}'.len_utf8();

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
    is_fragment: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShellParams {
    pub command: String,
    /// Maximum time in seconds to allow the command to run before it is killed.
    /// If omitted, defaults to DEFAULT_EXTENSION_TIMEOUT.
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
    /// Bytes dropped between the retained beginning and end of the output because
    /// the capture limit was exceeded. Non-zero means `stdout` and `stderr` are fragments.
    #[serde(default)]
    #[serde(skip_serializing_if = "is_zero")]
    pub capture_dropped_bytes: usize,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

pub struct ShellTool {
    output_dir: tempfile::TempDir,
    call_index: AtomicUsize,
}

impl ShellTool {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            output_dir: tempfile::tempdir()?,
            call_index: AtomicUsize::new(0),
        })
    }

    fn visible_text(text: impl Into<String>) -> ContentBlock {
        ContentBlock::Text(
            TextContent::new(text).with_annotations(Annotations::default().with_priority(0.0)),
        )
    }

    pub async fn shell(&self, params: ShellParams) -> CallToolResult {
        self.shell_with_cwd(params, None, None, CancellationToken::new())
            .await
    }

    pub async fn shell_with_cwd(
        &self,
        params: ShellParams,
        working_dir: Option<&std::path::Path>,
        session_id: Option<&str>,
        cancellation_token: CancellationToken,
    ) -> CallToolResult {
        self.shell_with_cwd_and_emitter(params, working_dir, session_id, None, cancellation_token)
            .await
    }

    pub(crate) async fn shell_with_cwd_and_emitter(
        &self,
        params: ShellParams,
        working_dir: Option<&std::path::Path>,
        session_id: Option<&str>,
        notification_emitter: Option<ToolCallNotificationEmitter>,
        cancellation_token: CancellationToken,
    ) -> CallToolResult {
        if params.command.trim().is_empty() {
            return Self::error_result("Command cannot be empty.", None);
        }

        #[cfg(windows)]
        if shell_basename(&windows_shell()) == "cmd" && params.command.contains(['\n', '\r']) {
            return Self::error_result(
                "cmd.exe silently truncates commands at newlines — only the first line executes, \
                 with exit code 0. Use `&` to chain commands on one line \
                 (e.g. `echo a & echo b`), or set GOOSE_SHELL=powershell.",
                None,
            );
        }

        let execution = match run_command(
            &params.command,
            params.timeout_secs,
            working_dir,
            session_id,
            notification_emitter,
            cancellation_token,
        )
        .await
        {
            Ok(execution) => execution,
            Err(error) => return Self::error_result(&error, None),
        };

        let capture_dropped_bytes = execution.capture_dropped_bytes();
        let (raw_stdout, raw_stderr, interleaved) = split_records(
            &execution.records,
            execution.head_record_count,
            execution.dropped_stdout_bytes,
            execution.dropped_stderr_bytes,
        );

        let output_dir = self.output_dir.path();
        let slot = self.call_index.fetch_add(1, Ordering::Relaxed) % OUTPUT_SLOTS;
        let stdout_result = if raw_stdout.is_empty() {
            TruncateResult {
                text: String::new(),
                truncation: None,
            }
        } else {
            match truncate_output(
                &raw_stdout,
                &format!("stdout-{slot}"),
                output_dir,
                execution.dropped_stdout_bytes > 0,
            ) {
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
            match truncate_output(
                &raw_stderr,
                &format!("stderr-{slot}"),
                output_dir,
                execution.dropped_stderr_bytes > 0,
            ) {
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
            capture_dropped_bytes,
        };
        let structured_content = serde_json::to_value(&shell_output).ok();
        let render_result = match render_output(
            &interleaved,
            &format!("output-{slot}"),
            output_dir,
            capture_dropped_bytes > 0,
        ) {
            Ok(r) => r,
            Err(error) => return Self::error_result(&error, None),
        };
        let mut rendered = render_result.text;

        let truncation_notices: Vec<String> = [
            &stdout_result.truncation,
            &stderr_result.truncation,
            &render_result.truncation,
        ]
        .iter()
        .filter_map(|t| t.as_ref().map(truncation_notice))
        .collect();

        let is_error = if execution.timed_out {
            rendered.push_str(&format!(
                "\n\nCommand timed out after {} seconds",
                resolve_shell_timeout(params.timeout_secs)
            ));
            true
        } else {
            execution.exit_code.unwrap_or(1) != 0
        };

        if capture_dropped_bytes > 0 {
            rendered.push_str(&format!(
                "\n\n[Capture limit: {capture_dropped_bytes} bytes of output were not captured. \
                 The beginning and end of the output are preserved.]"
            ));
        }
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
            let mut error_blocks = vec![Self::visible_text(rendered)];
            if !truncation_notices.is_empty() {
                error_blocks.push(Self::visible_text(truncation_notices.join("\n")));
            }
            let mut result = CallToolResult::error(error_blocks);
            result.structured_content = structured_content;
            return result;
        }

        let mut content_blocks = vec![Self::visible_text(rendered)];
        if !truncation_notices.is_empty() {
            content_blocks.push(Self::visible_text(truncation_notices.join("\n")));
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
            capture_dropped_bytes: 0,
        };
        let mut result = CallToolResult::error(vec![Self::visible_text(message)]);
        result.structured_content = serde_json::to_value(&shell_output).ok();
        result
    }
}

/// A piece of captured output. A line longer than the segment limit is split
/// into several records; every record except the last one of such a line has
/// `ends_line == false`, so the pieces can be rejoined without inserting newlines.
struct CaptureRecord {
    is_stderr: bool,
    text: String,
    ends_line: bool,
}

impl CaptureRecord {
    fn output_bytes(&self) -> usize {
        self.text.len() + usize::from(self.ends_line)
    }

    fn budget_cost(&self) -> usize {
        self.output_bytes() + RECORD_OVERHEAD_BYTES
    }
}

struct ExecutionOutput {
    records: Vec<CaptureRecord>,
    head_record_count: usize,
    dropped_stdout_bytes: usize,
    dropped_stderr_bytes: usize,
    exit_code: Option<i32>,
    timed_out: bool,
    output_truncated: bool,
    output_collection_error: Option<String>,
}

impl ExecutionOutput {
    fn capture_dropped_bytes(&self) -> usize {
        self.dropped_stdout_bytes + self.dropped_stderr_bytes
    }
}

#[derive(Default)]
struct CaptureBuffer {
    half_budget: usize,
    head: Vec<CaptureRecord>,
    tail: VecDeque<CaptureRecord>,
    head_cost: usize,
    tail_cost: usize,
    dropped_stdout_bytes: usize,
    dropped_stderr_bytes: usize,
}

impl CaptureBuffer {
    fn new(capture_limit: usize) -> Self {
        Self {
            half_budget: capture_limit / 2,
            ..Self::default()
        }
    }

    fn push(&mut self, record: CaptureRecord) {
        let cost = record.budget_cost();
        if self.tail.is_empty() && self.head_cost + cost <= self.half_budget {
            self.head_cost += cost;
            self.head.push(record);
            return;
        }
        while self.tail_cost + cost > self.half_budget {
            let Some(evicted) = self.tail.pop_front() else {
                break;
            };
            self.tail_cost -= evicted.budget_cost();
            let dropped = if evicted.is_stderr {
                &mut self.dropped_stderr_bytes
            } else {
                &mut self.dropped_stdout_bytes
            };
            *dropped += evicted.output_bytes();
        }
        self.tail_cost += cost;
        self.tail.push_back(record);
    }
}

fn capture_limit_bytes() -> usize {
    std::env::var("GOOSE_SHELL_CAPTURE_LIMIT_BYTES")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(DEFAULT_CAPTURE_LIMIT_BYTES)
        .max(MIN_CAPTURE_LIMIT_BYTES)
}

/// Each invalid byte decodes to a 3-byte replacement character, so a segment must
/// be a third of the budget it is allowed to occupy once decoded.
fn max_segment_bytes(capture_limit: usize) -> usize {
    ((capture_limit / 2).saturating_sub(RECORD_OVERHEAD_BYTES + 1) / REPLACEMENT_CHAR_LEN_UTF8)
        .clamp(1, MAX_SEGMENT_BYTES)
}

fn resolve_shell_timeout(timeout_secs: Option<u64>) -> u64 {
    timeout_secs.unwrap_or_else(|| {
        crate::config::Config::global()
            .get_goose_default_extension_timeout()
            .unwrap_or(crate::config::DEFAULT_EXTENSION_TIMEOUT)
    })
}

async fn run_command(
    command_line: &str,
    timeout_secs: Option<u64>,
    working_dir: Option<&std::path::Path>,
    session_id: Option<&str>,
    notification_emitter: Option<ToolCallNotificationEmitter>,
    cancellation_token: CancellationToken,
) -> Result<ExecutionOutput, String> {
    let capture_limit = capture_limit_bytes();
    let deadline = Some(resolve_shell_timeout(timeout_secs))
        .filter(|secs| *secs > 0)
        .map(|secs| tokio::time::Instant::now() + Duration::from_secs(secs));
    let not_started = |timed_out| ExecutionOutput {
        records: Vec::new(),
        head_record_count: 0,
        dropped_stdout_bytes: 0,
        dropped_stderr_bytes: 0,
        exit_code: None,
        timed_out,
        output_truncated: false,
        output_collection_error: None,
    };

    let mut command = tokio::select! {
        biased;
        _ = cancellation_token.cancelled() => return Ok(not_started(false)),
        _ = sleep_until_deadline(deadline) => return Ok(not_started(true)),
        command = build_shell_command(command_line, working_dir, session_id) => command,
    };

    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.stdin(Stdio::null());

    let mut process = ShellProcess::spawn(&mut command)
        .map_err(|error| format!("Failed to spawn shell command: {}", error))?;

    let child_stdout = process
        .child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture stdout".to_string())?;
    let child_stderr = process
        .child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture stderr".to_string())?;

    let capture = Arc::new(Mutex::new(CaptureBuffer::new(capture_limit)));
    let output_task = AbortOnDropHandle::new(tokio::spawn(collect_tagged_lines(
        child_stdout,
        child_stderr,
        capture.clone(),
        notification_emitter,
        max_segment_bytes(capture_limit),
    )));
    let abort_handle = output_task.abort_handle();

    let mut timed_out = false;
    let exit_code = tokio::select! {
        result = process.wait() => result
            .map_err(|error| format!("Failed waiting on shell command: {}", error))?
            .code(),
        _ = sleep_until_deadline(deadline) => {
            timed_out = true;
            process.kill().await
                .map_err(|error| format!("Failed to stop shell command: {}", error))?;
            None
        }
        _ = cancellation_token.cancelled() => {
            process.kill().await
                .map_err(|error| format!("Failed to stop shell command: {}", error))?;
            None
        }
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

    let captured = std::mem::take(&mut *capture.lock().unwrap());
    let head_record_count = captured.head.len();
    let mut records = captured.head;
    records.extend(captured.tail);

    Ok(ExecutionOutput {
        records,
        head_record_count,
        dropped_stdout_bytes: captured.dropped_stdout_bytes,
        dropped_stderr_bytes: captured.dropped_stderr_bytes,
        exit_code,
        timed_out,
        output_truncated,
        output_collection_error,
    })
}

async fn sleep_until_deadline(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// Build the `Command` that executes a single command line via the configured shell.
///
/// The invocation flags are selected automatically from the shell executable
/// name, so setting `GOOSE_SHELL` to a bare executable is enough — users never
/// need to supply command-style flags themselves:
///
/// - PowerShell (`pwsh`, `powershell`) → `-NoProfile -NonInteractive -Command`
/// - cmd (`cmd`) → `/C`
/// - POSIX shells (bash, zsh, … via Cygwin/MSYS2) → `-c`
///
/// On Unix the default shell (`bash`, falling back to `sh`) is likewise invoked
/// as `<shell> -c <line>`.
async fn build_shell_command(
    command_line: &str,
    working_dir: Option<&std::path::Path>,
    session_id: Option<&str>,
) -> tokio::process::Command {
    #[cfg(windows)]
    let mut command = {
        let shell = windows_shell();
        let shell_stem = shell_basename(&shell);
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
        command
    };

    #[cfg(not(windows))]
    let mut command = {
        let shell = unix_shell();

        if is_flatpak() {
            flatpak_shell_command(&shell, command_line, working_dir, session_id).await
        } else {
            let mut command = tokio::process::Command::new(shell);
            command.args(unix_shell_command_args(command_line));
            if let Some(path) = working_dir {
                command.current_dir(path);
            }
            apply_session_environment(&mut command, session_id);
            command
        }
    };

    #[cfg(windows)]
    apply_session_environment(&mut command, session_id);
    command.set_no_window();
    command
}

#[cfg(not(windows))]
async fn flatpak_shell_command(
    shell: &str,
    command_line: &str,
    working_dir: Option<&std::path::Path>,
    session_id: Option<&str>,
) -> tokio::process::Command {
    let mut command = flatpak_spawn_command().await;
    if let Some(dir) = working_dir {
        command.arg(format!("--directory={}", dir.display()));
    }
    apply_flatpak_session_environment(&mut command, session_id);
    command
        .arg(shell)
        .args(unix_shell_command_args(command_line));
    command
}

fn apply_session_environment(command: &mut tokio::process::Command, session_id: Option<&str>) {
    if let Some(session_id) = session_id.filter(|id| !id.is_empty()) {
        command.env("AGENT_SESSION_ID", session_id);
    } else {
        command.env_remove("AGENT_SESSION_ID");
    }
}

#[cfg(not(windows))]
fn apply_flatpak_session_environment(
    command: &mut tokio::process::Command,
    session_id: Option<&str>,
) {
    if let Some(session_id) = session_id.filter(|id| !id.is_empty()) {
        command.arg(format!("--env=AGENT_SESSION_ID={session_id}"));
    } else {
        command.arg("--unset-env=AGENT_SESSION_ID");
    }
}

#[derive(Default)]
struct JoinedText {
    text: String,
    wrote_any: bool,
    open_line: bool,
}

impl JoinedText {
    fn push(&mut self, record: &CaptureRecord) {
        if self.wrote_any && !self.open_line {
            self.text.push('\n');
        }
        self.text.push_str(&record.text);
        self.wrote_any = true;
        self.open_line = !record.ends_line;
    }

    fn mark_gap(&mut self) {
        self.open_line = false;
    }
}

fn split_records(
    records: &[CaptureRecord],
    head_record_count: usize,
    dropped_stdout_bytes: usize,
    dropped_stderr_bytes: usize,
) -> (String, String, String) {
    let dropped_bytes = dropped_stdout_bytes + dropped_stderr_bytes;
    let notice = format!(
        "[Capture limit reached: {dropped_bytes} bytes dropped. \
         Showing the beginning and end of the output.]"
    );
    let mut stdout = JoinedText::default();
    let mut stderr = JoinedText::default();
    let mut interleaved = JoinedText::default();
    let mut open_stream: Option<bool> = None;

    let push_notice = |interleaved: &mut JoinedText, open_stream: &mut Option<bool>| {
        if interleaved.wrote_any {
            interleaved.text.push('\n');
        }
        interleaved.text.push_str(&notice);
        interleaved.wrote_any = true;
        interleaved.open_line = false;
        *open_stream = None;
    };

    for (index, record) in records.iter().enumerate() {
        if dropped_bytes > 0 && index == head_record_count {
            push_notice(&mut interleaved, &mut open_stream);
            if dropped_stdout_bytes > 0 {
                stdout.mark_gap();
            }
            if dropped_stderr_bytes > 0 {
                stderr.mark_gap();
            }
        }

        interleaved.open_line = open_stream == Some(record.is_stderr);
        interleaved.push(record);
        open_stream = (!record.ends_line).then_some(record.is_stderr);

        if record.is_stderr {
            stderr.push(record);
        } else {
            stdout.push(record);
        }
    }
    if dropped_bytes > 0 && head_record_count >= records.len() {
        push_notice(&mut interleaved, &mut open_stream);
    }

    (stdout.text, stderr.text, interleaved.text)
}

struct RawSegment {
    bytes: Vec<u8>,
    ends_line: bool,
}

fn incomplete_utf8_suffix_len(bytes: &[u8]) -> usize {
    for back in 1..=bytes.len().min(3) {
        let byte = bytes[bytes.len() - back];
        if byte & 0xC0 == 0x80 {
            continue;
        }
        let sequence_len = match byte {
            0xF0.. => 4,
            0xE0.. => 3,
            0xC0.. => 2,
            _ => 1,
        };
        return if sequence_len > back { back } else { 0 };
    }
    0
}

/// A trailing `\r` stays pending so a CRLF split across segments is still seen as one line ending.
fn take_splittable_prefix(pending: &mut Vec<u8>) -> Option<Vec<u8>> {
    let mut cut = pending.len() - incomplete_utf8_suffix_len(pending);
    if cut > 0 && pending[cut - 1] == b'\r' {
        cut -= 1;
    }
    if cut == 0 {
        return None;
    }
    let rest = pending.split_off(cut);
    Some(std::mem::replace(pending, rest))
}

fn bounded_segments<R: AsyncRead + Unpin + Send + 'static>(
    mut reader: R,
    max_segment: usize,
) -> std::pin::Pin<Box<dyn Stream<Item = std::io::Result<RawSegment>> + Send>> {
    Box::pin(async_stream::stream! {
        let mut pending: Vec<u8> = Vec::new();
        let mut buf = vec![0u8; 4096];
        loop {
            match reader.read(&mut buf).await {
                Err(e) => { yield Err(e); break; }
                Ok(0) => {
                    if !pending.is_empty() {
                        yield Ok(RawSegment { bytes: std::mem::take(&mut pending), ends_line: false });
                    }
                    break;
                }
                Ok(n) => {
                    for &byte in &buf[..n] {
                        if byte == b'\n' {
                            if pending.last() == Some(&b'\r') {
                                pending.pop();
                            }
                            yield Ok(RawSegment { bytes: std::mem::take(&mut pending), ends_line: true });
                        } else {
                            pending.push(byte);
                            if pending.len() >= max_segment {
                                if let Some(bytes) = take_splittable_prefix(&mut pending) {
                                    yield Ok(RawSegment { bytes, ends_line: false });
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

async fn collect_tagged_lines(
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
    capture: Arc<Mutex<CaptureBuffer>>,
    notification_emitter: Option<ToolCallNotificationEmitter>,
    max_segment: usize,
) -> Result<(), std::io::Error> {
    let stdout_segments = bounded_segments(stdout, max_segment).map(|s| (false, s));
    let stderr_segments = bounded_segments(stderr, max_segment).map(|s| (true, s));
    let mut merged = stdout_segments.merge(stderr_segments);
    let mut output_batcher = notification_emitter.map(ShellOutputBatcher::new);
    let mut flush_interval = tokio::time::interval_at(
        tokio::time::Instant::now() + SHELL_LIVE_OUTPUT_FLUSH_INTERVAL,
        SHELL_LIVE_OUTPUT_FLUSH_INTERVAL,
    );
    flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            tagged_segment = merged.next() => {
                let Some((is_stderr, segment)) = tagged_segment else {
                    break;
                };
                let segment = match segment {
                    Ok(segment) => segment,
                    Err(error) => {
                        if let Some(output_batcher) = output_batcher.as_mut() {
                            output_batcher.flush();
                        }
                        return Err(error);
                    }
                };
                let record = CaptureRecord {
                    is_stderr,
                    text: String::from_utf8_lossy(&segment.bytes).into_owned(),
                    ends_line: segment.ends_line,
                };

                if let Some(output_batcher) = output_batcher.as_mut() {
                    if output_batcher.push_segment(is_stderr, &record.text, record.ends_line) {
                        flush_interval.reset();
                    }
                }
                capture.lock().unwrap().push(record);
            }
            _ = flush_interval.tick(), if output_batcher.is_some() => {
                if let Some(output_batcher) = output_batcher.as_mut() {
                    output_batcher.flush();
                }
            }
        }
    }

    if let Some(output_batcher) = output_batcher.as_mut() {
        output_batcher.flush();
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
    let saved = if info.is_fragment {
        format!(
            "Captured fragment (beginning and end) saved to {path}; \
             the dropped middle is unrecoverable."
        )
    } else {
        format!("Full output saved to {path}.")
    };
    format!(
        "[{reason} {saved} \
         Read it with {commands} up to {limit} lines at a time.]",
        reason = info.reason,
        limit = OUTPUT_LIMIT_LINES,
    )
}

fn render_output(
    full_output: &str,
    label: &str,
    output_dir: &std::path::Path,
    is_fragment: bool,
) -> Result<TruncateResult, String> {
    if full_output.is_empty() {
        return Ok(TruncateResult {
            text: "(no output)".to_string(),
            truncation: None,
        });
    }
    truncate_output(full_output, label, output_dir, is_fragment)
}

fn truncate_output(
    full_output: &str,
    label: &str,
    output_dir: &std::path::Path,
    is_fragment: bool,
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
    let preview = truncate_preview_bytes(lines[preview_start..].join("\n"));

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
            is_fragment,
        }),
    })
}

/// Keep the preview's tail within OUTPUT_PREVIEW_BYTES so lines without
/// newlines (progress bars using `\r`, minified or base64 content) cannot
/// smuggle an unbounded preview past the line-based truncation.
#[allow(clippy::string_slice)] // The start index is snapped to a char boundary.
fn truncate_preview_bytes(preview: String) -> String {
    if preview.len() <= OUTPUT_PREVIEW_BYTES {
        return preview;
    }
    let mut start = preview.len() - OUTPUT_PREVIEW_BYTES;
    while !preview.is_char_boundary(start) {
        start += 1;
    }
    preview[start..].to_string()
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
    use rmcp::model::{ContentBlock, ServerNotification};

    fn extract_text(result: &CallToolResult) -> &str {
        match &result.content[0] {
            ContentBlock::Text(text) => &text.text,
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
        let tool = ShellTool::new().unwrap();
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
    async fn full_live_notification_channel_does_not_change_final_output() {
        let tool = ShellTool::new().unwrap();
        let (sender, _receiver) = tokio::sync::mpsc::channel(1);
        let result = tool
            .shell_with_cwd_and_emitter(
                ShellParams {
                    command: "printf 'out-1\\nout-2\\n'; printf 'err-1\\nerr-2\\n' >&2".to_string(),
                    timeout_secs: None,
                },
                None,
                None,
                Some(ToolCallNotificationEmitter::new(sender)),
                CancellationToken::new(),
            )
            .await;

        let shell_output = extract_shell_output(&result);
        assert_eq!(result.is_error, Some(false));
        assert_eq!(shell_output.stdout, "out-1\nout-2");
        assert_eq!(shell_output.stderr, "err-1\nerr-2");
        assert_eq!(shell_output.exit_code, Some(0));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn shell_returns_error_for_non_zero_exit() {
        let tool = ShellTool::new().unwrap();
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
        let tool = ShellTool::new().unwrap();
        let result = tool
            .shell_with_cwd(
                ShellParams {
                    command: "pwd".to_string(),
                    timeout_secs: None,
                },
                Some(dir.path()),
                None,
                CancellationToken::new(),
            )
            .await;

        assert_eq!(result.is_error, Some(false));
        let observed = std::fs::canonicalize(extract_text(&result)).unwrap();
        let expected = std::fs::canonicalize(dir.path()).unwrap();
        assert_eq!(observed, expected);
    }

    #[test]
    fn session_environment_is_set_or_removed() {
        for (session_id, expected) in [
            (
                Some("session-123"),
                Some(Some(std::ffi::OsStr::new("session-123"))),
            ),
            (None, Some(None)),
        ] {
            let mut command = tokio::process::Command::new("ignored");
            command.env("AGENT_SESSION_ID", "stale-session");

            apply_session_environment(&mut command, session_id);

            assert_eq!(
                command
                    .as_std()
                    .get_envs()
                    .find_map(|(key, value)| (key == "AGENT_SESSION_ID").then_some(value)),
                expected
            );
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn flatpak_session_environment_is_set_or_unset() {
        for (session_id, expected) in [
            (Some("session-123"), "--env=AGENT_SESSION_ID=session-123"),
            (None, "--unset-env=AGENT_SESSION_ID"),
        ] {
            let mut command = tokio::process::Command::new("flatpak-spawn");

            apply_flatpak_session_environment(&mut command, session_id);

            assert_eq!(
                command
                    .as_std()
                    .get_args()
                    .map(|arg| arg.to_string_lossy().into_owned())
                    .collect::<Vec<_>>(),
                vec![expected]
            );
        }
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn shell_kills_child_on_cancellation() {
        let tool = ShellTool::new().unwrap();
        let token = CancellationToken::new();
        let token_clone = token.clone();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            token_clone.cancel();
        });

        let start = std::time::Instant::now();
        let result = tool
            .shell_with_cwd(
                ShellParams {
                    command: "sleep 30".to_string(),
                    timeout_secs: None,
                },
                None,
                None,
                token,
            )
            .await;

        assert!(
            start.elapsed().as_secs() < 5,
            "shell should return quickly after cancellation, not wait for the command"
        );
        let shell_output = extract_shell_output(&result);
        assert!(
            shell_output.exit_code.is_none(),
            "cancelled process should have no exit code"
        );
    }

    async fn capture_stdout(input: Vec<u8>, capture_limit: usize) -> (String, usize) {
        let mut buffer = CaptureBuffer::new(capture_limit);
        let mut segments = bounded_segments(
            std::io::Cursor::new(input),
            max_segment_bytes(capture_limit),
        );
        while let Some(segment) = segments.next().await {
            let segment = segment.unwrap();
            buffer.push(CaptureRecord {
                is_stderr: false,
                text: String::from_utf8_lossy(&segment.bytes).into_owned(),
                ends_line: segment.ends_line,
            });
        }
        let dropped = buffer.dropped_stdout_bytes;
        let head_count = buffer.head.len();
        let mut records = buffer.head;
        records.extend(buffer.tail);
        (split_records(&records, head_count, dropped, 0).0, dropped)
    }

    #[tokio::test]
    async fn line_longer_than_a_segment_is_rejoined_exactly() {
        let input = vec![b'x'; 200_000];
        let (stdout, dropped) = capture_stdout(input.clone(), 1024 * 1024).await;

        assert_eq!(dropped, 0);
        assert_eq!(stdout.as_bytes(), input);
    }

    #[tokio::test]
    async fn segments_of_invalid_utf8_decode_within_half_the_capture_limit() {
        for capture_limit in [MIN_CAPTURE_LIMIT_BYTES, 1500, 4096] {
            let segments: Vec<_> = bounded_segments(
                std::io::Cursor::new(vec![0xFF; 10_000]),
                max_segment_bytes(capture_limit),
            )
            .map(|segment| segment.unwrap())
            .collect()
            .await;

            assert!(segments.len() > 1);
            for segment in segments {
                let record = CaptureRecord {
                    is_stderr: false,
                    text: String::from_utf8_lossy(&segment.bytes).into_owned(),
                    ends_line: segment.ends_line,
                };
                assert!(record.budget_cost() <= capture_limit / 2);
            }
        }
    }

    fn record(is_stderr: bool, text: &str, ends_line: bool) -> CaptureRecord {
        CaptureRecord {
            is_stderr,
            text: text.to_string(),
            ends_line,
        }
    }

    #[test]
    fn split_records_joins_continuations_without_newlines() {
        let records = [
            record(false, "abc", false),
            record(true, "err", true),
            record(false, "def", false),
            record(false, "ghi", true),
            record(false, "next", true),
        ];
        let (stdout, stderr, interleaved) = split_records(&records, records.len(), 0, 0);

        assert_eq!(stdout, "abcdefghi\nnext");
        assert_eq!(stderr, "err");
        assert_eq!(interleaved, "abc\nerr\ndefghi\nnext");
    }

    #[test]
    fn split_records_keeps_continuations_of_streams_that_lost_nothing() {
        let records = [
            record(true, "err-start-", false),
            record(false, "out-head", true),
            record(false, "out-tail", true),
            record(true, "err-end", true),
        ];
        let (stdout, stderr, _) = split_records(&records, 2, 100, 0);

        assert_eq!(stdout, "out-head\nout-tail");
        assert_eq!(stderr, "err-start-err-end");
    }

    #[tokio::test]
    async fn bounded_segments_split_long_lines_on_character_boundaries() {
        let input = "a\u{20ac}\u{20ac}\u{20ac}b\ncd".as_bytes();
        let segments: Vec<_> = bounded_segments(input, 4)
            .map(|segment| segment.unwrap())
            .collect()
            .await;

        let rebuilt: Vec<(String, bool)> = segments
            .iter()
            .map(|s| (String::from_utf8(s.bytes.clone()).unwrap(), s.ends_line))
            .collect();
        assert_eq!(
            rebuilt,
            [
                ("a\u{20ac}".to_string(), false),
                ("\u{20ac}".to_string(), false),
                ("\u{20ac}b".to_string(), false),
                (String::new(), true),
                ("cd".to_string(), false),
            ]
        );
    }

    #[tokio::test]
    async fn crlf_split_across_segment_boundary_is_not_left_as_trailing_cr() {
        let mut input = vec![b'x'; MAX_SEGMENT_BYTES - 1];
        input.extend_from_slice(b"\r\nafter\n");
        let segments: Vec<_> = bounded_segments(std::io::Cursor::new(input), MAX_SEGMENT_BYTES)
            .map(|s| s.unwrap())
            .collect()
            .await;
        let texts: Vec<(String, bool)> = segments
            .iter()
            .map(|s| (String::from_utf8(s.bytes.clone()).unwrap(), s.ends_line))
            .collect();

        for (text, _) in &texts {
            assert!(!text.ends_with('\r'), "trailing CR in segment: {text:?}");
        }
        let all_text: String = texts
            .iter()
            .map(|(t, ends_line)| {
                if *ends_line {
                    format!("{t}\n")
                } else {
                    t.clone()
                }
            })
            .collect();
        assert!(all_text.contains("after\n"));
    }

    #[test]
    fn buffer_charges_per_record_overhead_and_tracks_dropped_per_stream() {
        let mut buffer = CaptureBuffer::new(2 * 10 * (RECORD_OVERHEAD_BYTES + 1));
        for index in 0..100 {
            buffer.push(record(index % 2 == 0, "", true));
        }

        let retained = buffer.head.len() + buffer.tail.len();
        assert!(retained <= 20, "retained {retained} empty records");
        assert_eq!(
            buffer.dropped_stdout_bytes + buffer.dropped_stderr_bytes,
            100 - retained
        );
        assert!(buffer.dropped_stdout_bytes > 0 && buffer.dropped_stderr_bytes > 0);
    }

    #[test]
    fn capture_limit_below_the_minimum_is_raised() {
        for value in ["0", "1", "1023"] {
            let _env = env_lock::lock_env([("GOOSE_SHELL_CAPTURE_LIMIT_BYTES", Some(value))]);
            assert_eq!(capture_limit_bytes(), MIN_CAPTURE_LIMIT_BYTES);
        }
    }

    #[tokio::test]
    async fn crlf_split_across_segments_is_normalized_in_live_output() {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
        let mut batcher = ShellOutputBatcher::new(ToolCallNotificationEmitter::new(sender));
        let mut segments = bounded_segments(std::io::Cursor::new(b"abc\r\ndef\n".to_vec()), 4);
        while let Some(segment) = segments.next().await {
            let segment = segment.unwrap();
            batcher.push_segment(
                false,
                &String::from_utf8(segment.bytes).unwrap(),
                segment.ends_line,
            );
        }
        batcher.flush();

        let mut output = String::new();
        while let Ok(ServerNotification::CustomNotification(notification)) = receiver.try_recv() {
            for chunk in parse_shell_output_notification(&notification)
                .unwrap()
                .chunks
            {
                output.push_str(&chunk.output);
            }
        }
        assert_eq!(output, "abc\ndef\n");
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn flatpak_commands_carry_the_host_accounts_login_path() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let login_shell = dir.path().join("login-shell");
        let fake_spawn = dir.path().join("flatpak-spawn");
        std::fs::write(
            &login_shell,
            "#!/bin/sh\n[ \"$*\" = '-l -i -c printenv PATH' ] || exit 1\necho 'Welcome back!'\necho\necho /home/me/.cargo/bin:/usr/bin\n",
        )
        .unwrap();
        std::fs::write(
            &fake_spawn,
            format!(
                "#!/bin/sh\n[ \"$1 $2\" = '--host --watch-bus' ] || exit 1\nshift 2\nif [ \"$1\" = sh ]; then echo 'me:x:1000:1000::/home/me:{}'; exit; fi\nexec \"$@\"\n",
                login_shell.display()
            ),
        )
        .unwrap();
        for script in [&login_shell, &fake_spawn] {
            std::fs::set_permissions(script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = format!(
            "{}:{}",
            dir.path().display(),
            std::env::var("PATH").unwrap()
        );
        let _guard = env_lock::lock_env([("PATH", Some(path.as_str()))]);

        let shell_command =
            flatpak_shell_command("bash", "echo hi", Some(dir.path()), Some("session-1")).await;
        let hook_command = crate::hooks::flatpak_hook_command("echo hi", dir.path()).await;

        for command in [&shell_command, &hook_command] {
            let command = command.as_std();
            let args: Vec<_> = command.get_args().collect();
            assert_eq!(command.get_program(), "flatpak-spawn");
            assert!(
                args.iter()
                    .any(|arg| *arg == "--env=PATH=/home/me/.cargo/bin:/usr/bin"),
                "{args:?}"
            );
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_shell_command_args_wrap_commands_for_execution() {
        assert_eq!(unix_shell_command_args("ls -la"), ["-c", "ls -la"]);
    }

    #[test]
    fn render_output_returns_full_output_when_under_limit() {
        let dir = tempfile::tempdir().unwrap();
        let input = (0..100)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");

        let result = render_output(&input, "test", dir.path(), false).unwrap();
        assert_eq!(result.text, input);
        assert!(result.truncation.is_none());
    }

    #[test]
    fn render_output_shows_empty_message() {
        let dir = tempfile::tempdir().unwrap();
        let result = render_output("", "test", dir.path(), false).unwrap();
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

        let result = render_output(&input, "test_lines", dir.path(), false).unwrap();
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

        let result = render_output(&input, "test_bytes", dir.path(), false).unwrap();
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
    fn render_output_preview_is_byte_bounded_for_giant_lines() {
        let dir = tempfile::tempdir().unwrap();
        let input = "x".repeat(200_000);

        let result = render_output(&input, "test_giant_line", dir.path(), false).unwrap();

        assert!(result.text.len() <= OUTPUT_PREVIEW_BYTES);
        let info = result
            .truncation
            .as_ref()
            .expect("expected truncation info");
        assert!(info.reason.contains("byte limit"));
        assert_eq!(std::fs::read_to_string(&info.path).unwrap().len(), 200_000);
    }

    #[test]
    fn truncate_preview_bytes_respects_char_boundaries() {
        let preview = "é".repeat(OUTPUT_PREVIEW_BYTES);
        let truncated = truncate_preview_bytes(preview);
        assert!(truncated.len() <= OUTPUT_PREVIEW_BYTES);
        assert!(truncated.chars().all(|c| c == 'é'));
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
        let tool = ShellTool::new().unwrap();
        for _cycle in 0..3 {
            for expected in 0..OUTPUT_SLOTS {
                let slot = tool.call_index.fetch_add(1, Ordering::Relaxed) % OUTPUT_SLOTS;
                assert_eq!(slot, expected);
            }
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn cmd_rejects_newline_in_command() {
        for command in ["echo a\necho b", "echo a\r\necho b", "echo a\recho b"] {
            let tool = ShellTool::new().unwrap();
            let result = tool
                .shell(ShellParams {
                    command: command.to_string(),
                    timeout_secs: None,
                })
                .await;
            assert_eq!(
                result.is_error,
                Some(true),
                "expected error for {command:?}"
            );
            assert!(
                extract_text(&result).contains("cmd.exe"),
                "error should mention cmd.exe for {command:?}"
            );
        }
    }

    #[test]
    fn concurrent_calls_get_distinct_slots() {
        let tool = ShellTool::new().unwrap();
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

        let tool = ShellTool::new().unwrap();
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

    #[test]
    fn resolve_shell_timeout_prefers_explicit_value() {
        assert_eq!(resolve_shell_timeout(Some(42)), 42);
    }

    #[test]
    fn resolve_shell_timeout_falls_back_to_a_bound_when_absent() {
        // The key behavioral guarantee: an omitted timeout no longer means
        // "run forever" — it resolves to the default extension timeout.
        assert!(resolve_shell_timeout(None) > 0);
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn shell_kills_hanging_command_after_explicit_timeout() {
        let tool = ShellTool::new().unwrap();
        let start = std::time::Instant::now();
        let result = tool
            .shell(ShellParams {
                command: "sleep 30".to_string(),
                timeout_secs: Some(1),
            })
            .await;

        assert!(
            start.elapsed().as_secs() < 10,
            "shell should return shortly after the timeout, not wait for the command"
        );
        assert_eq!(result.is_error, Some(true));
        let shell_output = extract_shell_output(&result);
        assert!(shell_output.timed_out, "command should be marked timed_out");
        assert!(
            shell_output.exit_code.is_none(),
            "killed process should have no exit code"
        );
        assert!(extract_text(&result).contains("Command timed out after 1 seconds"));
    }
}
