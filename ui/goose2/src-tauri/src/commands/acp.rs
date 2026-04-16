use crate::services::acp::GooseServeProcess;

#[tauri::command]
pub async fn get_goose_serve_url() -> Result<String, String> {
    GooseServeProcess::start().await?;
    let process = GooseServeProcess::get()?;
    Ok(process.ws_url())
}
