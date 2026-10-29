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
use crate::agents::services::GoosePlatform;
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
    resolved_config: ExtensionConfig,
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
    /// In-process platform extensions take the directory from each call.
    placement: Option<Placement>,
    scope_id: String,
    manager: Weak<ExtensionManager>,
    client: Arc<ClientContext>,
    config_generation: AtomicU64,
    runtime: OnceCell<Runtime>,
}

impl ExtensionSlot {
    fn reusable_for(
        &self,
        config: &ExtensionConfig,
        working_dir: Option<&Path>,
        container: Option<&Container>,
        client: &ClientContext,
    ) -> bool {
        if self.config != *config {
            return false;
        }
        self.placement.as_ref().is_none_or(|placement| {
            working_dir.is_none_or(|working_dir| placement.working_dir == working_dir)
                && placement.container.as_ref() == container
                && self.client.negotiates_like(client)
        })
    }

    /// Whether the secrets and environment the extension started with still hold. Only
    /// checked after this process wrote config, so a lease does not read the keyring.
    async fn config_is_current(&self) -> bool {
        let generation = Config::global().generation();
        if self.config_generation.load(Ordering::SeqCst) == generation {
            return true;
        }
        let current = match self.runtime.get() {
            None | Some(Runtime::Declined) => true,
            Some(Runtime::Failed(_)) => false,
            Some(Runtime::Running(extension)) => self
                .config
                .clone()
                .resolve(Config::global())
                .await
                .is_ok_and(|config| config == extension.resolved_config),
        };
        if current {
            self.config_generation.store(generation, Ordering::SeqCst);
        }
        current
    }

    async fn runtime(&self) -> &Runtime {
        self.runtime
            .get_or_init(|| async {
                let Some(manager) = self.manager.upgrade() else {
                    return Runtime::Failed("the extension manager is gone".to_string());
                };
                match manager
                    .start(
                        &self.config,
                        self.placement.as_ref(),
                        &self.scope_id,
                        &self.client,
                    )
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

/// The client identity and capabilities used to negotiate with MCP servers.
#[derive(Clone)]
pub struct ClientContext {
    pub name: String,
    pub capabilities: GooseMcpClientCapabilities,
}

impl ClientContext {
    pub fn new(platform: &GoosePlatform, host_info: Option<GooseMcpHostInfo>) -> Self {
        let mcpui = host_info
            .as_ref()
            .filter(|host_info| host_info.explicit_extensions)
            .map(GooseMcpHostInfo::mcpui_enabled)
            .unwrap_or(matches!(platform, GoosePlatform::GooseDesktop));
        let name = host_info
            .as_ref()
            .and_then(|host_info| host_info.client_name.clone())
            .unwrap_or_else(|| platform.to_string());
        Self {
            name,
            capabilities: GooseMcpClientCapabilities {
                mcpui,
                host_info,
                elicitation_handler: None,
                protocol_version: None,
            },
        }
    }

    fn hydrates_mcp_apps(&self) -> bool {
        match &self.capabilities.host_info {
            Some(host_info) if host_info.explicit_extensions => host_info.mcpui_enabled(),
            _ => self.capabilities.mcpui,
        }
    }

    fn negotiates_like(&self, other: &ClientContext) -> bool {
        let (a, b) = (&self.capabilities, &other.capabilities);
        self.name == other.name
            && a.mcpui == b.mcpui
            && a.host_info == b.host_info
            && a.protocol_version == b.protocol_version
            && match (&a.elicitation_handler, &b.elicitation_handler) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
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
    default_client: Arc<ClientContext>,
    clients: std::sync::Mutex<HashMap<String, Arc<ClientContext>>>,
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

fn remove_subagent_session_meta(result: &mut CallToolResult) {
    let Some(meta) = result.meta.as_mut() else {
        return;
    };

    meta.0.remove("subagent_session_id");

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
    pub fn new(
        providers: Arc<ProviderManager>,
        session_manager: Arc<crate::session::SessionManager>,
        scheduler: Option<Arc<dyn crate::scheduler_trait::SchedulerTrait>>,
        default_client: ClientContext,
    ) -> Self {
        Self {
            scopes: Mutex::new(HashMap::new()),
            enabling: Mutex::new(HashMap::new()),
            context: PlatformExtensionContext {
                extension_manager: None,
                providers,
                session_manager,
                scheduler,
            },
            default_client: Arc::new(default_client),
            clients: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn with_data_dir(data_dir: std::path::PathBuf) -> Self {
        let session_manager = Arc::new(crate::session::SessionManager::new(data_dir));
        Self::new(
            Default::default(),
            session_manager,
            None,
            ClientContext::new(&GoosePlatform::GooseCli, None),
        )
    }

    pub fn get_context(&self) -> &PlatformExtensionContext {
        &self.context
    }

    /// The latest client to attach to a session is the one its extensions start for from
    /// then on.
    pub fn attach_client(&self, session_id: &str, client: ClientContext) {
        self.clients
            .lock()
            .expect("clients mutex poisoned")
            .insert(session_id.to_string(), Arc::new(client));
    }

    pub(crate) fn client(&self, session_id: &str) -> Arc<ClientContext> {
        self.clients
            .lock()
            .expect("clients mutex poisoned")
            .get(session_id)
            .cloned()
            .unwrap_or_else(|| Arc::clone(&self.default_client))
    }

    fn lease(
        &self,
        scope_id: &str,
        working_dir: Option<PathBuf>,
        slots: Vec<Arc<ExtensionSlot>>,
        client: &ClientContext,
    ) -> ExtensionLease {
        ExtensionLease::new(
            scope_id,
            working_dir,
            slots,
            self.context.session_manager.action_required(),
            client.hydrates_mcp_apps(),
        )
    }

    fn new_slot(
        self: &Arc<Self>,
        scope_id: &str,
        config: &ExtensionConfig,
        working_dir: Option<&Path>,
        container: Option<&Container>,
        client: Arc<ClientContext>,
    ) -> ExtensionSlot {
        ExtensionSlot {
            key: config.key(),
            config: config.clone(),
            placement: Placement::for_config(config, working_dir, container),
            scope_id: scope_id.to_string(),
            manager: Arc::downgrade(self),
            client,
            config_generation: AtomicU64::new(Config::global().generation()),
            runtime: OnceCell::new(),
        }
    }

    pub async fn resolve(self: &Arc<Self>, set: &ExtensionSet) -> ExtensionLease {
        let client = self.client(set.scope_id());
        let slots = {
            let mut scopes = self.scopes.lock().await;
            let enabling = self.enabling.lock().await;
            let mut current = scopes.remove(set.scope_id()).unwrap_or_default();
            let mut selected = IndexMap::new();
            for config in set.extensions() {
                let key = config.key();
                let pending = enabling
                    .get(&(set.scope_id().to_string(), key.clone()))
                    .cloned();
                let mut reusable = None;
                for slot in current.swap_remove(&key).into_iter().chain(pending) {
                    if slot.reusable_for(
                        config,
                        set.working_dir.as_deref(),
                        set.container.as_ref(),
                        &client,
                    ) && slot.config_is_current().await
                    {
                        reusable = Some(slot);
                        break;
                    }
                }
                let slot = reusable.unwrap_or_else(|| {
                    Arc::new(self.new_slot(
                        set.scope_id(),
                        config,
                        set.working_dir.as_deref(),
                        set.container.as_ref(),
                        Arc::clone(&client),
                    ))
                });
                selected.insert(key, slot);
            }
            let slots = selected.values().cloned().collect();
            scopes.insert(set.scope_id().to_string(), selected);
            slots
        };
        self.lease(set.scope_id(), set.working_dir.clone(), slots, &client)
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
        let client = self.client(scope_id);
        let slots = self
            .scopes
            .lock()
            .await
            .get(scope_id)
            .into_iter()
            .flat_map(IndexMap::values)
            .cloned()
            .collect();
        self.lease(scope_id, None, slots, &client)
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
        client: &ClientContext,
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
            client_name: client.name.clone(),
            capabilities: client.capabilities.clone(),
            working_dir,
            docker_container: None,
            action_required: self.context.session_manager.action_required(),
            tools_version: Arc::clone(&tools_version),
        };

        let mcp_client: Box<dyn McpClientTrait> = match &resolved_config {
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

        let server_info = mcp_client.get_info().cloned();
        Ok(Some(Extension {
            key: config.key(),
            config: config.clone(),
            resolved_config,
            client: Arc::from(mcp_client),
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
        let client = self.client(session_id);
        let pending_key = (session_id.to_string(), config.key());
        let slot = {
            let scopes = self.scopes.lock().await;
            let mut enabling = self.enabling.lock().await;
            let selected = scopes
                .get(session_id)
                .and_then(|slots| slots.get(&pending_key.1))
                .cloned();
            let mut reusable = None;
            for slot in enabling
                .get(&pending_key)
                .cloned()
                .into_iter()
                .chain(selected)
            {
                if slot.reusable_for(
                    &config,
                    Some(&session.working_dir),
                    session.container.as_ref(),
                    &client,
                ) && slot.config_is_current().await
                    && !matches!(slot.runtime.get(), Some(Runtime::Failed(_)))
                {
                    reusable = Some(slot);
                    break;
                }
            }
            let slot = reusable.unwrap_or_else(|| {
                Arc::new(self.new_slot(
                    session_id,
                    &config,
                    Some(&session.working_dir),
                    session.container.as_ref(),
                    client,
                ))
            });
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
            if result.is_ok() {
                scopes
                    .entry(pending_key.0)
                    .or_default()
                    .insert(pending_key.1, slot);
            }
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

    /// Processes stop once no lease holds them.
    pub async fn release(&self, session_id: &str) {
        let mut scopes = self.scopes.lock().await;
        let mut enabling = self.enabling.lock().await;
        scopes.remove(session_id);
        enabling.retain(|(scope_id, _), _| scope_id != session_id);
        self.clients
            .lock()
            .expect("clients mutex poisoned")
            .remove(session_id);
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
mod tests;
