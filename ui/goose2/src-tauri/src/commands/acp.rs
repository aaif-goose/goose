use std::env;

use crate::services::acp::GooseServeProcess;

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
