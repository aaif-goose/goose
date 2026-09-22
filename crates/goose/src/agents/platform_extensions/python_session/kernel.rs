use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio_util::sync::CancellationToken;

const DRIVER_SOURCE: &str = include_str!("driver.py");
const READY_TIMEOUT: Duration = Duration::from_secs(15);
const INTERRUPT_GRACE: Duration = Duration::from_secs(5);
const TEARDOWN_GRACE: Duration = Duration::from_millis(500);
const STDERR_TAIL_CHARS: usize = 4096;

pub struct KernelSpec {
    pub python: PathBuf,
    pub working_dir: PathBuf,
    pub state_path: Option<PathBuf>,
    pub env: Vec<(&'static str, String)>,
}

#[derive(Debug, Deserialize)]
struct DriverResponse {
    id: i64,
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    stdout: String,
    #[serde(default)]
    stderr: String,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    ns: Option<String>,
    #[serde(default)]
    restored: Vec<String>,
    #[serde(default)]
    dropped: Vec<String>,
    #[serde(default)]
    images: Vec<ImageRequest>,
}

/// An image the cell asked to show the model via `view_image()`; the host loads
/// the pixels after the cell returns.
#[derive(Debug, Clone, Deserialize)]
pub struct ImageRequest {
    pub source: String,
    #[serde(default)]
    pub crop: Option<ImageCrop>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImageCrop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub struct ExecOutcome {
    pub stdout: String,
    pub stderr: String,
    pub value: Option<String>,
    pub error: Option<String>,
    pub duration_ms: Option<u64>,
    pub interrupted: bool,
    pub images: Vec<ImageRequest>,
}

pub struct Kernel {
    child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    next_id: i64,
    stderr_tail: Arc<Mutex<Vec<u8>>>,
    restored_names: Vec<String>,
    dropped_names: Vec<String>,
    _driver_dir: tempfile::TempDir,
}

impl Kernel {
    pub async fn spawn(spec: &KernelSpec) -> Result<Self> {
        // The driver lives in its own directory so the interpreter's implicit
        // `sys.path[0]` (the script's directory) is owner-only and empty apart
        // from the driver; on a shared /tmp another user cannot plant an
        // `ast.py`/`json.py` there to shadow the driver's stdlib imports.
        let driver_dir = tempfile::Builder::new()
            .prefix("goose-python-session-")
            .tempdir()
            .context("failed to create the python session driver directory")?;
        let driver_path = driver_dir.path().join("goose_python_session_driver.py");
        std::fs::write(&driver_path, DRIVER_SOURCE)?;

        let mut command = Command::new(&spec.python);
        command
            .arg(&driver_path)
            .current_dir(&spec.working_dir)
            .envs(spec.env.iter().cloned())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(path) = &spec.state_path {
            command.env("GOOSE_PYTHON_SESSION_STATE_PATH", path);
        }
        #[cfg(unix)]
        command.process_group(0);

        let mut child = command.spawn().with_context(|| {
            format!(
                "failed to start the python session with `{}`",
                spec.python.display()
            )
        })?;

        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        let stderr_tail = Arc::new(Mutex::new(Vec::<u8>::new()));
        let tail = stderr_tail.clone();
        tokio::spawn(async move {
            // Read raw bytes into a bounded ring buffer: a runaway subprocess that
            // never emits a newline cannot grow an unbounded intermediate line, and
            // invalid UTF-8 does not terminate the reader (it is decoded lossily
            // only when a death report is formatted).
            let mut reader = BufReader::new(stderr);
            let mut chunk = [0u8; 4096];
            loop {
                match reader.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut tail = tail.lock().unwrap();
                        tail.extend_from_slice(&chunk[..n]);
                        if tail.len() > STDERR_TAIL_CHARS {
                            let cut = tail.len() - STDERR_TAIL_CHARS;
                            tail.drain(..cut);
                        }
                    }
                }
            }
        });

        let mut kernel = Self {
            child,
            stdin,
            lines: BufReader::new(stdout).lines(),
            next_id: 1,
            stderr_tail,
            restored_names: Vec::new(),
            dropped_names: Vec::new(),
            _driver_dir: driver_dir,
        };

        let ready = tokio::time::timeout(READY_TIMEOUT, kernel.read_response(0))
            .await
            .map_err(|_| anyhow!("the python session did not become ready within 15s"))?
            .with_context(|| kernel.death_context("during startup"))?;
        if !ready.ok {
            return Err(anyhow!(
                "the python session failed to start: {}",
                ready.error.unwrap_or_default()
            ));
        }
        kernel.restored_names = ready.restored;
        kernel.dropped_names = ready.dropped;
        Ok(kernel)
    }

    pub fn restored_names(&self) -> &[String] {
        &self.restored_names
    }

    /// Variables that existed when the snapshot was taken but could not be
    /// persisted (unpicklable or over the size cap), so they did not survive.
    pub fn dropped_names(&self) -> &[String] {
        &self.dropped_names
    }

    pub async fn exec(
        &mut self,
        code: &str,
        timeout: Duration,
        cancellation: CancellationToken,
    ) -> Result<ExecOutcome> {
        let id = self.request(json!({"op": "exec", "code": code})).await?;

        let waited = {
            let read = self.read_response(id);
            tokio::pin!(read);
            tokio::select! {
                response = &mut read => Some(response),
                _ = tokio::time::sleep(timeout) => None,
                _ = cancellation.cancelled() => None,
            }
        };

        let (response, interrupted) = match waited {
            Some(response) => (response, false),
            None => {
                self.interrupt();
                let response = tokio::time::timeout(INTERRUPT_GRACE, self.read_response(id))
                    .await
                    .map_err(|_| {
                        anyhow!(
                            "cell did not stop within {}s of interrupt",
                            INTERRUPT_GRACE.as_secs()
                        )
                    })?;
                (response, true)
            }
        };

        let response = response.with_context(|| self.death_context("while executing a cell"))?;
        Ok(ExecOutcome {
            stdout: response.stdout,
            stderr: response.stderr,
            value: response.value,
            error: response.error,
            duration_ms: response.duration_ms,
            interrupted,
            images: response.images,
        })
    }

    pub async fn namespace(&mut self, timeout: Duration) -> Result<String> {
        let id = self.request(json!({"op": "ns"})).await?;
        let response = tokio::time::timeout(timeout, self.read_response(id))
            .await
            .map_err(|_| anyhow!("namespace probe timed out"))??;
        Ok(response.ns.unwrap_or_default())
    }

    pub async fn chdir(&mut self, dir: &Path, timeout: Duration) -> Result<()> {
        let code = format!(
            "import os\nos.chdir({})",
            serde_json::to_string(&dir.to_string_lossy())?
        );
        let outcome = self.exec(&code, timeout, CancellationToken::new()).await?;
        match outcome.error {
            Some(error) => Err(anyhow!(error)),
            None => Ok(()),
        }
    }

    pub fn kill(&mut self) {
        // Take the driver's subprocesses too, so a command it spawned is not
        // orphaned on reap or crash.
        self.kill_process_group();
        let _ = self.child.start_kill();
    }

    /// Stop the kernel, giving the driver a moment to reap shell commands that run
    /// in their own session (which a process-group SIGKILL cannot reach) before a
    /// hard kill. Used when a dropped request abandons a running cell.
    pub async fn graceful_kill(mut self) {
        self.terminate();
        tokio::time::sleep(TEARDOWN_GRACE).await;
        self.kill();
    }

    #[cfg(unix)]
    fn terminate(&self) {
        // SIGTERM the group; the driver's handler reaps its tracked shell sessions
        // and exits. A shell started with its own session is otherwise orphaned.
        if let Some(pid) = self.child.id() {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGTERM);
            }
        }
    }

    #[cfg(not(unix))]
    fn terminate(&self) {
        // taskkill /T already walks the whole process tree, so there is no
        // separate session to reap; the hard kill after the grace suffices.
    }

    async fn request(&mut self, mut payload: serde_json::Value) -> Result<i64> {
        let id = self.next_id;
        self.next_id += 1;
        payload["id"] = json!(id);
        let mut line = payload.to_string();
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .with_context(|| self.death_context("while sending a request"))?;
        Ok(id)
    }

    async fn read_response(&mut self, id: i64) -> Result<DriverResponse> {
        loop {
            let line = self
                .lines
                .next_line()
                .await?
                .ok_or_else(|| anyhow!("the python session closed its output stream"))?;
            match serde_json::from_str::<DriverResponse>(&line) {
                Ok(response) if response.id == id => return Ok(response),
                Ok(stale) => {
                    tracing::warn!(got = stale.id, want = id, "skipping stale kernel response")
                }
                Err(_) => tracing::warn!("skipping non-protocol kernel output line"),
            }
        }
    }

    #[cfg(unix)]
    fn interrupt(&self) {
        // The driver runs in its own process group (see `process_group(0)`), so
        // signal the whole group: the driver takes the KeyboardInterrupt and any
        // child it spawned directly is stopped instead of orphaned.
        if let Some(pid) = self.child.id() {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGINT);
            }
        }
    }

    #[cfg(not(unix))]
    fn interrupt(&mut self) {
        tracing::warn!("cell interrupt is not supported on this platform; killing the kernel");
        self.kill_process_group();
        let _ = self.child.start_kill();
    }

    /// The driver has its own process group (see `process_group(0)`), so one
    /// signal reaches every subprocess it spawned.
    #[cfg(unix)]
    fn kill_process_group(&self) {
        if let Some(pid) = self.child.id() {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }

    /// Without process groups, `taskkill /T` walks the driver's process tree while
    /// the driver is still alive to be its root; killing the driver first would
    /// orphan a shell command it was blocked in.
    #[cfg(not(unix))]
    fn kill_process_group(&self) {
        if let Some(pid) = self.child.id() {
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/T", "/PID", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    fn death_context(&self, when: &str) -> String {
        let tail = self.stderr_tail.lock().unwrap();
        let tail = String::from_utf8_lossy(&tail);
        let tail = tail.trim_end();
        if tail.is_empty() {
            format!("the python session died {when}")
        } else {
            format!("the python session died {when}; stderr tail:\n{tail}")
        }
    }
}

impl Drop for Kernel {
    fn drop(&mut self) {
        // `kill_on_drop` only reaps the driver PID; take its subprocesses too when
        // the kernel is dropped without an explicit `kill()` (e.g. the extension
        // is disabled before the idle reaper runs).
        self.kill_process_group();
    }
}

/// Owns a kernel while one cell runs. If the caller's future is dropped mid-cell
/// (user stop, loop teardown), `Drop` kills the kernel so the still-running cell
/// and its subprocesses do not outlive the request and block the next call;
/// `kill_on_drop` cannot help while the kernel is parked in a session slot rather
/// than owned by the future.
pub struct RunningKernel {
    kernel: Option<Kernel>,
}

impl RunningKernel {
    pub fn new(kernel: Kernel) -> Self {
        Self {
            kernel: Some(kernel),
        }
    }

    pub async fn exec(
        &mut self,
        code: &str,
        timeout: Duration,
        cancellation: CancellationToken,
    ) -> Result<ExecOutcome> {
        self.kernel
            .as_mut()
            .expect("kernel present until finish()")
            .exec(code, timeout, cancellation)
            .await
    }

    /// Reclaim the kernel after the cell ran to completion, disarming the guard.
    pub fn finish(mut self) -> Kernel {
        self.kernel.take().expect("kernel reclaimed once")
    }
}

impl Drop for RunningKernel {
    fn drop(&mut self) {
        let Some(mut kernel) = self.kernel.take() else {
            return;
        };
        // Give teardown a lifecycle independent of the abandoned request: a
        // detached task interrupts, waits out the grace, then hard-kills, so a
        // running cell's shell subprocesses are reaped rather than orphaned.
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move { kernel.graceful_kill().await });
            }
            Err(_) => kernel.kill(),
        }
    }
}
