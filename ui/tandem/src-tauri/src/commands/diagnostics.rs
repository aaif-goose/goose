use std::fs::File;
use std::io::Write;
use std::path::Path;

use chrono::{Datelike, Local, Timelike, Utc};
use serde::{Deserialize, Serialize};
use tauri::Manager;
use zip::{write::SimpleFileOptions, CompressionMethod, DateTime as ZipDateTime, ZipWriter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub app_version: String,
    pub app_name: String,
    pub tauri_version: String,
    pub os: String,
    pub os_version: String,
    pub architecture: String,
    pub timestamp_utc: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub enabled_mcp_servers: Vec<String>,
}

fn collect_system_info(
    provider: Option<String>,
    model: Option<String>,
    enabled_mcp_servers: Vec<String>,
) -> SystemInfo {
    let info = os_info::get();
    SystemInfo {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        app_name: "Tandem".to_string(),
        tauri_version: tauri::VERSION.to_string(),
        os: std::env::consts::OS.to_string(),
        os_version: info.version().to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        timestamp_utc: Utc::now().to_rfc3339(),
        provider: provider.or_else(|| read_goose_config_key("GOOSE_PROVIDER")),
        model: model.or_else(|| read_goose_config_key("GOOSE_MODEL")),
        enabled_mcp_servers,
    }
}

const MEMORY_TOTAL_CAP: u64 = 50 * 1024 * 1024;
const MEMORY_PER_FILE_CAP: u64 = 5 * 1024 * 1024;

/// Walk `root` recursively and stuff each text file under `memory/<rel path>`
/// in the ZIP. Skips dot-prefixed path segments (`.git`, `.DS_Store`, ...),
/// files larger than [`MEMORY_PER_FILE_CAP`], and anything with a null byte
/// in its first 8 KiB. Stops accepting files once [`MEMORY_TOTAL_CAP`] is hit
/// and writes a `memory/_TRUNCATED.txt` listing what was omitted.
fn add_memory_dir(
    zip: &mut ZipWriter<File>,
    options: SimpleFileOptions,
    root: &Path,
) -> Result<Vec<String>, String> {
    if !root.exists() {
        return Err(format!("memory dir does not exist: {}", root.display()));
    }
    let mut entries: Vec<String> = Vec::new();
    let mut omitted: Vec<String> = Vec::new();
    let mut total: u64 = 0;
    let mut stack: Vec<std::path::PathBuf> = vec![root.to_path_buf()];

    while let Some(current) = stack.pop() {
        let dir_iter = match std::fs::read_dir(&current) {
            Ok(d) => d,
            Err(e) => {
                omitted.push(format!("{}: read_dir failed ({e})", current.display()));
                continue;
            }
        };
        for entry in dir_iter.filter_map(Result::ok) {
            let path = entry.path();
            let name = match path.file_name().and_then(|s| s.to_str()) {
                Some(n) => n,
                None => continue,
            };
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path.is_file() {
                continue;
            }
            let rel = match path.strip_prefix(root) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");

            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    omitted.push(format!("{rel_str}: stat failed ({e})"));
                    continue;
                }
            };
            if meta.len() > MEMORY_PER_FILE_CAP {
                omitted.push(format!("{rel_str}: exceeds per-file cap ({} bytes)", meta.len()));
                continue;
            }
            if total + meta.len() > MEMORY_TOTAL_CAP {
                omitted.push(format!("{rel_str}: total cap reached"));
                continue;
            }

            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    omitted.push(format!("{rel_str}: read failed ({e})"));
                    continue;
                }
            };
            if looks_binary(&bytes) {
                omitted.push(format!("{rel_str}: looks binary"));
                continue;
            }

            let entry_name = format!("memory/{rel_str}");
            zip.start_file(&entry_name, options)
                .map_err(|e| format!("zip start_file {entry_name}: {e}"))?;
            zip.write_all(&bytes)
                .map_err(|e| format!("zip write {entry_name}: {e}"))?;
            total += meta.len();
            entries.push(entry_name);
        }
    }

    if !omitted.is_empty() {
        let mut note = String::from(
            "The following files from the memory folder were omitted from this bundle:\n\n",
        );
        for line in &omitted {
            note.push_str("- ");
            note.push_str(line);
            note.push('\n');
        }
        zip.start_file("memory/_TRUNCATED.txt", options)
            .map_err(|e| format!("zip start_file memory/_TRUNCATED.txt: {e}"))?;
        zip.write_all(note.as_bytes())
            .map_err(|e| format!("zip write memory/_TRUNCATED.txt: {e}"))?;
        entries.push("memory/_TRUNCATED.txt".to_string());
    }

    Ok(entries)
}

/// Classic heuristic: treat as binary if a null byte appears in the first
/// 8 KiB. Avoids shipping PDFs, images, etc.
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8 * 1024).any(|&b| b == 0)
}

fn read_file_if_exists(path: &Path) -> Option<Vec<u8>> {
    if path.exists() {
        std::fs::read(path).ok()
    } else {
        None
    }
}

/// Strip composer drafts from `ui.tabs[*].composer` before shipping state.json.
/// Returns the original bytes if parsing fails (don't silently drop the file).
fn scrub_state_json(bytes: &[u8]) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return bytes.to_vec();
    };
    if let Some(tabs) = root
        .get_mut("ui")
        .and_then(|ui| ui.get_mut("tabs"))
        .and_then(|t| t.as_array_mut())
    {
        for tab in tabs {
            if let Some(obj) = tab.as_object_mut() {
                if obj.contains_key("composer") {
                    obj.insert(
                        "composer".to_string(),
                        serde_json::Value::String(String::new()),
                    );
                }
            }
        }
    }
    serde_json::to_vec_pretty(&root).unwrap_or_else(|_| bytes.to_vec())
}

fn format_system_text(info: &SystemInfo) -> String {
    let mcp = if info.enabled_mcp_servers.is_empty() {
        "(none)".to_string()
    } else {
        info.enabled_mcp_servers.join(", ")
    };
    format!(
        "App:           {} {}\n\
         Tauri:         {}\n\
         OS:            {} {}\n\
         Architecture:  {}\n\
         Provider:      {}\n\
         Model:         {}\n\
         MCP enabled:   {}\n\
         Timestamp:     {}\n",
        info.app_name,
        info.app_version,
        info.tauri_version,
        info.os,
        info.os_version,
        info.architecture,
        info.provider.as_deref().unwrap_or("unknown"),
        info.model.as_deref().unwrap_or("unknown"),
        mcp,
        info.timestamp_utc,
    )
}

/// Resolve a goose config value: env var wins, then `config.yaml`, then None.
/// Config path mirrors goose's `etcetera` layout under `Block/goose/config/`.
fn read_goose_config_key(key: &str) -> Option<String> {
    if let Ok(v) = std::env::var(key) {
        if !v.is_empty() {
            return Some(v);
        }
    }
    let path = dirs::config_dir()?
        .join("Block")
        .join("goose")
        .join("config")
        .join("config.yaml");
    let text = std::fs::read_to_string(&path).ok()?;
    let value: serde_yaml::Value = serde_yaml::from_str(&text).ok()?;
    value.get(key)?.as_str().map(|s| s.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsRequest {
    pub session_transcript_json: String,
    pub session_title: String,
    pub session_tab_id: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub enabled_mcp_servers: Vec<String>,
    pub output_zip_path: String,
    #[serde(default)]
    pub include_memory_dir: bool,
    #[serde(default)]
    pub memory_dir: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteDiagnosticsResult {
    pub bytes_written: u64,
    pub entries: Vec<String>,
    pub output_path: String,
}

#[tauri::command]
pub async fn get_system_info() -> Result<SystemInfo, String> {
    Ok(collect_system_info(None, None, Vec::new()))
}

#[tauri::command]
pub async fn write_diagnostics_zip(
    app_handle: tauri::AppHandle,
    request: DiagnosticsRequest,
) -> Result<WriteDiagnosticsResult, String> {
    let output_path = request.output_zip_path.clone();
    let file = File::create(&output_path)
        .map_err(|e| format!("Failed to create {}: {e}", output_path))?;
    let mut zip = ZipWriter::new(file);
    let now = Local::now();
    let mtime = ZipDateTime::from_date_and_time(
        now.year() as u16,
        now.month() as u8,
        now.day() as u8,
        now.hour() as u8,
        now.minute() as u8,
        now.second() as u8,
    )
    .unwrap_or_default();
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(mtime);
    let mut entries: Vec<String> = Vec::new();

    let info = collect_system_info(
        request.provider.clone(),
        request.model.clone(),
        request.enabled_mcp_servers.clone(),
    );

    let system_json = serde_json::to_vec_pretty(&info)
        .map_err(|e| format!("Failed to serialize system info: {e}"))?;
    zip.start_file("system.json", options)
        .map_err(|e| format!("zip start_file system.json: {e}"))?;
    zip.write_all(&system_json)
        .map_err(|e| format!("zip write system.json: {e}"))?;
    entries.push("system.json".to_string());

    let system_txt = format_system_text(&info);
    zip.start_file("system.txt", options)
        .map_err(|e| format!("zip start_file system.txt: {e}"))?;
    zip.write_all(system_txt.as_bytes())
        .map_err(|e| format!("zip write system.txt: {e}"))?;
    entries.push("system.txt".to_string());

    zip.start_file("session.json", options)
        .map_err(|e| format!("zip start_file session.json: {e}"))?;
    zip.write_all(request.session_transcript_json.as_bytes())
        .map_err(|e| format!("zip write session.json: {e}"))?;
    entries.push("session.json".to_string());

    let readme = format!(
        "Tandem diagnostics bundle\n\
         ========================\n\
         Generated: {}\n\
         App: {} {}\n\
         Session: {} (tab id: {})\n\
         \n\
         This bundle contains system information and a snapshot of the current\n\
         chat transcript. Do not post this file publicly if the transcript may\n\
         contain secrets, API keys, file paths you consider sensitive, or other\n\
         private information.\n",
        info.timestamp_utc,
        info.app_name,
        info.app_version,
        request.session_title,
        request.session_tab_id,
    );
    zip.start_file("README.txt", options)
        .map_err(|e| format!("zip start_file README.txt: {e}"))?;
    zip.write_all(readme.as_bytes())
        .map_err(|e| format!("zip write README.txt: {e}"))?;
    entries.push("README.txt".to_string());

    if let Ok(config_dir) = app_handle.path().app_config_dir() {
        if let Some(bytes) = read_file_if_exists(&config_dir.join("settings.json")) {
            zip.start_file("settings.json", options)
                .map_err(|e| format!("zip start_file settings.json: {e}"))?;
            zip.write_all(&bytes)
                .map_err(|e| format!("zip write settings.json: {e}"))?;
            entries.push("settings.json".to_string());
        }
        if let Some(bytes) = read_file_if_exists(&config_dir.join("state.json")) {
            let scrubbed = scrub_state_json(&bytes);
            zip.start_file("state.json", options)
                .map_err(|e| format!("zip start_file state.json: {e}"))?;
            zip.write_all(&scrubbed)
                .map_err(|e| format!("zip write state.json: {e}"))?;
            entries.push("state.json".to_string());
        }
    }

    for log in crate::services::log_capture::find_recent_logs() {
        let bytes = match crate::services::log_capture::read_tail_bytes(
            &log.path,
            crate::services::log_capture::MAX_BYTES_PER_LOG,
        ) {
            Ok(b) => b,
            Err(e) => {
                log::warn!(
                    "Skipping log {} in diagnostics bundle: {e}",
                    log.path.display()
                );
                continue;
            }
        };
        let entry_name = format!("logs/{}/{}", log.date_dir, log.file_name);
        zip.start_file(&entry_name, options)
            .map_err(|e| format!("zip start_file {entry_name}: {e}"))?;
        zip.write_all(&bytes)
            .map_err(|e| format!("zip write {entry_name}: {e}"))?;
        entries.push(entry_name);
    }

    if request.include_memory_dir {
        if let Some(dir) = request.memory_dir.as_deref() {
            match add_memory_dir(&mut zip, options, Path::new(dir)) {
                Ok(added) => {
                    for name in added {
                        entries.push(name);
                    }
                }
                Err(e) => {
                    log::warn!("Skipping memory dir in diagnostics bundle: {e}");
                }
            }
        }
    }

    let mut file = zip
        .finish()
        .map_err(|e| format!("zip finish: {e}"))?;
    file.flush().ok();
    let metadata = std::fs::metadata(&output_path)
        .map_err(|e| format!("Failed to stat output zip: {e}"))?;

    Ok(WriteDiagnosticsResult {
        bytes_written: metadata.len(),
        entries,
        output_path,
    })
}
