use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::FutureExt;
use futures::Stream;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::{Mutex, OnceCell};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::container::Container;
use super::extension::{
    ExtensionConfig, ExtensionError, ExtensionResult, PlatformExtensionContext, PLATFORM_EXTENSIONS,
};
use super::tool_execution::ToolCallResult;
use crate::action_required_manager::ActionRequiredManager;
use crate::agents::mcp_client::{
    ConnectContext, GooseMcpClientCapabilities, GooseMcpHostInfo, McpClientTrait,
};
use crate::agents::provider_manager::{provider_name_for, ProviderManager};
use crate::config::extensions::name_to_key;
use crate::config::{get_extension_by_name, Config};
use crate::oauth::GooseCredentialStore;
use crate::session::{EnabledExtensionsState, Session};
use rmcp::model::{CallToolResult, ErrorCode, ErrorData, MetaObject, ServerConfig, Tool};
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
    pub(super) client: McpClientBox,
    server_info: Option<ServerConfig>,
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

struct Placement {
    working_dir: PathBuf,
    container: Option<Container>,
}

impl Placement {
    fn for_config(
        config: &ExtensionConfig,
        working_dir: Option<&Path>,
        container: Option<&Container>,
    ) -> Option<Self> {
        if is_in_process(config) {
            return None;
        }
        let working_dir = working_dir
            .map(Path::to_path_buf)
            .or_else(|| std::env::var("GOOSE_WORKING_DIR").ok().map(PathBuf::from))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        Some(Self {
            working_dir,
            container: container.cloned(),
        })
    }
}

enum Runtime {
    Running(Arc<Extension>),
    /// A platform extension the host cannot provide (no scheduler service,
    /// say) declines rather than registering with no tools.
    Declined,
    Failed(String),
}

pub(super) struct ExtensionSlot {
    key: String,
    config: ExtensionConfig,
    /// `None` for platform extensions and injected clients, which take the
    /// directory from each call.
    placement: Option<Placement>,
    scope_id: String,
    manager: Weak<ExtensionManager>,
    runtime: OnceCell<Runtime>,
}

impl ExtensionSlot {
    fn serves(&self, working_dir: Option<&Path>, container: Option<&Container>) -> bool {
        self.placement.as_ref().is_none_or(|placement| {
            working_dir.is_none_or(|working_dir| placement.working_dir == working_dir)
                && placement.container.as_ref() == container
        })
    }

    async fn runtime(&self) -> &Runtime {
        self.runtime
            .get_or_init(|| async {
                let Some(manager) = self.manager.upgrade() else {
                    return Runtime::Failed("the extension manager is gone".to_string());
                };
                match manager
                    .start(&self.config, self.placement.as_ref(), &self.scope_id)
                    .await
                {
                    Ok(Some(extension)) => Runtime::Running(Arc::new(extension)),
                    Ok(None) => Runtime::Declined,
                    Err(error) => {
                        warn!(extension = %self.key, %error, "failed to start extension");
                        Runtime::Failed(error.to_string())
                    }
                }
            })
            .await
    }

    pub(super) async fn start(&self) -> Option<&Arc<Extension>> {
        match self.runtime().await {
            Runtime::Running(extension) => Some(extension),
            Runtime::Declined | Runtime::Failed(_) => None,
        }
    }

    pub(super) async fn load_result(&self) -> ExtensionLoadResult {
        let error = match self.runtime().await {
            Runtime::Failed(error) => Some(error.clone()),
            Runtime::Running(_) | Runtime::Declined => None,
        };
        ExtensionLoadResult {
            name: self.config.name(),
            success: error.is_none(),
            error,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtensionLoadResult {
    pub name: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Extension {
    pub(super) fn supports_resources(&self) -> bool {
        self.server_info
            .as_ref()
            .and_then(|info| info.capabilities.resources.as_ref())
            .is_some()
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
        let version;
        {
            let cache = self.tools.lock().await;
            version = self.tools_version.load(Ordering::SeqCst);
            if let Some(cached) = &*cache {
                if cached.version == version && cached.scope_id == scope_id {
                    let tools = Arc::clone(&cached.tools);
                    if self.tools_version.load(Ordering::SeqCst) == version {
                        return tools;
                    }
                }
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
    scopes: Mutex<HashMap<String, IndexMap<String, Arc<ExtensionSlot>>>>,
    /// Slots `enable` has started but not yet selected. A lease resolved from
    /// the old selection in the meantime must not lose them, or the next
    /// lease would start a second process.
    enabling: Mutex<HashMap<(String, String), Arc<ExtensionSlot>>>,
    context: PlatformExtensionContext,
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

fn is_in_process(config: &ExtensionConfig) -> bool {
    matches!(
        config,
        ExtensionConfig::Platform { name, .. } | ExtensionConfig::Builtin { name, .. }
            if PLATFORM_EXTENSIONS.contains_key(name_to_key(name).as_str())
    )
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
    fn mcp_client_capabilities(&self) -> GooseMcpClientCapabilities {
        GooseMcpClientCapabilities {
            mcpui: self.capabilities.mcpui,
            host_info: self.capabilities.host_info.clone(),
            elicitation_handler: self.capabilities.elicitation_handler.clone(),
            protocol_version: self.capabilities.protocol_version.clone(),
        }
    }

    pub fn new(
        providers: Arc<ProviderManager>,
        session_manager: Arc<crate::session::SessionManager>,
        scheduler: Option<Arc<dyn crate::scheduler_trait::SchedulerTrait>>,
        client_name: String,
        capabilities: ExtensionManagerCapabilities,
        use_login_shell_path: bool,
    ) -> Self {
        Self {
            scopes: Mutex::new(HashMap::new()),
            enabling: Mutex::new(HashMap::new()),
            context: PlatformExtensionContext {
                extension_manager: None,
                providers,
                session_manager,
                scheduler,
                use_login_shell_path,
            },
            client_name,
            capabilities,
        }
    }

    pub fn with_data_dir(data_dir: std::path::PathBuf) -> Self {
        let session_manager = Arc::new(crate::session::SessionManager::new(data_dir));
        Self::new(
            Default::default(),
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

    fn hydrate_mcp_apps(&self) -> bool {
        match &self.capabilities.host_info {
            Some(host_info) if host_info.explicit_extensions => host_info.mcpui_enabled(),
            _ => self.capabilities.mcpui,
        }
    }

    fn lease(
        &self,
        scope_id: &str,
        working_dir: Option<PathBuf>,
        slots: Vec<Arc<ExtensionSlot>>,
    ) -> ExtensionLease {
        ExtensionLease::new(
            scope_id,
            working_dir,
            slots,
            self.context.session_manager.action_required(),
            self.hydrate_mcp_apps(),
        )
    }

    fn new_slot(
        self: &Arc<Self>,
        scope_id: &str,
        config: &ExtensionConfig,
        working_dir: Option<&Path>,
        container: Option<&Container>,
    ) -> ExtensionSlot {
        ExtensionSlot {
            key: config.key(),
            config: config.clone(),
            placement: Placement::for_config(config, working_dir, container),
            scope_id: scope_id.to_string(),
            manager: Arc::downgrade(self),
            runtime: OnceCell::new(),
        }
    }

    pub async fn resolve(self: &Arc<Self>, set: &ExtensionSet) -> ExtensionLease {
        let slots = {
            let mut scopes = self.scopes.lock().await;
            let enabling = self.enabling.lock().await;
            let current = scopes.remove(set.scope_id()).unwrap_or_default();
            let selected = set
                .extensions()
                .iter()
                .map(|config| {
                    let key = config.key();
                    let pending = enabling.get(&(set.scope_id().to_string(), key.clone()));
                    let slot = current
                        .get(&key)
                        .into_iter()
                        .chain(pending)
                        .find(|slot| {
                            slot.config == *config
                                && slot.serves(set.working_dir.as_deref(), set.container.as_ref())
                        })
                        .cloned()
                        .unwrap_or_else(|| {
                            Arc::new(self.new_slot(
                                set.scope_id(),
                                config,
                                set.working_dir.as_deref(),
                                set.container.as_ref(),
                            ))
                        });
                    (key, slot)
                })
                .collect::<IndexMap<_, _>>();
            let slots = selected.values().cloned().collect();
            scopes.insert(set.scope_id().to_string(), selected);
            slots
        };
        self.lease(set.scope_id(), set.working_dir.clone(), slots)
    }

    pub async fn current_lease(self: &Arc<Self>, session_id: &str) -> Result<ExtensionLease> {
        let session = self
            .context
            .session_manager
            .get_session(session_id, false)
            .await?;
        self.session_lease(&session).await
    }

    pub async fn current_session_snapshot(
        self: &Arc<Self>,
        session: &Session,
    ) -> Result<(Session, ExtensionLease)> {
        let session = self
            .context
            .session_manager
            .get_session(&session.id, session.conversation.is_some())
            .await?;
        let lease = self.session_lease(&session).await?;
        Ok((session, lease))
    }

    #[cfg(test)]
    pub(crate) async fn scope_lease(&self, scope_id: &str) -> ExtensionLease {
        let slots = self
            .scopes
            .lock()
            .await
            .get(scope_id)
            .into_iter()
            .flat_map(IndexMap::values)
            .cloned()
            .collect();
        self.lease(scope_id, None, slots)
    }

    async fn session_lease(self: &Arc<Self>, session: &Session) -> Result<ExtensionLease> {
        // A pinned provider may not be in the registry at all, so it answers
        // for itself when there is one.
        let provider_runs_tool_loop = match self.context.providers.pinned(session).await {
            Some(provider) => provider.manages_own_context(),
            None => match provider_name_for(session) {
                Ok(name) => crate::providers::get_from_registry(&name)
                    .await
                    .is_ok_and(|entry| entry.runs_own_tool_loop()),
                Err(_) => false,
            },
        };
        let extensions = selection(session)
            .into_iter()
            .filter(|config| {
                !(provider_runs_tool_loop
                    && matches!(
                        config,
                        ExtensionConfig::Stdio { .. } | ExtensionConfig::StreamableHttp { .. }
                    ))
            })
            .collect();
        let set = ExtensionSet::new(&session.id, Some(session.working_dir.clone()), extensions)?
            .with_container(session.container.clone());
        Ok(self.resolve(&set).await)
    }

    async fn start(
        self: &Arc<Self>,
        config: &ExtensionConfig,
        placement: Option<&Placement>,
        session_id: &str,
    ) -> ExtensionResult<Option<Extension>> {
        let resolved_config = config.clone().resolve(Config::global()).await?;
        let client_working_dir = match &resolved_config {
            ExtensionConfig::Stdio { cwd: Some(cwd), .. } => PathBuf::from(cwd),
            _ => placement
                .map(|placement| placement.working_dir.clone())
                .unwrap_or_default(),
        };
        let container = placement.and_then(|placement| placement.container.as_ref());

        let tools_version = Arc::new(AtomicU64::new(0));
        let ctx = |timeout: Option<u64>, working_dir: PathBuf| ConnectContext {
            timeout: Duration::from_secs(resolve_timeout(timeout)),
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
                    ctx: ctx(*timeout, client_working_dir),
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
                let Some(client) = (def.client_factory)(context) else {
                    return Ok(None);
                };
                client
            }
            ExtensionConfig::Builtin { name, timeout, .. } => {
                builtin::connect(name, container, ctx(*timeout, client_working_dir)).await?
            }
            ExtensionConfig::Platform { name, .. } => {
                builtin::connect(name, container, ctx(None, client_working_dir)).await?
            }
            ExtensionConfig::Stdio {
                cmd,
                args,
                envs,
                timeout,
                ..
            } => {
                let mut envs = envs.get_env();
                envs.insert("AGENT_SESSION_ID".to_string(), session_id.to_string());
                Box::new(
                    stdio::connect(
                        cmd,
                        args,
                        envs,
                        container,
                        ctx(*timeout, client_working_dir),
                    )
                    .await?,
                )
            }
        };

        let server_info = client.get_info().cloned();
        Ok(Some(Extension {
            key: config.key(),
            config: config.clone(),
            client: Arc::from(client),
            server_info,
            tools_version,
            tools: Mutex::new(None),
        }))
    }

    async fn session(&self, session_id: &str) -> ExtensionResult<Session> {
        self.context
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|error| ExtensionError::SetupError(error.to_string()))
    }

    async fn select(&self, session_id: &str, config: ExtensionConfig) -> ExtensionResult<()> {
        self.context
            .session_manager
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

    async fn install(&self, session_id: &str, slot: ExtensionSlot) {
        self.scopes
            .lock()
            .await
            .entry(session_id.to_string())
            .or_default()
            .insert(slot.key.clone(), Arc::new(slot));
    }

    /// Start the extension and select it for the session. It is started here
    /// rather than on the next lease so a failure reaches whoever asked, and
    /// the selection is only changed when it starts. A concurrent enable of
    /// the same config waits on the same start.
    pub async fn enable(
        self: &Arc<Self>,
        session_id: &str,
        config: ExtensionConfig,
    ) -> ExtensionResult<()> {
        let session = self.session(session_id).await?;
        let pending_key = (session_id.to_string(), config.key());
        let slot = {
            let scopes = self.scopes.lock().await;
            let mut enabling = self.enabling.lock().await;
            let selected = scopes
                .get(session_id)
                .and_then(|slots| slots.get(&pending_key.1));
            let reusable = enabling
                .get(&pending_key)
                .into_iter()
                .chain(selected)
                .find(|slot| {
                    slot.config == config
                        && slot.serves(Some(&session.working_dir), session.container.as_ref())
                        && !matches!(slot.runtime.get(), Some(Runtime::Failed(_)))
                });
            let slot = match reusable {
                Some(slot) => Arc::clone(slot),
                None => Arc::new(self.new_slot(
                    session_id,
                    &config,
                    Some(&session.working_dir),
                    session.container.as_ref(),
                )),
            };
            enabling.insert(pending_key.clone(), Arc::clone(&slot));
            slot
        };
        let result = match slot.runtime().await {
            Runtime::Failed(error) => Err(ExtensionError::StartFailed(error.clone())),
            Runtime::Running(_) | Runtime::Declined => self.select(session_id, config).await,
        };
        let mut scopes = self.scopes.lock().await;
        let mut enabling = self.enabling.lock().await;
        if enabling
            .get(&pending_key)
            .is_some_and(|pending| Arc::ptr_eq(pending, &slot))
        {
            enabling.remove(&pending_key);
        }
        if result.is_ok() {
            scopes
                .entry(pending_key.0)
                .or_default()
                .insert(pending_key.1, slot);
        }
        result
    }

    pub async fn disable(&self, session_id: &str, key: &str) -> ExtensionResult<bool> {
        self.context
            .session_manager
            .update_enabled_extensions(session_id, |extensions| {
                let selected = extensions.len();
                extensions.retain(|config| config.key() != key);
                extensions.len() != selected
            })
            .await
            .map_err(|error| ExtensionError::SetupError(error.to_string()))
    }

    pub async fn apply(
        self: &Arc<Self>,
        mutation: ExtensionMutation,
        session_id: &str,
    ) -> ExtensionResult<()> {
        match mutation {
            ExtensionMutation::Enable { name } => {
                let config = get_extension_by_name(&name).ok_or_else(|| {
                    ExtensionError::ConfigError(format!("Extension '{}' not found", name))
                })?;
                self.enable(session_id, config).await
            }
            ExtensionMutation::Disable { name } => {
                self.disable(session_id, &name_to_key(&name)).await?;
                Ok(())
            }
        }
    }

    pub fn applying_mutation(
        self: &Arc<Self>,
        result: ToolCallResult,
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
                        manager.apply(mutation, &session_id).await.map_err(|e| {
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
        session_id: &str,
        config: ExtensionConfig,
        client: McpClientBox,
        info: Option<ServerConfig>,
    ) {
        let key = config.key();
        self.install(
            session_id,
            ExtensionSlot {
                key: key.clone(),
                config: config.clone(),
                placement: None,
                scope_id: session_id.to_string(),
                manager: Weak::new(),
                runtime: OnceCell::new_with(Some(Runtime::Running(Arc::new(Extension {
                    key,
                    config: config.clone(),
                    client,
                    server_info: info,
                    tools_version: Arc::new(AtomicU64::new(0)),
                    tools: Mutex::new(None),
                })))),
            },
        )
        .await;
        // Otherwise the next lease, built from the record, drops the slot.
        if let Err(error) = self.select(session_id, config).await {
            warn!(%error, "failed to select injected extension");
        }
    }

    /// Processes stop once no lease holds them.
    pub async fn release(&self, session_id: &str) {
        self.scopes.lock().await.remove(session_id);
    }

    pub async fn get_extension_configs(&self, session_id: &str) -> Result<Vec<ExtensionConfig>> {
        let session = self
            .context
            .session_manager
            .get_session(session_id, false)
            .await?;
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

    use super::super::tool_execution::{ToolCallContext, ToolCallNotificationEmitter};
    use futures::StreamExt;
    use goose_test_support::mcp::McpFixtureServer;
    use rmcp::model::{CallToolRequestParams, GetPromptResult, Resource};
    use rmcp::ServiceExt;
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
            self.add_client("session", config, client, None).await;
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
        let extension_manager = Arc::new(ExtensionManager::with_data_dir(
            temp_dir.path().to_path_buf(),
        ));
        extension_manager
            .add_mock_extension(
                "notifications".to_string(),
                Arc::new(ContextNotificationClient),
            )
            .await;

        let tool_call = CallToolRequestParams::new("notifications__tool".to_string())
            .with_arguments(object!({}));
        let dispatched = extension_manager
            .scope_lease(&ctx.session_id)
            .await
            .call(
                tool_call,
                CallRequest::from(&ctx),
                CancellationToken::default(),
            )
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
        let extension_manager = Arc::new(ExtensionManager::with_data_dir(
            temp_dir.path().to_path_buf(),
        ));
        extension_manager
            .add_mock_extension(
                "notifications".to_string(),
                Arc::new(ContextNotificationClient),
            )
            .await;
        let (sender, mut receiver) = mpsc::channel(1);
        let ctx = ToolCallContext::new(
            "session".to_string(),
            None,
            Some("nested-request".to_string()),
        )
        .with_notification_emitter(ToolCallNotificationEmitter::new(sender));
        let tool_call = CallToolRequestParams::new("notifications__tool".to_string())
            .with_arguments(object!({}));

        let dispatched = extension_manager
            .scope_lease(&ctx.session_id)
            .await
            .call(
                tool_call,
                CallRequest::from(&ctx),
                CancellationToken::default(),
            )
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
        let extension_manager = Arc::new(ExtensionManager::with_data_dir(
            temp_dir.path().to_path_buf(),
        ));
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

        let lease = extension_manager.scope_lease("session").await;
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
            .scope_lease(&ctx.session_id)
            .await
            .call_for_app(
                CallToolRequestParams::new("ext_a__ext_b__secret".to_string()),
                other,
                CallRequest::from(&ctx),
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
    async fn resolve_replaces_a_slot_whose_config_changed() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager = Arc::new(ExtensionManager::with_data_dir(
            temp_dir.path().to_path_buf(),
        ));
        extension_manager
            .add_mock_extension("ext_a".to_string(), Arc::new(MockClient {}))
            .await;

        let same =
            ExtensionSet::new("session", None, vec![builtin_config("ext_a", vec![])]).unwrap();
        assert!(!extension_manager
            .resolve(&same)
            .await
            .tools()
            .await
            .is_empty());

        let narrower = ExtensionSet::new(
            "session",
            None,
            vec![builtin_config("ext_a", vec!["tool".to_string()])],
        )
        .unwrap();
        let lease = extension_manager.resolve(&narrower).await;
        assert!(lease.tools().await.is_empty());
        let results = lease.start().await;
        assert!(!results[0].success);
        assert!(results[0]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("Unknown extension")));
    }

    fn serve_fixture(read: tokio::io::DuplexStream, write: tokio::io::DuplexStream) {
        tokio::spawn(async move {
            let running = McpFixtureServer::new().serve((read, write)).await.unwrap();
            let _ = running.waiting().await;
        });
    }

    static COUNTED_FIXTURE_STARTS: AtomicUsize = AtomicUsize::new(0);

    fn serve_counted_fixture(read: tokio::io::DuplexStream, write: tokio::io::DuplexStream) {
        COUNTED_FIXTURE_STARTS.fetch_add(1, Ordering::SeqCst);
        serve_fixture(read, write);
    }

    static ENABLED_FIXTURE_STARTS: AtomicUsize = AtomicUsize::new(0);

    fn serve_enabled_fixture(read: tokio::io::DuplexStream, write: tokio::io::DuplexStream) {
        ENABLED_FIXTURE_STARTS.fetch_add(1, Ordering::SeqCst);
        serve_fixture(read, write);
    }

    static GATED_FIXTURE_STARTS: AtomicUsize = AtomicUsize::new(0);
    static GATED_FIXTURE_GATE: Semaphore = Semaphore::const_new(0);

    fn serve_gated_fixture(read: tokio::io::DuplexStream, write: tokio::io::DuplexStream) {
        GATED_FIXTURE_STARTS.fetch_add(1, Ordering::SeqCst);
        tokio::spawn(async move {
            GATED_FIXTURE_GATE.acquire().await.unwrap().forget();
            let running = McpFixtureServer::new().serve((read, write)).await.unwrap();
            let _ = running.waiting().await;
        });
    }

    /// A real MCP server that runs in-process, so starting it reads no
    /// secrets; an HTTP server would look up OAuth credentials in the keyring.
    fn fixture_config(
        name: &'static str,
        spawn: crate::builtin_extension::SpawnServerFn,
    ) -> ExtensionConfig {
        crate::builtin_extension::register_builtin_extension(name, spawn);
        builtin_config(name, vec![])
    }

    fn missing_stdio(name: &str, temp_dir: &Path) -> ExtensionConfig {
        ExtensionConfig::Stdio {
            name: name.to_string(),
            description: String::new(),
            cmd: temp_dir.join("missing-binary").display().to_string(),
            args: vec![],
            envs: Default::default(),
            env_keys: vec![],
            timeout: Some(5),
            cwd: None,
            bundled: None,
            available_tools: vec![],
        }
    }

    async fn session_selecting(
        manager: &ExtensionManager,
        working_dir: &Path,
        extensions: Vec<ExtensionConfig>,
    ) -> Session {
        let session_manager = &manager.get_context().session_manager;
        let session = session_manager
            .create_session(
                working_dir.to_path_buf(),
                "selection".to_string(),
                crate::session::SessionType::Hidden,
                crate::config::GooseMode::default(),
            )
            .await
            .unwrap();
        session_manager
            .update(&session.id)
            .provider_name("openai")
            .apply()
            .await
            .unwrap();
        session_manager
            .update_enabled_extensions(&session.id, |selected| *selected = extensions)
            .await
            .unwrap();
        session_manager
            .get_session(&session.id, false)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn concurrent_selection_changes_from_separate_connections_are_all_kept() {
        let temp_dir = tempfile::tempdir().unwrap();
        let first = ExtensionManager::with_data_dir(temp_dir.path().to_path_buf());
        let session = session_selecting(&first, temp_dir.path(), vec![]).await;
        let managers = (0..8)
            .map(|_| ExtensionManager::with_data_dir(temp_dir.path().to_path_buf()))
            .collect::<Vec<_>>();

        futures::future::join_all(managers.iter().enumerate().map(|(i, manager)| {
            manager.add_client(
                &session.id,
                builtin_config(&format!("ext_{i}"), vec![]),
                Arc::new(MockClient {}),
                None,
            )
        }))
        .await;

        let mut selected = first.list_extensions(&session.id).await.unwrap();
        selected.sort();
        assert_eq!(
            selected,
            (0..8).map(|i| format!("ext_{i}")).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn a_selection_with_colliding_keys_is_refused() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = ExtensionManager::with_data_dir(temp_dir.path().to_path_buf());
        let session =
            session_selecting(&manager, temp_dir.path(), vec![builtin_config("a", vec![])]).await;

        let result = manager
            .get_context()
            .session_manager
            .update_enabled_extensions(&session.id, |selected| {
                *selected = vec![builtin_config("a.b", vec![]), builtin_config("a/b", vec![])]
            })
            .await;

        assert!(result.unwrap_err().to_string().contains("appears twice"));
        assert_eq!(
            manager.list_extensions(&session.id).await.unwrap(),
            vec!["a"]
        );
    }

    #[tokio::test]
    async fn first_lease_starts_the_selection_in_the_session_record() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = Arc::new(ExtensionManager::with_data_dir(
            temp_dir.path().to_path_buf(),
        ));
        let session = session_selecting(
            &manager,
            temp_dir.path(),
            vec![fixture_config("fixture", serve_fixture)],
        )
        .await;

        let tools = manager
            .current_lease(&session.id)
            .await
            .unwrap()
            .tools()
            .await;

        assert!(tools.iter().any(|tool| tool.name == "fixture__app_card"));
    }

    #[tokio::test]
    async fn concurrent_first_leases_start_an_extension_once() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = Arc::new(ExtensionManager::with_data_dir(
            temp_dir.path().to_path_buf(),
        ));
        let session = session_selecting(
            &manager,
            temp_dir.path(),
            vec![fixture_config("counted_fixture", serve_counted_fixture)],
        )
        .await;

        let callers = 20;
        let barrier = Arc::new(tokio::sync::Barrier::new(callers));
        let starts = (0..callers).map(|_| {
            let manager = Arc::clone(&manager);
            let session_id = session.id.clone();
            let barrier = Arc::clone(&barrier);
            tokio::spawn(async move {
                barrier.wait().await;
                manager
                    .current_lease(&session_id)
                    .await
                    .unwrap()
                    .start()
                    .await
            })
        });
        for results in futures::future::join_all(starts).await {
            assert!(results.unwrap()[0].success);
        }

        assert_eq!(COUNTED_FIXTURE_STARTS.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn concurrent_enables_start_an_extension_once() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = Arc::new(ExtensionManager::with_data_dir(
            temp_dir.path().to_path_buf(),
        ));
        let session = session_selecting(&manager, temp_dir.path(), vec![]).await;
        let config = fixture_config("enabled_fixture", serve_enabled_fixture);

        let enables =
            futures::future::join_all((0..10).map(|_| manager.enable(&session.id, config.clone())))
                .await;

        assert!(enables.iter().all(Result::is_ok));
        assert_eq!(ENABLED_FIXTURE_STARTS.load(Ordering::SeqCst), 1);
        assert_eq!(
            manager.list_extensions(&session.id).await.unwrap(),
            vec!["enabled_fixture"]
        );
    }

    #[tokio::test]
    async fn a_lease_resolved_while_enabling_keeps_the_started_process() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = Arc::new(ExtensionManager::with_data_dir(
            temp_dir.path().to_path_buf(),
        ));
        let session = session_selecting(&manager, temp_dir.path(), vec![]).await;
        let config = fixture_config("gated_fixture", serve_gated_fixture);

        let enable = tokio::spawn({
            let manager = Arc::clone(&manager);
            let session_id = session.id.clone();
            async move { manager.enable(&session_id, config).await }
        });
        while GATED_FIXTURE_STARTS.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        let during = manager.current_lease(&session.id).await.unwrap();
        assert!(!during.is_enabled("gated_fixture"));
        GATED_FIXTURE_GATE.add_permits(2);
        enable.await.unwrap().unwrap();

        let after = manager.current_lease(&session.id).await.unwrap();
        assert!(after.start().await.iter().all(|result| result.success));
        assert_eq!(GATED_FIXTURE_STARTS.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn selection_changes_show_up_in_the_next_lease_only() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = Arc::new(ExtensionManager::with_data_dir(
            temp_dir.path().to_path_buf(),
        ));
        let session = session_selecting(
            &manager,
            temp_dir.path(),
            vec![fixture_config("fixture", serve_fixture)],
        )
        .await;
        let before = manager.current_lease(&session.id).await.unwrap();

        assert!(manager.disable(&session.id, "fixture").await.unwrap());
        let after = manager.current_lease(&session.id).await.unwrap();

        assert!(after.tools().await.is_empty());
        assert!(before
            .tools()
            .await
            .iter()
            .any(|tool| tool.name == "fixture__app_card"));
    }

    #[tokio::test]
    async fn a_provider_running_its_own_tool_loop_keeps_mcp_servers_out_of_the_lease() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = Arc::new(ExtensionManager::with_data_dir(
            temp_dir.path().to_path_buf(),
        ));
        let todo = ExtensionConfig::Platform {
            name: "todo".to_string(),
            display_name: None,
            description: String::new(),
            bundled: None,
            available_tools: vec![],
        };
        let session = session_selecting(
            &manager,
            temp_dir.path(),
            vec![todo, missing_stdio("stdio", temp_dir.path())],
        )
        .await;
        manager
            .get_context()
            .session_manager
            .update(&session.id)
            .provider_name("claude-code")
            .apply()
            .await
            .unwrap();

        let lease = manager.current_lease(&session.id).await.unwrap();

        assert_eq!(
            lease
                .configs()
                .iter()
                .map(ExtensionConfig::key)
                .collect::<Vec<_>>(),
            vec!["todo"]
        );
        assert!(lease.start().await.iter().all(|result| result.success));
    }

    #[tokio::test]
    async fn a_failing_extension_is_reported_and_left_out() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = Arc::new(ExtensionManager::with_data_dir(
            temp_dir.path().to_path_buf(),
        ));
        let session = session_selecting(
            &manager,
            temp_dir.path(),
            vec![
                missing_stdio("broken", temp_dir.path()),
                fixture_config("fixture", serve_fixture),
            ],
        )
        .await;

        let lease = manager.current_lease(&session.id).await.unwrap();
        let results = lease.start().await;

        assert_eq!(
            results
                .iter()
                .map(|result| (result.name.as_str(), result.success))
                .collect::<Vec<_>>(),
            vec![("broken", false), ("fixture", true)]
        );
        assert!(lease
            .tools()
            .await
            .iter()
            .all(|tool| get_tool_owner(tool).as_deref() == Some("fixture")));
    }

    #[tokio::test]
    async fn a_new_working_dir_restarts_only_extensions_that_run_in_it() {
        let temp_dir = tempfile::tempdir().unwrap();
        let new_working_dir = tempfile::tempdir().unwrap();
        let manager = Arc::new(ExtensionManager::with_data_dir(
            temp_dir.path().to_path_buf(),
        ));
        let developer = ExtensionConfig::Platform {
            name: "developer".to_string(),
            display_name: None,
            description: "developer".to_string(),
            bundled: None,
            available_tools: vec![],
        };
        let session = session_selecting(
            &manager,
            temp_dir.path(),
            vec![developer, fixture_config("fixture", serve_fixture)],
        )
        .await;
        manager
            .add_client(
                &session.id,
                builtin_config("external", vec![]),
                Arc::new(MockClient {}),
                None,
            )
            .await;
        manager
            .current_lease(&session.id)
            .await
            .unwrap()
            .start()
            .await;
        let before = manager.scopes.lock().await[&session.id].clone();

        manager
            .get_context()
            .session_manager
            .update(&session.id)
            .working_dir(new_working_dir.path().to_path_buf())
            .apply()
            .await
            .unwrap();
        let lease = manager.current_lease(&session.id).await.unwrap();

        let after = manager.scopes.lock().await[&session.id].clone();
        for key in ["developer", "external"] {
            assert!(
                Arc::ptr_eq(&before[key], &after[key]),
                "{key} was restarted"
            );
        }
        assert!(!Arc::ptr_eq(&before["fixture"], &after["fixture"]));
        assert_eq!(lease.working_dir(), Some(new_working_dir.path()));
        assert!(lease.start().await.iter().all(|result| result.success));
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
        let extension_manager = Arc::new(ExtensionManager::with_data_dir(
            temp_dir.path().to_path_buf(),
        ));
        let session = session_selecting(&extension_manager, temp_dir.path(), vec![]).await;
        extension_manager
            .enable(
                &session.id,
                ExtensionConfig::Platform {
                    name: "extensionmanager".to_string(),
                    display_name: None,
                    description: String::new(),
                    bundled: None,
                    available_tools: vec![],
                },
            )
            .await
            .unwrap();

        let tools = extension_manager
            .current_lease(&session.id)
            .await
            .unwrap()
            .tools_for("extensionmanager")
            .await;
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
                &session.id,
                builtin_config("resources", vec![]),
                Arc::new(MockClient {}),
                Some(resource_info),
            )
            .await;

        let tools = extension_manager
            .current_lease(&session.id)
            .await
            .unwrap()
            .tools_for("extensionmanager")
            .await;
        assert!(tools
            .iter()
            .any(|tool| tool.name == "extensionmanager__list_resources"));

        extension_manager
            .disable(&session.id, "resources")
            .await
            .unwrap();
        let tools = extension_manager
            .current_lease(&session.id)
            .await
            .unwrap()
            .tools_for("extensionmanager")
            .await;
        assert!(tools
            .iter()
            .all(|tool| tool.name != "extensionmanager__list_resources"));
    }

    #[tokio::test]
    async fn extension_manager_resource_tools_use_the_calling_lease() {
        let temp_dir = tempfile::tempdir().unwrap();
        let extension_manager = Arc::new(ExtensionManager::with_data_dir(
            temp_dir.path().to_path_buf(),
        ));
        let session = session_selecting(&extension_manager, temp_dir.path(), vec![]).await;
        extension_manager
            .enable(
                &session.id,
                ExtensionConfig::Platform {
                    name: "extensionmanager".to_string(),
                    display_name: None,
                    description: String::new(),
                    bundled: None,
                    available_tools: vec![],
                },
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
                &session.id,
                builtin_config("resources", vec![]),
                Arc::new(ResourceClient { label: "old" }),
                Some(resource_info.clone()),
            )
            .await;
        let lease = extension_manager.current_lease(&session.id).await.unwrap();
        extension_manager
            .disable(&session.id, "resources")
            .await
            .unwrap();
        assert!(lease
            .tools()
            .await
            .iter()
            .any(|tool| tool.name == "extensionmanager__read_resource"));

        extension_manager
            .add_client(
                &session.id,
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
        let extension_manager = Arc::new(ExtensionManager::with_data_dir(
            temp_dir.path().to_path_buf(),
        ));
        let tools_client = Arc::new(BlockingToolsClient {
            calls: AtomicUsize::new(0),
            first_fetch_started: Semaphore::new(0),
            release_first_fetch: Semaphore::new(0),
        });
        extension_manager
            .add_mock_extension("dynamic".to_string(), tools_client.clone())
            .await;
        let slot = extension_manager.scopes.lock().await["session"]["dynamic"].clone();
        let tools_version = slot.start().await.unwrap().tools_version.clone();

        let manager = extension_manager;
        let first_fetch = {
            let manager = manager.clone();
            tokio::spawn(async move { manager.scope_lease("session").await.tools().await })
        };

        let _started = tools_client.first_fetch_started.acquire().await.unwrap();
        tools_version.fetch_add(1, Ordering::SeqCst);
        tools_client.release_first_fetch.add_permits(1);

        let stale_result = first_fetch.await.unwrap();
        assert!(stale_result.iter().any(|tool| tool.name == "dynamic__old"));

        let refreshed = manager.scope_lease("session").await.tools().await;
        assert!(refreshed.iter().any(|tool| tool.name == "dynamic__new"));
        assert_eq!(tools_client.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn successful_mutation_is_persisted_when_another_mutation_fails() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = Arc::new(ExtensionManager::with_data_dir(
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
            &session.id,
        );
        let failed = manager.apply(
            ExtensionMutation::Enable {
                name: "missing-extension".to_string(),
            },
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
