use std::env;

use crate::services::acp::GooseServeProcess;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GooseServeHostInfo {
    pub http_base_url: String,
    pub secret_key: String,
}

#[tauri::command]
pub async fn get_goose_serve_url(app_handle: tauri::AppHandle) -> Result<String, String> {
    if let Ok(url) = env::var("GOOSE_SERVE_URL") {
        if !url.is_empty() {
            return Ok(url);
        }
    }
    let process = GooseServeProcess::get(app_handle).await?;
    Ok(process.ws_url())
}

#[tauri::command]
pub async fn get_goose_serve_host_info(
    app_handle: tauri::AppHandle,
) -> Result<GooseServeHostInfo, String> {
    let process = GooseServeProcess::get(app_handle).await?;
    Ok(GooseServeHostInfo {
        http_base_url: process.http_base_url(),
        secret_key: process.secret_key().to_string(),
    })
}
