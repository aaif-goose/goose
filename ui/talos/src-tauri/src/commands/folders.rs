use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;

/// A text-ish note surfaced in the Memory list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteInfo {
    pub id: String,
    pub title: String,
    pub path: String,
    pub kind: String,
    /// Milliseconds since epoch, or null if unavailable.
    pub updated_ms: Option<u128>,
    pub bytes: u64,
}

/// A project = a subdirectory of the projects root.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub note_count: usize,
}

fn expand_home(input: &str) -> PathBuf {
    if let Some(rest) = input.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    if input == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(input)
}

fn updated_ms(meta: &fs::Metadata) -> Option<u128> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
}

fn note_kind_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "md" | "markdown" | "mdx" => "note",
        "txt" => "note",
        "wiki" => "wiki",
        _ => "note",
    }
}

fn title_from_filename(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.display().to_string())
}

fn list_notes_in(dir: &Path) -> Result<Vec<NoteInfo>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read {}: {e}", dir.display()))?;
    let mut notes = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') {
                continue;
            }
        }
        notes.push(NoteInfo {
            id: path.display().to_string(),
            title: title_from_filename(&path),
            path: path.display().to_string(),
            kind: note_kind_for(&path).to_string(),
            updated_ms: updated_ms(&meta),
            bytes: meta.len(),
        });
    }
    notes.sort_by(|a, b| b.updated_ms.cmp(&a.updated_ms));
    Ok(notes)
}

#[tauri::command]
pub async fn list_memory_notes(dir: String) -> Result<Vec<NoteInfo>, String> {
    let root = expand_home(&dir);
    list_notes_in(&root)
}

#[tauri::command]
pub async fn read_note(path: String) -> Result<String, String> {
    let p = expand_home(&path);
    fs::read_to_string(&p).map_err(|e| format!("Failed to read {}: {e}", p.display()))
}

#[tauri::command]
pub async fn list_projects(dir: String) -> Result<Vec<ProjectInfo>, String> {
    let root = expand_home(&dir);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(&root)
        .map_err(|e| format!("Failed to read {}: {e}", root.display()))?;
    let mut projects = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_dir() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') {
                continue;
            }
        }
        let note_count = list_notes_in(&path).map(|n| n.len()).unwrap_or(0);
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| path.display().to_string());
        projects.push(ProjectInfo {
            id: path.display().to_string(),
            name,
            path: path.display().to_string(),
            note_count,
        });
    }
    projects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(projects)
}

#[tauri::command]
pub async fn list_project_notes(project_path: String) -> Result<Vec<NoteInfo>, String> {
    let root = expand_home(&project_path);
    list_notes_in(&root)
}
