use std::path::PathBuf;
use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use tokio::process::{Child, Command};
use tokio::sync::OnceCell;
use tokio_tungstenite::connect_async;

const GOOSE_SERVE_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const GOOSE_SERVE_CONNECT_RETRY_DELAY: Duration = Duration::from_millis(100);
const LOCALHOST: &str = "127.0.0.1";
pub(crate) const WS_BRIDGE_BUFFER_BYTES: usize = 64 * 1024;

// ---------------------------------------------------------------------------
// GooseServeProcess — singleton that owns the long-lived `goose serve` child
// ---------------------------------------------------------------------------

/// A long-lived `goose serve` process that accepts WebSocket connections.
///
/// Each WebSocket connection to the `/acp` endpoint creates an independent
/// ACP agent inside the server, so a single process can serve any number of
/// concurrent sessions.
pub struct GooseServeProcess {
    port: u16,
    _child: Child,
}

/// Global singleton — initialised once at app startup.
static GOOSE_SERVE: OnceCell<GooseServeProcess> = OnceCell::const_new();

impl GooseServeProcess {
    /// Return the WebSocket URL for connecting to this server.
    pub fn ws_url(&self) -> String {
        format!("ws://{LOCALHOST}:{}/acp", self.port)
    }

    /// Start the singleton `goose serve` process.
    ///
    /// This is called once from `lib.rs` during app startup.  Subsequent calls
    /// are no-ops (the `OnceCell` ensures single initialisation).  The process
    /// is spawned with `kill_on_drop(true)` so it is automatically terminated
    /// when the Tauri app exits.
    pub async fn start() -> Result<(), String> {
        GOOSE_SERVE
            .get_or_try_init(|| async { Self::spawn().await })
            .await
            .map(|_| ())
    }

    /// Get a reference to the running process, or an error if it was never
    /// started (should not happen in normal operation).
    pub fn get() -> Result<&'static GooseServeProcess, String> {
        GOOSE_SERVE
            .get()
            .ok_or_else(|| "Goose serve process has not been started".to_string())
    }

    async fn spawn() -> Result<GooseServeProcess, String> {
        let binary_path = resolve_goose_binary()?;
        let port = reserve_free_port()?;

        // Use a stable working directory for the long-lived server process.
        // Individual sessions will set their own cwd via the ACP protocol.
        let working_dir = default_serve_working_dir();
        std::fs::create_dir_all(&working_dir).map_err(|e| {
            format!(
                "Failed to create goose serve working directory {}: {e}",
                working_dir.display()
            )
        })?;

        let mut command = Command::new(&binary_path);
        command
            .arg("serve")
            .arg("--host")
            .arg(LOCALHOST)
            .arg("--port")
            .arg(port.to_string())
            .current_dir(&working_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);

        log::info!(
            "Spawning long-lived goose serve: binary={} port={} cwd={}",
            binary_path.display(),
            port,
            working_dir.display(),
        );

        let mut child = command.spawn().map_err(|error| {
            format!(
                "Failed to spawn goose serve (binary: {}, cwd: {}): {error}",
                binary_path.display(),
                working_dir.display()
            )
        })?;

        // Wait for the server to become ready by polling the WebSocket endpoint.
        let ws_url = format!("ws://{LOCALHOST}:{port}/acp");
        wait_for_server_ready(&ws_url, &mut child).await?;

        log::info!("Goose serve is ready on port {port}");

        Ok(GooseServeProcess {
            port,
            _child: child,
        })
    }
}

/// Wait for the goose serve process to accept WebSocket connections.
///
/// We do a connect-then-immediately-close loop until the server responds,
/// the child exits, or we time out.
async fn wait_for_server_ready(ws_url: &str, child: &mut Child) -> Result<(), String> {
    let deadline = Instant::now() + GOOSE_SERVE_CONNECT_TIMEOUT;

    loop {
        match connect_async(ws_url).await {
            Ok((ws_stream, _)) => {
                // Server is up — close the probe connection.
                let (mut writer, _) = ws_stream.split();
                let _ = writer.close().await;
                return Ok(());
            }
            Err(connect_error) => {
                if let Some(status) = child
                    .try_wait()
                    .map_err(|e| format!("Failed to poll goose serve process: {e}"))?
                {
                    return Err(format!(
                        "Goose serve exited before becoming ready: {status}"
                    ));
                }

                if Instant::now() >= deadline {
                    return Err(format!(
                        "Timed out waiting for goose serve at {ws_url}: {connect_error}"
                    ));
                }

                tokio::time::sleep(GOOSE_SERVE_CONNECT_RETRY_DELAY).await;
            }
        }
    }
}

fn default_serve_working_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".goose")
        .join("artifacts")
}

// ---------------------------------------------------------------------------
// Binary resolution
// ---------------------------------------------------------------------------

pub(crate) fn resolve_goose_binary() -> Result<PathBuf, String> {
    let binary_path = if let Ok(override_path) = std::env::var("GOOSE_BIN") {
        let path = PathBuf::from(&override_path);
        if !path.exists() {
            return Err(format!(
                "GOOSE_BIN points to non-existent path: {override_path}"
            ));
        }
        if !goose_binary_supports_serve(&path)? {
            return Err(format!(
                "GOOSE_BIN points to a goose binary without `serve` support: {}",
                path.display()
            ));
        }
        log::info!("Using GOOSE_BIN override: {override_path}");
        path
    } else {
        let agent = acp_client::find_acp_agent_by_id("goose")
            .ok_or_else(|| "Unknown or unavailable agent provider: goose".to_string())?;

        if !goose_binary_supports_serve(&agent.binary_path)? {
            return Err(format!(
                "Resolved goose binary does not support `serve`: {}. Set GOOSE_BIN to a newer goose binary.",
                agent.binary_path.display()
            ));
        }

        log::info!(
            "Resolved goose binary via login-shell discovery: {}",
            agent.binary_path.display()
        );
        agent.binary_path
    };

    // Log the binary version for debugging.
    match std::process::Command::new(&binary_path)
        .env("GOOSE_PATH_ROOT", goose_probe_root())
        .arg("--version")
        .output()
    {
        Ok(output) => {
            let version = String::from_utf8_lossy(&output.stdout);
            log::info!(
                "Goose binary version: {} (path: {})",
                version.trim(),
                binary_path.display()
            );
        }
        Err(err) => {
            log::warn!(
                "Could not determine goose binary version at {}: {err}",
                binary_path.display()
            );
        }
    }

    Ok(binary_path)
}

fn goose_binary_supports_serve(binary_path: &PathBuf) -> Result<bool, String> {
    let output = std::process::Command::new(binary_path)
        .env("GOOSE_PATH_ROOT", goose_probe_root())
        .arg("serve")
        .arg("--help")
        .output()
        .map_err(|error| {
            format!(
                "Failed to probe goose binary {}: {error}",
                binary_path.display()
            )
        })?;

    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    log::warn!(
        "Goose binary probe failed for {}: status={} stderr={} stdout={}",
        binary_path.display(),
        output
            .status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "signal".to_string()),
        stderr.trim(),
        stdout.trim(),
    );

    Ok(false)
}

fn goose_probe_root() -> PathBuf {
    std::env::temp_dir().join("block-goose2-goose-probe")
}

fn reserve_free_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind((LOCALHOST, 0))
        .map_err(|error| format!("Failed to reserve Goose serve port: {error}"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| format!("Failed to resolve reserved Goose serve port: {error}"))
}
