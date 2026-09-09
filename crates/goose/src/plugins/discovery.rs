use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{paths::Paths, Config};
use crate::plugins::plugin_install_dir;

pub(in crate::plugins) const PLUGINS_CONFIG_KEY: &str = "plugins";

/// Per-plugin entry stored under the `plugins` map in `config.yaml`, keyed by
/// the plugin's filesystem path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::plugins) struct PluginConfigEntry {
    pub enabled: bool,
}

/// A plugin found on disk and not disabled by any settings file.
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    pub name: String,
    pub root: PathBuf,
    pub scope: PluginScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginScope {
    User,
    Project,
}

/// Settings file format from <https://open-plugins.com/plugin-builders/installation>.
#[derive(Debug, Default, Deserialize)]
struct PluginSettings {
    #[serde(default, rename = "enabledPlugins")]
    enabled: Vec<String>,
    #[serde(default, rename = "disabledPlugins")]
    disabled: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SettingsScope {
    Local,
    Project,
    User,
}

/// Discover all plugins that should be considered active.
///
/// `project_root`, when supplied, enables project + local scope settings and
/// project-scope `.agents/plugins/` lookups.
pub fn discover_enabled_plugins(project_root: Option<&Path>) -> Vec<DiscoveredPlugin> {
    discover_enabled_plugins_with_config(project_root, Config::global())
}

pub(crate) fn discover_enabled_plugins_with_config(
    project_root: Option<&Path>,
    config: &Config,
) -> Vec<DiscoveredPlugin> {
    let scoped_settings = load_all_settings(project_root);
    let user_plugins_dir = plugin_install_dir();
    let mut found = Vec::new();

    if let Some(root) = project_root {
        let project_plugins_dir = project_plugin_dir(root);
        if !equivalent_paths(&project_plugins_dir, &user_plugins_dir) {
            found.extend(list_dir_children(&project_plugins_dir).into_iter().map(
                |(name, root)| DiscoveredPlugin {
                    name,
                    root,
                    scope: PluginScope::Project,
                },
            ));
        }
    }
    found.extend(
        list_dir_children(&user_plugins_dir)
            .into_iter()
            .map(|(name, root)| DiscoveredPlugin {
                name,
                root,
                scope: PluginScope::User,
            }),
    );

    let mut enabled_plugins: Vec<DiscoveredPlugin> = filter_by_config(found, config)
        .into_iter()
        .filter(|plugin| is_enabled(&plugin.name, &scoped_settings))
        .collect();
    enabled_plugins.sort_by(|left, right| {
        plugin_scope_rank(left.scope)
            .cmp(&plugin_scope_rank(right.scope))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.root.cmp(&right.root))
    });

    let mut seen_names = HashSet::new();
    enabled_plugins
        .into_iter()
        .filter(|plugin| seen_names.insert(plugin.name.clone()))
        .collect()
}

fn equivalent_paths(left: &Path, right: &Path) -> bool {
    left == right
        || left
            .canonicalize()
            .ok()
            .zip(right.canonicalize().ok())
            .is_some_and(|(left, right)| left == right)
}

fn plugin_scope_rank(scope: PluginScope) -> u8 {
    match scope {
        PluginScope::Project => 0,
        PluginScope::User => 1,
    }
}

/// Apply the `plugins` map in `config.yaml`. Newly discovered plugins are added
/// to the map with `enabled: true`; plugins explicitly set to `enabled: false`
/// are dropped.
fn filter_by_config(plugins: Vec<DiscoveredPlugin>, config: &Config) -> Vec<DiscoveredPlugin> {
    let mut entries: HashMap<String, PluginConfigEntry> =
        config.get_param(PLUGINS_CONFIG_KEY).unwrap_or_default();

    let mut dirty = false;
    let mut enabled = Vec::new();
    for plugin in plugins {
        let key = plugin.root.to_string_lossy().to_string();
        match entries.get(&key) {
            Some(entry) => {
                if entry.enabled {
                    enabled.push(plugin);
                }
            }
            None => {
                entries.insert(key, PluginConfigEntry { enabled: true });
                dirty = true;
                enabled.push(plugin);
            }
        }
    }

    if dirty {
        if let Err(e) = config.set_param(PLUGINS_CONFIG_KEY, entries) {
            tracing::warn!(error = %e, "Failed to persist plugin config entries");
        }
    }

    enabled
}

fn is_enabled(plugin_name: &str, scoped_settings: &[(SettingsScope, PluginSettings)]) -> bool {
    for scope in [
        SettingsScope::Local,
        SettingsScope::Project,
        SettingsScope::User,
    ] {
        let Some(settings) = scoped_settings
            .iter()
            .find_map(|(s, settings)| (*s == scope).then_some(settings))
        else {
            continue;
        };

        let listed_disabled = settings.disabled.iter().any(|n| n == plugin_name);
        let listed_enabled = settings.enabled.iter().any(|n| n == plugin_name);

        if listed_disabled {
            return false;
        }
        if listed_enabled {
            return true;
        }
    }

    true
}

fn project_plugin_dir(project_root: &Path) -> PathBuf {
    project_root.join(".agents").join("plugins")
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

fn load_all_settings(project_root: Option<&Path>) -> Vec<(SettingsScope, PluginSettings)> {
    let mut paths: Vec<(SettingsScope, PathBuf)> = Vec::new();
    if let Some(path) = user_settings_path() {
        paths.push((SettingsScope::User, path));
    }
    if let Some(root) = project_root {
        paths.push((SettingsScope::Project, project_settings_path(root, false)));
        paths.push((SettingsScope::Local, project_settings_path(root, true)));
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

fn user_settings_path() -> Option<PathBuf> {
    if let Some(path_root) = Paths::path_root() {
        return Some(
            path_root
                .join(".config")
                .join("goose")
                .join("settings.json"),
        );
    }
    Some(
        dirs::home_dir()?
            .join(".config")
            .join("goose")
            .join("settings.json"),
    )
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

    fn write_settings(dir: &Path, contents: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("settings.json"), contents).unwrap();
    }

    fn write_local_settings(dir: &Path, contents: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("settings.local.json"), contents).unwrap();
    }

    fn test_config(dir: &Path) -> Config {
        Config::new(dir.join("config.yaml"), "goose-discovery-test").unwrap()
    }

    fn discover_with_config(project: &Path, config: &Config) -> Vec<DiscoveredPlugin> {
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", None::<&str>)]);
        discover_enabled_plugins_with_config(Some(project), config)
    }

    fn discover(project: &Path) -> Vec<DiscoveredPlugin> {
        let cfg_dir = tempfile::tempdir().unwrap();
        discover_with_config(project, &test_config(cfg_dir.path()))
    }

    #[test]
    fn finds_project_scope_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        write_plugin_dir(&project.join(".agents").join("plugins"), "demo");

        let found = discover(project);
        let names: Vec<_> = found.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"demo"), "got: {names:?}");
        let demo = found.iter().find(|p| p.name == "demo").unwrap();
        assert_eq!(demo.scope, PluginScope::Project);
    }

    #[test]
    fn disabled_in_project_settings_drops_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        let plugin_root = project.join(".agents/plugins/demo");
        write_plugin_dir(&project.join(".agents").join("plugins"), "demo");

        write_settings(
            &project.join(".config").join("goose"),
            r#"{"disabledPlugins":["demo"]}"#,
        );

        let cfg_dir = tempfile::tempdir().unwrap();
        let config = test_config(cfg_dir.path());
        let found = discover_with_config(project, &config);
        assert!(found.iter().all(|p| p.name != "demo"));

        let entries: HashMap<String, PluginConfigEntry> =
            config.get_param(PLUGINS_CONFIG_KEY).unwrap();
        assert!(entries
            .get(&plugin_root.to_string_lossy().into_owned())
            .is_some_and(|entry| entry.enabled));
    }

    #[test]
    fn explicit_enabled_filters_out_unlisted_plugins() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        write_plugin_dir(&project.join(".agents").join("plugins"), "demo");
        write_plugin_dir(&project.join(".agents").join("plugins"), "other");

        write_settings(
            &project.join(".config").join("goose"),
            r#"{"enabledPlugins":["demo"]}"#,
        );

        let found = discover(project);
        let names: Vec<_> = found.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"demo"), "got: {names:?}");
        assert!(names.contains(&"other"), "got: {names:?}");
    }

    #[test]
    fn local_scope_overrides_project_scope() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        write_plugin_dir(&project.join(".agents").join("plugins"), "demo");

        write_settings(
            &project.join(".config").join("goose"),
            r#"{"disabledPlugins":["demo"]}"#,
        );
        write_local_settings(
            &project.join(".config").join("goose"),
            r#"{"enabledPlugins":["demo"]}"#,
        );

        let found = discover(project);
        assert!(
            found.iter().any(|p| p.name == "demo"),
            "local scope should win; got: {:?}",
            found.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn project_scope_overrides_user_scope() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        write_plugin_dir(&project.join(".agents").join("plugins"), "demo");

        let fake_home = tempfile::tempdir().unwrap();
        write_settings(
            &fake_home.path().join(".config").join("goose"),
            r#"{"disabledPlugins":["demo"]}"#,
        );

        write_settings(
            &project.join(".config").join("goose"),
            r#"{"enabledPlugins":["demo"]}"#,
        );

        let cfg_dir = tempfile::tempdir().unwrap();
        let found = {
            let _guard =
                env_lock::lock_env([("GOOSE_PATH_ROOT", Some(fake_home.path().to_str().unwrap()))]);
            discover_enabled_plugins_with_config(Some(project), &test_config(cfg_dir.path()))
        };

        assert!(
            found.iter().any(|p| p.name == "demo"),
            "project scope should win over user; got: {:?}",
            found.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn disabled_project_plugin_falls_back_to_enabled_user_plugin_with_same_name() {
        let project = tempfile::tempdir().unwrap();
        let project_plugins = project.path().join(".agents/plugins");
        write_plugin_dir(&project_plugins, "demo");

        let path_root = tempfile::tempdir().unwrap();
        let user_plugins = path_root.path().join(".agents/plugins");
        write_plugin_dir(&user_plugins, "demo");
        let config_dir = tempfile::tempdir().unwrap();
        let config = test_config(config_dir.path());
        config
            .set_param(
                PLUGINS_CONFIG_KEY,
                HashMap::from([
                    (
                        project_plugins.join("demo").to_string_lossy().into_owned(),
                        PluginConfigEntry { enabled: false },
                    ),
                    (
                        user_plugins.join("demo").to_string_lossy().into_owned(),
                        PluginConfigEntry { enabled: true },
                    ),
                ]),
            )
            .unwrap();
        let _guard = env_lock::lock_env([
            ("GOOSE_PATH_ROOT", path_root.path().to_str()),
            ("PLUGINS", None),
        ]);

        let found = discover_enabled_plugins_with_config(Some(project.path()), &config);
        let demo: Vec<_> = found
            .into_iter()
            .filter(|plugin| plugin.name == "demo")
            .collect();

        assert_eq!(demo.len(), 1);
        assert_eq!(demo[0].scope, PluginScope::User);
        assert_eq!(demo[0].root, user_plugins.join("demo"));
    }

    #[test]
    fn newly_discovered_plugin_is_added_to_config_as_enabled() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        write_plugin_dir(&project.join(".agents").join("plugins"), "demo");

        let cfg_dir = tempfile::tempdir().unwrap();
        let config = test_config(cfg_dir.path());

        let found = discover_with_config(project, &config);
        assert!(found.iter().any(|p| p.name == "demo"));

        let entries: HashMap<String, PluginConfigEntry> =
            config.get_param(PLUGINS_CONFIG_KEY).unwrap();
        let key = project
            .join(".agents")
            .join("plugins")
            .join("demo")
            .to_string_lossy()
            .to_string();
        assert!(
            entries.get(&key).is_some_and(|e| e.enabled),
            "got: {entries:?}"
        );
    }

    #[test]
    fn disabled_in_config_drops_plugin() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        write_plugin_dir(&project.join(".agents").join("plugins"), "demo");

        let cfg_dir = tempfile::tempdir().unwrap();
        let config = test_config(cfg_dir.path());
        let key = project
            .join(".agents")
            .join("plugins")
            .join("demo")
            .to_string_lossy()
            .to_string();
        let entries = HashMap::from([(key, PluginConfigEntry { enabled: false })]);
        config.set_param(PLUGINS_CONFIG_KEY, entries).unwrap();

        let found = discover_with_config(project, &config);
        assert!(found.iter().all(|p| p.name != "demo"));
    }

    #[test]
    fn enabled_in_config_keeps_plugin_without_modifying_config() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        write_plugin_dir(&project.join(".agents").join("plugins"), "demo");

        let cfg_dir = tempfile::tempdir().unwrap();
        let config = test_config(cfg_dir.path());
        let key = project
            .join(".agents")
            .join("plugins")
            .join("demo")
            .to_string_lossy()
            .to_string();
        config
            .set_param(
                PLUGINS_CONFIG_KEY,
                HashMap::from([(key.clone(), PluginConfigEntry { enabled: true })]),
            )
            .unwrap();

        let found = discover_with_config(project, &config);
        assert!(found.iter().any(|p| p.name == "demo"));

        let entries: HashMap<String, PluginConfigEntry> =
            config.get_param(PLUGINS_CONFIG_KEY).unwrap();
        assert!(entries.get(&key).is_some_and(|e| e.enabled));
    }

    #[test]
    fn orders_plugins_by_scope_then_name() {
        let project = tempfile::tempdir().unwrap();
        write_plugin_dir(&project.path().join(".agents/plugins"), "z-project-plugin");
        write_plugin_dir(&project.path().join(".agents/plugins"), "a-project-plugin");

        let path_root = tempfile::tempdir().unwrap();
        write_plugin_dir(&path_root.path().join(".agents/plugins"), "z-user-plugin");
        write_plugin_dir(&path_root.path().join(".agents/plugins"), "a-user-plugin");
        let config_dir = tempfile::tempdir().unwrap();
        let config = test_config(config_dir.path());
        let _guard = env_lock::lock_env([
            ("GOOSE_PATH_ROOT", path_root.path().to_str()),
            ("PLUGINS", None),
        ]);

        let found = discover_enabled_plugins_with_config(Some(project.path()), &config);
        let ordered: Vec<_> = found
            .into_iter()
            .map(|plugin| (plugin.name, plugin.scope))
            .collect();

        assert_eq!(
            ordered,
            vec![
                ("a-project-plugin".to_string(), PluginScope::Project),
                ("z-project-plugin".to_string(), PluginScope::Project),
                ("a-user-plugin".to_string(), PluginScope::User),
                ("z-user-plugin".to_string(), PluginScope::User),
            ]
        );
    }
}
