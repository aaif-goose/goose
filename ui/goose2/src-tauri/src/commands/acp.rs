use crate::services::acp::GooseServeProcess;

#[derive(serde::Serialize)]
pub struct GooseServeConnection {
    url: String,
    token: String,
}

#[tauri::command]
pub async fn get_goose_serve_connection(
    app_handle: tauri::AppHandle,
) -> Result<GooseServeConnection, String> {
    let process = GooseServeProcess::get(app_handle).await?;
    Ok(GooseServeConnection {
        url: process.ws_url(),
        token: process.token().to_string(),
    })
}
