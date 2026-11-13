//! What the runtime selects: a named [`Registration`] whose [`Connector`] opens the
//! MCP client. `ExtensionConfig` selections become registrations with a built-in
//! connector; hosts can register their own transports the same way.

use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

use async_trait::async_trait;

use super::stdio;
use super::streamable_http;
use super::{
    host::{ExtensionAuth, ExtensionEnvironment, ExtensionRuntimeSettings},
    ClientContext, ExtensionManager,
};
use crate::agents::mcp_client::{ConnectContext, McpClientTrait};
use goose_agent::extension::{name_to_key, ExtensionConfig, ExtensionResult};

/// Host-specific process placement; transport setup and supervision stay shared.
pub trait StdioCommand: Send + Sync {
    fn command(
        &self,
        program: &str,
        args: &[String],
        env: std::collections::HashMap<String, String>,
    ) -> ExtensionResult<tokio::process::Command>;
}

/// Opens the MCP client for one registration.
#[async_trait]
pub trait Connector: Send + Sync {
    /// `Ok(None)` declines: the extension is unavailable here and contributes no
    /// tools, without counting as a failure.
    async fn connect(
        &self,
        request: &ConnectRequest,
    ) -> ExtensionResult<Option<Box<dyn McpClientTrait>>>;

    /// Whether a running connection still reflects this connector's inputs. Asked
    /// only after the host's `config_generation` changes; `false` restarts the
    /// extension on the next lease.
    async fn is_current(&self) -> bool {
        true
    }
}

/// Where and for whom a registration connects.
pub struct ConnectRequest {
    pub scope_id: String,
    /// The scope's working directory (or the process's, if it has none).
    pub working_dir: PathBuf,
    pub client: Arc<ClientContext>,
    tools_version: Arc<AtomicU64>,
    pub(super) resolved_config: OnceLock<ExtensionConfig>,
    environment: Arc<dyn ExtensionEnvironment>,
    settings: ExtensionRuntimeSettings,
    pub manager: Weak<ExtensionManager>,
}

impl ConnectRequest {
    pub(super) fn new(
        scope_id: &str,
        working_dir: PathBuf,
        client: Arc<ClientContext>,
        manager: &Arc<ExtensionManager>,
    ) -> Self {
        Self {
            scope_id: scope_id.to_string(),
            working_dir,
            client,
            tools_version: Arc::new(AtomicU64::new(0)),
            resolved_config: OnceLock::new(),
            environment: Arc::clone(&manager.services.environment),
            settings: manager.services.settings.clone(),
            manager: Arc::downgrade(manager),
        }
    }

    /// Increment on `notifications/tools/list_changed`: a cached tool list is only
    /// reused while this is unchanged.
    pub fn tools_version(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.tools_version)
    }

    pub fn connect_context(&self, timeout: Option<u64>, working_dir: PathBuf) -> ConnectContext {
        ConnectContext {
            timeout: Duration::from_secs(
                timeout.unwrap_or_else(|| self.environment.default_timeout()),
            ),
            client_name: self.client.name.clone(),
            client_version: self.settings.client_version,
            capabilities: self.client.capabilities.clone(),
            working_dir,

            action_required: Arc::clone(&self.settings.action_required),
            tools_version: Arc::clone(&self.tools_version),
            trace_context: self.settings.trace_context.clone(),
        }
    }
}

/// One selectable extension. A lease reuses a running extension only for an
/// equivalent registration: bump the revision when its connector's inputs change.
#[derive(Clone)]
pub struct Registration {
    pub(super) key: String,
    pub(super) name: String,
    pub(super) connector: Arc<dyn Connector>,
    revision: u64,
    available_tools: Vec<String>,
    pub(super) unprefixed: bool,
    /// Trusted in-process connectors get the MOIM and take the working directory
    /// from each call rather than restarting when it changes.
    pub(super) trusted: bool,
    pub(super) placed: bool,
    metadata: serde_json::Value,
    pub(super) config: Option<ExtensionConfig>,
}

impl fmt::Debug for Registration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Registration")
            .field("name", &self.name)
            .field("revision", &self.revision)
            .field("available_tools", &self.available_tools)
            .field("unprefixed", &self.unprefixed)
            .finish_non_exhaustive()
    }
}

impl Registration {
    pub fn new(name: impl Into<String>, connector: Arc<dyn Connector>) -> Self {
        let name = name.into();
        Self {
            key: name_to_key(&name),
            name,
            connector,
            revision: 0,
            available_tools: Vec::new(),
            unprefixed: false,
            trusted: false,
            placed: true,
            metadata: serde_json::Value::Null,
            config: None,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn with_revision(mut self, revision: u64) -> Self {
        self.revision = revision;
        self
    }

    /// Expose only these tools (by server name). Empty exposes all.
    pub fn with_available_tools(mut self, tools: Vec<String>) -> Self {
        self.available_tools = tools;
        self
    }

    /// Expose tools under their server names instead of `{name}__{tool}`.
    pub fn unprefixed(mut self) -> Self {
        self.unprefixed = true;
        self
    }

    pub fn connector(&self) -> &Arc<dyn Connector> {
        &self.connector
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn from_config(
        config: ExtensionConfig,
        environment: Arc<dyn ExtensionEnvironment>,
        auth: Arc<dyn ExtensionAuth>,
    ) -> Self {
        let available_tools = match &config {
            ExtensionConfig::StreamableHttp {
                available_tools, ..
            }
            | ExtensionConfig::Stdio {
                available_tools, ..
            } => available_tools.clone(),
        };
        Self {
            key: config.key(),
            name: config.name(),
            connector: Arc::new(ConfigConnector {
                config: config.clone(),
                environment,
                auth,
                command: None,
            }),
            revision: 0,
            available_tools,
            unprefixed: false,
            trusted: false,
            placed: true,
            metadata: serde_json::Value::Null,
            config: Some(config),
        }
    }

    /// External transport registration with host-specific subprocess placement.
    pub fn from_config_with_command(
        config: ExtensionConfig,
        environment: Arc<dyn ExtensionEnvironment>,
        auth: Arc<dyn ExtensionAuth>,
        command: Arc<dyn StdioCommand>,
    ) -> Self {
        let mut registration =
            Self::from_config(config.clone(), Arc::clone(&environment), Arc::clone(&auth));
        registration.connector = Arc::new(ConfigConnector {
            config,
            environment,
            auth,
            command: Some(command),
        });
        registration
    }

    /// Mark an in-process connector as trusted to emit host-reserved metadata.
    pub fn trusted(mut self) -> Self {
        self.trusted = true;
        self
    }

    /// Do not restart this connector when the scope's working directory changes.
    pub fn location_independent(mut self) -> Self {
        self.placed = false;
        self
    }

    /// Opaque host data; participates in registration equivalence.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
    pub fn metadata(&self) -> &serde_json::Value {
        &self.metadata
    }

    pub(super) fn is_tool_available(&self, tool: &str) -> bool {
        self.available_tools.is_empty() || self.available_tools.iter().any(|name| name == tool)
    }

    /// Whether a slot started for `self` can serve `other`. Config-backed
    /// registrations compare configs; host registrations compare revisions.
    pub(super) fn is_equivalent(&self, other: &Registration) -> bool {
        self.key == other.key
            && self.revision == other.revision
            && self.available_tools == other.available_tools
            && self.unprefixed == other.unprefixed
            && self.trusted == other.trusted
            && self.placed == other.placed
            && self.metadata == other.metadata
            && self.config == other.config
    }
}

/// Connects an `ExtensionConfig` after the host resolves its secrets.
struct ConfigConnector {
    config: ExtensionConfig,
    environment: Arc<dyn ExtensionEnvironment>,
    auth: Arc<dyn ExtensionAuth>,
    command: Option<Arc<dyn StdioCommand>>,
}

#[async_trait]
impl Connector for ConfigConnector {
    async fn connect(
        &self,
        request: &ConnectRequest,
    ) -> ExtensionResult<Option<Box<dyn McpClientTrait>>> {
        let resolved = self.environment.resolve_config(self.config.clone()).await?;
        let working_dir = match &resolved {
            ExtensionConfig::Stdio { cwd: Some(cwd), .. } => PathBuf::from(cwd),
            _ => self
                .environment
                .connection_working_dir(&request.working_dir),
        };
        let environment = self.environment.as_ref();
        let client: ExtensionResult<Option<Box<dyn McpClientTrait>>> = match &resolved {
            ExtensionConfig::StreamableHttp {
                uri,
                timeout,
                headers,
                name,
                envs,
                socket,
                client_id,
                client_secret_key,
                scopes,
                ..
            } => {
                let static_oauth_client = streamable_http::resolve_static_oauth_client(
                    client_id.as_deref(),
                    client_secret_key.as_deref(),
                    scopes,
                    &envs.get_env(),
                )?;
                let params = streamable_http::ConnectParams {
                    uri: uri.clone(),
                    name: name.clone(),
                    headers: headers.clone(),
                    static_oauth_client,
                    ctx: request.connect_context(*timeout, working_dir),
                    auth: Arc::clone(&self.auth),
                };
                let client = streamable_http::connect(
                    params,
                    socket.as_deref(),
                    self.auth.credential_store(name),
                )
                .await?;
                Ok(Some(client))
            }
            ExtensionConfig::Stdio {
                cmd,
                args,
                envs,
                timeout,
                ..
            } => {
                let mut envs = envs.get_env();
                envs.insert("AGENT_SESSION_ID".to_string(), request.scope_id.clone());
                let client = Box::new(
                    stdio::connect(
                        cmd,
                        args,
                        envs,
                        self.command.as_deref(),
                        request.connect_context(*timeout, working_dir),
                        environment,
                    )
                    .await?,
                );
                Ok(Some(client as Box<dyn McpClientTrait>))
            }
        };
        let client = client?;
        let _ = request.resolved_config.set(resolved);
        Ok(client)
    }
}
