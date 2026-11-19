use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::model::{CallToolResult, JsonObject};
use rmcp::transport::auth::CredentialStore;
use rmcp::transport::AuthorizationManager;

use crate::extension::ExtensionConfig as TransportConfig;
use crate::extension::{
    name_to_key, ExtensionConfigResolver, ExtensionError, ExtensionResult,
    DEFAULT_EXTENSION_TIMEOUT,
};
use crate::mcp::client::{ConnectContext, McpClientTrait};
use crate::mcp::container::Container;

/// A config the runtime can select, compare and start.
pub trait ExtensionSpec: Clone + PartialEq + std::fmt::Debug + Send + Sync + 'static {
    fn name(&self) -> String;

    fn key(&self) -> String {
        name_to_key(&self.name())
    }

    fn is_tool_available(&self, tool: &str) -> bool;

    /// Configs the runtime connects itself. Any other config is connected by
    /// [`ExtensionHost::connect`].
    fn transport(&self) -> Option<&TransportConfig>;
}

impl ExtensionSpec for TransportConfig {
    fn name(&self) -> String {
        TransportConfig::name(self)
    }

    fn is_tool_available(&self, tool: &str) -> bool {
        TransportConfig::is_tool_available(self, tool)
    }

    fn transport(&self) -> Option<&TransportConfig> {
        Some(self)
    }
}

pub type TraceContextInjector = fn(&mut JsonObject);

/// Application services the runtime needs. Every method has a default, so a host
/// that only runs stdio and HTTP extensions can implement it with no methods.
#[async_trait::async_trait]
pub trait ExtensionHost<C: ExtensionSpec>: Send + Sync {
    fn get_secret(&self, key: &str) -> anyhow::Result<String> {
        anyhow::bail!("secret '{key}' is not available")
    }

    /// Changes whenever secrets or environment may have changed. On the next
    /// resolution, runtime-connected extensions are checked against their
    /// re-resolved config; host-connected extensions get new connections.
    /// Existing leases keep their connections.
    fn config_generation(&self) -> u64 {
        0
    }

    fn default_timeout(&self) -> u64 {
        DEFAULT_EXTENSION_TIMEOUT
    }

    /// Where extensions run when their set has no working directory.
    fn default_working_dir(&self) -> PathBuf {
        std::env::current_dir().unwrap_or_default()
    }

    fn resolve_command(&self, cmd: &str) -> PathBuf {
        PathBuf::from(cmd)
    }

    /// `PATH` for stdio extensions; `None` inherits the process environment.
    fn search_path(&self) -> Option<OsString> {
        None
    }

    /// OAuth for streamable HTTP extensions; `None` connects without it.
    fn auth(&self) -> Option<Arc<dyn ExtensionAuth>> {
        None
    }

    fn trace_context(&self) -> Option<TraceContextInjector> {
        None
    }

    async fn connect(
        &self,
        config: &C,
        _container: Option<&Container>,
        _ctx: ConnectContext,
    ) -> ExtensionResult<Box<dyn McpClientTrait>> {
        Err(ExtensionError::ConfigError(format!(
            "no connector for extension {}",
            config.name()
        )))
    }

    /// Remove result metadata that only the host may attach.
    fn sanitize_result(&self, _result: &mut CallToolResult) {}
}

#[async_trait::async_trait]
pub trait ExtensionAuth: Send + Sync {
    fn credential_store(&self, name: &str) -> Box<dyn CredentialStore>;

    async fn authorize(
        &self,
        uri: &str,
        name: &str,
        static_client: Option<&StaticOAuthClientConfig>,
        challenge: Option<String>,
    ) -> anyhow::Result<AuthorizationManager>;
}

/// OAuth client credentials registered with the authorization server out of
/// band, for servers whose authorization server supports neither Dynamic
/// Client Registration nor Client ID Metadata Documents.
#[derive(Clone, Debug, PartialEq)]
pub struct StaticOAuthClientConfig {
    pub client_id: String,
    /// Secret paired with the client ID. Optional: public clients using PKCE
    /// have no secret.
    pub client_secret: Option<String>,
    /// Scopes to request. When empty, scopes are selected from server
    /// metadata, which may be broader than the extension needs.
    pub scopes: Vec<String>,
}

pub(super) struct Secrets<'a, C: ExtensionSpec>(pub(super) &'a dyn ExtensionHost<C>);

impl<C: ExtensionSpec> ExtensionConfigResolver for Secrets<'_, C> {
    fn get_secret(&self, key: &str) -> anyhow::Result<String> {
        self.0.get_secret(key)
    }
}
