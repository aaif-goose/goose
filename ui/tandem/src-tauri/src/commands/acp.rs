use crate::services::goose_serve::GooseServeProcess;

#[tauri::command]
pub async fn get_goose_serve_url(app_handle: tauri::AppHandle) -> Result<String, String> {
    let process = GooseServeProcess::get(app_handle).await?;
    Ok(process.ws_url())
}
