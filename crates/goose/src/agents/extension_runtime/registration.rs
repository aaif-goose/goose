//! What the runtime selects: a named [`Registration`] whose [`Connector`] opens the
//! MCP client. `ExtensionConfig` selections become registrations with a built-in
//! connector; hosts can register their own transports the same way.

use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Weak};
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
}

/// Where and for whom a registration connects.
pub struct ConnectRequest {
    auth: Arc<dyn ExtensionAuth>,
    pub scope_id: String,
    /// The scope's working directory (or the process's, if it has none).
    pub working_dir: PathBuf,
    pub client: Arc<ClientContext>,
    tools_version: Arc<AtomicU64>,
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
            auth: Arc::clone(&manager.services.auth),
            scope_id: scope_id.to_string(),
            working_dir,
            client,
            tools_version: Arc::new(AtomicU64::new(0)),
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
/// equivalent registration: change its identity when connector inputs change.
#[derive(Clone)]
pub struct Registration {
    pub(super) key: String,
    pub(super) name: String,
    pub(super) connection: Connection,
    identity: serde_json::Value,
    available_tools: Vec<String>,
    pub(super) unprefixed: bool,
    /// Trusted in-process connectors get the MOIM and take the working directory
    /// from each call rather than restarting when it changes.
    pub(super) trusted: bool,
    pub(super) placed: bool,
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
            connection: Connection::Custom(connector),
            identity: serde_json::Value::Null,
            available_tools: Vec::new(),
            unprefixed: false,
            trusted: false,
            placed: true,
        }
    }

    pub fn with_identity(mut self, identity: serde_json::Value) -> Self {
        self.identity = identity;
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

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn key(&self) -> &str {
        &self.key
    }

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
            identity: serde_json::to_value(&config).expect("config serializes"),
            connection: Connection::Config {
                config: Box::new(config),
                command,
            },
            available_tools,
            unprefixed: false,
            trusted: false,
            placed: true,
        }
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

    pub(super) fn is_tool_available(&self, tool: &str) -> bool {
        self.available_tools.is_empty() || self.available_tools.iter().any(|name| name == tool)
    }

    /// Connection inputs and tool policy must both match to reuse a slot.
    pub(super) fn is_equivalent(&self, other: &Registration) -> bool {
        self.key == other.key
            && std::mem::discriminant(&self.connection) == std::mem::discriminant(&other.connection)
            && self.identity == other.identity
            && self.available_tools == other.available_tools
            && self.unprefixed == other.unprefixed
            && self.trusted == other.trusted
            && self.placed == other.placed
    }
}

pub(super) async fn connect_config(
    resolved: &ExtensionConfig,
    command: Option<&dyn StdioCommand>,
    request: &ConnectRequest,
) -> ExtensionResult<Box<dyn McpClientTrait>> {
    let working_dir = match resolved {
        ExtensionConfig::Stdio { cwd: Some(cwd), .. } => PathBuf::from(cwd),
        _ => request
            .environment
            .connection_working_dir(&request.working_dir),
    };
    let environment = request.environment.as_ref();
    match resolved {
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
    }
}
