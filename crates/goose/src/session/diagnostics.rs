use crate::config::base::Config;
use crate::config::extensions::get_enabled_extensions;
use crate::config::paths::Paths;
use crate::prompt_template::list_templates;
use crate::providers::utils::LOGS_TO_KEEP;
use crate::session::SessionManager;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Cursor;
use std::io::Write;
use std::path::PathBuf;
use utoipa::ToSchema;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SystemInfo {
    pub app_version: String,
    pub os: String,
    pub os_version: String,
    pub architecture: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub enabled_extensions: Vec<String>,
}

impl SystemInfo {
    pub fn collect() -> Self {
        let config = Config::global();
        let provider = config.get_goose_provider().ok();
        let model = config.get_goose_model().ok();
        let enabled_extensions = get_enabled_extensions()
            .into_iter()
            .map(|ext| ext.name().to_string())
            .collect();

        Self {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            os: std::env::consts::OS.to_string(),
            os_version: sys_info::os_release().unwrap_or_else(|_| "unknown".to_string()),
            architecture: std::env::consts::ARCH.to_string(),
            provider,
            model,
            enabled_extensions,
        }
    }

    pub fn to_text(&self) -> String {
        format!(
            "App Version: {}\n\
             OS: {}\n\
             OS Version: {}\n\
             Architecture: {}\n\
             Provider: {}\n\
             Model: {}\n\
             Enabled Extensions: {}\n\
             Timestamp: {}\n",
            self.app_version,
            self.os,
            self.os_version,
            self.architecture,
            self.provider.as_deref().unwrap_or("unknown"),
            self.model.as_deref().unwrap_or("unknown"),
            self.enabled_extensions.join(", "),
            chrono::Utc::now().to_rfc3339()
        )
    }
}

pub fn get_system_info() -> SystemInfo {
    SystemInfo::collect()
}

pub fn config_path() -> PathBuf {
    Paths::config_dir().join("config.yaml")
}

pub fn latest_server_log_path() -> Option<PathBuf> {
    let server_dir = Paths::in_state_dir("logs").join("server");
    let latest_date_dir = latest_entry_by_name(&server_dir)?;
    latest_entry_by_name(&latest_date_dir)
}

pub fn latest_llm_log_path() -> Option<PathBuf> {
    let path = Paths::in_state_dir("logs").join("llm_request.0.jsonl");
    path.exists().then_some(path)
}

pub fn read_tail(path: &std::path::Path, max_lines: usize) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    Some(lines[start..].join("\n"))
}

pub fn read_capped(path: &std::path::Path, max_bytes: usize) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    if content.len() <= max_bytes {
        return Some(content);
    }
    let half = max_bytes / 2;
    let head: String = content
        .chars()
        .take_while({
            let mut n = 0;
            move |c| {
                n += c.len_utf8();
                n <= half
            }
        })
        .collect();
    let tail: String = {
        let skip = content.len().saturating_sub(half);
        let mut chars = content.chars();
        let mut skipped = 0;
        for c in chars.by_ref() {
            skipped += c.len_utf8();
            if skipped >= skip {
                break;
            }
        }
        chars.collect()
    };
    let omitted = content.len() - head.len() - tail.len();
    Some(format!(
        "{}\n\n... ({} bytes omitted) ...\n\n{}",
        head, omitted, tail,
    ))
}

fn latest_entry_by_name(dir: &std::path::Path) -> Option<PathBuf> {
    let mut entries: Vec<_> = fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    entries.last().map(|e| e.path())
}

pub async fn generate_diagnostics(
    session_manager: &SessionManager,
    session_id: &str,
) -> anyhow::Result<Vec<u8>> {
    let logs_dir = Paths::in_state_dir("logs");
    let config_dir = Paths::config_dir();
    let config_path = config_dir.join("config.yaml");
    let data_dir = Paths::data_dir();

    let system_info = SystemInfo::collect();

    let mut buffer = Vec::new();
    {
        let mut zip = ZipWriter::new(Cursor::new(&mut buffer));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let mut log_files: Vec<_> = fs::read_dir(&logs_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
            .collect();

        log_files.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));

        for entry in log_files.iter().rev().take(LOGS_TO_KEEP) {
            let path = entry.path();
            let name = path.file_name().unwrap().to_str().unwrap();
            zip.start_file(format!("logs/{}", name), options)?;
            zip.write_all(&fs::read(&path)?)?;
        }

        if let Some(server_log) = latest_server_log_path() {
            if let Ok(content) = fs::read(&server_log) {
                let name = server_log.file_name().unwrap().to_str().unwrap();
                zip.start_file(format!("logs/server/{}", name), options)?;
                zip.write_all(&content)?;
            }
        }

        let session_data = session_manager.export_session(session_id).await?;
        zip.start_file("session.json", options)?;
        zip.write_all(session_data.as_bytes())?;

        if config_path.exists() {
            zip.start_file("config.yaml", options)?;
            zip.write_all(&fs::read(&config_path)?)?;
        }

        zip.start_file("system.txt", options)?;
        zip.write_all(system_info.to_text().as_bytes())?;

        let schedule_json = data_dir.join("schedule.json");
        if schedule_json.exists() {
            zip.start_file("schedule.json", options)?;
            zip.write_all(&fs::read(&schedule_json)?)?;
        }

        let scheduled_recipes_dir = data_dir.join("scheduled_recipes");
        if scheduled_recipes_dir.exists() && scheduled_recipes_dir.is_dir() {
            for entry in fs::read_dir(&scheduled_recipes_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let name = path.file_name().unwrap().to_str().unwrap();
                    zip.start_file(format!("scheduled_recipes/{}", name), options)?;
                    zip.write_all(&fs::read(&path)?)?;
                }
            }
        }

        for template in list_templates() {
            let content = template.user_content.unwrap_or(template.default_content);
            zip.start_file(format!("prompts/{}.txt", template.name), options)?;
            zip.write_all(content.as_bytes())?;
        }

        zip.finish()?;
    }

    Ok(buffer)
}
