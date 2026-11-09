use crate::agents::extension::{Envs, ExtensionConfig};
use crate::config::{DEFAULT_EXTENSION_DESCRIPTION, DEFAULT_EXTENSION_TIMEOUT};
use crate::plugins::discovery::discover_enabled_plugins;
use crate::plugins::formats::open_plugins;
use anyhow::{bail, Context, Result};
use fs_err as fs;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::warn;

const DEFAULT_MCP_CONFIG: &str = ".mcp.json";
const PLUGIN_ROOT: &str = "${PLUGIN_ROOT}";

#[derive(Debug, Deserialize)]
struct McpServersDocument {
    #[serde(default, rename = "mcpServers")]
    mcp_servers: HashMap<String, McpServerConfig>,
}

#[derive(Debug, Deserialize)]
struct McpServerConfig {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    cwd: Option<String>,
}

pub fn enabled_plugin_mcp_servers(project_root: Option<&Path>) -> Vec<ExtensionConfig> {
    let mut configs = Vec::new();
    for plugin in discover_enabled_plugins(project_root) {
        match plugin_mcp_servers(&plugin.name, &plugin.root) {
            Ok(plugin_configs) => configs.extend(plugin_configs),
            Err(err) => warn!(
                plugin = %plugin.name,
                root = %plugin.root.display(),
                error = %err,
                "Failed to load plugin MCP servers; skipping",
            ),
        }
    }
    configs
}

pub fn plugin_mcp_servers(plugin_name: &str, plugin_root: &Path) -> Result<Vec<ExtensionConfig>> {
    let manifest = open_plugins::read_manifest(plugin_root, "")?;
    let mut configs = Vec::new();
    let mut seen = HashSet::new();

    for path in mcp_config_paths(plugin_root, manifest.mcp_servers.as_ref())? {
        if !seen.insert(path.clone()) || !path.is_file() {
            continue;
        }

        let document = serde_json::from_str::<McpServersDocument>(&fs::read_to_string(&path)?)
            .with_context(|| format!("Failed to parse {}", path.display()))?;
        configs.extend(document_to_extension_configs(
            plugin_name,
            plugin_root,
            document.mcp_servers,
        ));
    }

    if let Some(value) = manifest
        .mcp_servers
        .as_ref()
        .filter(|value| is_inline_config(value))
    {
        let servers = parse_inline_servers(value)?;
        configs.extend(document_to_extension_configs(
            plugin_name,
            plugin_root,
            servers,
        ));
    }

    Ok(configs)
}

fn mcp_config_paths(
    plugin_root: &Path,
    config: Option<&serde_json::Value>,
) -> Result<Vec<PathBuf>> {
    let custom_paths = config
        .filter(|value| !is_inline_config(value))
        .map(open_plugins::parse_component_paths)
        .transpose()?
        .unwrap_or_default();

    let mut paths = Vec::new();
    if !custom_paths.exclusive {
        paths.push(plugin_root.join(DEFAULT_MCP_CONFIG));
    }

    for path in custom_paths.paths {
        paths.push(plugin_root.join(open_plugins::validate_relative_plugin_path(&path)?));
    }

    Ok(open_plugins::dedupe_paths(paths))
}

fn is_inline_config(value: &serde_json::Value) -> bool {
    value.as_object().is_some_and(|object| {
        !object
            .keys()
            .all(|key| matches!(key.as_str(), "paths" | "exclusive"))
    })
}

fn parse_inline_servers(value: &serde_json::Value) -> Result<HashMap<String, McpServerConfig>> {
    serde_json::from_value(value.clone())
        .with_context(|| "Failed to parse inline Open Plugins MCP servers")
}

fn document_to_extension_configs(
    plugin_name: &str,
    plugin_root: &Path,
    servers: HashMap<String, McpServerConfig>,
) -> Vec<ExtensionConfig> {
    let mut entries: Vec<_> = servers.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    entries
        .into_iter()
        .map(|(server_name, server)| {
            server_to_extension_config(plugin_name, plugin_root, server_name, server)
        })
        .collect()
}

fn server_to_extension_config(
    plugin_name: &str,
    plugin_root: &Path,
    server_name: String,
    server: McpServerConfig,
) -> ExtensionConfig {
    let root = plugin_root.to_string_lossy();
    let mut env = HashMap::from([("PLUGIN_ROOT".to_string(), root.to_string())]);
    env.extend(
        server
            .env
            .into_iter()
            .map(|(key, value)| (key, expand_plugin_root(&value, &root))),
    );

    ExtensionConfig::Stdio {
        name: format!("{plugin_name}:{server_name}"),
        description: DEFAULT_EXTENSION_DESCRIPTION.to_string(),
        cmd: expand_plugin_root(&server.command, &root),
        args: server
            .args
            .into_iter()
            .map(|arg| expand_plugin_root(&arg, &root))
            .collect(),
        envs: Envs::new(env),
        env_keys: Vec::new(),
        timeout: Some(DEFAULT_EXTENSION_TIMEOUT),
        cwd: server.cwd.map(|cwd| expand_plugin_root(&cwd, &root)),
        bundled: Some(false),
        available_tools: Vec::new(),
    }
}

fn expand_plugin_root(value: &str, plugin_root: &str) -> String {
    value.replace(PLUGIN_ROOT, plugin_root)
}

pub fn validate_mcp_servers_manifest_value(value: &serde_json::Value) -> Result<()> {
    if is_inline_config(value) {
        validate_servers(parse_inline_servers(value)?)?;
        return Ok(());
    }

    open_plugins::parse_component_paths(value)?;
    Ok(())
}

pub fn validate_mcp_server_document(value: &serde_json::Value) -> Result<()> {
    let document = serde_json::from_value::<McpServersDocument>(value.clone())?;
    validate_servers(document.mcp_servers)
}

fn validate_servers(servers: HashMap<String, McpServerConfig>) -> Result<()> {
    for (name, server) in servers {
        if server.command.trim().is_empty() {
            bail!(
                "Open Plugins MCP server '{}' command must not be empty",
                name
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::extension::ExtensionConfig;

    #[test]
    fn loads_default_mcp_json_with_plugin_root_expansion() {
        let plugin = tempfile::tempdir().unwrap();
        fs::write(
            plugin.path().join(DEFAULT_MCP_CONFIG),
            r#"{
              "mcpServers": {
                "database": {
                  "command": "${PLUGIN_ROOT}/servers/db-server",
                  "args": ["--config", "${PLUGIN_ROOT}/config.json"],
                  "env": {"DB_PATH": "${PLUGIN_ROOT}/data"},
                  "cwd": "${PLUGIN_ROOT}"
                }
              }
            }"#,
        )
        .unwrap();

        let configs = plugin_mcp_servers("test-plugin", plugin.path()).unwrap();
        assert_eq!(configs.len(), 1);
        let ExtensionConfig::Stdio {
            name,
            cmd,
            args,
            envs,
            cwd,
            ..
        } = &configs[0]
        else {
            panic!("expected stdio config");
        };

        assert_eq!(name, "test-plugin:database");
        assert_eq!(
            cmd,
            plugin
                .path()
                .join("servers/db-server")
                .to_string_lossy()
                .as_ref()
        );
        assert_eq!(
            args,
            &vec![
                "--config".to_string(),
                plugin
                    .path()
                    .join("config.json")
                    .to_string_lossy()
                    .to_string()
            ]
        );
        assert_eq!(
            envs.get_env().get("DB_PATH"),
            Some(&plugin.path().join("data").to_string_lossy().to_string())
        );
        assert_eq!(
            cwd.as_deref(),
            Some(plugin.path().to_string_lossy().as_ref())
        );
    }

    #[test]
    fn loads_inline_manifest_mcp_servers() {
        let plugin = tempfile::tempdir().unwrap();
        fs::create_dir_all(plugin.path().join(".plugin")).unwrap();
        fs::write(
            plugin.path().join(".plugin/plugin.json"),
            r#"{
              "name": "test-plugin",
              "mcpServers": {
                "api": {"command": "npx", "args": ["@company/mcp-server"]}
              }
            }"#,
        )
        .unwrap();

        let configs = plugin_mcp_servers("test-plugin", plugin.path()).unwrap();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].name(), "test-plugin:api");
    }

    #[test]
    fn manifest_paths_supplement_default_config() {
        let plugin = tempfile::tempdir().unwrap();
        fs::create_dir_all(plugin.path().join(".plugin")).unwrap();
        fs::write(
            plugin.path().join(".plugin/plugin.json"),
            r#"{"name":"test-plugin","mcpServers":"./custom-mcp.json"}"#,
        )
        .unwrap();
        fs::write(
            plugin.path().join(DEFAULT_MCP_CONFIG),
            r#"{"mcpServers":{"default":{"command":"default-server"}}}"#,
        )
        .unwrap();
        fs::write(
            plugin.path().join("custom-mcp.json"),
            r#"{"mcpServers":{"custom":{"command":"custom-server"}}}"#,
        )
        .unwrap();

        let names: Vec<_> = plugin_mcp_servers("test-plugin", plugin.path())
            .unwrap()
            .into_iter()
            .map(|config| config.name())
            .collect();

        assert_eq!(names, vec!["test-plugin:default", "test-plugin:custom"]);
    }

    #[test]
    fn validates_manifest_mcp_servers_value() {
        validate_mcp_servers_manifest_value(&serde_json::json!({
            "api": {"command": "npx"}
        }))
        .unwrap();
        validate_mcp_servers_manifest_value(&serde_json::json!({
            "paths": ["./mcp.json"],
            "exclusive": true
        }))
        .unwrap();
    }

    #[test]
    fn rejects_inline_mcp_server_with_empty_command() {
        let error = validate_mcp_servers_manifest_value(&serde_json::json!({
            "api": {"command": ""}
        }))
        .unwrap_err();
        assert!(error.to_string().contains("command must not be empty"));
    }
}
