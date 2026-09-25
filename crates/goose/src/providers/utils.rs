use crate::config::paths::Paths;
use anyhow::{anyhow, Result};
use fs_err::File;
use goose_providers::request_log::{install_logger, RequestLogHandle, RequestLogger};
use serde_json::Value;
use std::error::Error;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::OnceLock;
use uuid::Uuid;

pub fn filter_extensions_from_system_prompt(system: &str) -> String {
    let Some(extensions_start) = system.find("# Extensions") else {
        return system.to_string();
    };

    let Some(after_extensions) = system.get(extensions_start + 1..) else {
        return system.to_string();
    };

    if let Some(next_section_pos) = after_extensions.find("\n# ") {
        let Some(before) = system.get(..extensions_start) else {
            return system.to_string();
        };
        let Some(after) = system.get(extensions_start + next_section_pos + 1..) else {
            return system.to_string();
        };
        format!("{}{}", before.trim_end(), after)
    } else {
        system
            .get(..extensions_start)
            .map(|s| s.trim_end().to_string())
            .unwrap_or_else(|| system.to_string())
    }
}

/// Extract the model name from a JSON object. Common with most providers to have this top level attribute.
pub fn get_model(data: &Value) -> String {
    if let Some(model) = data.get("model") {
        if let Some(model_str) = model.as_str() {
            model_str.to_string()
        } else {
            "Unknown".to_string()
        }
    } else {
        "Unknown".to_string()
    }
}

pub const LOGS_TO_KEEP: usize = 10;

static INIT_LOGGER: OnceLock<Result<()>> = OnceLock::new();

pub fn init_goose_request_log() -> Result<()> {
    INIT_LOGGER
        .get_or_init(|| Ok(install_logger(RequestLog::new(LOGS_TO_KEEP)?)?))
        .as_ref()
        .map_err(|e| anyhow::anyhow!("failed to set up logger: {}", e))?;
    Ok(())
}

pub struct RequestLog {
    logs_to_keep: usize,
}

impl RequestLog {
    pub fn new(logs_to_keep: usize) -> Result<Self> {
        let logs_dir = Paths::in_state_dir("logs");
        fs_err::create_dir_all(&logs_dir)?;
        Ok(Self { logs_to_keep })
    }
}

struct FileLogHandle {
    writer: Option<BufWriter<File>>,
    temp_path: PathBuf,
    logs_to_keep: usize,
}

impl RequestLogger for RequestLog {
    fn start(&self) -> Result<Box<dyn RequestLogHandle>, Box<dyn Error + Send + Sync>> {
        let logs_dir = Paths::in_state_dir("logs");
        fs_err::create_dir_all(&logs_dir)?;

        let request_id = Uuid::new_v4();
        let temp_name = format!("llm_request.{request_id}.jsonl");
        let temp_path = logs_dir.join(PathBuf::from(temp_name));

        let writer = BufWriter::new(
            File::options()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&temp_path)?,
        );

        Ok(Box::new(FileLogHandle {
            writer: Some(writer),
            temp_path,
            logs_to_keep: self.logs_to_keep,
        }))
    }
}

impl RequestLogHandle for FileLogHandle {
    fn write(&mut self, s: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| anyhow!("logger is finished"))?;
        writeln!(writer, "{}", s)?;
        Ok(())
    }
}

impl FileLogHandle {
    fn finish(&mut self) -> Result<()> {
        if let Some(mut writer) = self.writer.take() {
            writer.flush()?;
            let logs_dir = Paths::in_state_dir("logs");
            let log_path = |i| logs_dir.join(format!("llm_request.{}.jsonl", i));

            if self.logs_to_keep == 0 {
                fs_err::remove_file(&self.temp_path)?;
                return Ok(());
            }

            for i in (0..self.logs_to_keep.saturating_sub(1)).rev() {
                let _ = fs_err::rename(log_path(i), log_path(i + 1));
            }

            fs_err::rename(&self.temp_path, log_path(0))?;
        }
        Ok(())
    }
}

impl Drop for FileLogHandle {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let _ = self.finish();
    }
}
