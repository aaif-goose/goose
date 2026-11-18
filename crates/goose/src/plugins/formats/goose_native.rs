use crate::agents::extension::{Envs, ExtensionConfig};
use crate::config::{Config, DEFAULT_EXTENSION_DESCRIPTION, DEFAULT_EXTENSION_TIMEOUT};
use crate::plugins::{
    write_install_metadata, FormatNotSupported, PluginFormat, PluginInstall, PluginInstallOptions,
};
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use fs_err as fs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExternalMcpServer {
    Stdio {
        name: String,
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        #[serde(default)]
        cwd: Option<String>,
    },
    Http {
        name: String,
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
}

pub const MANIFEST_FILE: &str = "goose-plugin.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoosePluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub binary: Option<String>,
    #[serde(default)]
    pub mcp: McpSurface,
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub config: HashMap<String, ConfigEntry>,
    #[serde(default, alias = "webUi")]
    pub web_ui: Option<WebUiSurface>,
    #[serde(default)]
    pub http: Option<HttpSurface>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub events: HashMap<String, Vec<EventHook>>,
    #[serde(default, alias = "externalMcp")]
    pub external_mcp: Vec<ExternalMcpServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventHook {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpSurface {
    #[serde(default)]
    pub tools: bool,
    #[serde(default)]
    pub resources: bool,
    #[serde(default)]
    pub prompts: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    #[serde(rename = "type", default = "default_config_type")]
    pub kind: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraint_range: Option<[f64; 2]>,
}

fn default_config_type() -> String {
    "string".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebUiSurface {
    #[serde(default)]
    pub pages: Vec<WebUiPage>,
    #[serde(default, alias = "chatSuffix", skip_serializing_if = "Option::is_none")]
    pub chat_suffix: Option<WebUiContributionSlot>,
    #[serde(default, alias = "configSection", skip_serializing_if = "Option::is_none")]
    pub config_section: Option<WebUiContributionSlot>,
    #[serde(default, alias = "chatActions", skip_serializing_if = "Vec::is_empty")]
    pub chat_actions: Vec<ChatActionContribution>,
    #[serde(default, alias = "rootLinks", skip_serializing_if = "Vec::is_empty")]
    pub root_links: Vec<RootLinkContribution>,
    #[serde(default, alias = "contentSuffixes", skip_serializing_if = "Vec::is_empty")]
    pub content_suffixes: Vec<WebUiContributionSlot>,
    #[serde(default, alias = "customRenders", skip_serializing_if = "Vec::is_empty")]
    pub custom_renders: Vec<CustomRenderContribution>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sidecars: Vec<SidecarContribution>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub themes: Vec<ThemeContribution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebUiContributionSlot {
    pub id: String,
    pub main: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatActionContribution {
    pub id: String,
    pub label: String,
    pub main: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootLinkContribution {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRenderContribution {
    pub id: String,
    pub main: String,
    #[serde(default, alias = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarContribution {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeContribution {
    pub id: String,
    pub label: String,
    pub main: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HttpSurface {
    #[serde(default)]
    pub routes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebUiPage {
    pub id: String,
    pub label: String,
    pub main: String,
}

pub(in crate::plugins) fn try_install_from_manifest_at_root(
    source: &str,
    checkout_dir: &Path,
    install_root: &Path,
    options: &PluginInstallOptions,
    last_update_check: Option<DateTime<Utc>>,
) -> Result<PluginInstall> {
    if !checkout_dir.join(MANIFEST_FILE).is_file() {
        return Err(FormatNotSupported.into());
    }

    let manifest = match read_manifest(checkout_dir)? {
        Some(m) => m,
        None => return Err(FormatNotSupported.into()),
    };
    let plugin_name = manifest.id.clone();

    let built_binary: Option<(String, PathBuf)> = if let Some(binary_name) = &manifest.binary {
        if !checkout_dir.join("Cargo.toml").is_file() {
            return Err(FormatNotSupported.into());
        }

        let status = std::process::Command::new("cargo")
            .args(["build", "--release"])
            .current_dir(checkout_dir)
            .status()?;
        if !status.success() {
            bail!("cargo build --release failed for plugin '{}'", plugin_name);
        }

        let release_dir = checkout_dir.join("target").join("release");
        let path = if cfg!(windows) {
            let with_exe = release_dir.join(format!("{}.exe", binary_name));
            if with_exe.exists() {
                with_exe
            } else {
                release_dir.join(binary_name)
            }
        } else {
            release_dir.join(binary_name)
        };
        if !path.exists() {
            bail!(
                "cargo build succeeded but binary '{}' not found at {}",
                binary_name,
                path.display()
            );
        }
        Some((binary_name.clone(), path))
    } else {
        None
    };

    fs::create_dir_all(install_root)?;
    let destination = install_root.join(&plugin_name);
    if destination.exists() {
        bail!("Plugin '{}' is already installed", plugin_name);
    }

    let staging_dir = tempfile::tempdir_in(install_root)?;
    let staged_destination = staging_dir.path().join(&plugin_name);
    fs::create_dir_all(&staged_destination)?;

    fs::copy(
        checkout_dir.join(MANIFEST_FILE),
        staged_destination.join(MANIFEST_FILE),
    )?;

    if let Some((binary_name, path)) = &built_binary {
        let bin_dir = staged_destination.join("bin");
        fs::create_dir_all(&bin_dir)?;
        let dest_binary = bin_dir.join(binary_name);
        fs::copy(path, &dest_binary)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&dest_binary)?.permissions();
            perms.set_mode(perms.mode() | 0o111);
            std::fs::set_permissions(&dest_binary, perms)?;
        }
    }

    if !manifest.events.is_empty() {
        write_hooks_json(&staged_destination, &manifest.events)?;
    }

    write_install_metadata(
        &staged_destination,
        source,
        "goose-native",
        &manifest.version,
        options.auto_update,
        last_update_check,
    )?;

    fs::rename(&staged_destination, &destination)?;

    Ok(PluginInstall {
        name: plugin_name,
        version: manifest.version,
        format: PluginFormat::GooseNative,
        source: source.to_string(),
        directory: destination,
        skills: Vec::new(),
    })
}

fn write_hooks_json(plugin_dir: &Path, events: &HashMap<String, Vec<EventHook>>) -> Result<()> {
    let hooks_dir = plugin_dir.join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    let mut hooks_map = serde_json::Map::new();
    for (event_name, handlers) in events {
        let rules: Vec<serde_json::Value> = handlers
            .iter()
            .map(|h| {
                let mut rule = serde_json::json!({
                    "hooks": [{ "type": "command", "command": h.command }]
                });
                if let Some(m) = &h.matcher {
                    rule["matcher"] = serde_json::Value::String(m.clone());
                }
                if let Some(t) = h.timeout {
                    rule["hooks"][0]["timeout"] = serde_json::Value::Number(t.into());
                }
                rule
            })
            .collect();
        hooks_map.insert(event_name.clone(), serde_json::Value::Array(rules));
    }

    let hooks_file = serde_json::json!({ "hooks": hooks_map });
    fs::write(
        hooks_dir.join("hooks.json"),
        serde_json::to_string_pretty(&hooks_file)?,
    )?;
    Ok(())
}

pub fn read_manifest(plugin_root: &Path) -> Result<Option<GoosePluginManifest>> {
    let path = plugin_root.join(MANIFEST_FILE);
    if !path.is_file() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)?;
    Ok(Some(serde_json::from_str(&content)?))
}

pub fn native_plugin_extension(
    plugin_name: &str,
    plugin_root: &Path,
) -> Result<Option<ExtensionConfig>> {
    let Some(manifest) = read_manifest(plugin_root)? else {
        return Ok(None);
    };

    if !manifest.mcp.tools && !manifest.mcp.resources && !manifest.mcp.prompts {
        return Ok(None);
    }

    let Some(binary_name) = &manifest.binary else {
        tracing::warn!(plugin = plugin_name, "plugin has MCP surface but no binary configured");
        return Ok(None);
    };
    let binary_path = resolve_binary(plugin_root, binary_name);
    if !binary_path.exists() {
        tracing::warn!(
            plugin = plugin_name,
            binary = %binary_path.display(),
            "goose-plugin binary not found; skipping MCP surface"
        );
        return Ok(None);
    }

    let granted = negotiate_permissions(plugin_name, &manifest.permissions);

    let mut env = HashMap::from([
        (
            "PLUGIN_ROOT".to_string(),
            plugin_root.to_string_lossy().to_string(),
        ),
        ("GOOSE_PLUGIN_ID".to_string(), manifest.id.clone()),
        ("GOOSE_GRANTED_PERMISSIONS".to_string(), granted.join(",")),
    ]);

    if manifest.http.as_ref().is_some_and(|h| !h.routes.is_empty()) {
        env.insert(
            "GOOSE_PLUGIN_HTTP_PORT".to_string(),
            http_port_for_plugin(&manifest.id).to_string(),
        );
    }

    inject_config_env(&manifest, plugin_name, &mut env);

    Ok(Some(ExtensionConfig::Stdio {
        name: plugin_name.to_string(),
        description: if manifest.description.is_empty() {
            DEFAULT_EXTENSION_DESCRIPTION.to_string()
        } else {
            manifest.description.clone()
        },
        cmd: binary_path.to_string_lossy().to_string(),
        args: vec![],
        envs: Envs::new(env),
        env_keys: Vec::new(),
        timeout: Some(DEFAULT_EXTENSION_TIMEOUT),
        cwd: Some(plugin_root.to_string_lossy().to_string()),
        bundled: Some(false),
        available_tools: Vec::new(),
    }))
}

pub fn external_plugin_extensions(
    plugin_name: &str,
    plugin_root: &Path,
) -> Result<Vec<ExtensionConfig>> {
    let Some(manifest) = read_manifest(plugin_root)? else {
        return Ok(vec![]);
    };
    let root = plugin_root.to_string_lossy();
    let mut configs = Vec::new();
    for server in &manifest.external_mcp {
        match server {
            ExternalMcpServer::Stdio {
                name,
                command,
                args,
                env,
                cwd,
            } => {
                let mut envs = HashMap::from([("PLUGIN_ROOT".to_string(), root.to_string())]);
                envs.extend(env.clone());
                configs.push(ExtensionConfig::Stdio {
                    name: format!("{plugin_name}:{name}"),
                    description: DEFAULT_EXTENSION_DESCRIPTION.to_string(),
                    cmd: command.replace("${PLUGIN_ROOT}", &root),
                    args: args
                        .iter()
                        .map(|a| a.replace("${PLUGIN_ROOT}", &root))
                        .collect(),
                    envs: Envs::new(envs),
                    env_keys: Vec::new(),
                    timeout: Some(DEFAULT_EXTENSION_TIMEOUT),
                    cwd: cwd
                        .as_deref()
                        .map(|c| c.replace("${PLUGIN_ROOT}", &root))
                        .or_else(|| Some(root.to_string())),
                    bundled: Some(false),
                    available_tools: Vec::new(),
                });
            }
            ExternalMcpServer::Http { name, url, headers } => {
                configs.push(ExtensionConfig::StreamableHttp {
                    name: format!("{plugin_name}:{name}"),
                    description: DEFAULT_EXTENSION_DESCRIPTION.to_string(),
                    uri: url.clone(),
                    envs: Envs::new(HashMap::new()),
                    env_keys: Vec::new(),
                    headers: headers.clone(),
                    timeout: Some(DEFAULT_EXTENSION_TIMEOUT),
                    socket: None,
                    client_id: None,
                    client_secret_key: None,
                    scopes: Vec::new(),
                    bundled: Some(false),
                    available_tools: Vec::new(),
                });
            }
        }
    }
    Ok(configs)
}

fn resolve_binary(plugin_root: &Path, binary: &str) -> PathBuf {
    let as_path = Path::new(binary);
    if as_path.is_absolute() {
        return as_path.to_path_buf();
    }

    let joined = plugin_root.join(binary);
    if joined.exists() {
        return joined;
    }

    let in_bin = plugin_root.join("bin").join(binary);
    if in_bin.exists() {
        return in_bin;
    }

    joined
}

pub fn http_port_for_plugin(plugin_id: &str) -> u16 {
    let hash = plugin_id
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    9100 + (hash % 400) as u16
}

const KNOWN_PERMISSIONS: &[&str] = &[
    "net:fetch",
    "storage:read",
    "storage:write",
    "shell:execute",
    "fs:read",
    "fs:write",
    "mcp:tools",
    "mcp:resources",
    "mcp:prompts",
];

fn negotiate_permissions(plugin_name: &str, requested: &[String]) -> Vec<String> {
    let config = Config::global();
    let approved_key = format!("plugins.{plugin_name}.permissions.approved");
    let approved: Vec<String> = config.get_param(&approved_key).unwrap_or_default();

    let mut granted = Vec::new();
    for perm in requested {
        if !KNOWN_PERMISSIONS.contains(&perm.as_str()) {
            tracing::warn!(
                plugin = plugin_name,
                permission = perm,
                "unknown permission requested"
            );
            continue;
        }
        if !approved.is_empty() && !approved.contains(perm) {
            tracing::warn!(
                plugin = plugin_name,
                permission = perm,
                "permission not approved by user"
            );
            continue;
        }
        granted.push(perm.clone());
    }
    granted
}

fn validate_config_value(value: &str, entry: &ConfigEntry) -> Option<String> {
    match entry.kind.as_str() {
        "integer" => {
            let n: i64 = value.parse().ok()?;
            if let Some([min, max]) = entry.constraint_range {
                if (n as f64) < min || (n as f64) > max {
                    tracing::warn!("config value {n} out of range [{min}, {max}], skipping");
                    return None;
                }
            }
            Some(n.to_string())
        }
        "float" => {
            let n: f64 = value.parse().ok()?;
            if let Some([min, max]) = entry.constraint_range {
                if n < min || n > max {
                    tracing::warn!("config value {n} out of range [{min}, {max}], skipping");
                    return None;
                }
            }
            Some(n.to_string())
        }
        "boolean" => match value.to_lowercase().as_str() {
            "true" | "1" | "yes" => Some("true".to_string()),
            "false" | "0" | "no" => Some("false".to_string()),
            _ => {
                tracing::warn!("invalid boolean config value: {value}");
                None
            }
        },
        "enum" => {
            if let Some(allowed) = &entry.enum_values {
                if allowed.contains(&value.to_string()) {
                    Some(value.to_string())
                } else {
                    tracing::warn!("config value '{value}' not in enum {:?}, skipping", allowed);
                    None
                }
            } else {
                Some(value.to_string())
            }
        }
        "url" => {
            if value.starts_with("http://") || value.starts_with("https://") {
                Some(value.to_string())
            } else {
                tracing::warn!("config value '{value}' is not a valid URL, skipping");
                None
            }
        }
        _ => Some(value.to_string()),
    }
}

fn inject_config_env(
    manifest: &GoosePluginManifest,
    plugin_name: &str,
    env: &mut HashMap<String, String>,
) {
    if manifest.config.is_empty() {
        return;
    }
    let config = Config::global();
    for (key, entry) in &manifest.config {
        let env_key = format!(
            "GOOSE_PLUGIN_{}_{}",
            plugin_name.to_uppercase().replace('-', "_"),
            key.to_uppercase().replace('-', "_")
        );
        let config_key = format!("plugins.{}.config.{}", plugin_name, key);
        let raw = config.get_param::<String>(&config_key).ok().or_else(|| {
            entry.default.as_ref().map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
        });
        if let Some(raw_val) = raw {
            if let Some(validated) = validate_config_value(&raw_val, entry) {
                env.insert(env_key, validated);
            }
        } else if entry.required.unwrap_or(false) {
            tracing::warn!(
                plugin = plugin_name,
                key = key,
                "required config key not set"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_manifest(dir: &Path, content: &str) {
        let mut f = std::fs::File::create(dir.join(MANIFEST_FILE)).unwrap();
        write!(f, "{content}").unwrap();
    }

    #[test]
    fn returns_none_when_no_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let result = native_plugin_extension("test", dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn returns_none_when_no_mcp_surface() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            r#"{"id":"test","name":"Test","version":"0.1.0","binary":"test-bin"}"#,
        );
        let result = native_plugin_extension("test", dir.path()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn returns_extension_when_binary_exists() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("my-plugin");
        std::fs::write(&binary, "").unwrap();

        write_manifest(
            dir.path(),
            r#"{"id":"my-plugin","name":"My Plugin","version":"0.1.0","binary":"my-plugin","mcp":{"tools":true}}"#,
        );

        let result = native_plugin_extension("my-plugin", dir.path())
            .unwrap()
            .unwrap();

        assert_eq!(result.name(), "my-plugin");
        let ExtensionConfig::Stdio { cmd, .. } = result else {
            panic!("expected stdio");
        };
        assert_eq!(cmd, binary.to_string_lossy());
    }

    #[test]
    fn resolves_binary_from_bin_subdir() {
        let dir = tempfile::tempdir().unwrap();
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir(&bin_dir).unwrap();
        std::fs::write(bin_dir.join("wallet"), "").unwrap();

        write_manifest(
            dir.path(),
            r#"{"id":"wallet","name":"Wallet","version":"0.1.0","binary":"wallet","mcp":{"tools":true}}"#,
        );

        let result = native_plugin_extension("wallet", dir.path())
            .unwrap()
            .unwrap();
        assert_eq!(result.name(), "wallet");
    }
}
