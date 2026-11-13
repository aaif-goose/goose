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

/// Application services used by the extension runtime. The host owns persistence,
/// configuration, extension registration, and authorization policy.
///
/// Only `action_required` is required. The defaults suit a host that resolves its
/// own [`ExtensionSet`](super::ExtensionSet)s with [`super::ExtensionManager::resolve`] and
/// does not change selections at runtime: session lookup and selection changes
/// fail, configs are used as given, and authorization is unavailable.
#[async_trait]
pub trait ExtensionHost: Send + Sync {
    /// Needed by [`super::ExtensionManager::current_lease`], `enable`, and `disable`.
    async fn session(&self, session_id: &str) -> Result<ExtensionSession> {
        anyhow::bail!("this host does not look up extension sessions ({session_id})")
    }

    async fn select(
        &self,
        _session_id: &str,
        registration: super::Registration,
    ) -> ExtensionResult<()> {
        Err(runtime_changes_unsupported(registration.name()))
    }

    async fn disable(&self, _session_id: &str, key: &str) -> ExtensionResult<bool> {
        Err(runtime_changes_unsupported(key))
    }

    /// Resolve secrets and environment before connecting. Defaults to the config as given.
    async fn resolve_config(&self, config: ExtensionConfig) -> ExtensionResult<ExtensionConfig> {
        Ok(config)
    }

    /// Result `_meta` keys only trusted in-process connectors may set. They are
    /// removed from other extensions' results, so an external server cannot trigger
    /// host behavior keyed on them.
    fn reserved_result_meta_keys(&self) -> &'static [&'static str] {
        &[]
    }

    /// Whether a lease's catalog includes `tool` (its server name) from the extension
    /// keyed `extension`. `catalog_has_resources` is whether any extension in the
    /// lease supports MCP resources.
    fn exposes_tool(&self, _extension: &str, _tool: &str, _catalog_has_resources: bool) -> bool {
        true
    }

    /// Sanitize and decorate a tool result before it leaves the runtime. Applies to
    /// all results, including in-process extensions; defaults to no decoration.
    async fn process_result(&self, _context: ResultContext<'_>, _result: &mut CallToolResult) {}

    fn action_required(&self) -> Arc<ActionRequiredManager>;

    fn auth(&self) -> Arc<dyn ExtensionAuth> {
        Arc::new(NoExtensionAuth)
    }

    fn config_generation(&self) -> u64 {
        0
    }

    fn client_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn default_timeout(&self) -> u64 {
        DEFAULT_EXTENSION_TIMEOUT
    }
    /// Connection directory when a transport does not specify an explicit cwd.
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
    fn trace_context(&self) -> Option<TraceContextInjector> {
        None
    }
}

fn runtime_changes_unsupported(name: &str) -> goose_agent::extension::ExtensionError {
    goose_agent::extension::ExtensionError::ConfigError(format!(
        "this host does not support changing extension '{name}' at runtime"
    ))
}
