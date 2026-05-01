use crate::config::paths::Paths;
use crate::subprocess::SubprocessExt;
use anyhow::{anyhow, bail, Context, Result};
use fs_err as fs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

const GEMINI_MANIFEST: &str = "gemini-extension.json";
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

#[derive(Debug, Deserialize)]
struct GeminiManifest {
    name: String,
    version: String,
}

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
    let manifest_path = checkout_dir.join(GEMINI_MANIFEST);
    if !manifest_path.is_file() {
        bail!(
            "No supported plugin format found. Expected {GEMINI_MANIFEST} at the repository root"
        );
    }

    let manifest: GeminiManifest = serde_json::from_str(&fs::read_to_string(&manifest_path)?)
        .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;

    validate_gemini_extension_name(&manifest.name)?;

    let install_root = plugin_install_dir();
    fs::create_dir_all(&install_root)?;
    let destination = install_root.join(&manifest.name);
    if destination.exists() {
        bail!(
            "Plugin '{}' is already installed at {}",
            manifest.name,
            destination.display()
        );
    }

    let skills = find_gemini_skills(checkout_dir)?;
    if skills.is_empty() {
        bail!(
            "Plugin '{}' does not contain any Gemini skills",
            manifest.name
        );
    }

    copy_dir_all(checkout_dir, &destination)?;

    let metadata = InstallMetadata {
        source,
        source_type: "git",
        format: "gemini",
    };
    fs::write(
        destination.join(INSTALL_METADATA),
        serde_json::to_string_pretty(&metadata)?,
    )?;

    Ok(PluginInstall {
        name: manifest.name,
        version: manifest.version,
        format: PluginFormat::Gemini,
        source: source.to_string(),
        directory: destination.clone(),
        skills: skills
            .into_iter()
            .map(|skill| ImportedSkill {
                name: skill.name,
                directory: destination.join(skill.relative_directory),
            })
            .collect(),
    })
}

fn validate_gemini_extension_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Gemini extension name must not be empty");
    }

    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        bail!(
            "Invalid Gemini extension name '{}'. Names may only contain letters, numbers, and dashes",
            name
        );
    }

    Ok(())
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

struct SkillCandidate {
    name: String,
    relative_directory: PathBuf,
}

fn find_gemini_skills(extension_dir: &Path) -> Result<Vec<SkillCandidate>> {
    let skills_dir = extension_dir.join("skills");
    if !skills_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();
    collect_skill_candidate(extension_dir, &skills_dir, &mut skills)?;

    for entry in fs::read_dir(&skills_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_skill_candidate(extension_dir, &path, &mut skills)?;
        }
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

fn collect_skill_candidate(
    extension_dir: &Path,
    skill_dir: &Path,
    skills: &mut Vec<SkillCandidate>,
) -> Result<()> {
    let skill_file = skill_dir.join("SKILL.md");
    if !skill_file.is_file() {
        return Ok(());
    }

    let raw = fs::read_to_string(&skill_file)?;
    let name = extract_skill_name(&raw).unwrap_or_else(|| {
        skill_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unnamed")
            .to_string()
    });
    let relative_directory = skill_dir.strip_prefix(extension_dir)?.to_path_buf();

    skills.push(SkillCandidate {
        name,
        relative_directory,
    });

    Ok(())
}

fn extract_skill_name(raw: &str) -> Option<String> {
    let (metadata, _): (crate::skills::SkillFrontmatter, String) =
        crate::sources::parse_frontmatter(raw).ok()??;
    metadata.name.filter(|name| !name.is_empty())
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
    fn installs_gemini_extension_skills() {
        let _guard = ENV_LOCK.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        std::env::set_var("GOOSE_PATH_ROOT", root.path());

        let repo = tempfile::tempdir().unwrap();
        fs::write(
            repo.path().join(GEMINI_MANIFEST),
            r#"{"name":"test-plugin","version":"1.0.0"}"#,
        )
        .unwrap();
        let skill_dir = repo.path().join("skills").join("audit");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: audit\ndescription: Audit code\n---\nDo an audit.",
        )
        .unwrap();

        let installed =
            install_from_checkout("https://example.invalid/repo.git", repo.path()).unwrap();

        assert_eq!(installed.name, "test-plugin");
        assert_eq!(installed.version, "1.0.0");
        assert_eq!(installed.skills.len(), 1);
        assert_eq!(installed.skills[0].name, "audit");
        assert!(installed.directory.join(GEMINI_MANIFEST).is_file());
        assert!(installed.directory.join(INSTALL_METADATA).is_file());
        assert_eq!(
            installed_plugin_skill_dirs(),
            vec![installed.directory.join("skills")]
        );

        std::env::remove_var("GOOSE_PATH_ROOT");
    }

    #[test]
    fn rejects_repo_without_gemini_manifest() {
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
