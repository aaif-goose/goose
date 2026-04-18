use tauri_plugin_shell::ShellExt;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::OnceCell;

const GOOSE_SERVE_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const GOOSE_SERVE_CONNECT_RETRY_DELAY: Duration = Duration::from_millis(100);
const LOCALHOST: &str = "127.0.0.1";
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
    token: String,
    _child: Child,
}

/// Global singleton — initialised once at app startup.
static GOOSE_SERVE: OnceCell<GooseServeProcess> = OnceCell::const_new();

impl GooseServeProcess {
    /// Return the WebSocket URL for connecting to this server.
    pub fn ws_url(&self) -> String {
        format!("ws://{LOCALHOST}:{}/acp", self.port)
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// Get a reference to the running process, or an error if it was never
    /// started (should not happen in normal operation).
    pub async fn get(app_handle: tauri::AppHandle) -> Result<&'static GooseServeProcess, String> {
        GOOSE_SERVE
            .get_or_try_init(|| async { Self::spawn(app_handle).await })
            .await
    }

    async fn spawn(app_handle: tauri::AppHandle) -> Result<GooseServeProcess, String> {
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

        let mut command: Command = get_goose_command(&app_handle)?;
        let binary_display = command.as_std().get_program().to_string_lossy().to_string();

        command
            .arg("serve")
            .arg("--host")
            .arg(LOCALHOST)
            .arg("--port")
            .arg(port.to_string())
            .current_dir(&working_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);

        log::info!(
            "Spawning long-lived goose serve: binary={binary_display} port={port} cwd={}",
            working_dir.display(),
        );

        let mut child = command.spawn().map_err(|error| {
            format!(
                "Failed to spawn goose serve (binary: {binary_display}, cwd: {}): {error}",
                working_dir.display()
            )
        })?;

        let token = read_auth_token(&mut child).await?;
        wait_for_server_ready(port, &mut child).await?;

        log::info!("Goose serve is ready on port {port}");

        Ok(GooseServeProcess {
            port,
            token,
            _child: child,
        })
    }
}

pub fn get_goose_command(app_handle: &tauri::AppHandle) -> Result<Command, String> {
    if let Ok(override_path) = std::env::var("GOOSE_BIN") {
        Ok(Command::new(override_path))
    } else {
        let tauri_command = app_handle
            .shell()
            .sidecar("goose")
            .map_err(|e| format!("could not resolve goose binary: {e}"))?;
        let std_command: std::process::Command = tauri_command.into();
        Ok(std_command.into())
    }
}

async fn wait_for_server_ready(port: u16, child: &mut Child) -> Result<(), String> {
    let deadline = Instant::now() + GOOSE_SERVE_CONNECT_TIMEOUT;
    let addr = format!("{LOCALHOST}:{port}");

    loop {
        match tokio::net::TcpStream::connect(&addr).await {
            Ok(_) => return Ok(()),
            Err(_) => {
                if let Some(status) = child
                    .try_wait()
                    .map_err(|e| format!("Failed to poll goose serve process: {e}"))?
                {
                    return Err(format!(
                        "Goose serve exited before becoming ready: {status}"
                    ));
                }

                if Instant::now() >= deadline {
                    return Err(format!("Timed out waiting for goose serve on port {port}"));
                }

                tokio::time::sleep(GOOSE_SERVE_CONNECT_RETRY_DELAY).await;
            }
        }
    }
}

async fn read_auth_token(child: &mut Child) -> Result<String, String> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Goose serve stdout was not piped".to_string())?;
    let deadline = Instant::now() + GOOSE_SERVE_CONNECT_TIMEOUT;
    let mut lines = BufReader::new(stdout).lines();

    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("Failed to poll goose serve process: {e}"))?
        {
            return Err(format!(
                "Goose serve exited before emitting ACP token: {status}"
            ));
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("Timed out waiting for goose serve auth token".to_string());
        }

        match tokio::time::timeout(remaining, lines.next_line()).await {
            Ok(Ok(Some(line))) => {
                if let Some(token) = line.strip_prefix("ACP_TOKEN=") {
                    if token.is_empty() {
                        return Err("Goose serve emitted an empty ACP token".to_string());
                    }

                    tokio::spawn(async move {
                        while matches!(lines.next_line().await, Ok(Some(_))) {}
                    });

                    return Ok(token.to_string());
                }
            }
            Ok(Ok(None)) => {
                return Err("Goose serve closed stdout before emitting ACP token".to_string());
            }
            Ok(Err(error)) => {
                return Err(format!("Failed to read goose serve stdout: {error}"));
            }
            Err(_) => {
                return Err("Timed out waiting for goose serve auth token".to_string());
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

fn reserve_free_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind((LOCALHOST, 0))
        .map_err(|error| format!("Failed to reserve Goose serve port: {error}"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| format!("Failed to resolve reserved Goose serve port: {error}"))
}
