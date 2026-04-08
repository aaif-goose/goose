pub mod server;

#[cfg(test)]
mod tests;

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{distributions::Alphanumeric, Rng};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::timeout;

/// Default models for Tetrate Agent Router Service configuration
pub const TETRATE_DEFAULT_MODEL: &str = "claude-haiku-4-5";

// Auth endpoints are on the main web domain
const TETRATE_AUTH_URL: &str = "https://router.tetrate.ai/auth";
const TETRATE_TOKEN_URL: &str = "https://router.tetrate.ai/api/api-keys/verify";
const CALLBACK_BASE: &str = "http://localhost";
const AUTH_TIMEOUT: Duration = Duration::from_secs(180); // 3 minutes

#[derive(Debug)]
pub struct PkceAuthFlow {
    code_verifier: String,
    code_challenge: String,
    server_shutdown_tx: Option<oneshot::Sender<()>>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    key: String,
}

#[derive(Debug, Serialize)]
struct TokenRequest {
    code: String,
    code_verifier: String,
}

impl PkceAuthFlow {
    pub fn new() -> Result<Self> {
        let code_verifier: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(128)
            .map(char::from)
            .collect();

        let mut hasher = Sha256::new();
        hasher.update(&code_verifier);
        let hash = hasher.finalize();

        let code_challenge = URL_SAFE_NO_PAD.encode(hash);

        Ok(Self {
            code_verifier,
            code_challenge,
            server_shutdown_tx: None,
        })
    }

    pub fn get_auth_url(&self, port: u16) -> String {
        let callback_url = format!("{}:{}", CALLBACK_BASE, port);
        format!(
            "{}?callback={}&code_challenge={}",
            TETRATE_AUTH_URL,
            urlencoding::encode(&callback_url),
            urlencoding::encode(&self.code_challenge)
        )
    }

    pub async fn exchange_code(&self, code: String) -> Result<String> {
        let client = Client::new();

        let request_body = TokenRequest {
            code: code.clone(),
            code_verifier: self.code_verifier.clone(),
        };

        eprintln!("Exchanging code for API key...");
        eprintln!("Code: {}", code);
        eprintln!("Code verifier length: {}", self.code_verifier.len());
        eprintln!("Code challenge: {}", self.code_challenge);

        let response = client
            .post(TETRATE_TOKEN_URL)
            .header("X-Title", "goose")
            .header("Referer", "https://github.com/aaif-goose/goose")
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            eprintln!("Token exchange failed!");
            eprintln!("Status: {}", status);
            eprintln!("Error response: {}", error_text);
            return Err(anyhow!(
                "Failed to exchange code: {} - {}",
                status,
                error_text
            ));
        }

        let token_response: TokenResponse = response.json().await?;
        Ok(token_response.key)
    }

    /// Complete flow: start server, open browser, wait for callback, exchange code
    pub async fn complete_flow(&mut self) -> Result<String> {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();

        let (code_tx, code_rx) = oneshot::channel::<String>();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        self.server_shutdown_tx = Some(shutdown_tx);

        tokio::spawn(async move {
            if let Err(e) = server::run_callback_server(listener, code_tx, shutdown_rx).await {
                eprintln!("Server error: {}", e);
            }
        });

        let auth_url = self.get_auth_url(port);

        println!("Opening browser for Tetrate Agent Router Service authentication...");
        eprintln!("Auth URL: {}", auth_url);

        if let Err(e) = webbrowser::open(&auth_url) {
            eprintln!("Failed to open browser automatically: {}", e);
            println!("Please open this URL manually: {}", auth_url);
        }

        println!("Waiting for authentication callback on port {}...", port);

        let code = match timeout(AUTH_TIMEOUT, code_rx).await {
            Ok(Ok(code)) => Ok(code),
            Ok(Err(_)) => Err(anyhow!("Failed to receive authorization code")),
            Err(_) => Err(anyhow!("Authentication timeout - please try again")),
        }?;

        println!("Authorization code received. Exchanging for API key...");
        eprintln!("Received code: {}", code);

        let api_key = self.exchange_code(code).await?;

        // Shutdown the server if it's still running
        if let Some(tx) = self.server_shutdown_tx.take() {
            let _ = tx.send(());
        }

        Ok(api_key)
    }
}

pub use self::PkceAuthFlow as TetrateAuth;

use crate::config::Config;

pub fn configure_tetrate(config: &Config, api_key: String) -> Result<()> {
    config.set_secret("TETRATE_API_KEY", &api_key)?;
    config.set_goose_provider("tetrate")?;
    config.set_goose_model(TETRATE_DEFAULT_MODEL)?;
    Ok(())
}
