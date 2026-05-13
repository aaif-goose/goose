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
        ensure_configured_goose_serve_supports_inline_apps(&url)?;
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
            format!("{GOOSE_SERVER_SECRET_KEY_ENV} must be set when {GOOSE_SERVE_URL_ENV} is set")
        })
}

fn goose_serve_http_base_url(goose_serve_url: &str) -> Result<String, String> {
    let (scheme, rest) = goose_serve_url
        .trim()
        .split_once("://")
        .ok_or_else(|| format!("{GOOSE_SERVE_URL_ENV} must include a URL scheme"))?;
    let http_scheme = match scheme {
        "ws" => "http",
        "http" => "http",
        _ => {
            return Err(format!(
                "{GOOSE_SERVE_URL_ENV} must use ws or http for inline MCP apps because the app guest origin is served over local http"
            ));
        }
    };
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .filter(|authority| !authority.is_empty())
        .ok_or_else(|| format!("{GOOSE_SERVE_URL_ENV} must include a host"))?;
    let path = rest
        .get(authority.len()..)
        .unwrap_or_default()
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    let path_prefix = goose_serve_http_path_prefix(path);

    Ok(format!("{http_scheme}://{authority}{path_prefix}"))
}

fn goose_serve_http_path_prefix(path: &str) -> String {
    let path = path.trim_end_matches('/');
    if path.is_empty() || path == "/acp" {
        return String::new();
    }

    if let Some(prefix) = path.strip_suffix("/acp") {
        return prefix.to_string();
    }

    path.to_string()
}

fn ensure_configured_goose_serve_supports_inline_apps(goose_serve_url: &str) -> Result<(), String> {
    if !goose_serve_url_uses_plaintext_http(goose_serve_url)? {
        return Err(format!(
            "{GOOSE_SERVE_URL_ENV} must use ws or http for inline MCP apps because the app guest origin is served over local http"
        ));
    }

    if goose_serve_url_is_loopback(goose_serve_url)? {
        return Ok(());
    }

    Err(format!(
        "{GOOSE_SERVE_URL_ENV} must point to localhost for inline MCP apps because the app guest origin is served from a loopback-only sandbox"
    ))
}

fn goose_serve_url_uses_plaintext_http(goose_serve_url: &str) -> Result<bool, String> {
    let (scheme, _) = goose_serve_url
        .trim()
        .split_once("://")
        .ok_or_else(|| format!("{GOOSE_SERVE_URL_ENV} must include a URL scheme"))?;
    Ok(matches!(scheme, "ws" | "http"))
}

fn goose_serve_url_is_loopback(goose_serve_url: &str) -> Result<bool, String> {
    let (_, rest) = goose_serve_url
        .trim()
        .split_once("://")
        .ok_or_else(|| format!("{GOOSE_SERVE_URL_ENV} must include a URL scheme"))?;
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .filter(|authority| !authority.is_empty())
        .ok_or_else(|| format!("{GOOSE_SERVE_URL_ENV} must include a host"))?;
    if authority.contains('@') {
        return Ok(false);
    }

    let host = if let Some(remainder) = authority.strip_prefix('[') {
        remainder
            .split_once(']')
            .map(|(host, _)| host)
            .unwrap_or(remainder)
    } else {
        authority.split(':').next().unwrap_or(authority)
    }
    .to_ascii_lowercase();

    Ok(host == "localhost"
        || host
            .parse::<std::net::Ipv4Addr>()
            .is_ok_and(|addr| addr.is_loopback())
        || host
            .parse::<std::net::Ipv6Addr>()
            .is_ok_and(|addr| addr.is_loopback()))
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_configured_goose_serve_supports_inline_apps, goose_serve_http_base_url,
        goose_serve_url_is_loopback,
    };

    #[test]
    fn derives_http_base_url_from_websocket_url() {
        assert_eq!(
            goose_serve_http_base_url("ws://127.0.0.1:12345/acp").unwrap(),
            "http://127.0.0.1:12345"
        );
        assert_eq!(
            goose_serve_http_base_url("http://localhost:3000/acp").unwrap(),
            "http://localhost:3000"
        );
    }

    #[test]
    fn preserves_path_prefix_from_websocket_url() {
        assert_eq!(
            goose_serve_http_base_url("ws://localhost:3000/goose/acp").unwrap(),
            "http://localhost:3000/goose"
        );
        assert_eq!(
            goose_serve_http_base_url("http://localhost:3000/goose/acp?token=abc").unwrap(),
            "http://localhost:3000/goose"
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
        assert!(goose_serve_http_base_url("wss://localhost:3000/acp").is_err());
        assert!(goose_serve_http_base_url("https://localhost:3000/acp").is_err());
        assert!(goose_serve_http_base_url("ws:///acp").is_err());
    }

    #[test]
    fn detects_loopback_goose_serve_urls() {
        assert!(goose_serve_url_is_loopback("ws://127.0.0.1:12345/acp").unwrap());
        assert!(goose_serve_url_is_loopback("ws://localhost:12345/acp").unwrap());
        assert!(goose_serve_url_is_loopback("ws://[::1]:12345/acp").unwrap());
        assert!(!goose_serve_url_is_loopback("wss://example.test/acp").unwrap());
    }

    #[test]
    fn rejects_remote_configured_urls_for_inline_apps() {
        assert!(
            ensure_configured_goose_serve_supports_inline_apps("ws://127.0.0.1:12345/acp").is_ok()
        );
        assert!(
            ensure_configured_goose_serve_supports_inline_apps("wss://example.test/acp").is_err()
        );
        assert!(
            ensure_configured_goose_serve_supports_inline_apps("wss://localhost:12345/acp")
                .is_err()
        );
    }
}
