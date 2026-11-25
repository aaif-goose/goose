//! Goose's extension selection and application services over the MCP extension runtime.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use rmcp::model::{CallToolResult, MetaObject, Tool};
use rmcp::transport::auth::CredentialStore;
use rmcp::transport::AuthorizationManager;

use crate::agents::container::Container;
use crate::agents::extension::{
    name_to_key, ExtensionConfig, ExtensionError, ExtensionResult, TransportConfig,
};
use crate::agents::extension_runtime::{
    self, ExtensionAuth, ExtensionHost, ExtensionSpec, RejectedRequest, StaticOAuthClientConfig,
    TraceContextInjector,
};
use crate::agents::services::GoosePlatform;
use crate::config::search_path::SearchPaths;
use crate::config::Config;
use crate::oauth::GooseCredentialStore;
use crate::session::{EnabledExtensionsState, Session, SessionManager};
use oi_kit_agent::mcp::client::{
    ConnectContext, GooseMcpClientCapabilities, GooseMcpHostInfo, McpClientTrait,
};

mod builtin;

pub use crate::agents::extension_runtime::{
    get_tool_owner, get_tool_resource_uri, is_tool_owned_by_extension, recover_mangled_tool_name,
    CallRequest, ClientContext, ExtensionLoadResult, GooseMcpAppToolAttachment, LeaseId,
    TRUSTED_TOOL_UPDATE_META_KEY,
};

pub type ExtensionSet = extension_runtime::ExtensionSet<ExtensionConfig>;
pub type McpLease = extension_runtime::ExtensionLease<ExtensionConfig>;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum ExtensionMutation {
    Enable { name: String },
    Disable { name: String },
}

const EXTENSION_MUTATION_META_KEY: &str = "goose_extension_mutation";

impl ExtensionMutation {
    pub fn attach(self, result: &mut CallToolResult) {
        let mut meta = result.meta.take().map(|m| m.0).unwrap_or_default();
        meta.insert(
            EXTENSION_MUTATION_META_KEY.to_string(),
            serde_json::to_value(self).expect("mutation serializes"),
        );
        result.meta = Some(MetaObject(meta));
    }

    pub fn take(result: &mut CallToolResult) -> Option<Self> {
        let meta = result.meta.as_mut()?;
        let value = meta.0.remove(EXTENSION_MUTATION_META_KEY)?;
        if meta.0.is_empty() {
            result.meta = None;
        }
        serde_json::from_value(value).ok()
    }
}

fn remove_subagent_session_meta(result: &mut CallToolResult) {
    let Some(meta) = result.meta.as_mut() else {
        return;
    };

    meta.0.remove("subagent_session_id");

    if meta.0.is_empty() {
        result.meta = None;
    }
}

pub fn get_parameter_names(tool: &Tool) -> Vec<String> {
    let mut names: Vec<String> = tool
        .input_schema
        .get("properties")
        .and_then(|props| props.as_object())
        .map(|props| props.keys().cloned().collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// The identity Goose negotiates with MCP servers on behalf of a platform and,
/// for ACP sessions, the connected client.
pub fn client_context(
    platform: &GoosePlatform,
    host_info: Option<GooseMcpHostInfo>,
) -> ClientContext {
    let mcpui = host_info
        .as_ref()
        .filter(|host_info| host_info.explicit_extensions)
        .map(GooseMcpHostInfo::mcpui_enabled)
        .unwrap_or(matches!(platform, GoosePlatform::GooseDesktop));
    let name = host_info
        .as_ref()
        .and_then(|host_info| host_info.client_name.clone())
        .unwrap_or_else(|| platform.to_string());
    ClientContext {
        name,
        version: std::env::var("GOOSE_MCP_CLIENT_VERSION")
            .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string()),
        capabilities: GooseMcpClientCapabilities {
            mcpui,
            host_info,
            elicitation_handler: None,
            protocol_version: None,
        },
    }
}

impl ExtensionSpec for ExtensionConfig {
    fn name(&self) -> String {
        ExtensionConfig::name(self)
    }

    fn is_tool_available(&self, tool: &str) -> bool {
        ExtensionConfig::is_tool_available(self, tool)
    }

    fn transport(&self) -> Option<&TransportConfig> {
        self.external()
    }
}

struct GooseExtensionHost;

#[async_trait::async_trait]
impl ExtensionHost<ExtensionConfig> for GooseExtensionHost {
    fn get_secret(&self, key: &str) -> Result<String> {
        crate::agents::extension_config::ExtensionConfigResolver::get_secret(Config::global(), key)
    }

    fn config_generation(&self) -> u64 {
        Config::global().generation()
    }

    fn default_timeout(&self) -> u64 {
        Config::global()
            .get_goose_default_extension_timeout()
            .unwrap_or(crate::config::DEFAULT_EXTENSION_TIMEOUT)
    }

    fn resolve_command(&self, cmd: &str) -> PathBuf {
        SearchPaths::builder()
            .with_npm()
            .resolve(cmd)
            .unwrap_or_else(|_| {
                // let the OS raise the error
                PathBuf::from(cmd)
            })
    }

    fn default_working_dir(&self) -> PathBuf {
        std::env::var("GOOSE_WORKING_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default())
    }

    fn search_path(&self) -> Option<OsString> {
        SearchPaths::builder().path().ok()
    }

    fn auth(&self) -> Option<Arc<dyn ExtensionAuth>> {
        Some(Arc::new(GooseAuth))
    }

    fn trace_context(&self) -> Option<TraceContextInjector> {
        #[cfg(feature = "otel")]
        {
            Some(crate::otel::trace_context::inject_current)
        }
        #[cfg(not(feature = "otel"))]
        {
            None
        }
    }

    async fn connect(
        &self,
        config: &ExtensionConfig,
        container: Option<&Container>,
        mut ctx: ConnectContext,
    ) -> ExtensionResult<Box<dyn McpClientTrait>> {
        match config {
            ExtensionConfig::Builtin { name, timeout, .. } => {
                if let Some(timeout) = timeout {
                    ctx.timeout = Duration::from_secs(*timeout);
                }
                builtin::connect(name, container, ctx).await
            }
            ExtensionConfig::Platform { name, .. } => builtin::connect(name, container, ctx).await,
            ExtensionConfig::External(_) => Err(ExtensionError::ConfigError(format!(
                "{config} is connected by the runtime"
            ))),
        }
    }

    fn sanitize_result(&self, result: &mut CallToolResult) {
        ExtensionMutation::take(result);
        remove_subagent_session_meta(result);
    }
}

struct GooseAuth;

#[async_trait::async_trait]
impl ExtensionAuth for GooseAuth {
    fn credential_store(&self, name: &str) -> Box<dyn CredentialStore> {
        Box::new(GooseCredentialStore::new(name.to_string()))
    }

    async fn authorize(
        &self,
        uri: &str,
        name: &str,
        static_client: Option<&StaticOAuthClientConfig>,
        rejected: Option<RejectedRequest>,
    ) -> Result<AuthorizationManager> {
        crate::oauth::oauth_flow(uri, name, static_client, rejected).await
    }
}

/// Owns Goose's MCP extensions: the runtime's slots and leases, and each
/// session's persisted selection.
pub struct ExtensionManager {
    runtime: Arc<extension_runtime::ExtensionManager<ExtensionConfig>>,
    session_manager: Arc<SessionManager>,
}

impl ExtensionManager {
    pub fn new(session_manager: Arc<SessionManager>, default_client: ClientContext) -> Self {
        Self {
            runtime: Arc::new(extension_runtime::ExtensionManager::new(
                Arc::new(GooseExtensionHost),
                session_manager.action_required(),
                default_client,
            )),
            session_manager,
        }
    }

    /// The latest client to attach to a session is the one its extensions start for from
    /// then on.
    pub fn attach_client(&self, session_id: &str, client: ClientContext) {
        self.runtime.attach_client(session_id, client);
    }

    #[cfg(test)]
    pub(crate) fn client(&self, session_id: &str) -> Arc<ClientContext> {
        self.runtime.client(session_id)
    }

    pub async fn resolve(&self, set: &ExtensionSet) -> McpLease {
        self.runtime.resolve(set).await
    }

    #[cfg(test)]
    pub(crate) fn runtime(&self) -> &Arc<extension_runtime::ExtensionManager<ExtensionConfig>> {
        &self.runtime
    }

    #[cfg(test)]
    pub(crate) async fn scope_lease(&self, scope_id: &str) -> McpLease {
        self.runtime.scope_lease(scope_id).await
    }

    async fn session(&self, session_id: &str) -> ExtensionResult<Session> {
        self.session_manager
            .get_session(session_id, false)
            .await
            .map_err(|error| ExtensionError::SetupError(error.to_string()))
    }

    pub(crate) async fn select(
        &self,
        session_id: &str,
        config: ExtensionConfig,
    ) -> ExtensionResult<()> {
        self.session_manager
            .update_enabled_extensions(session_id, |extensions| {
                let key = config.key();
                match extensions.iter_mut().find(|selected| selected.key() == key) {
                    Some(selected) => *selected = config,
                    None => extensions.push(config),
                }
            })
            .await
            .map_err(|error| ExtensionError::SetupError(error.to_string()))
    }

    /// Start the extension and select it for the session. It is started here
    /// rather than on the next lease so a failure reaches whoever asked, and
    /// the selection is only changed when it starts.
    pub async fn enable(&self, session_id: &str, config: ExtensionConfig) -> ExtensionResult<()> {
        let session = self.session(session_id).await?;
        self.runtime
            .enable(
                session_id,
                config.clone(),
                &session.working_dir,
                session.container.as_ref(),
                self.select(session_id, config),
            )
            .await
    }

    pub async fn disable(&self, session_id: &str, key: &str) -> ExtensionResult<bool> {
        self.session_manager
            .update_enabled_extensions(session_id, |extensions| {
                let selected = extensions.len();
                extensions.retain(|config| config.key() != key);
                extensions.len() != selected
            })
            .await
            .map_err(|error| ExtensionError::SetupError(error.to_string()))
    }

    /// Processes stop once no lease holds them.
    pub async fn release(&self, session_id: &str) {
        self.runtime.release(session_id).await;
    }

    pub async fn get_extension_configs(&self, session_id: &str) -> Result<Vec<ExtensionConfig>> {
        let session = self.session_manager.get_session(session_id, false).await?;
        Ok(selection(&session))
    }

    pub async fn list_extensions(&self, session_id: &str) -> Result<Vec<String>> {
        Ok(self
            .get_extension_configs(session_id)
            .await?
            .iter()
            .map(ExtensionConfig::key)
            .collect())
    }

    pub async fn is_extension_enabled(&self, session_id: &str, name: &str) -> Result<bool> {
        let key = name_to_key(name);
        Ok(self.list_extensions(session_id).await?.contains(&key))
    }
}

fn selection(session: &Session) -> Vec<ExtensionConfig> {
    EnabledExtensionsState::from_extension_data(&session.extension_data)
        .map(|state| state.extensions)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
