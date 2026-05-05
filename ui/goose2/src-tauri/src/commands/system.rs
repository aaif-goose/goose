use base64::Engine;
use serde::Serialize;
use tauri::Window;
use tauri_plugin_dialog::DialogExt;

use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::process::Command;

const DEFAULT_FILE_MENTION_LIMIT: usize = 1500;
const MAX_FILE_MENTION_LIMIT: usize = 5000;
const MAX_SCAN_DEPTH: usize = 8;
const MAX_IMAGE_ATTACHMENT_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentPathInfo {
    pub name: String,
    pub path: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImageAttachmentPayload {
    pub base64: String,
    pub mime_type: String,
}

#[tauri::command]
pub fn get_home_dir() -> Result<String, String> {
    let home_dir = dirs::home_dir().ok_or("Could not determine home directory")?;
    Ok(home_dir.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn save_exported_session_file(
    window: Window,
    default_filename: String,
    contents: String,
) -> Result<Option<String>, String> {
    let desktop =
        dirs::desktop_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Desktop"));

    let mut dialog = window
        .dialog()
        .file()
        .set_title("Export Session")
        .set_file_name(default_filename)
        .set_directory(desktop)
        .add_filter("JSON", &["json"]);

    #[cfg(desktop)]
    {
        dialog = dialog.set_parent(&window);
    }

    let Some(path) = dialog.blocking_save_file() else {
        return Ok(None);
    };

    let path = path
        .into_path()
        .map_err(|_| "Selected save path is not available".to_string())?;
    std::fs::write(&path, contents)
        .map_err(|e| format!("Failed to write file '{}': {}", path.display(), e))?;

    Ok(Some(path.to_string_lossy().into_owned()))
}

fn path_components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect()
}

fn has_sequence_with_remaining(path: &Path, sequence: &[&str], remaining: usize) -> bool {
    let components = path_components(path);
    components
        .windows(sequence.len())
        .enumerate()
        .any(|(index, window)| {
            window
                .iter()
                .map(String::as_str)
                .eq(sequence.iter().copied())
                && components.len() == index + sequence.len() + remaining
        })
}

fn is_shareable_agent_file(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("md")
        && (has_sequence_with_remaining(path, &[".agents", "agents"], 1)
            || has_sequence_with_remaining(path, &[".goose", "agents"], 1))
}

fn is_shareable_skill_file(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md")
        && (has_sequence_with_remaining(path, &[".agents", "skills"], 2)
            || has_sequence_with_remaining(path, &[".goose", "skills"], 2)
            || has_sequence_with_remaining(path, &[".claude", "skills"], 2)
            || has_sequence_with_remaining(path, &[".config", "agents", "skills"], 2)
            || has_sequence_with_remaining(path, &["goose", "skills"], 2)
            || is_plugin_skill_file(path))
}

fn is_plugin_skill_file(path: &Path) -> bool {
    let components = path_components(path);
    components.windows(6).enumerate().any(|(index, window)| {
        window[0] == "goose"
            && window[1] == "plugins"
            && window[3] == "skills"
            && components.len() == index + 6
    })
}

fn validate_shareable_source_file(path: &Path) -> Result<PathBuf, String> {
    let source = path
        .canonicalize()
        .map_err(|error| format!("Failed to inspect '{}': {}", path.display(), error))?;
    let metadata = fs::metadata(&source)
        .map_err(|error| format!("Failed to inspect '{}': {}", source.display(), error))?;
    if !metadata.is_file() {
        return Err(format!("Path is not a file: {}", source.display()));
    }
    if !is_shareable_agent_file(&source) && !is_shareable_skill_file(&source) {
        return Err(format!(
            "Path is not a shareable agent or skill file: {}",
            source.display()
        ));
    }
    Ok(source)
}

#[tauri::command]
pub fn copy_file_to_clipboard(path: String) -> Result<(), String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let source = validate_shareable_source_file(Path::new(&path))?;

    #[cfg(target_os = "macos")]
    {
        let output = Command::new("osascript")
            .args([
                "-e",
                "on run argv",
                "-e",
                "set the clipboard to POSIX file (item 1 of argv)",
                "-e",
                "end run",
            ])
            .arg(source.as_os_str())
            .output()
            .map_err(|error| format!("Failed to copy file: {}", error))?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            "Failed to copy file".to_string()
        } else {
            stderr
        })
    }

    #[cfg(target_os = "windows")]
    {
        let output = Command::new("powershell")
            .args([
                "-Sta",
                "-NoProfile",
                "-Command",
                "Add-Type -AssemblyName System.Windows.Forms; $files = New-Object System.Collections.Specialized.StringCollection; [void]$files.Add($args[0]); [System.Windows.Forms.Clipboard]::SetFileDropList($files)",
            ])
            .arg(source.as_os_str())
            .output()
            .map_err(|error| format!("Failed to copy file: {}", error))?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "Failed to copy file".to_string()
        } else {
            stderr
        });
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        validate_shareable_source_file(Path::new(&path))?;
        Err("Copy file is not supported on this platform yet.".to_string())
    }
}

#[tauri::command]
pub async fn save_file_copy(window: Window, source_path: String) -> Result<Option<String>, String> {
    let source = validate_shareable_source_file(Path::new(&source_path))?;
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Source file is missing a valid filename".to_string())?;
    let default_dir =
        dirs::desktop_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Desktop"));

    let mut dialog = window
        .dialog()
        .file()
        .set_title("Save a Copy")
        .set_file_name(file_name)
        .set_directory(default_dir);

    #[cfg(desktop)]
    {
        dialog = dialog.set_parent(&window);
    }

    let Some(destination) = dialog.blocking_save_file() else {
        return Ok(None);
    };
    let destination = destination
        .into_path()
        .map_err(|_| "Selected save path is not available".to_string())?;

    if Some(source.as_path()) == destination.canonicalize().ok().as_deref() {
        return Ok(Some(destination.to_string_lossy().into_owned()));
    }

    fs::copy(&source, &destination).map_err(|error| {
        format!(
            "Failed to save copy from '{}' to '{}': {}",
            source.display(),
            destination.display(),
            error
        )
    })?;

    Ok(Some(destination.to_string_lossy().into_owned()))
}

#[tauri::command]
#[allow(dead_code)]
pub fn path_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

fn read_directory_entries(path: &Path) -> Result<Vec<FileTreeEntry>, String> {
    if !path.exists() {
        return Err(format!("Directory does not exist: {}", path.display()));
    }

    let metadata = fs::metadata(path)
        .map_err(|error| format!("Failed to inspect '{}': {}", path.display(), error))?;
    if !metadata.is_dir() {
        return Err(format!("Path is not a directory: {}", path.display()));
    }

    let mut entries = Vec::new();
    let reader = fs::read_dir(path)
        .map_err(|error| format!("Failed to read directory '{}': {}", path.display(), error))?;

    for entry in reader {
        let Ok(entry) = entry else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        let Some(file_tree_entry) = build_file_tree_entry(entry.path(), name) else {
            continue;
        };

        entries.push(file_tree_entry);
    }

    entries.sort_by(|a, b| {
        let a_rank = if a.kind == "directory" { 0 } else { 1 };
        let b_rank = if b.kind == "directory" { 0 } else { 1 };
        a_rank
            .cmp(&b_rank)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(entries)
}

fn build_file_tree_entry(path: PathBuf, name: String) -> Option<FileTreeEntry> {
    let metadata = fs::symlink_metadata(&path).ok()?;
    let file_type = metadata.file_type();

    Some(FileTreeEntry {
        name,
        path: path.to_string_lossy().into_owned(),
        kind: if file_type.is_dir() {
            "directory".to_string()
        } else {
            "file".to_string()
        },
    })
}

#[tauri::command]
pub fn list_directory_entries(path: String) -> Result<Vec<FileTreeEntry>, String> {
    read_directory_entries(Path::new(&path))
}

fn inspect_attachment_path(path: &Path) -> Result<AttachmentPathInfo, String> {
    if !path.exists() {
        return Err(format!(
            "Attachment path does not exist: {}",
            path.display()
        ));
    }

    let metadata = fs::metadata(path)
        .map_err(|error| format!("Failed to inspect '{}': {}", path.display(), error))?;
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    Ok(AttachmentPathInfo {
        name,
        path: path.to_string_lossy().into_owned(),
        kind: if metadata.is_dir() {
            "directory".to_string()
        } else {
            "file".to_string()
        },
        mime_type: if metadata.is_file() {
            mime_guess::from_path(path)
                .first_raw()
                .map(std::borrow::ToOwned::to_owned)
        } else {
            None
        },
    })
}

fn normalized_path_key(path: &Path) -> String {
    if let Ok(canonical) = path.canonicalize() {
        return canonical.to_string_lossy().into_owned();
    }

    let raw = path.to_string_lossy().into_owned();
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        raw.to_lowercase()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        raw
    }
}

fn normalize_attachment_paths(paths: Vec<String>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();

    for raw_path in paths {
        let trimmed = raw_path.trim();
        if trimmed.is_empty() {
            continue;
        }

        let path = PathBuf::from(trimmed);
        let key = normalized_path_key(&path);
        if seen.insert(key) {
            normalized.push(path);
        }
    }

    normalized
}

#[tauri::command]
pub fn inspect_attachment_paths(paths: Vec<String>) -> Result<Vec<AttachmentPathInfo>, String> {
    let mut attachments = Vec::new();

    for path in normalize_attachment_paths(paths) {
        if let Ok(attachment) = inspect_attachment_path(&path) {
            attachments.push(attachment);
        }
    }

    Ok(attachments)
}

#[tauri::command]
pub fn read_image_attachment(path: String) -> Result<ImageAttachmentPayload, String> {
    let attachment = inspect_attachment_path(Path::new(&path))?;
    let mime_type = attachment
        .mime_type
        .ok_or_else(|| format!("Unable to determine image type for '{}'", attachment.path))?;

    if !mime_type.starts_with("image/") {
        return Err(format!("Attachment is not an image: {}", attachment.path));
    }

    let metadata = fs::metadata(&attachment.path)
        .map_err(|error| format!("Failed to inspect image '{}': {}", attachment.path, error))?;
    if metadata.len() > MAX_IMAGE_ATTACHMENT_BYTES {
        return Err(format!(
            "Image attachment '{}' exceeds the {} MB limit",
            attachment.path,
            MAX_IMAGE_ATTACHMENT_BYTES / (1024 * 1024)
        ));
    }

    let bytes = fs::read(&attachment.path)
        .map_err(|error| format!("Failed to read image '{}': {}", attachment.path, error))?;

    Ok(ImageAttachmentPayload {
        base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        mime_type,
    })
}

fn normalize_roots(roots: Vec<String>) -> Vec<PathBuf> {
    let mut dedup = HashSet::new();
    let mut normalized = Vec::new();
    for root in roots {
        let trimmed = root.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = PathBuf::from(trimmed);
        let key = normalized_path_key(&path);
        if dedup.insert(key) {
            normalized.push(path);
        }
    }
    normalized
}

fn scan_files_for_mentions(roots: Vec<String>, max_results: Option<usize>) -> Vec<String> {
    let roots = normalize_roots(roots);
    if roots.is_empty() {
        return Vec::new();
    }

    let limit = max_results
        .unwrap_or(DEFAULT_FILE_MENTION_LIMIT)
        .clamp(1, MAX_FILE_MENTION_LIMIT);

    let mut builder = ignore::WalkBuilder::new(&roots[0]);
    for root in &roots[1..] {
        builder.add(root);
    }
    builder
        .max_depth(Some(MAX_SCAN_DEPTH))
        .follow_links(false) // don't traverse symlinks
        .hidden(true) // skip hidden files/dirs
        .git_ignore(true) // respect .gitignore
        .git_global(true) // respect global gitignore
        .git_exclude(true); // respect .git/info/exclude

    // Canonicalize roots so we can reject paths that escape via symlink targets
    let canonical_roots: Vec<PathBuf> = roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .collect();

    let mut seen = HashSet::new();
    let mut files = Vec::new();

    for entry in builder.build().flatten() {
        if files.len() >= limit {
            break;
        }
        let Some(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_file() {
            continue;
        }
        // Reject any path that resolved outside the project roots
        let canonical = match entry.path().canonicalize() {
            Ok(c) => c,
            Err(_) => continue,
        };
        if !canonical_roots
            .iter()
            .any(|root| canonical.starts_with(root))
        {
            continue;
        }
        let path_str = entry.path().to_string_lossy().to_string();
        let dedup_key = normalized_path_key(entry.path());
        if seen.insert(dedup_key) {
            files.push(path_str);
        }
    }

    files.sort_by_key(|path| path.to_lowercase());
    files
}

#[tauri::command]
pub async fn list_files_for_mentions(
    roots: Vec<String>,
    max_results: Option<usize>,
) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || scan_files_for_mentions(roots, max_results))
        .await
        .map_err(|error| format!("Failed to scan files for mentions: {}", error))
}

#[cfg(test)]
#[path = "system_tests.rs"]
mod tests;
