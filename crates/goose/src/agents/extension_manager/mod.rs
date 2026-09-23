use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::stream::{FuturesUnordered, StreamExt};
use futures::FutureExt;
use futures::Stream;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::container::Container;
use super::extension::{
    ExtensionConfig, ExtensionError, ExtensionInfo, ExtensionResult, PlatformExtensionContext,
    PLATFORM_EXTENSIONS,
};
use super::tool_execution::{ToolCallContext, ToolCallResult};
use super::types::SharedProvider;
use crate::action_required_manager::ActionRequiredManager;
use crate::agents::mcp_client::{
    ConnectContext, GooseMcpClientCapabilities, GooseMcpHostInfo, McpClientTrait,
};
use crate::config::extensions::name_to_key;
use crate::config::{get_extension_by_name, Config};
use crate::oauth::GooseCredentialStore;
use crate::session::{EnabledExtensionsState, ExtensionState};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ErrorCode, ErrorData, GetPromptResult,
    ListResourcesResult, ListToolsResult, MetaObject, Prompt, Resource, ServerInfo, Tool,
};
use serde_json::Value;

mod builtin;
mod lease;
mod stdio;
mod streamable_http;

pub use lease::{CallRequest, ExtensionLease, ExtensionSet, LeaseId};

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

type McpClientBox = Arc<dyn McpClientTrait>;

const TOOL_CALL_NOTIFICATION_CHANNEL_CAPACITY: usize = 32;

struct ActionRequiredStream {
    inner: ReceiverStream<crate::conversation::message::Message>,
    manager: Arc<ActionRequiredManager>,
    session_id: String,
    tool_call_request_id: String,
}

impl ActionRequiredStream {
    fn new(
        receiver: tokio::sync::mpsc::Receiver<crate::conversation::message::Message>,
        manager: Arc<ActionRequiredManager>,
        session_id: String,
        tool_call_request_id: String,
    ) -> Self {
        Self {
            inner: ReceiverStream::new(receiver),
            manager,
            session_id,
            tool_call_request_id,
        }
    }
}

impl Stream for ActionRequiredStream {
    type Item = crate::conversation::message::Message;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl Drop for ActionRequiredStream {
    fn drop(&mut self) {
        let manager = self.manager.clone();
        let session_id = self.session_id.clone();
        let tool_call_request_id = self.tool_call_request_id.clone();
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        handle.spawn(async move {
            manager
                .unregister_action_required_stream(&session_id, &tool_call_request_id)
                .await;
        });
    }
}

fn resolve_timeout(timeout: Option<u64>) -> u64 {
    timeout.unwrap_or_else(|| {
        Config::global()
            .get_goose_default_extension_timeout()
            .unwrap_or(crate::config::DEFAULT_EXTENSION_TIMEOUT)
    })
}

pub(super) struct Extension {
    pub(super) key: String,
    pub(super) config: ExtensionConfig,
    /// Resolved config snapshot (with secrets from keyring substituted)
    /// captured at client-creation time. Used to detect secret rotation
    /// without re-reading the keyring on every comparison. Only held in
    /// memory — never serialized to disk.
    resolved_config: ExtensionConfig,
    pub(super) client: McpClientBox,
    server_info: Option<ServerInfo>,
    /// Bumped by the client on tools/list_changed; a cached list is only valid
    /// for the version it was fetched under.
    tools_version: Arc<AtomicU64>,
    /// Servers may publish different tools to different sessions (extension
    /// management hides itself from subagents), so the scope is part of the
    /// cache key.
    tools: Mutex<Option<CachedTools>>,
}

struct CachedTools {
    scope_id: String,
    version: u64,
    tools: Arc<Vec<Tool>>,
}

impl Extension {
    fn new(
        key: String,
        config: ExtensionConfig,
        resolved_config: ExtensionConfig,
        client: McpClientBox,
        server_info: Option<ServerInfo>,
        tools_version: Arc<AtomicU64>,
    ) -> Self {
        Self {
            key,
            config,
            resolved_config,
            client,
            server_info,
            tools_version,
            tools: Mutex::new(None),
        }
    }

    pub(super) fn supports_resources(&self) -> bool {
        self.server_info
            .as_ref()
            .and_then(|info| info.capabilities.resources.as_ref())
            .is_some()
    }

    fn invalidate_tools(&self) {
        self.tools_version.fetch_add(1, Ordering::SeqCst);
    }

    pub(super) fn is_platform(&self) -> bool {
        match &self.config {
            ExtensionConfig::Platform { .. } => true,
            ExtensionConfig::Builtin { name, .. } => {
                PLATFORM_EXTENSIONS.contains_key(name_to_key(name).as_str())
            }
            _ => false,
        }
    }

    /// The extension's tools as the model sees them: filtered by
    /// `available_tools`, prefixed unless first-class, tagged with the owner,
    /// schema-normalized.
    pub(super) async fn public_tools(&self, scope_id: &str) -> Arc<Vec<Tool>> {
        let version = self.tools_version.load(Ordering::SeqCst);
        if let Some(cached) = &*self.tools.lock().await {
            if cached.version == version && cached.scope_id == scope_id {
                return Arc::clone(&cached.tools);
            }
        }

        let tools = Arc::new(self.fetch_public_tools(scope_id).await);

        let mut cache = self.tools.lock().await;
        if self.tools_version.load(Ordering::SeqCst) == version {
            *cache = Some(CachedTools {
                scope_id: scope_id.to_string(),
                version,
                tools: Arc::clone(&tools),
            });
        }
        tools
    }

    async fn fetch_public_tools(&self, session_id: &str) -> Vec<Tool> {
        let cancel_token = CancellationToken::default();
        let expose_unprefixed = is_unprefixed_extension(&self.config);
        let mut tools = Vec::new();
        let mut cursor = None;
        loop {
            let page = match self
                .client
                .list_tools(session_id, cursor, cancel_token.clone())
                .await
            {
                Ok(page) => page,
                Err(e) => {
                    warn!(extension = %self.key, error = %e, "Failed to list tools");
                    break;
                }
            };
            for mut tool in page.tools {
                if !self.config.is_tool_available(&tool.name) {
                    continue;
                }
                if !expose_unprefixed {
                    tool.name = format!("{}__{}", self.key, tool.name).into();
                }
                let mut meta = tool.meta.as_ref().map(|m| m.0.clone()).unwrap_or_default();
                meta.insert(
                    TOOL_EXTENSION_META_KEY.to_string(),
                    Value::String(self.key.clone()),
                );
                tool.meta = Some(MetaObject(meta));
                let mut schema = (*tool.input_schema).clone();
                if super::tool_schema_normalize::normalize_input_schema(&mut schema) {
                    tool.input_schema = Arc::new(schema);
                }
                tools.push(tool);
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        tools
    }
}

pub struct ExtensionManagerCapabilities {
    pub mcpui: bool,
    pub host_info: Option<GooseMcpHostInfo>,
    pub elicitation_handler: Option<crate::agents::mcp_client::ElicitationHandler>,
    pub protocol_version: Option<rmcp::model::ProtocolVersion>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GooseMcpAppToolAttachment {
    pub tool_name: String,
    pub tool_name_is_actual: bool,
    pub extension_name: String,
    pub resource_uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_meta: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_error: Option<String>,
}

pub(crate) const TRUSTED_TOOL_UPDATE_META_KEY: &str = "__goose_tool_update_meta";

/// Manages goose extensions / MCP clients and their interactions
pub struct ExtensionManager {
    extensions: Mutex<IndexMap<String, Arc<Extension>>>,
    mutation_lock: Mutex<()>,
    context: PlatformExtensionContext,
    provider: SharedProvider,
    client_name: String,
    capabilities: ExtensionManagerCapabilities,
}

/// A flattened representation of a resource used by the agent to prepare inference
#[derive(Debug, Clone)]
pub struct ResourceItem {
    pub extension_name: String, // The name of the extension that owns the resource
    pub uri: String,            // The URI of the resource
    pub name: String,           // The name of the resource
    pub content: String,        // The content of the resource
    pub timestamp: DateTime<Utc>, // The timestamp of the resource
    pub priority: f32,          // The priority of the resource
    pub token_count: Option<u32>, // The token count of the resource (filled in by the agent)
}

impl ResourceItem {
    pub fn new(
        extension_name: String,
        uri: String,
        name: String,
        content: String,
        timestamp: DateTime<Utc>,
        priority: f32,
    ) -> Self {
        Self {
            extension_name,
            uri,
            name,
            content,
            timestamp,
            priority,
            token_count: None,
        }
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

const TOOL_EXTENSION_META_KEY: &str = "goose_extension";

pub fn get_tool_owner(tool: &Tool) -> Option<String> {
    tool.meta
        .as_ref()
        .and_then(|m| m.0.get(TOOL_EXTENSION_META_KEY))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

pub(crate) fn is_tool_owned_by_extension(tool: &Tool, extension_name: &str) -> bool {
    let expected_owner = name_to_key(extension_name);
    get_tool_owner(tool).is_some_and(|owner| name_to_key(&owner) == expected_owner)
}

/// `tools` pairs each advertised public tool name with its owning extension's
/// key, when known (`None` for tools with no owner metadata, e.g. those
/// appended outside the extension manager).
pub(crate) fn recover_mangled_tool_name<'a>(
    emitted: &str,
    tools: impl Iterator<Item = (&'a str, Option<&'a str>)>,
) -> Option<String> {
    let trimmed = emitted.trim();
    let stripped = trimmed
        .strip_prefix("functions.")
        .or_else(|| trimmed.strip_prefix("functions:"))
        .unwrap_or(trimmed);

    let mut matched: Option<&str> = None;
    for (name, owner) in tools {
        // Prefixed tools: the model turns Goose's "__" separator into a dot
        // ("developer__shell" -> "developer.shell").
        let separator_mangled = name
            .split_once("__")
            .map(|(extension, tool)| format!("{extension}.{tool}"));

        // Unprefixed tools (e.g. platform extensions like "developer" with
        // unprefixed_tools=true) carry no "__" in their public name at all —
        // the owner is only in metadata — so the model's "developer.shell"
        // has to be checked against "{owner}.{name}" instead (see #9486).
        let owner_mangled = owner.map(|o| format!("{o}.{name}"));
        let owner_prefixed = owner.map(|o| format!("{o}__{name}"));

        let matches = stripped == name
            || separator_mangled.as_deref() == Some(stripped)
            || owner_mangled.as_deref() == Some(stripped)
            || owner_prefixed.as_deref() == Some(stripped);
        if name == emitted || !matches {
            continue;
        }

        match matched {
            None => matched = Some(name),
            Some(prev) if prev == name => {}
            Some(_) => return None,
        }
    }
    matched.map(|s| s.to_string())
}

fn get_tool_meta_value(tool: &Tool) -> Option<Value> {
    tool.meta.as_ref().map(|meta| Value::Object(meta.0.clone()))
}

pub(crate) fn get_tool_resource_uri(tool: &Tool) -> Option<String> {
    tool.meta
        .as_ref()
        .and_then(|meta| meta.0.get("ui"))
        .and_then(Value::as_object)
        .and_then(|ui| ui.get("resourceUri"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn remove_untrusted_mcp_app_meta(result: &mut CallToolResult) {
    let Some(meta) = result.meta.as_mut() else {
        return;
    };

    meta.0.remove(TRUSTED_TOOL_UPDATE_META_KEY);

    let remove_goose = meta
        .0
        .get_mut("goose")
        .and_then(Value::as_object_mut)
        .map(|goose_meta| {
            goose_meta.remove("mcpApp");
            goose_meta.is_empty()
        })
        .unwrap_or(false);

    if remove_goose {
        meta.0.remove("goose");
    }

    if meta.0.is_empty() {
        result.meta = None;
    }
}

fn insert_trusted_tool_update_meta(
    result: &mut CallToolResult,
    attachment: &GooseMcpAppToolAttachment,
) {
    let Ok(attachment_value) = serde_json::to_value(attachment) else {
        return;
    };

    let mut meta_map = result
        .meta
        .as_ref()
        .map(|meta| meta.0.clone())
        .unwrap_or_default();
    let mut trusted_meta = serde_json::Map::new();
    trusted_meta.insert("mcpApp".to_string(), attachment_value);
    meta_map.insert(
        TRUSTED_TOOL_UPDATE_META_KEY.to_string(),
        Value::Object(trusted_meta),
    );
    result.meta = Some(MetaObject(meta_map));
}

fn is_unprefixed_extension(config: &ExtensionConfig) -> bool {
    match config {
        ExtensionConfig::Platform { name, .. } | ExtensionConfig::Builtin { name, .. } => {
            PLATFORM_EXTENSIONS
                .get(name_to_key(name).as_str())
                .is_some_and(|def| def.unprefixed_tools)
        }
        _ => false,
    }
}

/// Returns true if the named extension is a first-class platform extension
/// whose tools are exposed unprefixed and remain visible during code execution mode.
pub fn is_first_class_extension(name: &str) -> bool {
    PLATFORM_EXTENSIONS
        .get(name_to_key(name).as_str())
        .is_some_and(|def| def.unprefixed_tools)
}

pub fn is_hidden_extension(name: &str) -> bool {
    PLATFORM_EXTENSIONS
        .get(name_to_key(name).as_str())
        .is_some_and(|def| def.hidden)
}

impl ExtensionManager {
    fn invalidate_extension_manager_tools(extensions: &IndexMap<String, Arc<Extension>>) {
        if let Some(extension_manager) = extensions.get("extensionmanager") {
            extension_manager.invalidate_tools();
        }
    }

    fn mcp_client_capabilities(&self) -> GooseMcpClientCapabilities {
        GooseMcpClientCapabilities {
            mcpui: self.capabilities.mcpui,
            host_info: self.capabilities.host_info.clone(),
            elicitation_handler: self.capabilities.elicitation_handler.clone(),
            protocol_version: self.capabilities.protocol_version.clone(),
        }
    }

    pub fn new(
        provider: SharedProvider,
        session_manager: Arc<crate::session::SessionManager>,
        scheduler: Option<Arc<dyn crate::scheduler_trait::SchedulerTrait>>,
        client_name: String,
        capabilities: ExtensionManagerCapabilities,
        use_login_shell_path: bool,
    ) -> Self {
        Self {
            extensions: Mutex::new(IndexMap::new()),
            mutation_lock: Mutex::new(()),
            context: PlatformExtensionContext {
                extension_manager: None,
                provider: provider.clone(),
                session_manager,
                scheduler,
                session: None,
                use_login_shell_path,
            },
            provider,
            client_name,
            capabilities,
        }
    }

    pub fn new_without_provider(data_dir: std::path::PathBuf) -> Self {
        let session_manager = Arc::new(crate::session::SessionManager::new(data_dir));
        Self::new(
            Arc::new(Mutex::new(None)),
            session_manager,
            None,
            "goose-cli".to_string(),
            ExtensionManagerCapabilities {
                mcpui: false,
                host_info: None,
                elicitation_handler: None,
                protocol_version: None,
            },
            false,
        )
    }

    pub fn get_context(&self) -> &PlatformExtensionContext {
        &self.context
    }

    pub fn get_provider(&self) -> &SharedProvider {
        &self.provider
    }

    fn hydrate_mcp_apps(&self) -> bool {
        match &self.capabilities.host_info {
            Some(host_info) if host_info.explicit_extensions => host_info.mcpui_enabled(),
            _ => self.capabilities.mcpui,
        }
    }

    /// Resolve a set against what is running. A selected extension that is
    /// not running, or is running under a different config, is left out.
    pub async fn resolve(&self, set: &ExtensionSet) -> ExtensionLease {
        let members = {
            let extensions = self.extensions.lock().await;
            set.extensions()
                .iter()
                .filter_map(|config| {
                    let running = extensions.get(&config.key())?;
                    if running.config != *config {
                        warn!(
                            extension = %config.key(),
                            "selected config differs from the running one; leaving it out"
                        );
                        return None;
                    }
                    Some(Arc::clone(running))
                })
                .collect()
        };
        ExtensionLease::new(
            set.scope_id(),
            set.working_dir.clone(),
            members,
            self.context.session_manager.action_required(),
            self.hydrate_mcp_apps(),
        )
    }

    pub async fn current_lease(
        &self,
        session_id: &str,
        working_dir: Option<&Path>,
    ) -> ExtensionLease {
        let mut extensions = self
            .extensions
            .lock()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        extensions.sort_by(|left, right| left.key.cmp(&right.key));
        ExtensionLease::new(
            session_id,
            working_dir.map(Path::to_path_buf),
            extensions,
            self.context.session_manager.action_required(),
            self.hydrate_mcp_apps(),
        )
    }

    /// Add an extension with an optional working directory.
    /// If working_dir is None, falls back to current_dir.
    pub async fn add_extension(
        self: &Arc<Self>,
        config: ExtensionConfig,
        working_dir: Option<PathBuf>,
        container: Option<&Container>,
        session_id: Option<&str>,
    ) -> ExtensionResult<()> {
        let sanitized_name = config.key();

        // Compare both the unresolved config (to detect structural changes like
        // migrating from plaintext envs to env_keys) and the resolved config (to
        // detect secret rotation where only keyring values changed). Only skip
        // restart if both match.
        let resolved_config = config.clone().resolve(Config::global()).await?;

        if let Some(existing) = self.extensions.lock().await.get(&sanitized_name) {
            if existing.config == config && existing.resolved_config == resolved_config {
                return Ok(());
            }
            tracing::debug!(
                name = sanitized_name,
                "extension config changed, restarting with updated config"
            );
        }

        let working_dir = working_dir
            .or_else(|| std::env::var("GOOSE_WORKING_DIR").ok().map(PathBuf::from))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let tools_version = Arc::new(AtomicU64::new(0));
        let ctx = |timeout: Option<u64>, working_dir: PathBuf| ConnectContext {
            timeout: Duration::from_secs(resolve_timeout(timeout)),
            provider: self.provider.clone(),
            client_name: self.client_name.clone(),
            capabilities: self.mcp_client_capabilities(),
            working_dir,
            docker_container: None,
            action_required: self.context.session_manager.action_required(),
            tools_version: Arc::clone(&tools_version),
        };

        let client: Box<dyn McpClientTrait> = match &resolved_config {
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
                    ctx: ctx(*timeout, working_dir),
                };
                streamable_http::connect(
                    params,
                    socket.as_deref(),
                    Box::new(GooseCredentialStore::new(name.clone())),
                )
                .await?
            }
            ExtensionConfig::Builtin { name, .. } | ExtensionConfig::Platform { name, .. }
                if PLATFORM_EXTENSIONS.contains_key(name_to_key(name).as_str()) =>
            {
                let def = &PLATFORM_EXTENSIONS[name_to_key(name).as_str()];
                let mut context = self.context.clone();
                context.extension_manager = Some(Arc::downgrade(self));
                if let Some(id) = session_id {
                    if let Ok(session) = self.context.session_manager.get_session(id, false).await {
                        context.session = Some(Arc::new(session));
                    }
                }
                // A platform extension the host cannot provide (no scheduler
                // service, say) declines rather than registering with no tools.
                let Some(client) = (def.client_factory)(context) else {
                    return Ok(());
                };
                client
            }
            ExtensionConfig::Builtin { name, timeout, .. } => {
                builtin::connect(name, container, ctx(*timeout, working_dir)).await?
            }
            ExtensionConfig::Platform { name, .. } => {
                builtin::connect(name, container, ctx(None, working_dir)).await?
            }
            ExtensionConfig::Stdio {
                cmd,
                args,
                envs,
                timeout,
                cwd,
                ..
            } => {
                let mut envs = envs.get_env();
                if let Some(sid) = session_id {
                    envs.insert("AGENT_SESSION_ID".to_string(), sid.to_string());
                }
                let working_dir = cwd.as_deref().map(PathBuf::from).unwrap_or(working_dir);
                Box::new(
                    stdio::connect(cmd, args, envs, container, ctx(*timeout, working_dir)).await?,
                )
            }
        };

        let server_info = client.get_info().cloned();

        let mut extensions = self.extensions.lock().await;
        extensions.insert(
            sanitized_name.clone(),
            Arc::new(Extension::new(
                sanitized_name,
                config,
                resolved_config,
                Arc::from(client),
                server_info,
                tools_version,
            )),
        );
        Self::invalidate_extension_manager_tools(&extensions);
        Ok(())
    }

    pub async fn apply(
        self: &Arc<Self>,
        mutation: ExtensionMutation,
        working_dir: Option<PathBuf>,
        container: Option<&Container>,
        session_id: &str,
    ) -> ExtensionResult<()> {
        let _guard = self.mutation_lock.lock().await;
        match mutation {
            ExtensionMutation::Enable { name } => {
                let config = get_extension_by_name(&name).ok_or_else(|| {
                    ExtensionError::ConfigError(format!("Extension '{}' not found", name))
                })?;
                self.add_extension(config, working_dir, container, Some(session_id))
                    .await
            }
            ExtensionMutation::Disable { name } => self.remove_extension(&name).await,
        }?;

        let mut session = self
            .context
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|error| ExtensionError::SetupError(error.to_string()))?;
        EnabledExtensionsState::new(self.get_extension_configs().await)
            .to_extension_data(&mut session.extension_data)
            .map_err(|error| ExtensionError::SetupError(error.to_string()))?;
        self.context
            .session_manager
            .update(session_id)
            .extension_data(session.extension_data)
            .apply()
            .await
            .map_err(|error| ExtensionError::SetupError(error.to_string()))
    }

    pub fn applying_mutation(
        self: &Arc<Self>,
        result: ToolCallResult,
        working_dir: Option<PathBuf>,
        container: Option<Container>,
        session_id: &str,
    ) -> ToolCallResult {
        let manager = Arc::clone(self);
        let session_id = session_id.to_string();
        let inner = result.result;
        ToolCallResult {
            result: Box::new(
                async move {
                    let mut result = inner.await?;
                    if let Some(mutation) = ExtensionMutation::take(&mut result) {
                        manager
                            .apply(mutation, working_dir, container.as_ref(), &session_id)
                            .await
                            .map_err(|e| {
                                ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                            })?;
                    }
                    Ok(result)
                }
                .boxed(),
            ),
            ..result
        }
    }

    pub async fn add_client(
        &self,
        config: ExtensionConfig,
        client: McpClientBox,
        info: Option<ServerInfo>,
    ) {
        let key = config.key();
        let mut extensions = self.extensions.lock().await;
        extensions.insert(
            key.clone(),
            Arc::new(Extension::new(
                key,
                config.clone(),
                config,
                client,
                info,
                Arc::new(AtomicU64::new(0)),
            )),
        );
        Self::invalidate_extension_manager_tools(&extensions);
    }

    /// Get extensions info for building the system prompt
    pub async fn get_extensions_info(&self, working_dir: &Path) -> Vec<ExtensionInfo> {
        self.current_lease("", Some(working_dir))
            .await
            .instructions()
    }

    pub async fn remove_extension(&self, name: &str) -> ExtensionResult<()> {
        let sanitized_name = name_to_key(name);
        self.remove_extension_by_key(&sanitized_name).await?;
        Ok(())
    }

    pub async fn remove_extension_by_key(&self, key: &str) -> ExtensionResult<bool> {
        let mut extensions = self.extensions.lock().await;
        let removed = extensions.shift_remove(key).is_some();
        if removed {
            Self::invalidate_extension_manager_tools(&extensions);
        }
        Ok(removed)
    }

    pub async fn update_working_dir(&self, new_dir: &std::path::Path) {
        let extensions = self.extensions.lock().await;
        for (name, ext) in extensions.iter() {
            if let Err(e) = ext.client.update_working_dir(new_dir.to_path_buf()).await {
                tracing::warn!(extension = %name, error = %e, "failed to update roots");
            }
        }
    }

    pub async fn list_extensions(&self) -> ExtensionResult<Vec<String>> {
        Ok(self.extensions.lock().await.keys().cloned().collect())
    }

    pub async fn is_extension_enabled(&self, name: &str) -> bool {
        let normalized = name_to_key(name);
        self.extensions.lock().await.contains_key(&normalized)
    }

    pub async fn get_extension_configs(&self) -> Vec<ExtensionConfig> {
        self.extensions
            .lock()
            .await
            .values()
            .map(|ext| ext.config.clone())
            .collect()
    }

    /// Get all tools from all clients with proper prefixing
    pub async fn get_prefixed_tools(
        &self,
        session_id: &str,
        extension_name: Option<String>,
    ) -> ExtensionResult<Vec<Tool>> {
        let lease = self.current_lease(session_id, None).await;
        Ok(match extension_name {
            Some(name) => lease.tools_for(&name).await,
            None => lease.tools().await,
        })
    }

    pub async fn list_tools_from_extension(
        &self,
        session_id: &str,
        extension_name: &str,
        cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, ErrorData> {
        let client = self
            .get_server_client(extension_name)
            .await
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("Extension {} is not valid", extension_name),
                    None,
                )
            })?;

        client
            .list_tools(session_id, None, cancellation_token)
            .await
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Unable to list tools for {}, {:?}", extension_name, e),
                    None,
                )
            })
    }

    pub async fn get_prefixed_tools_excluding(
        &self,
        session_id: &str,
        exclude: &str,
    ) -> ExtensionResult<Vec<Tool>> {
        let lease = self.current_lease(session_id, None).await;
        Ok(lease.tools_excluding(exclude).await)
    }

    pub async fn read_resource_tool(
        &self,
        session_id: &str,
        params: Value,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<ContentBlock>, ErrorData> {
        self.current_lease(session_id, None)
            .await
            .read_resource_tool(params, cancellation_token)
            .await
    }

    pub async fn read_resource(
        &self,
        session_id: &str,
        uri: &str,
        extension_name: &str,
        cancellation_token: CancellationToken,
    ) -> Result<rmcp::model::ReadResourceResult, ErrorData> {
        self.current_lease(session_id, None)
            .await
            .read_resource(uri, extension_name, cancellation_token)
            .await
    }

    pub async fn get_ui_resources(
        &self,
        session_id: &str,
    ) -> Result<Vec<(String, Resource)>, ErrorData> {
        let mut ui_resources = Vec::new();

        let extensions_to_check: Vec<(String, McpClientBox)> = {
            let extensions = self.extensions.lock().await;
            extensions
                .iter()
                .map(|(name, ext)| (name.clone(), ext.client.clone()))
                .collect()
        };

        for (extension_name, client) in extensions_to_check {
            match client
                .list_resources(session_id, None, CancellationToken::default())
                .await
            {
                Ok(list_response) => {
                    for resource in list_response.resources {
                        if resource.uri.starts_with("ui://") {
                            ui_resources.push((extension_name.clone(), resource));
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to list resources for {}: {:?}", extension_name, e);
                }
            }
        }

        Ok(ui_resources)
    }

    pub async fn list_resources_result_from_extension(
        &self,
        session_id: &str,
        extension_name: &str,
        cancellation_token: CancellationToken,
    ) -> Result<ListResourcesResult, ErrorData> {
        self.current_lease(session_id, None)
            .await
            .list_resources_result_from_extension(extension_name, cancellation_token)
            .await
    }

    pub async fn list_resources(
        &self,
        session_id: &str,
        params: Value,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<ContentBlock>, ErrorData> {
        self.current_lease(session_id, None)
            .await
            .list_resources(params, cancellation_token)
            .await
    }

    pub async fn dispatch_tool_call(
        &self,
        ctx: &ToolCallContext,
        tool_call: CallToolRequestParams,
        cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ErrorData> {
        self.current_lease(&ctx.session_id, ctx.working_dir.as_deref())
            .await
            .call(tool_call, CallRequest::from(ctx), cancellation_token)
            .await
    }

    pub async fn dispatch_app_tool_call(
        &self,
        ctx: &ToolCallContext,
        tool_call: CallToolRequestParams,
        extension_name: &str,
        cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ErrorData> {
        self.current_lease(&ctx.session_id, ctx.working_dir.as_deref())
            .await
            .call_for_app(
                tool_call,
                extension_name,
                CallRequest::from(ctx),
                cancellation_token,
            )
            .await
    }

    pub async fn list_prompts_from_extension(
        &self,
        session_id: &str,
        extension_name: &str,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<Prompt>, ErrorData> {
        let client = self
            .get_server_client(extension_name)
            .await
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("Extension {} is not valid", extension_name),
                    None,
                )
            })?;

        client
            .list_prompts(session_id, None, cancellation_token)
            .await
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Unable to list prompts for {}, {:?}", extension_name, e),
                    None,
                )
            })
            .map(|lp| lp.prompts)
    }

    pub async fn list_prompts(
        &self,
        session_id: &str,
        cancellation_token: CancellationToken,
    ) -> Result<HashMap<String, Vec<Prompt>>, ErrorData> {
        let mut futures = FuturesUnordered::new();

        let names: Vec<_> = self.extensions.lock().await.keys().cloned().collect();
        for extension_name in names {
            let token = cancellation_token.clone();
            futures.push(async move {
                (
                    extension_name.clone(),
                    self.list_prompts_from_extension(session_id, extension_name.as_str(), token)
                        .await,
                )
            });
        }

        let mut all_prompts = HashMap::new();
        let mut errors = Vec::new();

        // Process results as they complete
        while let Some(result) = futures.next().await {
            let (name, prompts) = result;
            match prompts {
                Ok(content) => {
                    all_prompts.insert(name.to_string(), content);
                }
                Err(tool_error) => {
                    errors.push(tool_error);
                }
            }
        }

        if !errors.is_empty() {
            tracing::debug!(
                errors = ?errors
                    .into_iter()
                    .map(|e| format!("{:?}", e))
                    .collect::<Vec<_>>(),
                "errors from listing prompts"
            );
        }

        Ok(all_prompts)
    }

    pub async fn get_prompt(
        &self,
        session_id: &str,
        extension_name: &str,
        name: &str,
        arguments: Value,
        cancellation_token: CancellationToken,
    ) -> Result<GetPromptResult> {
        let client = self
            .get_server_client(extension_name)
            .await
            .ok_or_else(|| anyhow::anyhow!("Extension {} not found", extension_name))?;

        client
            .get_prompt(session_id, name, arguments, cancellation_token)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get prompt: {}", e))
    }

    async fn get_server_client(&self, name: impl Into<String>) -> Option<McpClientBox> {
        let normalized = name_to_key(&name.into());
        self.extensions
            .lock()
            .await
            .get(&normalized)
            .map(|ext| ext.client.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::CallToolResult;
    use rmcp::model::{CustomNotification, InitializeResult, JsonObject};
    use rmcp::{object, ServiceError as Error};

    use rmcp::model::ListPromptsResult;
    use rmcp::model::ListResourcesResult;
    use rmcp::model::ListToolsResult;
    use rmcp::model::ReadResourceResult;
    use rmcp::model::ServerNotification;

    use super::super::tool_execution::ToolCallNotificationEmitter;
    use std::sync::atomic::AtomicUsize;
    use tokio::sync::{mpsc, Semaphore};

    impl ExtensionManager {
        async fn add_mock_extension(&self, name: String, client: McpClientBox) {
            self.add_mock_extension_with_tools(name, client, vec![])
                .await;
        }

        async fn add_mock_extension_with_tools(
            &self,
            name: String,
            client: McpClientBox,
            available_tools: Vec<String>,
        ) {
            let config = ExtensionConfig::Builtin {
                name: name.clone(),
                display_name: Some(name.clone()),
                description: "built-in".to_string(),
                timeout: None,
                bundled: None,
                available_tools,
            };
            self.add_client(config, client, None).await;
        }
    }

    struct MockClient {}

    #[async_trait::async_trait]
    impl McpClientTrait for MockClient {
        fn get_info(&self) -> Option<&InitializeResult> {
            None
        }

        async fn list_resources(
            &self,
            _session_id: &str,
            _next_cursor: Option<String>,
            _cancellation_token: CancellationToken,
        ) -> Result<ListResourcesResult, Error> {
            Err(Error::TransportClosed)
        }

        async fn read_resource(
            &self,
            _session_id: &str,
            _uri: &str,
            _cancellation_token: CancellationToken,
        ) -> Result<ReadResourceResult, Error> {
            Err(Error::TransportClosed)
        }

        async fn list_tools(
            &self,
            _session_id: &str,
            _next_cursor: Option<String>,
            _cancellation_token: CancellationToken,
        ) -> Result<ListToolsResult, Error> {
            use serde_json::json;
            use std::sync::Arc;
            Ok(ListToolsResult {
                tools: vec![
                    Tool::new(
                        "tool".to_string(),
                        "A basic tool".to_string(),
                        Arc::new(json!({}).as_object().unwrap().clone()),
                    ),
                    Tool::new(
                        "available_tool".to_string(),
                        "An available tool".to_string(),
                        Arc::new(json!({}).as_object().unwrap().clone()),
                    ),
                    Tool::new(
                        "hidden_tool".to_string(),
                        "hidden tool".to_string(),
                        Arc::new(json!({}).as_object().unwrap().clone()),
                    ),
                    {
                        let mut t = Tool::new(
                            "render_chart".to_string(),
                            "Render a chart".to_string(),
                            Arc::new(json!({}).as_object().unwrap().clone()),
                        );
                        t.meta = Some(MetaObject(
                            json!({ "ui": { "resourceUri": "ui://autovisualiser/chart" } })
                                .as_object()
                                .unwrap()
                                .clone(),
                        ));
                        t
                    },
                ],
                next_cursor: None,
                meta: None,
                ..Default::default()
            })
        }

        async fn call_tool(
            &self,
            _ctx: &ToolCallContext,
            name: &str,
            _arguments: Option<JsonObject>,
            _cancellation_token: CancellationToken,
        ) -> Result<CallToolResult, Error> {
            match name {
                "tool" | "test__tool" | "available_tool" | "hidden_tool" | "render_chart"
                | "unadvertised_tool" => Ok(CallToolResult::success(vec![])),
                _ => Err(Error::TransportClosed),
            }
        }

        async fn list_prompts(
            &self,
            _session_id: &str,
            _next_cursor: Option<String>,
            _cancellation_token: CancellationToken,
        ) -> Result<ListPromptsResult, Error> {
            Err(Error::TransportClosed)
        }

        async fn get_prompt(
            &self,
            _session_id: &str,
            _name: &str,
            _arguments: Value,
            _cancellation_token: CancellationToken,
        ) -> Result<GetPromptResult, Error> {
            Err(Error::TransportClosed)
        }

        async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
            mpsc::channel(1).1
        }
    }

    struct ResourceClient {
        label: &'static str,
    }

    #[async_trait::async_trait]
    impl McpClientTrait for ResourceClient {
        async fn list_resources(
            &self,
            _session_id: &str,
            _next_cursor: Option<String>,
            _cancellation_token: CancellationToken,
        ) -> Result<ListResourcesResult, Error> {
            Ok(ListResourcesResult {
                resources: vec![Resource::new(
                    "resource://snapshot".to_string(),
                    format!("{} resource", self.label),
                )],
                ..Default::default()
            })
        }

        async fn read_resource(
            &self,
            _session_id: &str,
            uri: &str,
            _cancellation_token: CancellationToken,
        ) -> Result<ReadResourceResult, Error> {
            Ok(ReadResourceResult::new(vec![
                rmcp::model::ResourceContents::text(self.label, uri),
            ]))
        }

        async fn list_tools(
            &self,
            _session_id: &str,
            _next_cursor: Option<String>,
            _cancellation_token: CancellationToken,
        ) -> Result<ListToolsResult, Error> {
            Ok(ListToolsResult::default())
        }

        async fn call_tool(
            &self,
            _ctx: &ToolCallContext,
            _name: &str,
            _arguments: Option<JsonObject>,
            _cancellation_token: CancellationToken,
        ) -> Result<CallToolResult, Error> {
            Err(Error::TransportClosed)
        }

        fn get_info(&self) -> Option<&InitializeResult> {
            None
        }
    }

    struct ContextNotificationClient;

    #[async_trait::async_trait]
    impl McpClientTrait for ContextNotificationClient {
        fn get_info(&self) -> Option<&InitializeResult> {
            None
        }

        async fn list_tools(
            &self,
            session_id: &str,
            next_cursor: Option<String>,
            cancellation_token: CancellationToken,
        ) -> Result<ListToolsResult, Error> {
            MockClient {}
                .list_tools(session_id, next_cursor, cancellation_token)
                .await
        }

        async fn call_tool(
            &self,
            ctx: &ToolCallContext,
            _name: &str,
            _arguments: Option<JsonObject>,
            _cancellation_token: CancellationToken,
        ) -> Result<CallToolResult, Error> {
            if let Some(emitter) = ctx.notification_emitter() {
                let request_id = ctx
                    .tool_call_request_id
                    .as_deref()
                    .expect("an emitter requires a request ID");
                emitter.emit_best_effort(ServerNotification::CustomNotification(
                    CustomNotification::new(format!("scoped/{request_id}"), None),
                ));
            }
            Ok(CallToolResult::success(vec![]))
        }

        async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
            let (sender, receiver) = mpsc::channel(1);
            sender
                .try_send(ServerNotification::CustomNotification(
                    CustomNotification::new("client/subscription", None),
                ))
                .expect("test notification should fit");
            receiver
        }
    }

    async fn dispatch_notification_methods(ctx: ToolCallContext) -> Vec<String> {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager =
            ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());
        extension_manager
            .add_mock_extension(
                "notifications".to_string(),
                Arc::new(ContextNotificationClient),
            )
            .await;

        let tool_call = CallToolRequestParams::new("notifications__tool".to_string())
            .with_arguments(object!({}));
        let dispatched = extension_manager
            .dispatch_tool_call(&ctx, tool_call, CancellationToken::default())
            .await
            .expect("tool call should dispatch");

        assert!(dispatched.result.await.is_ok());

        let mut methods = dispatched
            .notification_stream
            .expect("notification stream should exist")
            .filter_map(|notification| async move {
                match notification {
                    ServerNotification::CustomNotification(notification) => {
                        Some(notification.method)
                    }
                    _ => None,
                }
            })
            .collect::<Vec<_>>()
            .await;
        methods.sort();
        methods
    }

    #[tokio::test]
    async fn dispatch_merges_request_scoped_and_client_notifications() {
        let methods = dispatch_notification_methods(ToolCallContext::new(
            "session".to_string(),
            None,
            Some("request".to_string()),
        ))
        .await;

        assert_eq!(methods, vec!["client/subscription", "scoped/request"]);
    }

    #[tokio::test]
    async fn dispatch_reuses_existing_notification_emitter() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager =
            ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());
        extension_manager
            .add_mock_extension(
                "notifications".to_string(),
                Arc::new(ContextNotificationClient),
            )
            .await;
        let (sender, mut receiver) = mpsc::channel(1);
        let ctx = ToolCallContext::new(
            "nested-session".to_string(),
            None,
            Some("nested-request".to_string()),
        )
        .with_notification_emitter(ToolCallNotificationEmitter::new(sender));
        let tool_call = CallToolRequestParams::new("notifications__tool".to_string())
            .with_arguments(object!({}));

        let dispatched = extension_manager
            .dispatch_tool_call(&ctx, tool_call, CancellationToken::default())
            .await
            .expect("tool call should dispatch");
        assert!(dispatched.result.await.is_ok());

        let notification = receiver
            .try_recv()
            .expect("parent emitter should receive nested notification");
        let ServerNotification::CustomNotification(notification) = notification else {
            panic!("expected a custom notification");
        };
        assert_eq!(notification.method, "scoped/nested-request");

        let methods = dispatched
            .notification_stream
            .expect("client notification stream should exist")
            .filter_map(|notification| async move {
                match notification {
                    ServerNotification::CustomNotification(notification) => {
                        Some(notification.method)
                    }
                    _ => None,
                }
            })
            .collect::<Vec<_>>()
            .await;
        assert_eq!(methods, vec!["client/subscription"]);
    }

    #[tokio::test]
    async fn dispatch_without_request_id_uses_only_client_notifications() {
        let methods =
            dispatch_notification_methods(ToolCallContext::new("session".to_string(), None, None))
                .await;

        assert_eq!(methods, vec!["client/subscription"]);
    }

    #[test]
    fn test_tool_owner_binding_uses_metadata_not_flattened_name() {
        let tool = |name: &str, owner: &str| {
            let mut tool = Tool::new(
                name.to_string(),
                "test tool".to_string(),
                Arc::new(serde_json::Map::new()),
            );
            tool.meta = Some(MetaObject(
                serde_json::json!({ TOOL_EXTENSION_META_KEY: owner })
                    .as_object()
                    .unwrap()
                    .clone(),
            ));
            tool
        };

        let own_tool = tool("ext_a__own", "ext_a");
        let sibling_tool = tool("ext_a__ext_b__secret", "ext_a__ext_b");

        assert!(is_tool_owned_by_extension(&own_tool, "ext_a"));
        assert!(!is_tool_owned_by_extension(&sibling_tool, "ext_a"));
    }

    struct NamedToolsClient(Vec<Tool>);

    #[async_trait::async_trait]
    impl McpClientTrait for NamedToolsClient {
        async fn list_tools(
            &self,
            _session_id: &str,
            _next_cursor: Option<String>,
            _cancellation_token: CancellationToken,
        ) -> Result<ListToolsResult, Error> {
            Ok(ListToolsResult {
                tools: self.0.clone(),
                next_cursor: None,
                meta: None,
                ..Default::default()
            })
        }

        async fn call_tool(
            &self,
            _ctx: &ToolCallContext,
            _name: &str,
            _arguments: Option<JsonObject>,
            _cancellation_token: CancellationToken,
        ) -> Result<CallToolResult, Error> {
            Ok(CallToolResult::success(vec![]))
        }

        fn get_info(&self) -> Option<&InitializeResult> {
            None
        }
    }

    fn app_tool(name: &str) -> Tool {
        let mut tool = Tool::new(
            name.to_string(),
            "test tool".to_string(),
            Arc::new(serde_json::Map::new()),
        );
        tool.meta = Some(MetaObject(
            serde_json::json!({ "ui": { "resourceUri": "ui://test/app" } })
                .as_object()
                .unwrap()
                .clone(),
        ));
        tool
    }

    /// `ext_a` publishing `ext_b__secret` and `ext_a__ext_b` publishing `secret`
    /// flatten to the same public name. Whoever the catalog keeps, an app
    /// dispatch scoped to the other extension must be refused.
    #[tokio::test]
    async fn app_dispatch_rejects_colliding_flattened_name_from_sibling_owner() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager =
            ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());
        extension_manager
            .add_mock_extension(
                "ext_a__ext_b".to_string(),
                Arc::new(NamedToolsClient(vec![app_tool("secret")])),
            )
            .await;
        extension_manager
            .add_mock_extension(
                "ext_a".to_string(),
                Arc::new(NamedToolsClient(vec![app_tool("ext_b__secret")])),
            )
            .await;

        let lease = extension_manager.current_lease("session", None).await;
        assert_eq!(
            lease.tools().await.len(),
            1,
            "colliding names collapse to one entry"
        );
        let owner = get_tool_owner(&lease.tools().await[0]).unwrap();
        let other = if owner == "ext_a" {
            "ext_a__ext_b"
        } else {
            "ext_a"
        };

        let ctx = ToolCallContext::new("session".to_string(), None, None);
        let result = extension_manager
            .dispatch_app_tool_call(
                &ctx,
                CallToolRequestParams::new("ext_a__ext_b__secret".to_string()),
                other,
                CancellationToken::default(),
            )
            .await;
        let Err(error) = result else {
            panic!("app dispatch accepted a sibling owner's colliding tool name");
        };
        assert_eq!(error.code, ErrorCode::RESOURCE_NOT_FOUND);
    }

    struct BlockingToolsClient {
        calls: AtomicUsize,
        first_fetch_started: Semaphore,
        release_first_fetch: Semaphore,
    }

    #[async_trait::async_trait]
    impl McpClientTrait for BlockingToolsClient {
        async fn list_tools(
            &self,
            _session_id: &str,
            _next_cursor: Option<String>,
            _cancel_token: CancellationToken,
        ) -> Result<ListToolsResult, Error> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            let name = if call == 0 { "old" } else { "new" };

            if call == 0 {
                self.first_fetch_started.add_permits(1);
                let _permit = self.release_first_fetch.acquire().await.unwrap();
            }

            Ok(ListToolsResult {
                tools: vec![Tool::new(
                    name,
                    format!("{name} tool list"),
                    Arc::new(JsonObject::new()),
                )],
                next_cursor: None,
                meta: None,
                ..Default::default()
            })
        }

        async fn call_tool(
            &self,
            _ctx: &ToolCallContext,
            _name: &str,
            _arguments: Option<JsonObject>,
            _cancel_token: CancellationToken,
        ) -> Result<CallToolResult, Error> {
            Ok(CallToolResult::success(vec![]))
        }

        fn get_info(&self) -> Option<&InitializeResult> {
            None
        }
    }

    fn builtin_config(name: &str, available_tools: Vec<String>) -> ExtensionConfig {
        ExtensionConfig::Builtin {
            name: name.to_string(),
            display_name: Some(name.to_string()),
            description: "built-in".to_string(),
            timeout: None,
            bundled: None,
            available_tools,
        }
    }

    #[tokio::test]
    async fn resolve_leaves_out_an_extension_running_under_a_different_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager =
            ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());
        extension_manager
            .add_mock_extension("ext_a".to_string(), Arc::new(MockClient {}))
            .await;

        let same = ExtensionSet::new("s", None, vec![builtin_config("ext_a", vec![])]).unwrap();
        assert!(extension_manager.resolve(&same).await.is_enabled("ext_a"));

        let narrower = ExtensionSet::new(
            "s",
            None,
            vec![builtin_config("ext_a", vec!["tool".to_string()])],
        )
        .unwrap();
        let lease = extension_manager.resolve(&narrower).await;
        assert!(!lease.is_enabled("ext_a"));
        assert!(lease.tools().await.is_empty());
    }

    #[test]
    fn set_rejects_the_same_extension_twice() {
        let error = ExtensionSet::new(
            "s",
            None,
            vec![
                builtin_config("Ext-A", vec![]),
                builtin_config("ext-a", vec![]),
            ],
        )
        .unwrap_err();
        assert!(error.to_string().contains("appears twice"));
    }

    #[tokio::test]
    async fn extension_manager_tools_follow_resource_support() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager = Arc::new(ExtensionManager::new_without_provider(
            temp_dir.path().to_path_buf(),
        ));
        extension_manager
            .add_extension(
                ExtensionConfig::Platform {
                    name: "extensionmanager".to_string(),
                    display_name: None,
                    description: String::new(),
                    bundled: None,
                    available_tools: vec![],
                },
                None,
                None,
                None,
            )
            .await
            .unwrap();

        let tools = extension_manager
            .get_prefixed_tools("session", Some("extensionmanager".to_string()))
            .await
            .unwrap();
        assert!(tools
            .iter()
            .all(|tool| tool.name != "extensionmanager__list_resources"));

        let resource_info = InitializeResult::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_resources()
                .build(),
        );
        extension_manager
            .add_client(
                builtin_config("resources", vec![]),
                Arc::new(MockClient {}),
                Some(resource_info),
            )
            .await;

        let tools = extension_manager
            .get_prefixed_tools("session", Some("extensionmanager".to_string()))
            .await
            .unwrap();
        assert!(tools
            .iter()
            .any(|tool| tool.name == "extensionmanager__list_resources"));

        extension_manager
            .remove_extension("resources")
            .await
            .unwrap();
        let tools = extension_manager
            .get_prefixed_tools("session", Some("extensionmanager".to_string()))
            .await
            .unwrap();
        assert!(tools
            .iter()
            .all(|tool| tool.name != "extensionmanager__list_resources"));
    }

    #[tokio::test]
    async fn extension_manager_resource_tools_use_the_calling_lease() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager = Arc::new(ExtensionManager::new_without_provider(
            temp_dir.path().to_path_buf(),
        ));
        extension_manager
            .add_extension(
                ExtensionConfig::Platform {
                    name: "extensionmanager".to_string(),
                    display_name: None,
                    description: String::new(),
                    bundled: None,
                    available_tools: vec![],
                },
                None,
                None,
                None,
            )
            .await
            .unwrap();
        let resource_info = InitializeResult::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_resources()
                .build(),
        );
        extension_manager
            .add_client(
                builtin_config("resources", vec![]),
                Arc::new(ResourceClient { label: "old" }),
                Some(resource_info.clone()),
            )
            .await;
        let lease = extension_manager.current_lease("session", None).await;
        extension_manager
            .remove_extension("resources")
            .await
            .unwrap();
        assert!(lease
            .tools()
            .await
            .iter()
            .any(|tool| tool.name == "extensionmanager__read_resource"));

        extension_manager
            .add_client(
                builtin_config("resources", vec![]),
                Arc::new(ResourceClient { label: "new" }),
                Some(resource_info),
            )
            .await;

        let listed = lease
            .call(
                CallToolRequestParams::new("extensionmanager__list_resources")
                    .with_arguments(object!({"extension_name": "resources"})),
                CallRequest::default(),
                CancellationToken::default(),
            )
            .await
            .unwrap()
            .result
            .await
            .unwrap();
        assert!(listed.content.iter().any(|content| content
            .as_text()
            .is_some_and(|text| text.text.contains("old resource"))));

        let read = lease
            .call(
                CallToolRequestParams::new("extensionmanager__read_resource").with_arguments(
                    object!({
                        "extension_name": "resources",
                        "uri": "resource://snapshot"
                    }),
                ),
                CallRequest::default(),
                CancellationToken::default(),
            )
            .await
            .unwrap()
            .result
            .await
            .unwrap();
        assert!(read.content.iter().any(|content| content
            .as_text()
            .is_some_and(|text| text.text.ends_with("\n\nold"))));
    }

    #[tokio::test]
    async fn tool_list_changed_during_fetch_prevents_stale_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager =
            ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());
        let tools_client = Arc::new(BlockingToolsClient {
            calls: AtomicUsize::new(0),
            first_fetch_started: Semaphore::new(0),
            release_first_fetch: Semaphore::new(0),
        });
        extension_manager
            .add_mock_extension("dynamic".to_string(), tools_client.clone())
            .await;
        let tools_version = extension_manager.extensions.lock().await["dynamic"]
            .tools_version
            .clone();

        let manager = Arc::new(extension_manager);
        let first_fetch = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager
                    .get_prefixed_tools("test-session", None)
                    .await
                    .unwrap()
            })
        };

        let _started = tools_client.first_fetch_started.acquire().await.unwrap();
        tools_version.fetch_add(1, Ordering::SeqCst);
        tools_client.release_first_fetch.add_permits(1);

        let stale_result = first_fetch.await.unwrap();
        assert!(stale_result.iter().any(|tool| tool.name == "dynamic__old"));

        let refreshed = manager
            .get_prefixed_tools("test-session", None)
            .await
            .unwrap();
        assert!(refreshed.iter().any(|tool| tool.name == "dynamic__new"));
        assert_eq!(tools_client.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn successful_mutation_is_persisted_when_another_mutation_fails() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = Arc::new(ExtensionManager::new_without_provider(
            temp_dir.path().to_path_buf(),
        ));
        let session = manager
            .get_context()
            .session_manager
            .create_session(
                temp_dir.path().to_path_buf(),
                "mixed-mutations".to_string(),
                crate::session::SessionType::Hidden,
                crate::config::GooseMode::default(),
            )
            .await
            .unwrap();

        let successful = manager.apply(
            ExtensionMutation::Enable {
                name: "analyze".to_string(),
            },
            Some(session.working_dir.clone()),
            None,
            &session.id,
        );
        let failed = manager.apply(
            ExtensionMutation::Enable {
                name: "missing-extension".to_string(),
            },
            Some(session.working_dir.clone()),
            None,
            &session.id,
        );
        let (successful, failed) = tokio::join!(successful, failed);

        assert!(successful.is_ok());
        assert!(failed.is_err());
        let stored_session = manager
            .get_context()
            .session_manager
            .get_session(&session.id, false)
            .await
            .unwrap();
        let stored_extensions =
            EnabledExtensionsState::from_extension_data(&stored_session.extension_data).unwrap();
        assert!(stored_extensions
            .extensions
            .iter()
            .any(|config| config.key() == "analyze"));
    }

    #[test]
    fn test_recover_mangled_tool_name() {
        let tools = [("developer__shell", None), ("platform__search", None)];
        assert_eq!(
            recover_mangled_tool_name("developer.shell", tools.iter().copied()).as_deref(),
            Some("developer__shell")
        );
        assert_eq!(
            recover_mangled_tool_name("functions.developer__shell", tools.iter().copied())
                .as_deref(),
            Some("developer__shell")
        );
        assert_eq!(
            recover_mangled_tool_name("functions.developer.shell", tools.iter().copied())
                .as_deref(),
            Some("developer__shell")
        );
        assert_eq!(
            recover_mangled_tool_name("developer shell", tools.iter().copied()),
            None
        );
        assert_eq!(
            recover_mangled_tool_name("developer__shell!", tools.iter().copied()),
            None
        );
        assert_eq!(
            recover_mangled_tool_name("nonexistent.tool", tools.iter().copied()),
            None
        );

        let dotted_tool = [("dotted__db.query", None)];
        assert_eq!(
            recover_mangled_tool_name("dotted.db.query", dotted_tool.iter().copied()).as_deref(),
            Some("dotted__db.query")
        );
    }

    #[test]
    fn test_recover_mangled_tool_name_unprefixed_extension() {
        // Platform extensions with unprefixed_tools=true (e.g. "developer")
        // advertise tools with no "__" prefix at all; the owner lives only in
        // metadata. GLM's documented "developer.shell" reproduction (#9486)
        // and emulated "developer__shell" calls must recover via the owner,
        // not the tool's own (absent) prefix.
        let tools = [("shell", Some("developer")), ("write", Some("developer"))];
        assert_eq!(
            recover_mangled_tool_name("developer.shell", tools.iter().copied()).as_deref(),
            Some("shell")
        );
        assert_eq!(
            recover_mangled_tool_name("developer__shell", tools.iter().copied()).as_deref(),
            Some("shell")
        );
        assert_eq!(
            recover_mangled_tool_name("functions.developer.shell", tools.iter().copied())
                .as_deref(),
            Some("shell")
        );

        // Wrong owner must not match.
        assert_eq!(
            recover_mangled_tool_name("other_extension.shell", tools.iter().copied()),
            None
        );

        // Ambiguity across two different unprefixed extensions that both own
        // a tool matching the same mangled input must refuse, not guess.
        let ambiguous = [("shell", Some("dev_a")), ("shell", Some("dev_b"))];
        assert_eq!(
            recover_mangled_tool_name("dev_a.shell", ambiguous.iter().copied()).as_deref(),
            Some("shell")
        );
    }

    #[test]
    fn test_recover_mangled_tool_name_non_extension_manager_tools() {
        // recipe__final_output and platform__manage_schedule are appended by
        // Agent::list_tools outside the extension manager (see #9486); they
        // use the same "__" convention, so no owner metadata is needed.
        let tools = [
            ("recipe__final_output", None),
            ("platform__manage_schedule", None),
        ];
        assert_eq!(
            recover_mangled_tool_name("recipe.final_output", tools.iter().copied()).as_deref(),
            Some("recipe__final_output")
        );
        assert_eq!(
            recover_mangled_tool_name("platform.manage_schedule", tools.iter().copied()).as_deref(),
            Some("platform__manage_schedule")
        );
    }
}
