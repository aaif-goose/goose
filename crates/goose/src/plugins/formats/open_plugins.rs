//! Open Plugins format adapter (<https://open-plugins.com>).
//!
//! Plugins are recognized as Open Plugins if they contain either:
//! - `plugin.json` (the spec's optional manifest), or
//! - `hooks/hooks.json`
//!
//! A bare `skills/` directory alone is not enough — it falls through to the
//! Gemini adapter, which requires its own manifest. We probe Open Plugins
//! first so a plugin that ships both a `plugin.json` and a
//! `gemini-extension.json` is treated as Open Plugins.

use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use fs_err as fs;
use serde::Deserialize;

use crate::plugins::{
    copy_dir_all, write_install_metadata, FormatNotSupported, ImportedSkill, PluginFormat,
    PluginInstall, PluginInstallOptions,
};

const MANIFEST: &str = "plugin.json";
const HOOKS_FILE: &str = "hooks/hooks.json";
const SKILLS_DIR: &str = "skills";

/// Optional `plugin.json` shape. We only need name + version; everything
/// else is forwarded as-is when the plugin is loaded at runtime.
#[derive(Debug, Default, Deserialize)]
struct OpenPluginsManifest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
}

pub(in crate::plugins) fn try_install_from_manifest_at_root(
    source: &str,
    checkout_dir: &Path,
    install_root: &Path,
    options: &PluginInstallOptions,
    last_update_check: Option<DateTime<Utc>>,
) -> Result<PluginInstall> {
    let manifest_path = checkout_dir.join(MANIFEST);
    let hooks_path = checkout_dir.join(HOOKS_FILE);
    let skills_path = checkout_dir.join(SKILLS_DIR);

    let has_manifest = manifest_path.is_file();
    let has_hooks = hooks_path.is_file();
    let has_skills = skills_path.is_dir();

    if !has_manifest && !has_hooks {
        return Err(FormatNotSupported.into());
    }

    let manifest: OpenPluginsManifest = if has_manifest {
        serde_json::from_str(&fs::read_to_string(&manifest_path)?)
            .with_context(|| format!("Failed to parse {}", manifest_path.display()))?
    } else {
        OpenPluginsManifest::default()
    };

    let name = manifest
        .name
        .unwrap_or_else(|| infer_name_from_source(source));
    validate_plugin_name(&name)?;

    let version = manifest.version.unwrap_or_else(|| "0.0.0".to_string());

    fs::create_dir_all(install_root)?;
    let destination = install_root.join(&name);
    if destination.exists() {
        bail!(
            "Plugin '{}' is already installed at {}",
            name,
            destination.display()
        );
    }

    copy_dir_all(checkout_dir, &destination)?;
    write_install_metadata(
        &destination,
        source,
        "open-plugins",
        options.auto_update,
        last_update_check,
    )?;

    let skills = if has_skills {
        find_skills(&destination)
    } else {
        Vec::new()
    };

    Ok(PluginInstall {
        name,
        version,
        format: PluginFormat::OpenPlugins,
        source: source.to_string(),
        directory: destination,
        skills,
    })
}

fn infer_name_from_source(source: &str) -> String {
    let trimmed = source.trim_end_matches('/').trim_end_matches(".git");
    trimmed
        .rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("plugin")
        .to_string()
}

fn validate_plugin_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Plugin name must not be empty");
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        bail!(
            "Invalid plugin name '{}'. Names may only contain letters, numbers, dashes, and underscores",
            name
        );
    }
    Ok(())
}

fn find_skills(plugin_dir: &Path) -> Vec<ImportedSkill> {
    let skills_dir = plugin_dir.join(SKILLS_DIR);
    let entries = match fs::read_dir(&skills_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut skills: Vec<ImportedSkill> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join("SKILL.md").is_file() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_string();
        skills.push(ImportedSkill {
            name,
            directory: path,
        });
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}
