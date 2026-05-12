pub mod server;

use anyhow::{anyhow, Result};
use rand::{distributions::Alphanumeric, Rng};
use serde::Deserialize;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::timeout;

const APP_SLUG: &str = "goose-copilot";
const CALLBACK_PORT: u16 = 3458;
const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Deserialize)]
pub struct InstallCallback {
    pub installation_id: u64,
    pub oauth_code: String,
}

pub struct CopilotInstallFlow {
    state: String,
    server_shutdown_tx: Option<oneshot::Sender<()>>,
}

impl CopilotInstallFlow {
    pub fn new() -> Self {
        let state: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(64)
            .map(char::from)
            .collect();
        Self {
            state,
            server_shutdown_tx: None,
        }
    }

    pub fn install_url(&self) -> String {
        format!(
            "https://github.com/apps/{}/installations/new?state={}",
            APP_SLUG, self.state
        )
    }

    pub fn callback_url() -> String {
        format!("http://localhost:{}/", CALLBACK_PORT)
    }

    pub async fn complete_flow(&mut self) -> Result<InstallCallback> {
        let url = self.install_url();
        tracing::info!("[copilot-install] opening browser to {}", url);
        if let Err(e) = webbrowser::open(&url) {
            tracing::warn!("[copilot-install] failed to open browser: {}", e);
        }
        self.await_callback().await
    }

    pub async fn await_callback(&mut self) -> Result<InstallCallback> {
        let (cb_tx, cb_rx) = oneshot::channel::<InstallCallback>();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        self.server_shutdown_tx = Some(shutdown_tx);

        let expected_state = self.state.clone();
        tokio::spawn(async move {
            if let Err(e) =
                server::run_callback_server(cb_tx, shutdown_rx, expected_state, CALLBACK_PORT).await
            {
                tracing::error!("[copilot-install] callback server error: {}", e);
            }
        });

        match timeout(INSTALL_TIMEOUT, cb_rx).await {
            Ok(Ok(cb)) => Ok(cb),
            Ok(Err(_)) => Err(anyhow!("install callback channel closed unexpectedly")),
            Err(_) => Err(anyhow!("install timeout - please try again")),
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(tx) = self.server_shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

impl Default for CopilotInstallFlow {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CopilotInstallFlow {
    fn drop(&mut self) {
        self.shutdown();
    }
}
