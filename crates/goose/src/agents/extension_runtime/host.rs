use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use rmcp::transport::auth::{CredentialStore, InMemoryCredentialStore};
use rmcp::transport::AuthorizationManager;

use super::ClientContext;
use rmcp::model::{CallToolResult, Tool};
use tokio_util::sync::CancellationToken;

/// Inputs for host-specific tool-result decoration.
pub struct ResultContext<'a> {
    pub scope_id: &'a str,
    pub extension: &'a str,
    pub server_tool_name: &'a str,
    pub tool: &'a Tool,
    pub client_context: &'a ClientContext,
    pub client: &'a dyn McpClientTrait,
    pub cancellation_token: CancellationToken,
}

use crate::action_required_manager::ActionRequiredManager;
use crate::agents::mcp_client::{McpClientTrait, TraceContextInjector};
use goose_agent::extension::{ExtensionConfig, ExtensionResult, DEFAULT_EXTENSION_TIMEOUT};

#[derive(Clone)]
pub struct ExtensionSession {
    pub id: String,
    pub working_dir: PathBuf,
    pub extensions: Vec<super::Registration>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StaticOAuthClientConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scopes: Vec<String>,
}

#[async_trait]
pub trait ExtensionAuth: Send + Sync {
    fn credential_store(&self, name: &str) -> Box<dyn CredentialStore>;

    async fn authorize(
        &self,
        uri: &str,
        name: &str,
        static_client: Option<&StaticOAuthClientConfig>,
        challenge: Option<String>,
    ) -> Result<AuthorizationManager>;
}

/// Authorization for hosts without credential storage or an OAuth flow: servers
/// that require authorization fail to start.
pub struct NoExtensionAuth;

#[async_trait]
impl ExtensionAuth for NoExtensionAuth {
    fn credential_store(&self, _name: &str) -> Box<dyn CredentialStore> {
        Box::new(InMemoryCredentialStore::new())
    }

    async fn authorize(
        &self,
        uri: &str,
        name: &str,
        _static_client: Option<&StaticOAuthClientConfig>,
        _challenge: Option<String>,
    ) -> Result<AuthorizationManager> {
        anyhow::bail!(
            "extension '{name}' at {uri} requires authorization, which this host does not support"
        )
    }
}

/// Optional persistence for hosts that manage selections by scope ID.
#[async_trait]
pub trait ExtensionSelectionStore: Send + Sync {
    async fn session(&self, scope_id: &str) -> Result<ExtensionSession>;
    async fn select(
        &self,
        scope_id: &str,
        registration: super::Registration,
    ) -> ExtensionResult<()>;
    async fn disable(&self, scope_id: &str, key: &str) -> ExtensionResult<bool>;
}

/// Host-dependent configuration and process environment, evaluated when connecting.
#[async_trait]
pub trait ExtensionEnvironment: Send + Sync {
    async fn resolve_config(&self, config: ExtensionConfig) -> ExtensionResult<ExtensionConfig> {
        Ok(config)
    }

    fn config_generation(&self) -> u64 {
        0
    }

    // Goose's timeout can change with configuration without recreating the manager.
    fn default_timeout(&self) -> u64 {
        DEFAULT_EXTENSION_TIMEOUT
    }

    fn connection_working_dir(&self, directory: &std::path::Path) -> PathBuf {
        if directory.as_os_str().is_empty() {
            std::env::current_dir().unwrap_or_default()
        } else {
            directory.to_path_buf()
        }
    }

    fn resolve_command(&self, cmd: &str) -> PathBuf {
        PathBuf::from(cmd)
    }

    fn search_path(&self) -> Result<OsString> {
        std::env::var_os("PATH").ok_or_else(|| anyhow::anyhow!("PATH is not set"))
    }
}

pub struct DefaultExtensionEnvironment;
impl ExtensionEnvironment for DefaultExtensionEnvironment {}

/// Catalog visibility and the result trust boundary belong to the same policy.
#[async_trait]
pub trait ExtensionToolPolicy: Send + Sync {
    /// Keys removed from external results before host decoration.
    fn reserved_result_meta_keys(&self) -> &'static [&'static str] {
        &[]
    }

    fn exposes_tool(&self, _extension: &str, _tool: &str, _catalog_has_resources: bool) -> bool {
        true
    }

    async fn process_result(&self, _context: ResultContext<'_>, _result: &mut CallToolResult) {}
}

pub struct DefaultExtensionToolPolicy;
impl ExtensionToolPolicy for DefaultExtensionToolPolicy {}

#[derive(Clone)]
pub struct ExtensionRuntimeSettings {
    pub client_version: &'static str,
    pub action_required: Arc<ActionRequiredManager>,
    pub trace_context: Option<TraceContextInjector>,
}

impl ExtensionRuntimeSettings {
    pub fn new(action_required: Arc<ActionRequiredManager>) -> Self {
        Self {
            client_version: env!("CARGO_PKG_VERSION"),
            action_required,
            trace_context: None,
        }
    }
}

pub struct ExtensionServices {
    pub selections: Option<Arc<dyn ExtensionSelectionStore>>,
    pub environment: Arc<dyn ExtensionEnvironment>,
    pub tool_policy: Arc<dyn ExtensionToolPolicy>,
    pub auth: Arc<dyn ExtensionAuth>,
    pub settings: ExtensionRuntimeSettings,
}

impl ExtensionServices {
    pub fn new(action_required: Arc<ActionRequiredManager>) -> Self {
        Self {
            selections: None,
            environment: Arc::new(DefaultExtensionEnvironment),
            tool_policy: Arc::new(DefaultExtensionToolPolicy),
            auth: Arc::new(NoExtensionAuth),
            settings: ExtensionRuntimeSettings::new(action_required),
        }
    }
}
