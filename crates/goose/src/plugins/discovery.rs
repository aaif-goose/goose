//! Discovery of installed and user-placed plugins, honoring the Open Plugins
//! `settings.json` enabled/disabled lists.
//!
//! Two sources of plugins are supported:
//!
//! 1. **Installed**: plugins installed via [`crate::plugins::install_plugin`]
//!    live under [`crate::plugins::plugin_install_dir`] (i.e.
//!    `<data>/plugins/<name>/`).
//! 2. **User-placed**: plugins dropped into `~/.agents/plugins/<name>/` or
//!    `<project>/.agents/plugins/<name>/` per the Open Plugins spec.
//!
//! Settings files (`<config>/settings.json`) declare which plugins are
//! enabled. The default is **enabled** for every discovered plugin — to
//! disable one, list it under `disabledPlugins`. (We deliberately diverge
//! from the spec's strict "enabled list only" model so users who drop a
//! plugin into `.agents/plugins/` see it work without also editing a
//! settings file.)

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::config::paths::Paths;
use crate::plugins::plugin_install_dir;

/// A plugin found on disk and not disabled by any settings file.
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    pub name: String,
    pub root: PathBuf,
    pub source: PluginSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginSource {
    /// Installed via `goose plugins install` into the data dir cache.
    Installed,
    /// User-placed under `~/.agents/plugins/` or `.agents/plugins/`.
    UserPlaced,
}

/// Settings file format from <https://open-plugins.com/plugin-builders/installation>.
#[derive(Debug, Default, Deserialize)]
struct PluginSettings {
    #[serde(default, rename = "enabledPlugins")]
    enabled: Vec<String>,
    #[serde(default, rename = "disabledPlugins")]
    disabled: Vec<String>,
}

/// Scope of a settings file, in precedence order (highest first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Scope {
    Local,
    Project,
    User,
}

/// Discover all plugins that should be considered active.
///
/// `project_root`, when supplied, enables project + local scope settings and
/// project-scope `.agents/plugins/` lookups.
pub fn discover_enabled_plugins(project_root: Option<&Path>) -> Vec<DiscoveredPlugin> {
    let settings = load_all_settings(project_root);

    let mut found: HashMap<String, DiscoveredPlugin> = HashMap::new();

    // Installed plugins (from `goose plugins install`).
    for (name, root) in list_installed_plugins() {
        found.entry(name.clone()).or_insert(DiscoveredPlugin {
            name,
            root,
            source: PluginSource::Installed,
        });
    }

    // User-placed plugins. Project scope wins over user scope.
    let mut placed_roots: Vec<PathBuf> = Vec::new();
    if let Some(root) = project_root {
        placed_roots.push(root.join(".agents").join("plugins"));
    }
    if let Some(home) = dirs::home_dir() {
        placed_roots.push(home.join(".agents").join("plugins"));
    }
    for dir in placed_roots {
        for (name, root) in list_dir_children(&dir) {
            found.entry(name.clone()).or_insert(DiscoveredPlugin {
                name,
                root,
                source: PluginSource::UserPlaced,
            });
        }
    }

    // Apply settings: a plugin disabled at any scope is dropped. (Strictly
    // the spec says higher precedence wins, but a "disabled" mark anywhere
    // is the safer default and matches what users intuitively expect.)
    let disabled: HashSet<&str> = settings
        .iter()
        .flat_map(|(_, s)| s.disabled.iter().map(String::as_str))
        .collect();
    let explicit_enabled: HashSet<&str> = settings
        .iter()
        .flat_map(|(_, s)| s.enabled.iter().map(String::as_str))
        .collect();

    found
        .into_values()
        .filter(|p| !disabled.contains(p.name.as_str()))
        // If the user has explicitly listed plugins as enabled, treat the
        // list as a filter for installed plugins (project teams pinning what
        // teammates run). User-placed plugins remain available unconditionally
        // unless explicitly disabled, so demos drop in and just work.
        .filter(|p| {
            if explicit_enabled.is_empty() {
                return true;
            }
            match p.source {
                PluginSource::Installed => explicit_enabled.contains(p.name.as_str()),
                PluginSource::UserPlaced => true,
            }
        })
        .collect()
}

fn list_installed_plugins() -> Vec<(String, PathBuf)> {
    list_dir_children(&plugin_install_dir())
}

fn list_dir_children(dir: &Path) -> Vec<(String, PathBuf)> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let name = path.file_name()?.to_str()?.to_string();
            Some((name, path))
        })
        .collect()
}

fn load_all_settings(project_root: Option<&Path>) -> Vec<(Scope, PluginSettings)> {
    let mut paths: Vec<(Scope, PathBuf)> = vec![(Scope::User, user_settings_path())];
    if let Some(root) = project_root {
        paths.push((Scope::Project, project_settings_path(root, false)));
        paths.push((Scope::Local, project_settings_path(root, true)));
    }

    paths
        .into_iter()
        .filter_map(|(scope, path)| match read_settings(&path) {
            Ok(Some(s)) => Some((scope, s)),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Failed to read plugin settings");
                None
            }
        })
        .collect()
}

fn user_settings_path() -> PathBuf {
    Paths::in_config_dir("settings.json")
}

fn project_settings_path(project_root: &Path, local: bool) -> PathBuf {
    let file = if local {
        "settings.local.json"
    } else {
        "settings.json"
    };
    project_root.join(".config").join("goose").join(file)
}

fn read_settings(path: &Path) -> anyhow::Result<Option<PluginSettings>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    let parsed: PluginSettings = serde_json::from_str(&text)?;
    Ok(Some(parsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_plugin_dir(root: &Path, name: &str) {
        let dir = root.join(name);
        std::fs::create_dir_all(dir.join("hooks")).unwrap();
        std::fs::write(
            dir.join("hooks").join("hooks.json"),
            r#"{"hooks":{"SessionStart":[{"hooks":[]}]}}"#,
        )
        .unwrap();
    }

    #[test]
    fn finds_user_placed_plugin_under_project_root() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        write_plugin_dir(&project.join(".agents").join("plugins"), "demo");

        let found = discover_enabled_plugins(Some(project));
        let names: Vec<_> = found.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"demo"), "got: {names:?}");
    }

    #[test]
    fn disabled_plugin_is_filtered_out() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        write_plugin_dir(&project.join(".agents").join("plugins"), "demo");

        let settings_dir = project.join(".config").join("goose");
        std::fs::create_dir_all(&settings_dir).unwrap();
        std::fs::write(
            settings_dir.join("settings.json"),
            r#"{"disabledPlugins":["demo"]}"#,
        )
        .unwrap();

        let found = discover_enabled_plugins(Some(project));
        assert!(found.iter().all(|p| p.name != "demo"));
    }

    #[test]
    fn explicit_enabled_does_not_block_user_placed() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        write_plugin_dir(&project.join(".agents").join("plugins"), "demo");

        let settings_dir = project.join(".config").join("goose");
        std::fs::create_dir_all(&settings_dir).unwrap();
        std::fs::write(
            settings_dir.join("settings.json"),
            r#"{"enabledPlugins":["something-else"]}"#,
        )
        .unwrap();

        let found = discover_enabled_plugins(Some(project));
        assert!(
            found.iter().any(|p| p.name == "demo"),
            "user-placed plugin should remain available; got: {:?}",
            found.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
    }
}
