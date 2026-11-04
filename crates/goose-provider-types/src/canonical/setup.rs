#[cfg(not(feature = "std"))]
use crate::prelude::*;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSetupCategory {
    Agent,
    Model,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSetupMethod {
    None,
    SingleApiKey,
    ConfigFields,
    HostWithOauthFallback,
    OauthBrowser,
    OauthDeviceCode,
    CloudCredentials,
    Local,
    CliAuth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSetupGroup {
    Default,
    Additional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSetupField {
    pub key: String,
    pub label: String,
    pub secret: bool,
    pub required: bool,
    pub placeholder: Option<String>,
    pub default_value: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct ProviderSetupCapabilities {
    pub install: bool,
    pub auth: bool,
    pub auth_status: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSetupCatalogEntry {
    pub provider_id: String,
    pub display_name: String,
    pub category: ProviderSetupCategory,
    pub acp: bool,
    pub description: String,
    pub setup_method: ProviderSetupMethod,
    pub docs_url: Option<String>,
    pub group: ProviderSetupGroup,
    pub fields: Vec<ProviderSetupField>,
    pub aliases: Vec<String>,
    pub native_connect_query: Option<String>,
    pub binary_name: Option<String>,
    #[serde(default)]
    pub setup_capabilities: ProviderSetupCapabilities,
    pub show_only_when_installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSetupMetadata {
    pub category: ProviderSetupCategory,
    #[serde(default)]
    pub acp: bool,
    pub setup_method: ProviderSetupMethod,
    pub group: ProviderSetupGroup,
    #[serde(default)]
    pub docs_url: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub native_connect_query: Option<String>,
    #[serde(default)]
    pub binary_name: Option<String>,
    #[serde(default)]
    pub setup_capabilities: ProviderSetupCapabilities,
    #[serde(default)]
    pub show_only_when_installed: bool,
    #[serde(default)]
    pub field_overrides: Vec<ProviderSetupFieldOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSetupFieldOverride {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub default_value: Option<String>,
}
