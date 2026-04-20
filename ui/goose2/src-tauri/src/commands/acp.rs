use std::env;

use crate::services::acp::GooseServeProcess;

#[tauri::command]
pub async fn get_goose_serve_url(app_handle: tauri::AppHandle) -> Result<String, String> {
    if env::var("GOOSE_SERVE_URL").is_ok() {
        return Ok(env::var("GOOSE_SERVE_URL").unwrap());
    }
    let process = GooseServeProcess::get(app_handle).await?;
    Ok(process.ws_url())
}
