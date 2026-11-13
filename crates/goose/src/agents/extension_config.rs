//! Goose-only extension kinds, composed with the GDK's external transports.
pub use goose_agent::extension::ExtensionConfig as TransportConfig;
pub use goose_agent::extension::{
    name_to_key, Envs, ExtensionConfigResolver, ExtensionError, ExtensionInfo, ExtensionResult,
    ProcessExit, DEFAULT_EXTENSION_TIMEOUT,
};
use serde::{Deserialize, Deserializer, Serialize};

pub const DEFAULT_EXTENSION: &str = "developer";
pub const DEFAULT_DISPLAY_NAME: &str = "Developer";

// Keep transport variants directly matchable, like the GDK transport enum;
// boxing here would require a second match just to inspect transport fields.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type")]
pub enum ExtensionConfig {
    /// Built-in extension that is part of the bundled goose MCP server
    #[serde(rename = "builtin")]
    Builtin {
        name: String,
        #[serde(default)]
        #[serde(deserialize_with = "deserialize_null_with_default")]
        description: String,
        display_name: Option<String>, // needed for the UI
        timeout: Option<u64>,
        #[serde(default)]
        bundled: Option<bool>,
        #[serde(default)]
        #[serde(skip_serializing_if = "Vec::is_empty")]
        available_tools: Vec<String>,
    },
    /// Platform extensions that have direct access to the agent etc and run in the agent process
    #[serde(rename = "platform")]
    Platform {
        name: String,
        #[serde(default)]
        #[serde(deserialize_with = "deserialize_null_with_default")]
        description: String,
        display_name: Option<String>,
        #[serde(default)]
        bundled: Option<bool>,
        #[serde(default)]
        #[serde(skip_serializing_if = "Vec::is_empty")]
        available_tools: Vec<String>,
    },
    /// Serialize the GDK config directly, preserving the existing tagged JSON.
    #[serde(untagged)]
    External(TransportConfig),
}

impl Default for ExtensionConfig {
    fn default() -> Self {
        Self::Builtin {
            name: DEFAULT_EXTENSION.to_string(),
            display_name: Some(DEFAULT_DISPLAY_NAME.to_string()),
            description: "default".to_string(),
            timeout: Some(DEFAULT_EXTENSION_TIMEOUT),
            bundled: Some(true),
            available_tools: Vec::new(),
        }
    }
}
impl ExtensionConfig {
    pub fn stdio<S: Into<String>, T: Into<u64>>(
        name: S,
        cmd: S,
        description: S,
        timeout: T,
    ) -> Self {
        Self::External(TransportConfig::stdio(name, cmd, description, timeout))
    }
    pub fn streamable_http<S: Into<String>, T: Into<u64>>(
        name: S,
        uri: S,
        description: S,
        timeout: T,
    ) -> Self {
        Self::External(TransportConfig::streamable_http(
            name,
            uri,
            description,
            timeout,
        ))
    }
    pub fn with_args<I, S>(self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        match self {
            Self::External(config) => Self::External(config.with_args(args)),
            other => other,
        }
    }
    pub fn name(&self) -> String {
        match self {
            Self::External(config) => config.name(),
            Self::Builtin { name, .. } | Self::Platform { name, .. } => name.clone(),
        }
    }
    pub fn key(&self) -> String {
        name_to_key(&self.name())
    }
    pub fn is_tool_available(&self, name: &str) -> bool {
        match self {
            Self::External(config) => config.is_tool_available(name),
            Self::Builtin {
                available_tools, ..
            }
            | Self::Platform {
                available_tools, ..
            } => available_tools.is_empty() || available_tools.iter().any(|tool| tool == name),
        }
    }
    pub async fn resolve(self, resolver: &impl ExtensionConfigResolver) -> ExtensionResult<Self> {
        match self {
            Self::External(config) => Ok(Self::External(config.resolve(resolver).await?)),
            other => Ok(other),
        }
    }
    pub fn external(&self) -> Option<&TransportConfig> {
        match self {
            Self::External(config) => Some(config),
            _ => None,
        }
    }
}
impl std::fmt::Display for ExtensionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::External(config) => config.fmt(f),
            Self::Builtin { name, .. } => write!(f, "Builtin({name})"),
            Self::Platform { name, .. } => write!(f, "Platform({name})"),
        }
    }
}
fn deserialize_null_with_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: Default + Deserialize<'de>,
    D: Deserializer<'de>,
{
    Ok(Option::deserialize(deserializer)?.unwrap_or_default())
}
pub fn check_unique_keys(configs: &[ExtensionConfig]) -> anyhow::Result<()> {
    let mut keys = std::collections::HashSet::new();
    for config in configs {
        anyhow::ensure!(
            keys.insert(config.key()),
            "extension '{}' appears twice in the selection",
            config.name()
        );
    }
    Ok(())
}
