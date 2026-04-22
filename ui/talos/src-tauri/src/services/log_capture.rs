//! Locates `goose-cli`'s JSON log files on disk so the diagnostics bundle can
//! include them. The goose-cli binary writes its logs via `tracing_appender`
//! to a fixed path under the user's roaming data dir — it does NOT emit
//! anything to stdout/stderr, so tee-ing the child process would capture
//! nothing useful. Instead we walk goose's own log directory at bundle time.
//!
//! Layout (goose, on Windows):
//!   %APPDATA%\Block\goose\data\logs\cli\<YYYY-MM-DD>\<HHMMSS>.log
//!
//! On macOS/Linux the `Block/goose/data/logs/cli` suffix is the same, only
//! the base `dirs::data_dir()` differs.

use std::path::PathBuf;

pub const LOGS_PER_BUNDLE: usize = 5;
pub const MAX_BYTES_PER_LOG: u64 = 10 * 1024 * 1024;

/// Absolute path to `{data_dir}/Block/goose/data/logs/cli`, matching
/// `goose::config::paths::Paths::in_state_dir("logs").join("cli")`.
pub fn goose_cli_log_root() -> Option<PathBuf> {
    Some(
        dirs::data_dir()?
            .join("Block")
            .join("goose")
            .join("data")
            .join("logs")
            .join("cli"),
    )
}

/// A discovered log file, together with the subdirectory name it came from
/// (e.g. `"2026-04-22"`), ready to be namespaced inside a ZIP.
pub struct DiscoveredLog {
    pub path: PathBuf,
    pub date_dir: String,
    pub file_name: String,
}

/// Find up to [`LOGS_PER_BUNDLE`] most recent `*.log` files under
/// `{cli_root}/<date>/`. Walks date subdirectories newest-first so recent
/// sessions are preferred. Never errors on missing dirs — returns empty.
pub fn find_recent_logs() -> Vec<DiscoveredLog> {
    let Some(root) = goose_cli_log_root() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };

    // Collect date subdirectories and sort newest-first by name (they're ISO
    // dates so lexicographic sort == chronological).
    let mut date_dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    date_dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

    let mut found: Vec<DiscoveredLog> = Vec::new();
    for dir in date_dirs {
        if found.len() >= LOGS_PER_BUNDLE {
            break;
        }
        let date_name = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let Ok(files) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut logs: Vec<PathBuf> = files
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("log"))
            .collect();
        // Newest log filename (timestamp-prefixed) first.
        logs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

        for log in logs {
            if found.len() >= LOGS_PER_BUNDLE {
                break;
            }
            let file_name = log
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("log")
                .to_string();
            found.push(DiscoveredLog {
                path: log,
                date_dir: date_name.clone(),
                file_name,
            });
        }
    }
    found
}

/// Read the trailing `max_bytes` of a file. Used to cap oversized logs before
/// stuffing them into the ZIP.
pub fn read_tail_bytes(path: &std::path::Path, max_bytes: u64) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(max_bytes);
    if start > 0 {
        file.seek(SeekFrom::Start(start))?;
    }
    let mut buf = Vec::with_capacity((len - start) as usize);
    file.read_to_end(&mut buf)?;
    Ok(buf)
}
