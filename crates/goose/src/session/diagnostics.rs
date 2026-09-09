use crate::config::base::Config;
use crate::config::extensions::get_enabled_extensions;
use crate::config::paths::Paths;
use crate::prompt_template::list_templates;
use crate::providers::utils::LOGS_TO_KEEP;
use crate::session::SessionManager;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const LLM_LOG_MAX_BYTES: usize = 2 * 1024 * 1024;
const CONFIG_MAX_BYTES: usize = 256 * 1024;
const CLI_LOG_TAIL_LINES: usize = 400;
const CLI_LOGS_TO_INCLUDE: usize = 3;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticsLevel {
    #[default]
    Summary,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SystemInfo {
    pub app_version: String,
    pub os: String,
    pub os_version: String,
    pub architecture: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub enabled_extensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsConfig {
    pub config_path: String,
    pub config_yaml: Option<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsExtensions {
    pub enabled: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsTextFile {
    pub path: String,
    pub content: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsLogs {
    pub cli: Vec<DiagnosticsTextFile>,
    pub llm: Vec<DiagnosticsTextFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsPrompt {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsScheduledRecipe {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsError {
    pub path: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsReport {
    pub schema_version: u32,
    pub generated_at: String,
    pub level: DiagnosticsLevel,
    pub system: SystemInfo,
    pub config: Option<DiagnosticsConfig>,
    pub extensions: DiagnosticsExtensions,
    pub session: Option<serde_json::Value>,
    pub logs: DiagnosticsLogs,
    pub prompts: Vec<DiagnosticsPrompt>,
    pub schedule: Option<serde_json::Value>,
    pub scheduled_recipes: Vec<DiagnosticsScheduledRecipe>,
    pub errors: Vec<DiagnosticsError>,
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

pub fn latest_llm_log_path() -> Option<PathBuf> {
    let path = Paths::in_state_dir("logs").join("llm_request.0.jsonl");
    path.exists().then_some(path)
}

pub fn recent_cli_log_paths() -> Vec<PathBuf> {
    let component_dir = Paths::in_state_dir("logs").join("cli");
    let mut paths = Vec::new();

    if let Ok(entries) = fs::read_dir(component_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(files) = fs::read_dir(path) {
                    paths.extend(
                        files
                            .flatten()
                            .map(|entry| entry.path())
                            .filter(|path| path.is_file()),
                    );
                }
            } else if path.is_file() {
                paths.push(path);
            }
        }
    }

    paths.sort_by(|left, right| {
        log_modified(right)
            .cmp(&log_modified(left))
            .then_with(|| log_name(left).cmp(&log_name(right)))
    });
    paths.truncate(CLI_LOGS_TO_INCLUDE);
    paths
}

fn recent_llm_log_paths() -> Vec<PathBuf> {
    let logs_dir = Paths::in_state_dir("logs");
    let paths: Vec<_> = fs::read_dir(logs_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("llm_request.") && name.ends_with(".jsonl"))
        })
        .collect();

    let (mut numbered, mut temp): (Vec<_>, Vec<_>) = paths
        .into_iter()
        .partition(|path| llm_log_index(path).is_some());

    numbered.sort_by_key(|path| llm_log_index(path).unwrap_or(usize::MAX));
    temp.sort_by(|left, right| {
        log_modified(right)
            .cmp(&log_modified(left))
            .then_with(|| log_name(left).cmp(&log_name(right)))
    });

    if temp.is_empty() || numbered.len() < LOGS_TO_KEEP {
        numbered.extend(temp);
        numbered.truncate(LOGS_TO_KEEP);
        numbered
    } else {
        let temp_slots = 1;
        let numbered_slots = LOGS_TO_KEEP.saturating_sub(temp_slots);
        temp.truncate(temp_slots);
        numbered.truncate(numbered_slots);
        temp.extend(numbered);
        temp
    }
}

fn llm_log_index(path: &std::path::Path) -> Option<usize> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    name.strip_prefix("llm_request.")
        .and_then(|name| name.strip_suffix(".jsonl"))
        .and_then(|name| name.parse::<usize>().ok())
}

fn log_modified(path: &std::path::Path) -> std::time::SystemTime {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
}

fn log_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
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

fn was_truncated(content: &str) -> bool {
    content.contains("... (") && content.contains(" bytes omitted) ...")
}

pub async fn generate_diagnostics(
    session_manager: &SessionManager,
    session_id: &str,
    level: DiagnosticsLevel,
) -> anyhow::Result<DiagnosticsReport> {
    let config_path = config_path();
    let data_dir = Paths::data_dir();
    let system_info = SystemInfo::collect();
    let is_full = matches!(level, DiagnosticsLevel::Full);
    let mut errors = Vec::new();

    let session = if is_full {
        let session_data = session_manager.export_session(session_id).await?;
        Some(serde_json::from_str(&session_data)?)
    } else {
        None
    };

    let config = if is_full {
        let config_yaml = if config_path.exists() {
            read_capped(&config_path, CONFIG_MAX_BYTES)
        } else {
            None
        };
        let truncated = config_yaml.as_deref().is_some_and(was_truncated);
        Some(DiagnosticsConfig {
            config_path: config_path.display().to_string(),
            config_yaml,
            truncated,
        })
    } else {
        None
    };

    let logs = if is_full {
        DiagnosticsLogs {
            cli: recent_cli_log_paths()
                .into_iter()
                .filter_map(|path| {
                    read_tail(&path, CLI_LOG_TAIL_LINES).map(|content| DiagnosticsTextFile {
                        path: path.display().to_string(),
                        content,
                        truncated: true,
                    })
                })
                .collect(),
            llm: recent_llm_log_paths()
                .into_iter()
                .filter_map(|path| {
                    read_capped(&path, LLM_LOG_MAX_BYTES).map(|content| {
                        let truncated = was_truncated(&content);
                        DiagnosticsTextFile {
                            path: path.display().to_string(),
                            content,
                            truncated,
                        }
                    })
                })
                .collect(),
        }
    } else {
        DiagnosticsLogs::default()
    };

    let prompts = if is_full {
        list_templates()
            .into_iter()
            .map(|template| DiagnosticsPrompt {
                name: template.name,
                content: template.user_content.unwrap_or(template.default_content),
            })
            .collect()
    } else {
        Vec::new()
    };

    let schedule = if is_full {
        let schedule_json = data_dir.join("schedule.json");
        if schedule_json.exists() {
            fs::read_to_string(&schedule_json).ok().and_then(|content| {
                match serde_json::from_str(&content) {
                    Ok(value) => Some(value),
                    Err(err) => {
                        errors.push(DiagnosticsError {
                            path: Some(schedule_json.display().to_string()),
                            message: err.to_string(),
                        });
                        None
                    }
                }
            })
        } else {
            None
        }
    } else {
        None
    };

    let mut scheduled_recipes = Vec::new();
    if is_full {
        let scheduled_recipes_dir = data_dir.join("scheduled_recipes");
        if scheduled_recipes_dir.exists() && scheduled_recipes_dir.is_dir() {
            for entry in fs::read_dir(&scheduled_recipes_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    match fs::read_to_string(&path) {
                        Ok(content) => scheduled_recipes.push(DiagnosticsScheduledRecipe {
                            path: path.display().to_string(),
                            content,
                        }),
                        Err(err) => errors.push(DiagnosticsError {
                            path: Some(path.display().to_string()),
                            message: err.to_string(),
                        }),
                    }
                }
            }
        }
    }

    Ok(DiagnosticsReport {
        schema_version: 1,
        generated_at: chrono::Utc::now().to_rfc3339(),
        level,
        system: system_info.clone(),
        config,
        extensions: DiagnosticsExtensions {
            enabled: system_info.enabled_extensions,
        },
        session,
        logs,
        prompts,
        schedule,
        scheduled_recipes,
        errors,
    })
}
