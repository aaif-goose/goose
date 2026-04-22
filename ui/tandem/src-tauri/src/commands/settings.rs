use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::Manager;

/// User-configurable settings persisted as JSON under the app's config dir.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Absolute path to the Memory folder (flat list of notes).
    pub memory_dir: Option<String>,
    /// Absolute path to the Projects folder (each subfolder is a project).
    pub projects_dir: Option<String>,
}

fn settings_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app_handle
        .path()
        .app_config_dir()
        .map_err(|e| format!("Failed to resolve app config dir: {e}"))?;
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create config dir {}: {e}", dir.display()))?;
    Ok(dir.join("settings.json"))
}

#[tauri::command]
pub async fn get_settings(app_handle: tauri::AppHandle) -> Result<Settings, String> {
    let path = settings_path(&app_handle)?;
    if !path.exists() {
        return Ok(Settings::default());
    }
    let bytes = fs::read(&path)
        .map_err(|e| format!("Failed to read settings {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| format!("Failed to parse settings {}: {e}", path.display()))
}

#[tauri::command]
pub async fn update_settings(
    app_handle: tauri::AppHandle,
    settings: Settings,
) -> Result<Settings, String> {
    let path = settings_path(&app_handle)?;
    let json = serde_json::to_vec_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;
    fs::write(&path, json)
        .map_err(|e| format!("Failed to write settings {}: {e}", path.display()))?;
    Ok(settings)
}
