use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, Weak};

use anyhow::Result;
use async_trait::async_trait;
use rmcp::transport::auth::CredentialStore;
use rmcp::transport::AuthorizationManager;

use super::ExtensionManager;
use crate::action_required_manager::ActionRequiredManager;
use crate::container::Container;
use crate::extension::{ExtensionConfig, ExtensionResult, DEFAULT_EXTENSION_TIMEOUT};
use crate::mcp_client::{McpClientTrait, TraceContextInjector};

pub type SpawnServerFn = fn(tokio::io::DuplexStream, tokio::io::DuplexStream);

#[derive(Clone, Copy, Default)]
pub struct PlatformExtension {
    pub unprefixed_tools: bool,
}

#[derive(Clone)]
pub struct ExtensionSession {
    pub id: String,
    pub working_dir: PathBuf,
    pub container: Option<Container>,
    pub extensions: Vec<ExtensionConfig>,
    pub provider_runs_tool_loop: bool,
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

/// Application services used by the extension runtime. The host owns persistence,
/// configuration, extension registration, and authorization policy.
#[async_trait]
pub trait ExtensionHost: Send + Sync {
    async fn session(&self, session_id: &str) -> Result<ExtensionSession>;
    async fn select(&self, session_id: &str, config: ExtensionConfig) -> ExtensionResult<()>;
    async fn disable(&self, session_id: &str, key: &str) -> ExtensionResult<bool>;
    async fn resolve_config(&self, config: ExtensionConfig) -> ExtensionResult<ExtensionConfig>;
    fn extension_by_name(&self, name: &str) -> Option<ExtensionConfig>;
    fn action_required(&self) -> Arc<ActionRequiredManager>;
    fn auth(&self) -> Arc<dyn ExtensionAuth>;

    fn config_generation(&self) -> u64 {
        0
    }

    fn client_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn default_timeout(&self) -> u64 {
        DEFAULT_EXTENSION_TIMEOUT
    }
    fn platform_extension(&self, _name: &str) -> Option<PlatformExtension> {
        None
    }
    fn connect_platform(
        &self,
        _name: &str,
        _manager: Weak<ExtensionManager>,
    ) -> ExtensionResult<Option<Box<dyn McpClientTrait>>> {
        Ok(None)
    }
    fn builtin_extension(&self, _name: &str) -> Option<SpawnServerFn> {
        None
    }
    fn builtin_command(&self, name: &str) -> ExtensionResult<(String, Vec<String>)> {
        Err(crate::extension::ExtensionError::ConfigError(format!(
            "No container command for builtin extension: {name}"
        )))
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
