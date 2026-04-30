use std::env;

use crate::services::acp::GooseServeProcess;
use serde::Serialize;

const GOOSE_SERVE_URL_ENV: &str = "GOOSE_SERVE_URL";
const GOOSE_SERVER_SECRET_KEY_ENV: &str = "GOOSE_SERVER__SECRET_KEY";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GooseServeHostInfo {
    pub http_base_url: String,
    pub secret_key: String,
}

#[tauri::command]
pub async fn get_goose_serve_url(app_handle: tauri::AppHandle) -> Result<String, String> {
    if let Some(url) = configured_goose_serve_url() {
        return Ok(url);
    }
    let process = GooseServeProcess::get(app_handle).await?;
    Ok(process.ws_url())
}

#[tauri::command]
pub async fn get_goose_serve_host_info(
    app_handle: tauri::AppHandle,
) -> Result<GooseServeHostInfo, String> {
    if let Some(url) = configured_goose_serve_url() {
        return Ok(GooseServeHostInfo {
            http_base_url: goose_serve_http_base_url(&url)?,
            secret_key: configured_goose_serve_secret_key()?,
        });
    }

    let process = GooseServeProcess::get(app_handle).await?;
    Ok(GooseServeHostInfo {
        http_base_url: process.http_base_url(),
        secret_key: process.secret_key().to_string(),
    })
}

fn configured_goose_serve_url() -> Option<String> {
    env::var(GOOSE_SERVE_URL_ENV)
        .ok()
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty())
}

fn configured_goose_serve_secret_key() -> Result<String, String> {
    env::var(GOOSE_SERVER_SECRET_KEY_ENV)
        .ok()
        .map(|secret| secret.trim().to_string())
        .filter(|secret| !secret.is_empty())
        .ok_or_else(|| {
            format!(
                "{GOOSE_SERVER_SECRET_KEY_ENV} must be set when {GOOSE_SERVE_URL_ENV} is set"
            )
        })
}

fn goose_serve_http_base_url(goose_serve_url: &str) -> Result<String, String> {
    let (scheme, rest) = goose_serve_url
        .trim()
        .split_once("://")
        .ok_or_else(|| format!("{GOOSE_SERVE_URL_ENV} must include a URL scheme"))?;
    let http_scheme = match scheme {
        "ws" => "http",
        "wss" => "https",
        "http" => "http",
        "https" => "https",
        _ => {
            return Err(format!(
                "{GOOSE_SERVE_URL_ENV} must use ws, wss, http, or https"
            ));
        }
    };
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .filter(|authority| !authority.is_empty())
        .ok_or_else(|| format!("{GOOSE_SERVE_URL_ENV} must include a host"))?;

    Ok(format!("{http_scheme}://{authority}"))
}

#[cfg(test)]
mod tests {
    use super::goose_serve_http_base_url;

    #[test]
    fn derives_http_base_url_from_websocket_url() {
        assert_eq!(
            goose_serve_http_base_url("ws://127.0.0.1:12345/acp").unwrap(),
            "http://127.0.0.1:12345"
        );
        assert_eq!(
            goose_serve_http_base_url("wss://example.test/acp").unwrap(),
            "https://example.test"
        );
    }

    #[test]
    fn derives_http_base_url_without_path() {
        assert_eq!(
            goose_serve_http_base_url("http://localhost:3000").unwrap(),
            "http://localhost:3000"
        );
    }

    #[test]
    fn rejects_invalid_goose_serve_url() {
        assert!(goose_serve_http_base_url("localhost:3000").is_err());
        assert!(goose_serve_http_base_url("ftp://localhost:3000/acp").is_err());
        assert!(goose_serve_http_base_url("ws:///acp").is_err());
    }
}
