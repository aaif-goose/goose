#![cfg(unix)]

use goose::agents::platform_extensions::developer::shell::{ShellOutput, ShellParams, ShellTool};
use rmcp::model::CallToolResult;
use serial_test::serial;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

#[cfg(target_os = "linux")]
const EXTINCTION_HELPER_ENV: &str = "GOOSE_TEST_SHELL_EXTINCTION_HELPER";
#[cfg(target_os = "linux")]
const EXTINCTION_MODE_ENV: &str = "GOOSE_TEST_SHELL_EXTINCTION_MODE";
#[cfg(target_os = "linux")]
const EXTINCTION_PID_FILE_ENV: &str = "GOOSE_TEST_SHELL_EXTINCTION_PID_FILE";

#[cfg(target_os = "linux")]
struct HelperProcess(Child);

#[cfg(target_os = "linux")]
impl Drop for HelperProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct ProcessKillGuard(u32);

impl Drop for ProcessKillGuard {
    fn drop(&mut self) {
        if self.0 > 0 && process_exists(self.0) {
            unsafe {
                let _ = libc::kill(self.0 as libc::pid_t, libc::SIGKILL);
            }
        }
    }
}

#[cfg(target_os = "linux")]
#[ctor::ctor]
unsafe fn maybe_run_extinction_helper() {
    if std::env::var_os(EXTINCTION_HELPER_ENV).is_none() {
        return;
    }

    let pid_file_path = match std::env::var(EXTINCTION_PID_FILE_ENV) {
        Ok(path) => PathBuf::from(path),
        Err(_) => libc::_exit(2),
    };

    let mode = std::env::var(EXTINCTION_MODE_ENV).unwrap_or_else(|_| "exit".to_string());

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(_) => libc::_exit(3),
    };

    runtime.block_on(async {
        let tool = match ShellTool::new(false) {
            Ok(t) => t,
            Err(_) => libc::_exit(4),
        };

        let cmd = format!("echo $$ > '{}' && exec sleep 60", pid_file_path.display());
        tokio::spawn(async move {
            let _ = tool
                .shell(ShellParams {
                    command: cmd,
                    timeout_secs: Some(60),
                })
                .await;
        });

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut child_pid = None;
        while Instant::now() < deadline {
            if let Ok(content) = fs::read_to_string(&pid_file_path) {
                let trimmed = content.trim();
                if let Ok(pid) = trimmed.parse::<u32>() {
                    child_pid = Some(pid);
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        let pid = match child_pid {
            Some(p) => p,
            None => libc::_exit(5),
        };

        tokio::time::sleep(Duration::from_millis(50)).await;

        println!("{pid}");
        let _ = std::io::stdout().flush();

        if mode == "park" {
            loop {
                std::thread::park();
            }
        } else {
            libc::_exit(0);
        }
    });
}

fn extract_shell_output(result: &CallToolResult) -> ShellOutput {
    let value = result
        .structured_content
        .as_ref()
        .expect("expected structured content in CallToolResult");
    serde_json::from_value(value.clone()).expect("expected valid ShellOutput JSON")
}

fn extract_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| match block {
            rmcp::model::ContentBlock::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn process_exists(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        if libc::kill(pid as libc::pid_t, 0) == 0 {
            true
        } else {
            let err = std::io::Error::last_os_error().raw_os_error();
            err == Some(libc::EPERM)
        }
    }
}

#[cfg(target_os = "linux")]
fn process_is_running(pid: u32) -> bool {
    match process_state(pid) {
        Some('Z') | None => false,
        Some(_) => true,
    }
}

#[cfg(not(target_os = "linux"))]
fn process_is_running(pid: u32) -> bool {
    process_exists(pid)
}

#[cfg(target_os = "linux")]
fn process_state(pid: u32) -> Option<char> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, after_name) = stat.rsplit_once(") ")?;
    after_name.chars().next()
}

#[tokio::test]
#[serial]
async fn test_shell_runtime_delegation() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let args_log = temp_dir.path().join("args.log");
    let cwd_log = temp_dir.path().join("cwd.log");
    let mock_wrapper = temp_dir.path().join("mock_fencer.sh");
    let work_dir = tempfile::tempdir().expect("work dir");

    let script_content = r#"#!/bin/sh
set -e
args_file="$1"
cwd_file="$2"
shift 2

pwd > "$cwd_file"

: > "$args_file"
for arg in "$@"; do
    printf '%s\n' "$arg" >> "$args_file"
done

while [ "$#" -gt 0 ]; do
    if [ "$1" = "--" ]; then
        shift
        break
    fi
    shift
done

exec "$@"
"#;
    fs::write(&mock_wrapper, script_content).expect("write mock_fencer.sh");
    let mut perms = fs::metadata(&mock_wrapper).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&mock_wrapper, perms).expect("set perms");

    let runtime_config = format!(
        "{} {} {}",
        mock_wrapper.display(),
        args_log.display(),
        cwd_log.display()
    );

    let _env = env_lock::lock_env([
        ("GOOSE_FENCE_RUNTIME", Some(runtime_config.as_str())),
        ("GOOSE_PROCESS_FENCE", None),
    ]);

    let tool = ShellTool::new(false).expect("ShellTool::new");
    let command_line = "echo runtime-out-test && echo runtime-err-test >&2 && pwd";

    let result = tool
        .shell_with_cwd(
            ShellParams {
                command: command_line.to_string(),
                timeout_secs: Some(10),
            },
            Some(work_dir.path()),
            None,
            CancellationToken::new(),
        )
        .await;

    assert_eq!(result.is_error, Some(false));
    let output = extract_shell_output(&result);
    assert_eq!(output.exit_code, Some(0));
    assert!(!output.timed_out);
    assert!(
        output.stdout.contains("runtime-out-test"),
        "expected stdout to contain 'runtime-out-test', got: {}",
        output.stdout
    );
    assert!(
        output.stderr.contains("runtime-err-test"),
        "expected stderr to contain 'runtime-err-test', got: {}",
        output.stderr
    );

    let canonical_work_dir = work_dir.path().canonicalize().expect("canonical work dir");
    assert!(
        output
            .stdout
            .contains(&canonical_work_dir.display().to_string()),
        "expected stdout to contain work dir {}, got: {}",
        canonical_work_dir.display(),
        output.stdout
    );

    let logged_cwd = fs::read_to_string(&cwd_log).expect("read cwd_log");
    let canonical_logged_cwd = Path::new(logged_cwd.trim())
        .canonicalize()
        .expect("canonical logged cwd");
    assert_eq!(
        canonical_logged_cwd, canonical_work_dir,
        "delegation wrapper should preserve current working directory"
    );

    let logged_args = fs::read_to_string(&args_log).expect("read args_log");
    let lines: Vec<&str> = logged_args.lines().collect();
    assert_eq!(
        lines.len(),
        4,
        "expected exactly 4 arguments logged (-- <shell> -c <cmd>), got: {:?}",
        lines
    );
    assert_eq!(lines[0], "--");
    assert!(
        lines[1] == "bash" || lines[1] == "sh",
        "expected shell to be 'bash' or 'sh', got: {}",
        lines[1]
    );
    assert_eq!(lines[2], "-c");
    assert_eq!(lines[3], command_line);

    let visible = extract_text(&result);
    assert!(visible.contains("runtime-out-test"));
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[serial]
async fn test_shell_fencing_standard_privilege_boundary() {
    let _env = env_lock::lock_env([
        ("GOOSE_PROCESS_FENCE", Some("standard")),
        ("GOOSE_FENCE_RUNTIME", None),
    ]);

    let tool = ShellTool::new(false).expect("ShellTool::new");
    let result = tool
        .shell(ShellParams {
            command: "grep NoNewPrivs /proc/self/status".to_string(),
            timeout_secs: Some(10),
        })
        .await;

    assert_eq!(result.is_error, Some(false));
    let output = extract_shell_output(&result);
    assert_eq!(output.exit_code, Some(0));
    assert!(
        output.stdout.contains("NoNewPrivs:\t1") || output.stdout.contains("NoNewPrivs: 1"),
        "expected NoNewPrivs:\t1 in /proc/self/status under standard fencing, got: {}",
        output.stdout
    );
}

/// Verifies strict fencing privilege and isolation boundaries.
///
/// On unprivileged Linux hosts without CAP_SYS_ADMIN, namespace isolation fails closed.
/// On privileged hosts, execution succeeds with PR_SET_NO_NEW_PRIVS verified.
#[cfg(target_os = "linux")]
#[tokio::test]
#[serial]
async fn test_shell_fencing_strict_privilege_boundary() {
    let _env = env_lock::lock_env([
        ("GOOSE_PROCESS_FENCE", Some("strict")),
        ("GOOSE_FENCE_RUNTIME", None),
    ]);

    let tool = ShellTool::new(false).expect("ShellTool::new");
    let result = tool
        .shell(ShellParams {
            command: "grep NoNewPrivs /proc/self/status".to_string(),
            timeout_secs: Some(10),
        })
        .await;

    if result.is_error == Some(true) {
        let text = extract_text(&result);
        assert!(
            text.contains("Operation not permitted")
                || text.contains("Permission denied")
                || text.contains("failed")
                || text.contains("error"),
            "expected fail-closed error message under unprivileged strict mode, got: {}",
            text
        );
        return;
    }

    assert_eq!(result.is_error, Some(false));
    let output = extract_shell_output(&result);
    assert_eq!(output.exit_code, Some(0));
    assert!(
        output.stdout.contains("NoNewPrivs:\t1") || output.stdout.contains("NoNewPrivs: 1"),
        "expected NoNewPrivs:\t1 in /proc/self/status under strict fencing, got: {}",
        output.stdout
    );
}

#[cfg(target_os = "linux")]
#[test]
#[serial]
fn test_shell_subprocess_extinction_on_parent_exit() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let pid_file = temp_dir.path().join("child.pid");

    let current_exe = std::env::current_exe().expect("current test binary");
    let mut helper = HelperProcess(
        Command::new(current_exe)
            .env(EXTINCTION_HELPER_ENV, "1")
            .env(EXTINCTION_MODE_ENV, "exit")
            .env(EXTINCTION_PID_FILE_ENV, pid_file.to_str().unwrap())
            .env("GOOSE_PROCESS_FENCE", "standard")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn helper"),
    );

    let pid_line = {
        let stdout = helper.0.stdout.take().expect("helper stdout");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line).expect("read child pid");
        line
    };

    let child_pid = pid_line
        .trim()
        .parse::<u32>()
        .expect("valid child pid from helper");
    let _guard = ProcessKillGuard(child_pid);

    let status = helper.0.wait().expect("wait for helper");
    assert!(status.success(), "helper exited with failure: {status}");

    let deadline = Instant::now() + Duration::from_secs(5);
    while process_is_running(child_pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }

    assert!(
        !process_is_running(child_pid),
        "child process {child_pid} survived parent process termination"
    );
}

#[cfg(target_os = "linux")]
#[test]
#[serial]
fn test_shell_subprocess_extinction_on_parent_killed() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let pid_file = temp_dir.path().join("child.pid");

    let current_exe = std::env::current_exe().expect("current test binary");
    let mut helper = HelperProcess(
        Command::new(current_exe)
            .env(EXTINCTION_HELPER_ENV, "1")
            .env(EXTINCTION_MODE_ENV, "park")
            .env(EXTINCTION_PID_FILE_ENV, pid_file.to_str().unwrap())
            .env("GOOSE_PROCESS_FENCE", "standard")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn helper"),
    );

    let pid_line = {
        let stdout = helper.0.stdout.take().expect("helper stdout");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line).expect("read child pid");
        line
    };

    let child_pid = pid_line
        .trim()
        .parse::<u32>()
        .expect("valid child pid from helper");
    let _guard = ProcessKillGuard(child_pid);

    assert!(
        process_is_running(child_pid),
        "child process {child_pid} should be running while helper is active"
    );

    unsafe {
        libc::kill(helper.0.id() as libc::pid_t, libc::SIGKILL);
    }
    let status = helper.0.wait().expect("wait for helper");
    assert!(
        !status.success(),
        "helper should have terminated via signal: {status}"
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    while process_is_running(child_pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }

    assert!(
        !process_is_running(child_pid),
        "child process {child_pid} survived abrupt parent termination (SIGKILL)"
    );
}

#[tokio::test]
#[serial]
async fn test_shell_fencing_opt_in_default() {
    let _env = env_lock::lock_env([
        ("GOOSE_PROCESS_FENCE", None::<&str>),
        ("GOOSE_FENCE_RUNTIME", None),
        ("GOOSE_FENCE_PGROUP", None),
    ]);

    assert!(!goose::subprocess::is_process_fencing_enabled());
    let config = goose::subprocess::ProcessFenceConfig::from_env();
    assert_eq!(config.mode, goose::subprocess::ProcessFenceMode::None);
    assert!(!config.isolate_process_group);
    assert!(!config.parent_death_signal);

    let tool = ShellTool::new(false).expect("ShellTool::new");
    let result = tool
        .shell(ShellParams {
            command: "echo opt-in-default-test".to_string(),
            timeout_secs: Some(10),
        })
        .await;

    assert_eq!(result.is_error, Some(false));
    let output = extract_shell_output(&result);
    assert_eq!(output.exit_code, Some(0));
    assert!(output.stdout.contains("opt-in-default-test"));
}

#[tokio::test]
#[serial]
async fn test_shell_fencing_explicit_none() {
    let _env = env_lock::lock_env([
        ("GOOSE_PROCESS_FENCE", Some("none")),
        ("GOOSE_FENCE_RUNTIME", None),
        ("GOOSE_FENCE_PGROUP", None),
    ]);

    assert!(!goose::subprocess::is_process_fencing_enabled());
    let config = goose::subprocess::ProcessFenceConfig::from_env();
    assert_eq!(config.mode, goose::subprocess::ProcessFenceMode::None);
    assert!(!config.isolate_process_group);
    assert!(!config.parent_death_signal);

    let tool = ShellTool::new(false).expect("ShellTool::new");
    let result = tool
        .shell(ShellParams {
            command: "echo explicit-none-test".to_string(),
            timeout_secs: Some(10),
        })
        .await;

    assert_eq!(result.is_error, Some(false));
    let output = extract_shell_output(&result);
    assert_eq!(output.exit_code, Some(0));
    assert!(output.stdout.contains("explicit-none-test"));
}

#[tokio::test]
#[serial]
async fn test_shell_fencing_timeout_kills_process_group() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let pid_file = temp_dir.path().join("child.pid");

    let _env = env_lock::lock_env([
        ("GOOSE_PROCESS_FENCE", Some("standard")),
        ("GOOSE_FENCE_RUNTIME", None),
    ]);

    let tool = ShellTool::new(false).expect("ShellTool::new");
    let command = format!(
        "sh -c 'sleep 30 & echo $! > \"{}\"; wait'",
        pid_file.display()
    );

    let result = tool
        .shell(ShellParams {
            command,
            timeout_secs: Some(1),
        })
        .await;

    let output = extract_shell_output(&result);
    assert!(output.timed_out);

    let mut child_pid = None;
    for _ in 0..50 {
        if let Ok(content) = fs::read_to_string(&pid_file) {
            if let Ok(pid) = content.trim().parse::<u32>() {
                child_pid = Some(pid);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let pid = child_pid.expect("child pid should have been written");
    let _guard = ProcessKillGuard(pid);
    let deadline = Instant::now() + Duration::from_secs(3);
    while process_exists(pid) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        !process_exists(pid),
        "child process {pid} in process group should have been killed on timeout"
    );
}

#[tokio::test]
#[serial]
async fn test_shell_fencing_cancellation_kills_process_group() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let pid_file = temp_dir.path().join("child.pid");

    let _env = env_lock::lock_env([
        ("GOOSE_PROCESS_FENCE", Some("standard")),
        ("GOOSE_FENCE_RUNTIME", None),
    ]);

    let tool = ShellTool::new(false).expect("ShellTool::new");
    let token = CancellationToken::new();
    let token_clone = token.clone();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        token_clone.cancel();
    });

    let command = format!(
        "sh -c 'sleep 30 & echo $! > \"{}\"; wait'",
        pid_file.display()
    );
    let result = tool
        .shell_with_cwd(
            ShellParams {
                command,
                timeout_secs: Some(10),
            },
            None,
            None,
            token,
        )
        .await;

    let output = extract_shell_output(&result);
    assert_eq!(output.exit_code, None);

    let mut child_pid = None;
    for _ in 0..50 {
        if let Ok(content) = fs::read_to_string(&pid_file) {
            if let Ok(pid) = content.trim().parse::<u32>() {
                child_pid = Some(pid);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let pid = child_pid.expect("child pid should have been written");
    let _guard = ProcessKillGuard(pid);
    let deadline = Instant::now() + Duration::from_secs(3);
    while process_exists(pid) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        !process_exists(pid),
        "child process {pid} in process group should have been killed on cancellation"
    );
}
