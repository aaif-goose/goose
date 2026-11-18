use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const MANIFEST_FILE: &str = "goose-plugin.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub engines: PluginEngines,
    pub binary: String,
    #[serde(default)]
    pub mcp: McpSurface,
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub config: HashMap<String, ConfigEntry>,
    #[serde(default)]
    pub web_ui: Option<WebUiSurface>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginEngines {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goose: Option<String>,
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
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebUiSurface {
    #[serde(default)]
    pub pages: Vec<WebUiPage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebUiPage {
    pub id: String,
    pub label: String,
    pub main: String,
}
