pub mod formats;

use crate::config::paths::Paths;
use crate::subprocess::SubprocessExt;
use anyhow::{anyhow, bail, Result};
use fs_err as fs;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

const INSTALL_METADATA: &str = ".goose-plugin-install.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginFormat {
    Gemini,
}

impl std::fmt::Display for PluginFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginFormat::Gemini => write!(f, "gemini"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PluginInstall {
    pub name: String,
    pub version: String,
    pub format: PluginFormat,
    pub source: String,
    pub directory: PathBuf,
    pub skills: Vec<ImportedSkill>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedSkill {
    pub name: String,
    pub directory: PathBuf,
}

#[derive(Debug, thiserror::Error)]
#[error("format not supported")]
pub struct FormatNotSupported;

#[derive(Debug, Serialize)]
struct InstallMetadata<'a> {
    source: &'a str,
    source_type: &'a str,
    format: &'a str,
}

pub fn plugin_install_dir() -> PathBuf {
    Paths::data_dir().join("plugins")
}

pub fn installed_plugin_skill_dirs() -> Vec<PathBuf> {
    let plugins_dir = plugin_install_dir();
    let entries = match fs::read_dir(plugins_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    entries
        .flatten()
        .map(|entry| entry.path().join("skills"))
        .filter(|path| path.is_dir())
        .collect()
}

pub fn install_plugin(source: &str) -> Result<PluginInstall> {
    if source.trim().is_empty() {
        bail!("Plugin source URL must not be empty");
    }

    let temp_dir = tempfile::tempdir()?;
    let checkout_dir = temp_dir.path().join("checkout");
    clone_git_repo(source, &checkout_dir)?;

    install_from_checkout(source, &checkout_dir)
}

fn install_from_checkout(source: &str, checkout_dir: &Path) -> Result<PluginInstall> {
    match formats::gemini::try_install_from_manifest(source, checkout_dir) {
        Ok(install) => Ok(install),
        Err(err) if err.is::<FormatNotSupported>() => {
            bail!("No supported plugin format found")
        }
        Err(err) => Err(err),
    }
}

fn clone_git_repo(source: &str, destination: &Path) -> Result<()> {
    let output = Command::new("git")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(source)
        .arg(destination)
        .set_no_window()
        .output()
        .map_err(|e| anyhow!("Failed to run git clone: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if stderr.is_empty() { stdout } else { stderr };
        bail!("Failed to clone plugin repository: {message}");
    }

    Ok(())
}

fn write_install_metadata(destination: &Path, source: &str, format: &str) -> Result<()> {
    let metadata = InstallMetadata {
        source,
        source_type: "git",
        format,
    };
    fs::write(
        destination.join(INSTALL_METADATA),
        serde_json::to_string_pretty(&metadata)?,
    )?;
    Ok(())
}

fn copy_dir_all(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_dir_all(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path)?;
        } else if file_type.is_symlink() {
            copy_symlink(&source_path, &destination_path)?;
        }
    }

    Ok(())
}

#[cfg(unix)]
fn copy_symlink(source: &Path, destination: &Path) -> Result<()> {
    std::os::unix::fs::symlink(fs::read_link(source)?, destination)?;
    Ok(())
}

#[cfg(windows)]
fn copy_symlink(source: &Path, destination: &Path) -> Result<()> {
    let target = fs::read_link(source)?;
    if source.is_dir() {
        std::os::windows::fs::symlink_dir(target, destination)?;
    } else {
        std::os::windows::fs::symlink_file(target, destination)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn rejects_repo_without_supported_manifest() {
        let _guard = ENV_LOCK.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        std::env::set_var("GOOSE_PATH_ROOT", root.path());
        let repo = tempfile::tempdir().unwrap();

        let err =
            install_from_checkout("https://example.invalid/repo.git", repo.path()).unwrap_err();

        assert!(err.to_string().contains("No supported plugin format found"));
        std::env::remove_var("GOOSE_PATH_ROOT");
    }
}
