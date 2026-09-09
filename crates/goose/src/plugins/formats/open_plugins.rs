//! Open Plugins format adapter (<https://open-plugins.com>).

use crate::plugins::{
    copy_dir_all, write_install_metadata, FormatNotSupported, ImportedSkill, PluginFormat,
    PluginInstall, PluginInstallOptions,
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use fs_err as fs;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

const MANIFESTS: [&str; 3] = [
    ".goose-plugin/plugin.json",
    ".plugin/plugin.json",
    "plugin.json",
];
const FORMAT: &str = "open-plugins";

const COMPONENT_MARKERS: &[&str] = &["hooks/hooks.json", "commands", "agents", ".mcp.json"];

#[derive(Debug, Deserialize)]
pub struct OpenPluginsManifest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub skills: Option<serde_json::Value>,
    #[serde(default, rename = "mcpServers")]
    pub mcp_servers: Option<serde_json::Value>,
}

#[derive(Debug)]
struct SkillCandidate {
    name: String,
    relative_directory: PathBuf,
}

pub(in crate::plugins) fn try_install_from_manifest_at_root(
    source: &str,
    checkout_dir: &Path,
    install_root: &Path,
    options: &PluginInstallOptions,
    last_update_check: Option<DateTime<Utc>>,
) -> Result<PluginInstall> {
    let has_manifest = manifest_path(checkout_dir).is_some();
    let has_component = has_component_marker(checkout_dir);

    if !has_manifest && !has_component {
        return Err(FormatNotSupported.into());
    }

    if !has_manifest && checkout_dir.join(super::gemini::MANIFEST).is_file() {
        return Err(FormatNotSupported.into());
    }

    install_from_manifest(
        source,
        checkout_dir,
        install_root,
        options,
        last_update_check,
    )
}

fn install_from_manifest(
    source: &str,
    checkout_dir: &Path,
    install_root: &Path,
    options: &PluginInstallOptions,
    last_update_check: Option<DateTime<Utc>>,
) -> Result<PluginInstall> {
    let manifest = read_manifest(checkout_dir, source)?;
    let plugin_name = manifest
        .name
        .clone()
        .expect("read_manifest always sets a name");
    validate_plugin_name(&plugin_name)?;

    fs::create_dir_all(install_root)?;
    let destination = install_root.join(&plugin_name);
    if destination.exists() {
        bail!(
            "Plugin '{}' is already installed at {}",
            plugin_name,
            destination.display()
        );
    }

    let skills = find_agent_skills(checkout_dir, manifest.skills.as_ref())?;
    validate_mcp_servers(checkout_dir, manifest.mcp_servers.as_ref())?;

    let staging_dir = tempfile::tempdir_in(install_root)?;
    let staged_destination = staging_dir.path().join(&plugin_name);
    copy_dir_all(checkout_dir, &staged_destination)?;

    let mut imported_skills = Vec::new();
    for skill in skills {
        let namespaced_name = namespaced_component_name(&plugin_name, &skill.name);
        let staged_skill_dir = staged_destination.join(&skill.relative_directory);
        rewrite_skill_name(&staged_skill_dir.join("SKILL.md"), &namespaced_name)?;
        imported_skills.push(ImportedSkill {
            name: namespaced_name,
            directory: destination.join(&skill.relative_directory),
        });
    }

    write_install_metadata(
        &staged_destination,
        source,
        FORMAT,
        options.auto_update,
        last_update_check,
    )?;
    fs::rename(&staged_destination, &destination)?;

    imported_skills.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(PluginInstall {
        name: plugin_name,
        version: manifest.version.unwrap_or_else(|| "unknown".to_string()),
        format: PluginFormat::OpenPlugins,
        source: source.to_string(),
        directory: destination,
        skills: imported_skills,
    })
}

pub(in crate::plugins) fn installed_skill_dirs(plugin_dir: &Path) -> Vec<PathBuf> {
    let manifest = match read_manifest(plugin_dir, "") {
        Ok(manifest) => manifest,
        Err(_) => return Vec::new(),
    };

    let mut dirs = Vec::new();
    if !plugin_dir.join("skills").is_dir()
        && manifest.skills.is_none()
        && plugin_dir.join("SKILL.md").is_file()
    {
        dirs.push(plugin_dir.to_path_buf());
    }

    dirs.extend(
        skill_root_directories(plugin_dir, manifest.skills.as_ref())
            .unwrap_or_default()
            .into_iter()
            .filter(|path| path.is_dir()),
    );

    dedupe_paths(dirs)
}

pub(in crate::plugins) fn read_manifest(
    plugin_dir: &Path,
    source: &str,
) -> Result<OpenPluginsManifest> {
    let mut manifest = match manifest_path(plugin_dir) {
        Some(manifest_path) => {
            serde_json::from_str::<OpenPluginsManifest>(&fs::read_to_string(&manifest_path)?)
                .with_context(|| format!("Failed to parse {}", manifest_path.display()))?
        }
        None => OpenPluginsManifest {
            name: None,
            version: None,
            skills: None,
            mcp_servers: None,
        },
    };

    if manifest.name.as_deref().map(str::is_empty).unwrap_or(true) {
        manifest.name = Some(infer_name(plugin_dir, source));
    }

    Ok(manifest)
}

fn manifest_path(plugin_dir: &Path) -> Option<PathBuf> {
    MANIFESTS
        .iter()
        .map(|relative| plugin_dir.join(relative))
        .find(|path| path.is_file())
}

fn has_component_marker(plugin_dir: &Path) -> bool {
    COMPONENT_MARKERS.iter().any(|marker| {
        let path = plugin_dir.join(marker);
        path.is_file() || path.is_dir()
    })
}

fn infer_name(plugin_dir: &Path, source: &str) -> String {
    let trimmed = source.trim_end_matches('/').trim_end_matches(".git");
    let from_source = trimmed.rsplit('/').find(|s| !s.is_empty());
    if let Some(name) = from_source {
        return name.to_string();
    }
    plugin_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("plugin")
        .to_string()
}

fn validate_plugin_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 64 {
        bail!(
            "Invalid Open Plugins name '{}'. Names must be 1-64 characters",
            name
        );
    }

    if !name
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '.')
    {
        bail!(
            "Invalid Open Plugins name '{}'. Names may only contain lowercase letters, numbers, dashes, and periods",
            name
        );
    }

    if !name
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
        || !name
            .chars()
            .last()
            .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
    {
        bail!(
            "Invalid Open Plugins name '{}'. Names must start and end with a letter or number",
            name
        );
    }

    if name.contains("--") || name.contains("..") {
        bail!(
            "Invalid Open Plugins name '{}'. Names must not contain consecutive dashes or periods",
            name
        );
    }

    Ok(())
}

fn namespaced_component_name(plugin_name: &str, component_name: &str) -> String {
    format!("{plugin_name}:{component_name}")
}

fn validate_mcp_servers(
    plugin_dir: &Path,
    mcp_servers_config: Option<&serde_json::Value>,
) -> Result<()> {
    if let Some(value) = mcp_servers_config {
        crate::plugins::mcp_servers::validate_mcp_servers_manifest_value(value)?;
    }

    for path in mcp_config_paths_for_validation(plugin_dir, mcp_servers_config)? {
        if !path.is_file() {
            continue;
        }
        let value = serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&path)?)
            .with_context(|| format!("Failed to parse {}", path.display()))?;
        crate::plugins::mcp_servers::validate_mcp_server_document(&value)?;
    }

    Ok(())
}

fn mcp_config_paths_for_validation(
    plugin_dir: &Path,
    config: Option<&serde_json::Value>,
) -> Result<Vec<PathBuf>> {
    let custom_paths = config
        .filter(|value| {
            !value
                .as_object()
                .is_some_and(|object| object.contains_key("mcpServers"))
        })
        .map(parse_component_paths)
        .transpose()?
        .unwrap_or_default();

    let mut paths = Vec::new();
    if !custom_paths.exclusive {
        paths.push(plugin_dir.join(".mcp.json"));
    }
    for path in custom_paths.paths {
        paths.push(plugin_dir.join(validate_relative_plugin_path(&path)?));
    }
    Ok(dedupe_paths(paths))
}

fn find_agent_skills(
    plugin_dir: &Path,
    skills_config: Option<&serde_json::Value>,
) -> Result<Vec<SkillCandidate>> {
    let mut skills = Vec::new();

    for root in skill_root_directories(plugin_dir, skills_config)? {
        collect_skill_candidates(plugin_dir, &root, &mut skills)?;
    }

    if !plugin_dir.join("skills").is_dir()
        && skills_config.is_none()
        && plugin_dir.join("SKILL.md").is_file()
    {
        collect_skill_candidate(plugin_dir, plugin_dir, &mut skills)?;
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

fn skill_root_directories(
    plugin_dir: &Path,
    skills_config: Option<&serde_json::Value>,
) -> Result<Vec<PathBuf>> {
    let custom_paths = skills_config
        .map(parse_component_paths)
        .transpose()?
        .unwrap_or_default();

    let mut roots = Vec::new();
    if !custom_paths.exclusive {
        roots.push(plugin_dir.join("skills"));
    }

    for path in custom_paths.paths {
        roots.push(plugin_dir.join(validate_relative_plugin_path(&path)?));
    }

    Ok(dedupe_paths(roots))
}

#[derive(Default)]
pub(in crate::plugins) struct ComponentPaths {
    pub paths: Vec<String>,
    pub exclusive: bool,
}

pub(in crate::plugins) fn parse_component_paths(
    value: &serde_json::Value,
) -> Result<ComponentPaths> {
    match value {
        serde_json::Value::String(path) => Ok(ComponentPaths {
            paths: vec![path.clone()],
            exclusive: false,
        }),
        serde_json::Value::Array(paths) => Ok(ComponentPaths {
            paths: paths
                .iter()
                .map(|path| match path {
                    serde_json::Value::String(path) => Ok(path.clone()),
                    _ => bail!("Open Plugins component paths must be strings"),
                })
                .collect::<Result<Vec<_>>>()?,
            exclusive: false,
        }),
        serde_json::Value::Object(config) => {
            let paths = match config.get("paths") {
                Some(serde_json::Value::String(path)) => vec![path.clone()],
                Some(serde_json::Value::Array(paths)) => paths
                    .iter()
                    .map(|path| match path {
                        serde_json::Value::String(path) => Ok(path.clone()),
                        _ => bail!("Open Plugins component paths must be strings"),
                    })
                    .collect::<Result<Vec<_>>>()?,
                Some(_) => {
                    bail!("Open Plugins component paths must be a string or array of strings")
                }
                None => Vec::new(),
            };
            let exclusive = match config.get("exclusive") {
                Some(serde_json::Value::Bool(exclusive)) => *exclusive,
                Some(_) => bail!("Open Plugins component exclusive setting must be a boolean"),
                None => false,
            };

            Ok(ComponentPaths { paths, exclusive })
        }
        serde_json::Value::Null => Ok(ComponentPaths::default()),
        _ => bail!("Open Plugins component paths must be a string, array, or object"),
    }
}

pub(in crate::plugins) fn validate_relative_plugin_path(path: &str) -> Result<PathBuf> {
    if !path.starts_with("./") {
        bail!(
            "Open Plugins component paths must start with './': {}",
            path
        );
    }

    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        bail!(
            "Open Plugins component paths must stay within the plugin: {}",
            path.display()
        );
    }

    Ok(path.to_path_buf())
}

fn collect_skill_candidates(
    plugin_dir: &Path,
    skill_root: &Path,
    skills: &mut Vec<SkillCandidate>,
) -> Result<()> {
    if !skill_root.is_dir() {
        return Ok(());
    }

    collect_skill_candidate(plugin_dir, skill_root, skills)?;

    for entry in fs::read_dir(skill_root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_skill_candidate(plugin_dir, &path, skills)?;
        }
    }

    Ok(())
}

fn collect_skill_candidate(
    plugin_dir: &Path,
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
    let relative_directory = skill_dir.strip_prefix(plugin_dir)?.to_path_buf();

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

fn rewrite_skill_name(skill_file: &Path, name: &str) -> Result<()> {
    let raw = fs::read_to_string(skill_file)?;
    let rewritten = rewrite_skill_content_name(&raw, name)?;
    fs::write(skill_file, rewritten)?;
    Ok(())
}

fn rewrite_skill_content_name(raw: &str, name: &str) -> Result<String> {
    if !raw.trim_start().starts_with("---") {
        return Ok(build_skill_md(name, raw));
    }

    let mut parts = raw.splitn(3, "---");
    let prefix = parts.next().unwrap_or_default();
    let yaml_content = parts.next().unwrap_or_default();
    let body = parts.next().unwrap_or_default();

    if !prefix.trim().is_empty() || body.is_empty() {
        return Ok(build_skill_md(name, raw));
    }

    let mut metadata = serde_yaml::from_str::<serde_yaml::Mapping>(yaml_content.trim())
        .unwrap_or_else(|_| serde_yaml::Mapping::new());
    metadata.insert(
        serde_yaml::Value::String("name".to_string()),
        serde_yaml::Value::String(name.to_string()),
    );

    Ok(format!(
        "---\n{}---{}",
        serde_yaml::to_string(&metadata)?,
        body
    ))
}

fn build_skill_md(name: &str, body: &str) -> String {
    let mut metadata = serde_yaml::Mapping::new();
    metadata.insert(
        serde_yaml::Value::String("name".to_string()),
        serde_yaml::Value::String(name.to_string()),
    );

    format!(
        "---\n{}---\n{}\n",
        serde_yaml::to_string(&metadata).unwrap(),
        body
    )
}

pub(in crate::plugins) fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_open_plugins_skills() {
        let install_root = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir_all(repo.path().join(".plugin")).unwrap();
        fs::write(
            repo.path().join(".plugin/plugin.json"),
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
        fs::write(
            repo.path().join(crate::plugins::INSTALL_METADATA),
            r#"{"source":"attacker-source","source_type":"git","format":"open-plugins","auto_update":true}"#,
        )
        .unwrap();

        let installed = install_from_manifest(
            "https://example.invalid/repo.git",
            repo.path(),
            install_root.path(),
            &PluginInstallOptions::default(),
            None,
        )
        .unwrap();

        assert_eq!(installed.name, "test-plugin");
        assert_eq!(installed.version, "1.0.0");
        assert_eq!(installed.format, PluginFormat::OpenPlugins);
        assert_eq!(installed.skills.len(), 1);
        assert_eq!(installed.skills[0].name, "test-plugin:audit");
        assert_eq!(
            installed.skills[0].directory,
            installed.directory.join("skills/audit")
        );
        assert!(installed.directory.join(".plugin/plugin.json").is_file());
        assert!(installed
            .directory
            .join(crate::plugins::INSTALL_METADATA)
            .is_file());
        assert_eq!(installed.directory, install_root.path().join("test-plugin"));
        assert_eq!(
            installed_skill_dirs(&installed.directory),
            vec![installed.directory.join("skills")]
        );
        assert!(
            fs::read_to_string(installed.directory.join("skills/audit/SKILL.md"))
                .unwrap()
                .contains("name: test-plugin:audit")
        );
        let metadata: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(installed.directory.join(crate::plugins::INSTALL_METADATA))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(metadata["source"], "https://example.invalid/repo.git");
        assert_eq!(metadata["auto_update"], false);
    }

    #[test]
    fn preserves_existing_plugin_directory() {
        let install_root = tempfile::tempdir().unwrap();
        let destination = install_root.path().join("test-plugin");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("sentinel"), "existing").unwrap();

        let repo = tempfile::tempdir().unwrap();
        fs::write(
            repo.path().join("plugin.json"),
            r#"{"name":"test-plugin","version":"2.0.0"}"#,
        )
        .unwrap();

        let err = install_from_manifest(
            "https://example.invalid/repo.git",
            repo.path(),
            install_root.path(),
            &PluginInstallOptions::default(),
            None,
        )
        .unwrap_err();

        assert!(err.to_string().contains("already installed"));
        assert_eq!(
            fs::read_to_string(destination.join("sentinel")).unwrap(),
            "existing"
        );
        assert_eq!(fs::read_dir(install_root.path()).unwrap().count(), 1);
    }

    #[test]
    fn installs_open_plugins_with_only_hooks() {
        let install_root = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        fs::write(
            repo.path().join("plugin.json"),
            r#"{"name":"hello-hooks","version":"0.1.0"}"#,
        )
        .unwrap();
        fs::create_dir_all(repo.path().join("hooks")).unwrap();
        fs::write(
            repo.path().join("hooks/hooks.json"),
            r#"{"hooks":{"SessionStart":[{"hooks":[]}]}}"#,
        )
        .unwrap();

        let installed = install_from_manifest(
            "https://example.invalid/hello-hooks.git",
            repo.path(),
            install_root.path(),
            &PluginInstallOptions::default(),
            None,
        )
        .unwrap();

        assert_eq!(installed.name, "hello-hooks");
        assert_eq!(installed.version, "0.1.0");
        assert_eq!(installed.format, PluginFormat::OpenPlugins);
        assert!(installed.skills.is_empty());
        assert!(installed.directory.join("hooks/hooks.json").is_file());
        assert!(installed.directory.join("plugin.json").is_file());
    }

    #[test]
    fn bare_skills_directory_is_not_claimed_as_open_plugin() {
        let install_root = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let skill_dir = repo.path().join("skills").join("audit");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: audit\ndescription: Audit code\n---\nDo an audit.",
        )
        .unwrap();

        let err = try_install_from_manifest_at_root(
            "https://example.invalid/repo.git",
            repo.path(),
            install_root.path(),
            &PluginInstallOptions::default(),
            None,
        )
        .unwrap_err();

        assert!(err.is::<FormatNotSupported>(), "got: {err}");
    }

    #[test]
    fn installs_manifestless_open_plugins_with_only_hooks() {
        let install_root = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir_all(repo.path().join("hooks")).unwrap();
        fs::write(
            repo.path().join("hooks/hooks.json"),
            r#"{"hooks":{"SessionStart":[{"hooks":[]}]}}"#,
        )
        .unwrap();

        let installed = try_install_from_manifest_at_root(
            "https://example.invalid/hello-hooks.git",
            repo.path(),
            install_root.path(),
            &PluginInstallOptions::default(),
            None,
        )
        .unwrap();

        assert_eq!(installed.name, "hello-hooks");
        assert!(installed.skills.is_empty());
        assert!(installed.directory.join("hooks/hooks.json").is_file());
    }

    #[test]
    fn rejects_repo_with_no_manifest_or_components() {
        let install_root = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        fs::write(repo.path().join("README.md"), "Hi").unwrap();

        let err = try_install_from_manifest_at_root(
            "https://example.invalid/repo.git",
            repo.path(),
            install_root.path(),
            &PluginInstallOptions::default(),
            None,
        )
        .unwrap_err();

        assert!(err.is::<FormatNotSupported>(), "got: {err}");
    }

    #[test]
    fn installs_custom_skill_paths() {
        let install_root = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir_all(repo.path().join(".plugin")).unwrap();
        fs::write(
            repo.path().join(".plugin/plugin.json"),
            r#"{"name":"test-plugin","skills":{"paths":["./custom-skills"],"exclusive":true}}"#,
        )
        .unwrap();
        let skill_dir = repo.path().join("custom-skills").join("audit");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: audit\ndescription: Audit code\n---\nDo an audit.",
        )
        .unwrap();

        let installed = install_from_manifest(
            "https://example.invalid/repo.git",
            repo.path(),
            install_root.path(),
            &PluginInstallOptions::default(),
            None,
        )
        .unwrap();

        assert_eq!(installed.skills.len(), 1);
        assert_eq!(installed.skills[0].name, "test-plugin:audit");
        assert_eq!(
            installed_skill_dirs(&installed.directory),
            vec![installed.directory.join("custom-skills")]
        );
    }

    #[test]
    fn rejects_manifest_paths_that_escape_plugin() {
        let err = find_agent_skills(
            tempfile::tempdir().unwrap().path(),
            Some(&serde_json::json!("./../outside")),
        )
        .unwrap_err();

        assert!(err.to_string().contains("must stay within the plugin"));
    }

    #[test]
    fn defers_to_gemini_when_gemini_manifest_present_without_open_plugin_manifest() {
        let install_root = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();

        fs::write(
            repo.path().join(super::super::gemini::MANIFEST),
            r#"{"name":"gemini-ext","version":"1.0.0"}"#,
        )
        .unwrap();
        let commands_dir = repo.path().join("commands");
        fs::create_dir_all(&commands_dir).unwrap();
        fs::write(commands_dir.join("deploy.md"), "Deploy to staging.").unwrap();

        let err = try_install_from_manifest_at_root(
            "https://example.invalid/Gemini-Ext.git",
            repo.path(),
            install_root.path(),
            &PluginInstallOptions::default(),
            None,
        )
        .unwrap_err();

        assert!(
            err.is::<FormatNotSupported>(),
            "expected FormatNotSupported so Gemini installer can take over, got: {err}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn failed_install_leaves_no_live_plugin_state() {
        use std::os::unix::fs::symlink;

        let scratch = tempfile::tempdir().unwrap();
        let install_root = scratch.path().join("plugins");
        let repo = scratch.path().join("repo");
        fs::create_dir_all(repo.join("skills")).unwrap();
        fs::write(
            repo.join("plugin.json"),
            r#"{"name":"failed-plugin","version":"1.0.0"}"#,
        )
        .unwrap();
        fs::write(
            repo.join(crate::plugins::INSTALL_METADATA),
            r#"{"source":"ext::attacker","source_type":"git","format":"open-plugins","auto_update":true}"#,
        )
        .unwrap();
        fs::write(
            repo.join(".mcp.json"),
            r#"{"mcpServers":{"payload":{"command":"attacker-command"}}}"#,
        )
        .unwrap();

        let external_skill = scratch.path().join("external-skill");
        fs::create_dir_all(&external_skill).unwrap();
        fs::write(
            external_skill.join("SKILL.md"),
            "---\nname: audit\ndescription: Audit code\n---\nDo an audit.",
        )
        .unwrap();
        symlink(&external_skill, repo.join("skills/audit")).unwrap();

        let result = install_from_manifest(
            "https://example.invalid/failed-plugin.git",
            &repo,
            &install_root,
            &PluginInstallOptions::default(),
            None,
        );

        assert!(result.is_err());
        assert!(!install_root.join("failed-plugin").exists());
        assert_eq!(fs::read_dir(&install_root).unwrap().count(), 0);
    }
}
