//! What the runtime selects: a named [`Registration`] whose [`Connector`] opens the
//! MCP client. `ExtensionConfig` selections become registrations with a built-in
//! connector; hosts can register their own transports the same way.

#[cfg(feature = "mcp-http")]
use super::host::ExtensionAuth;
use std::any::Any;
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

#[cfg(feature = "mcp-stdio")]
use super::stdio;
#[cfg(feature = "mcp-http")]
use super::streamable_http;
use super::{
    host::{ExtensionEnvironment, ExtensionRuntimeSettings},
    ClientContext, ExtensionManager,
};
use crate::extension::{name_to_key, ExtensionConfig, ExtensionResult};
use crate::mcp_client::{ConnectContext, McpClientTrait};

/// Object-safe typed equality for connection inputs.
pub trait DynPartialEq: Any {
    fn dyn_eq(&self, other: &dyn Any) -> bool;
}

impl<T: Any + PartialEq> DynPartialEq for T {
    fn dyn_eq(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<T>().is_some_and(|other| self == other)
    }
}

/// Host-specific process placement; transport setup and supervision stay shared.
pub trait StdioCommand: DynPartialEq + Send + Sync {
    fn command(
        &self,
        program: &str,
        args: &[String],
        env: std::collections::HashMap<String, String>,
    ) -> ExtensionResult<tokio::process::Command>;
}

/// Opens the MCP client for one registration.
#[async_trait]
pub trait Connector: DynPartialEq + Send + Sync {
    async fn connect(&self, request: &ConnectRequest) -> ExtensionResult<Box<dyn McpClientTrait>>;
}

/// Where and for whom a registration connects.
pub struct ConnectRequest {
    #[cfg(feature = "mcp-http")]
    auth: Arc<dyn ExtensionAuth>,
    pub scope_id: String,
    /// The scope's working directory (or the process's, if it has none).
    pub working_dir: PathBuf,
    pub client: Arc<ClientContext>,
    tools_version: Arc<AtomicU64>,
    environment: Arc<dyn ExtensionEnvironment>,
    settings: ExtensionRuntimeSettings,
}

impl ConnectRequest {
    pub(super) fn new(
        scope_id: &str,
        working_dir: PathBuf,
        client: Arc<ClientContext>,
        manager: &Arc<ExtensionManager>,
    ) -> Self {
        Self {
            #[cfg(feature = "mcp-http")]
            auth: Arc::clone(&manager.services.auth),
            scope_id: scope_id.to_string(),
            working_dir,
            client,
            tools_version: Arc::new(AtomicU64::new(0)),
            environment: Arc::clone(&manager.services.environment),
            settings: manager.services.settings.clone(),
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
/// equivalent registration, comparing connector inputs through typed equality.
#[derive(Clone)]
pub struct Registration {
    pub(super) key: String,
    pub(super) name: String,
    pub(super) connection: Connection,
    available_tools: Vec<String>,
}

#[derive(Clone)]
pub(super) enum Connection {
    Config {
        config: Box<ExtensionConfig>,
        command: Option<Arc<dyn StdioCommand>>,
    },
    Custom(Arc<dyn Connector>),
}

impl fmt::Debug for Registration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Registration")
            .field("name", &self.name)
            .field("available_tools", &self.available_tools)
            .finish_non_exhaustive()
    }
}

impl Registration {
    /// Connector equality must compare all inputs that affect connection reuse.
    pub fn new(name: impl Into<String>, connector: Arc<dyn Connector>) -> Self {
        let name = name.into();
        Self {
            key: name_to_key(&name),
            name,
            connection: Connection::Custom(connector),
            available_tools: Vec::new(),
        }
    }

    /// Expose only these tools (by server name). Empty exposes all.
    pub fn with_available_tools(mut self, tools: Vec<String>) -> Self {
        self.available_tools = tools;
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    /// Command equality must include placement and any other process-launch inputs.
    pub fn from_config(config: ExtensionConfig, command: Option<Arc<dyn StdioCommand>>) -> Self {
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
            connection: Connection::Config {
                config: Box::new(config),
                command,
            },
            available_tools,
        }
    }

    pub(super) fn is_tool_available(&self, tool: &str) -> bool {
        self.available_tools.is_empty() || self.available_tools.iter().any(|name| name == tool)
    }

    /// Connection inputs and tool policy must both match to reuse a slot.
    pub(super) fn is_equivalent(&self, other: &Registration) -> bool {
        self.key == other.key
            && match (&self.connection, &other.connection) {
                (
                    Connection::Config {
                        config: a,
                        command: ac,
                    },
                    Connection::Config {
                        config: b,
                        command: bc,
                    },
                ) => {
                    a == b
                        && match (ac, bc) {
                            (Some(a), Some(b)) => a.dyn_eq(b.as_ref()),
                            (None, None) => true,
                            _ => false,
                        }
                }
                (Connection::Custom(a), Connection::Custom(b)) => a.dyn_eq(b.as_ref()),
                _ => false,
            }
            && self.available_tools == other.available_tools
    }
}

pub(super) async fn connect_config(
    resolved: &ExtensionConfig,
    command: Option<&dyn StdioCommand>,
    request: &ConnectRequest,
) -> ExtensionResult<Box<dyn McpClientTrait>> {
    let _ = (command, request);
    #[cfg(any(feature = "mcp-stdio", feature = "mcp-http"))]
    let working_dir = match resolved {
        ExtensionConfig::Stdio { cwd: Some(cwd), .. } => PathBuf::from(cwd),
        _ => request
            .environment
            .connection_working_dir(&request.working_dir),
    };
    #[cfg(feature = "mcp-stdio")]
    let environment = request.environment.as_ref();
    match resolved {
        #[cfg(feature = "mcp-http")]
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
                auth: Arc::clone(&request.auth),
            };
            let client = streamable_http::connect(
                params,
                socket.as_deref(),
                request.auth.credential_store(name),
            )
            .await?;
            Ok(client)
        }
        #[cfg(feature = "mcp-stdio")]
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
                    command,
                    request.connect_context(*timeout, working_dir),
                    environment,
                )
                .await?,
            );
            Ok(client as Box<dyn McpClientTrait>)
        }
        #[cfg(not(feature = "mcp-http"))]
        ExtensionConfig::StreamableHttp { .. } => {
            Err(transport_disabled("streamable_http", "mcp-http"))
        }
        #[cfg(not(feature = "mcp-stdio"))]
        ExtensionConfig::Stdio { .. } => Err(transport_disabled("stdio", "mcp-stdio")),
    }
}

#[cfg(not(all(feature = "mcp-stdio", feature = "mcp-http")))]
fn transport_disabled(kind: &str, feature: &str) -> crate::extension::ExtensionError {
    crate::extension::ExtensionError::ConfigError(format!(
        "{kind} extensions require goose-agent's `{feature}` feature"
    ))
}
