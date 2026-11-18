#![cfg(any(unix, windows))]

use goose::agents::platform_extensions::developer::shell::{ShellOutput, ShellParams, ShellTool};
use rmcp::model::CallToolResult;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;

const HELPER_ENV: &str = "GOOSE_SHELL_CLEANUP_HELPER";
const HELPER_ARG: &str = "--goose-shell-cleanup-helper";
const WAIT: Duration = Duration::from_secs(10);
const LIFETIME: Duration = Duration::from_secs(30);

// These exercise native shells, not Flatpak's separate host transport.
#[ctor::ctor]
unsafe fn run_helper() {
    if std::env::var_os(HELPER_ENV).is_none()
        || std::env::args().nth(1).as_deref() != Some(HELPER_ARG)
    {
        return;
    }
    let mode = std::env::args().nth(2).expect("helper mode");
    let deadline = Instant::now() + LIFETIME;
    if mode == "leaf" {
        std::fs::write("leaf.pid.tmp", std::process::id().to_string()).unwrap();
        std::fs::rename("leaf.pid.tmp", "leaf.pid").unwrap();
        while Instant::now() < deadline {
            if Path::new("probe").exists() {
                std::fs::write("leaf.alive", "alive").unwrap();
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        std::process::exit(0);
    }

    std::fs::write("parent.pid", std::process::id().to_string()).unwrap();
    let mut leaf = Command::new(std::env::current_exe().unwrap())
        .args([HELPER_ARG, "leaf"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    while !Path::new("leaf.pid").exists() {
        if Instant::now() >= deadline || leaf.try_wait().unwrap().is_some() {
            let _ = leaf.kill();
            let _ = leaf.wait();
            std::process::exit(1);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    println!("helper stdout ready");
    eprintln!("helper stderr ready");
    std::fs::write("tree.ready", "ready").unwrap();
    while !Path::new("finish").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    if mode == "background" {
        std::process::exit(0);
    }
    let _ = leaf.kill();
    let _ = leaf.wait();
    std::process::exit(0);
}

struct Process {
    pid: u32,
    #[cfg(windows)]
    handle: std::os::windows::io::OwnedHandle,
}

impl Process {
    fn retain(pid: u32) -> Option<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::io::FromRawHandle;
            use winapi::um::processthreadsapi::OpenProcess;
            use winapi::um::winnt::{PROCESS_TERMINATE, SYNCHRONIZE};
            let handle = unsafe { OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, 0, pid) };
            if handle.is_null() {
                return None;
            }
            Some(Self {
                pid,
                handle: unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(handle) },
            })
        }
        #[cfg(unix)]
        Some(Self { pid })
    }

    fn running(&self) -> bool {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use winapi::shared::winerror::WAIT_TIMEOUT;
            use winapi::um::synchapi::WaitForSingleObject;
            use winapi::um::winbase::WAIT_OBJECT_0;
            let status = unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 0) };
            assert!(
                status == WAIT_OBJECT_0 || status == WAIT_TIMEOUT,
                "process wait failed"
            );
            status == WAIT_TIMEOUT
        }
        #[cfg(unix)]
        {
            if unsafe { libc::kill(self.pid as libc::pid_t, 0) } != 0 {
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::ESRCH)
                );
                return false;
            }
            // Orphaned grandchildren can remain zombies until the system reaps them.
            #[cfg(target_os = "linux")]
            let state = std::fs::read_to_string(format!("/proc/{}/stat", self.pid))
                .ok()
                .and_then(|stat| {
                    stat.rsplit_once(") ")
                        .and_then(|(_, tail)| tail.chars().next())
                });
            #[cfg(not(target_os = "linux"))]
            let state = {
                let output = Command::new("ps")
                    .args(["-o", "stat=", "-p", &self.pid.to_string()])
                    .output()
                    .expect("read native process status");
                String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .chars()
                    .next()
            };
            !matches!(state, None | Some('Z' | 'X'))
        }
    }

    fn kill(&self) {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            unsafe {
                winapi::um::processthreadsapi::TerminateProcess(self.handle.as_raw_handle(), 1);
            }
        }
        #[cfg(unix)]
        if self.running() {
            unsafe {
                libc::kill(self.pid as libc::pid_t, libc::SIGKILL);
            }
        }
    }
}

struct Tree {
    dir: tempfile::TempDir,
    processes: Vec<Process>,
    direct_child: Option<Child>,
}

impl Tree {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
            processes: Vec::new(),
            direct_child: None,
        }
    }

    async fn ready(&mut self) {
        let deadline = Instant::now() + WAIT;
        while !self.dir.path().join("tree.ready").exists() {
            assert!(
                Instant::now() < deadline,
                "helper tree did not become ready"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        for name in ["parent.pid", "leaf.pid"] {
            let pid = std::fs::read_to_string(self.dir.path().join(name))
                .unwrap()
                .parse()
                .unwrap();
            self.processes
                .push(Process::retain(pid).expect("retain live helper process"));
        }
        self.assert_running();
    }

    fn assert_running(&self) {
        for process in &self.processes {
            assert!(
                process.running(),
                "process {} exited unexpectedly",
                process.pid
            );
        }
    }

    async fn stopped(&self) {
        let deadline = Instant::now() + WAIT;
        while self.processes.iter().any(Process::running) {
            assert!(
                Instant::now() < deadline,
                "command descendants survived termination"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        for process in &self.processes {
            process.kill();
        }
        // Read any PIDs not retained yet if a readiness assertion failed.
        for name in ["leaf.pid", "parent.pid"] {
            let pid = std::fs::read_to_string(self.dir.path().join(name))
                .ok()
                .and_then(|text| text.parse::<u32>().ok());
            if let Some(pid) = pid.filter(|pid| !self.processes.iter().any(|p| p.pid == *pid)) {
                if let Some(process) = Process::retain(pid) {
                    process.kill();
                }
            }
        }
        if let Some(child) = &mut self.direct_child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn shell_env() -> env_lock::EnvGuard<'static> {
    #[cfg(unix)]
    let shell = "sh";
    #[cfg(windows)]
    let shell = "cmd";
    env_lock::lock_env([("GOOSE_SHELL", Some(shell)), (HELPER_ENV, Some("1"))])
}

fn invoke(
    tool: &Arc<ShellTool>,
    tree: &Tree,
    mode: &str,
    timeout: u64,
    token: CancellationToken,
) -> AbortOnDropHandle<CallToolResult> {
    let exe = std::env::current_exe().unwrap();
    #[cfg(unix)]
    let command = format!(
        "'{}' {HELPER_ARG} {mode}; :",
        exe.to_string_lossy().replace('\'', "'\\''")
    );
    #[cfg(windows)]
    let command = format!("\"\"{}\" {HELPER_ARG} {mode}\"", exe.display());
    let cwd = tree.dir.path().to_path_buf();
    let tool = Arc::clone(tool);
    AbortOnDropHandle::new(tokio::spawn(async move {
        tool.shell_with_cwd(
            ShellParams {
                command,
                timeout_secs: Some(timeout),
            },
            Some(&cwd),
            None,
            token,
        )
        .await
    }))
}

async fn output(task: &mut AbortOnDropHandle<CallToolResult>) -> ShellOutput {
    let result = tokio::time::timeout(WAIT, task)
        .await
        .expect("shell did not return promptly")
        .unwrap();
    serde_json::from_value(result.structured_content.expect("structured shell output")).unwrap()
}

#[tokio::test]
async fn cancellation_kills_tree_without_affecting_concurrent_or_unrelated_processes() {
    let _env = shell_env();
    let tool = Arc::new(ShellTool::new().unwrap());
    let mut cancelled = Tree::new();
    let mut concurrent = Tree::new();
    let mut unrelated = Tree::new();
    unrelated.direct_child = Some(
        Command::new(std::env::current_exe().unwrap())
            .args([HELPER_ARG, "tree"])
            .current_dir(unrelated.dir.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let token = CancellationToken::new();
    let mut first = invoke(&tool, &cancelled, "tree", 0, token.clone());
    let mut second = invoke(&tool, &concurrent, "tree", 0, CancellationToken::new());
    cancelled.ready().await;
    concurrent.ready().await;
    unrelated.ready().await;

    token.cancel();
    let result = output(&mut first).await;
    assert_eq!(result.exit_code, None);
    assert!(!result.timed_out);
    cancelled.stopped().await;
    concurrent.assert_running();
    unrelated.assert_running();
    assert!(
        !second.is_finished(),
        "cancellation ended another invocation"
    );
    std::fs::write(concurrent.dir.path().join("finish"), "finish").unwrap();
    assert_eq!(output(&mut second).await.exit_code, Some(0));
    concurrent.stopped().await;
    unrelated.assert_running();
}

#[tokio::test]
async fn timeout_kills_tree_and_preserves_output_metadata() {
    let _env = shell_env();
    let tool = Arc::new(ShellTool::new().unwrap());
    let mut tree = Tree::new();
    let mut task = invoke(&tool, &tree, "tree", 5, CancellationToken::new());
    tree.ready().await;
    let result = output(&mut task).await;
    assert!(result.timed_out);
    assert_eq!(result.exit_code, None);
    assert!(result.stdout.contains("helper stdout ready"));
    assert!(result.stderr.contains("helper stderr ready"));
    assert!(!result.output_truncated);
    assert!(result.output_collection_error.is_none());
    tree.stopped().await;
}

#[tokio::test]
async fn aborting_shell_future_kills_tree() {
    let _env = shell_env();
    let tool = Arc::new(ShellTool::new().unwrap());
    let mut tree = Tree::new();
    let mut task = invoke(&tool, &tree, "tree", 0, CancellationToken::new());
    tree.ready().await;
    task.abort();
    let error = tokio::time::timeout(WAIT, &mut task)
        .await
        .unwrap()
        .unwrap_err();
    assert!(error.is_cancelled());
    tree.stopped().await;
}

#[tokio::test]
async fn successful_shell_exit_preserves_background_descendant() {
    let _env = shell_env();
    let tool = Arc::new(ShellTool::new().unwrap());
    let mut tree = Tree::new();
    let mut task = invoke(&tool, &tree, "background", 0, CancellationToken::new());
    tree.ready().await;
    std::fs::write(tree.dir.path().join("finish"), "finish").unwrap();
    let result = output(&mut task).await;
    assert_eq!(result.exit_code, Some(0));
    assert!(!result.timed_out);
    assert!(!result.output_truncated);
    assert!(result.output_collection_error.is_none());
    std::fs::write(tree.dir.path().join("probe"), "probe").unwrap();
    let deadline = Instant::now() + WAIT;
    while !tree.dir.path().join("leaf.alive").exists() {
        assert!(
            tree.processes[1].running(),
            "successful shell exit killed background work"
        );
        assert!(
            Instant::now() < deadline,
            "background descendant stopped responding"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(tree.processes[1].running());
}
