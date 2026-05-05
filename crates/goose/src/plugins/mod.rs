pub mod formats;

use crate::config::paths::Paths;
use crate::subprocess::SubprocessExt;
use anyhow::{anyhow, bail, Result};
use fs_err as fs;
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Deserialize, Serialize)]
struct InstallMetadata {
    source: String,
    source_type: String,
    format: String,
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

pub fn update_plugin(name: &str) -> Result<PluginInstall> {
    if name.trim().is_empty() {
        bail!("Plugin name must not be empty");
    }

    let install_root = plugin_install_dir();
    let current_install_dir = install_root.join(name);
    if !current_install_dir.is_dir() {
        bail!("Plugin '{}' is not installed", name);
    }

    let metadata = read_install_metadata(&current_install_dir)?;
    if metadata.source_type != "git" {
        bail!(
            "Plugin '{}' was installed from '{}' and cannot be updated with this command",
            name,
            metadata.source_type
        );
    }

    fs::create_dir_all(&install_root)?;
    let temp_dir = tempfile::tempdir_in(&install_root)?;
    let checkout_dir = temp_dir.path().join("checkout");
    clone_git_repo(&metadata.source, &checkout_dir)?;

    let updated = install_from_checkout_at_root(&metadata.source, &checkout_dir, temp_dir.path())?;
    if updated.name != name {
        bail!(
            "Updated plugin name '{}' does not match installed plugin '{}'",
            updated.name,
            name
        );
    }

    replace_plugin_dir(&updated.directory, &current_install_dir)?;

    Ok(PluginInstall {
        directory: current_install_dir,
        ..updated
    })
}

fn install_from_checkout(source: &str, checkout_dir: &Path) -> Result<PluginInstall> {
    match formats::gemini::try_install_from_manifest(source, checkout_dir) {
        Ok(install) => Ok(install),
        Err(err) if err.is::<FormatNotSupported>() => bail!("No supported plugin format found"),
        Err(err) => Err(err),
    }
}

fn install_from_checkout_at_root(
    source: &str,
    checkout_dir: &Path,
    install_root: &Path,
) -> Result<PluginInstall> {
    match formats::gemini::try_install_from_manifest_at_root(source, checkout_dir, install_root) {
        Ok(install) => Ok(install),
        Err(err) if err.is::<FormatNotSupported>() => bail!("No supported plugin format found"),
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

fn read_install_metadata(directory: &Path) -> Result<InstallMetadata> {
    let metadata_path = directory.join(INSTALL_METADATA);
    if !metadata_path.is_file() {
        bail!(
            "Plugin at {} does not contain install metadata and cannot be updated",
            directory.display()
        );
    }

    Ok(serde_json::from_str(&fs::read_to_string(metadata_path)?)?)
}

fn write_install_metadata(destination: &Path, source: &str, format: &str) -> Result<()> {
    let metadata = InstallMetadata {
        source: source.to_string(),
        source_type: "git".to_string(),
        format: format.to_string(),
    };
    fs::write(
        destination.join(INSTALL_METADATA),
        serde_json::to_string_pretty(&metadata)?,
    )?;
    Ok(())
}

fn replace_plugin_dir(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow!("Plugin destination has no parent directory"))?;
    let backup_dir = tempfile::tempdir_in(parent)?;
    let backup_plugin_dir = backup_dir.path().join("plugin");

    fs::rename(destination, &backup_plugin_dir)?;
    if let Err(err) = fs::rename(source, destination) {
        fs::rename(&backup_plugin_dir, destination)?;
        return Err(err.into());
    }

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

    #[test]
    fn updates_git_backed_plugin() {
        let _guard = ENV_LOCK.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        std::env::set_var("GOOSE_PATH_ROOT", root.path());
        let repo = tempfile::tempdir().unwrap();
        write_gemini_plugin(repo.path(), "1.0.0", "Audit code");
        init_git_repo(repo.path());
        commit_git_repo(repo.path(), "initial");
        let source = repo.path().to_path_buf();

        let installed = install_plugin(source.to_str().unwrap()).unwrap();
        assert_eq!(installed.version, "1.0.0");
        assert!(installed.directory.join(".git").is_dir());

        fs::remove_dir_all(installed.directory.join(".git")).unwrap();
        write_gemini_plugin(&source, "2.0.0", "Audit updated code");
        commit_git_repo(&source, "update");

        let updated = update_plugin("test-plugin").unwrap();

        assert_eq!(updated.version, "2.0.0");
        assert_eq!(updated.directory, plugin_install_dir().join("test-plugin"));
        assert_eq!(
            fs::read_to_string(updated.directory.join("skills/audit/SKILL.md")).unwrap(),
            "---\nname: audit\ndescription: Audit updated code\n---\nDo an audit."
        );
        assert!(updated.directory.join(".git").is_dir());
        std::env::remove_var("GOOSE_PATH_ROOT");
    }

    #[test]
    fn rejects_update_when_metadata_is_missing() {
        let _guard = ENV_LOCK.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        std::env::set_var("GOOSE_PATH_ROOT", root.path());
        let plugin_dir = plugin_install_dir().join("test-plugin");
        fs::create_dir_all(&plugin_dir).unwrap();

        let err = update_plugin("test-plugin").unwrap_err();

        assert!(err
            .to_string()
            .contains("does not contain install metadata"));
        std::env::remove_var("GOOSE_PATH_ROOT");
    }

    #[test]
    fn rejects_update_when_source_type_is_not_git() {
        let _guard = ENV_LOCK.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        std::env::set_var("GOOSE_PATH_ROOT", root.path());
        let plugin_dir = plugin_install_dir().join("test-plugin");
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(
            plugin_dir.join(INSTALL_METADATA),
            r#"{"source":"/tmp/test-plugin","source_type":"local","format":"gemini"}"#,
        )
        .unwrap();

        let err = update_plugin("test-plugin").unwrap_err();

        assert!(err
            .to_string()
            .contains("cannot be updated with this command"));
        std::env::remove_var("GOOSE_PATH_ROOT");
    }

    fn write_gemini_plugin(repo: &Path, version: &str, description: &str) {
        fs::write(
            repo.join(formats::gemini::MANIFEST),
            format!(r#"{{"name":"test-plugin","version":"{version}"}}"#),
        )
        .unwrap();
        let skill_dir = repo.join("skills").join("audit");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: audit\ndescription: {description}\n---\nDo an audit."),
        )
        .unwrap();
    }

    fn init_git_repo(repo: &Path) {
        run_git(repo, &["init"]);
        run_git(repo, &["config", "user.email", "goose@example.com"]);
        run_git(repo, &["config", "user.name", "Goose"]);
    }

    fn commit_git_repo(repo: &Path, message: &str) {
        run_git(repo, &["add", "."]);
        run_git(repo, &["commit", "-m", message]);
    }

    fn run_git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo)
            .set_no_window()
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
