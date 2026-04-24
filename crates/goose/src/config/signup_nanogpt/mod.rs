use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use tokio::time::{sleep, timeout};

use crate::config::Config;
use crate::providers::api_client::{ApiClient, AuthMethod};

const NANOGPT_CLI_LOGIN_HOST: &str = "https://nano-gpt.com/api/cli-login";
const AUTH_TIMEOUT: Duration = Duration::from_secs(180); // 3 minutes
const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Deserialize)]
struct StartResponse {
    device_code: String,
    verification_uri_complete: String,
}

#[derive(Debug, Deserialize)]
struct PollResponse {
    key: String,
}

fn build_client() -> Result<ApiClient> {
    ApiClient::new(NANOGPT_CLI_LOGIN_HOST.to_string(), AuthMethod::NoAuth)?
        .with_header("x-client", "goose")
}

async fn poll_for_token(client: &ApiClient, device_code: &str) -> Result<String> {
    loop {
        sleep(POLL_INTERVAL).await;

        let body = json!({ "device_code": device_code });

        let response = client.response_post(None, "poll", &body).await?;
        // https://docs.nano-gpt.com/integrations/cli-login#response-codes
        match response.status().as_u16() {
            200 => {
                let poll_resp: PollResponse = response.json().await?;
                return Ok(poll_resp.key);
            }
            202 => {
                continue;
            }
            410 => {
                return Err(anyhow!("Device code has expired - please try again"));
            }
            409 => {
                return Err(anyhow!("Device code has already been consumed"));
            }
            404 => {
                return Err(anyhow!("Invalid device code"));
            }
            429 => {
                return Err(anyhow!(
                    "Too many requests to NanoGPT. Please wait a moment and try again."
                ));
            }
            other => {
                let error_text = response.text().await.unwrap_or_default();
                return Err(anyhow!(
                    "Unexpected poll response: {} - {}",
                    other,
                    error_text
                ));
            }
        }
    }
}

pub async fn complete_nanogpt_auth() -> Result<String> {
    let client = build_client()?;
    let body = json!({ "client_name": "goose" });

    let response = client.response_post(None, "start", &body).await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "Failed to start NanoGPT device flow: {} - {}",
            status,
            error_text
        ));
    }

    let start_resp: StartResponse = response.json().await?;

    println!("Opening browser for NanoGPT authentication...");

    if let Err(e) = webbrowser::open(&start_resp.verification_uri_complete) {
        eprintln!("Failed to open browser automatically: {}", e);
        println!(
            "Please open this URL manually: {}",
            start_resp.verification_uri_complete
        );
    }

    println!("Waiting for NanoGPT authorization...");

    match timeout(
        AUTH_TIMEOUT,
        poll_for_token(&client, &start_resp.device_code),
    )
    .await
    {
        Ok(Ok(api_key)) => Ok(api_key),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(anyhow!("Authentication timed out - please try again")),
    }
}

pub fn configure_nanogpt(config: &Config, api_key: String) -> Result<()> {
    config.set_secret("NANOGPT_API_KEY", &api_key)?;
    config.set_goose_provider("nano-gpt")?;
    Ok(())
}
