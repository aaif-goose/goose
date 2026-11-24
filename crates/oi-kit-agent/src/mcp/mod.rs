//! The MCP extension runtime: scoped slots, leases, tool catalogs, dispatch and
//! transports. Everything application-specific comes from an [`ExtensionHost`].
//!
//! Acquire a lease from the current session's [`ExtensionSet`] at point of use.
//! Leases keep resources alive for one logical operation, not across inference
//! and approval. The manager owns process reuse and reconnection; session changes
//! made while approval is pending therefore apply when execution acquires its lease.

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

use crate::action_required_manager::ActionRequiredManager;
use crate::extension::ExtensionConfig as TransportConfig;
use crate::extension::{name_to_key, ExtensionError, ExtensionResult};
use crate::mcp::client::{ConnectContext, GooseMcpClientCapabilities, McpClientTrait};
pub use container::Container;
use oi_kit_provider_types::conversation::message::Message;
use rmcp::model::{CallToolResult, MetaObject, ServerConfig, Tool};
use serde_json::Value;

mod batch;
mod call;
pub mod client;
pub mod container;
mod host;
mod lease;
pub mod malware_check;
mod operation;
mod stdio;
mod streamable_http;
pub mod tool_schema_normalize;

pub use batch::{ResultNotification, ToolCallBatch};
pub use call::{ToolCallNotificationEmitter, ToolCallResult, ToolResult};
pub use host::{
    ExtensionAuth, ExtensionAuthorization, ExtensionHost, ExtensionSpec, PresentedToken,
    RejectedRequest, StaticOAuthClientConfig, StoredCredentialsError, TraceContextInjector,
};
pub use lease::{CallRequest, ExtensionLease, ExtensionSet, LeaseId};
pub use operation::{ExtensionOperation, ExtensionSource};
pub use stdio::{docker_exec, spawn as spawn_stdio};

type McpClientBox = Arc<dyn McpClientTrait>;

const TOOL_CALL_NOTIFICATION_CHANNEL_CAPACITY: usize = 32;

struct ActionRequiredStream {
    inner: ReceiverStream<Message>,
    manager: Arc<ActionRequiredManager>,
    session_id: String,
    tool_call_request_id: String,
}

impl ActionRequiredStream {
    fn new(
        receiver: tokio::sync::mpsc::Receiver<Message>,
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
    type Item = Message;

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

struct Extension<C> {
    key: String,
    config: C,
    /// The transport config with secrets substituted; `None` for host-connected configs.
    resolved_config: Option<TransportConfig>,
    client: McpClientBox,
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
    fn new(working_dir: PathBuf, container: Option<&Container>) -> Self {
        Self {
            working_dir,
            container: container.cloned(),
        }
    }
}

enum Runtime<C> {
    Running(Arc<Extension<C>>),
    Failed(String),
}

pub struct ExtensionSlot<C: ExtensionSpec> {
    key: String,
    config: C,
    placement: Option<Placement>,
    scope_id: String,
    manager: Weak<ExtensionManager<C>>,
    client: Arc<ClientContext>,
    config_generation: AtomicU64,
    runtime: OnceCell<Runtime<C>>,
}

impl<C: ExtensionSpec> ExtensionSlot<C> {
    fn reusable_for(
        &self,
        config: &C,
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
        let Some(manager) = self.manager.upgrade() else {
            return false;
        };
        let generation = manager.host.config_generation();
        if self.config_generation.load(Ordering::SeqCst) == generation {
            return true;
        }
        let current = match self.runtime.get() {
            None => true,
            Some(Runtime::Failed(_)) => false,
            Some(Runtime::Running(extension)) => {
                extension.resolved_config.is_some()
                    && manager
                        .resolve_transport(&self.config)
                        .await
                        .is_ok_and(|config| config == extension.resolved_config)
            }
        };
        if current {
            self.config_generation.store(generation, Ordering::SeqCst);
        }
        current
    }

    async fn runtime(&self) -> &Runtime<C> {
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
                    Ok(extension) => Runtime::Running(Arc::new(extension)),
                    Err(error) => {
                        warn!(extension = %self.key, %error, "failed to start extension");
                        Runtime::Failed(error.to_string())
                    }
                }
            })
            .await
    }

    async fn start(&self) -> Option<&Arc<Extension<C>>> {
        match self.runtime().await {
            Runtime::Running(extension) => Some(extension),
            Runtime::Failed(_) => None,
        }
    }

    async fn load_result(&self) -> ExtensionLoadResult {
        let error = match self.runtime().await {
            Runtime::Failed(error) => Some(error.clone()),
            Runtime::Running(_) => None,
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

impl<C: ExtensionSpec> Extension<C> {
    fn supports_resources(&self) -> bool {
        self.server_info
            .as_ref()
            .and_then(|info| info.capabilities.resources.as_ref())
            .is_some()
    }

    /// The extension's tools as the model sees them: filtered by
    /// `available_tools`, prefixed unless first-class, tagged with the owner,
    /// schema-normalized.
    async fn public_tools(&self, scope_id: &str) -> Arc<Vec<Tool>> {
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
                tool.name = format!("{}__{}", self.key, tool.name).into();
                let mut meta = tool.meta.as_ref().map(|m| m.0.clone()).unwrap_or_default();
                meta.insert(
                    TOOL_EXTENSION_META_KEY.to_string(),
                    Value::String(self.key.clone()),
                );
                tool.meta = Some(MetaObject(meta));
                let mut schema = (*tool.input_schema).clone();
                if crate::mcp::tool_schema_normalize::normalize_input_schema(&mut schema) {
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
    pub version: String,
    pub capabilities: GooseMcpClientCapabilities,
}

impl ClientContext {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            capabilities: GooseMcpClientCapabilities::default(),
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
            && self.version == other.version
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

pub const TRUSTED_TOOL_UPDATE_META_KEY: &str = "__goose_tool_update_meta";

type Slots<C> = IndexMap<String, Arc<ExtensionSlot<C>>>;
type ScopeKey = (String, String);

/// Owns MCP connections, scoped slots, and their advertised leases.
pub struct ExtensionManager<C: ExtensionSpec> {
    scopes: Mutex<HashMap<String, Slots<C>>>,
    /// Slots `enable` has started but not yet selected. A lease resolved from
    /// the old selection in the meantime must not lose them, or the next
    /// lease would start a second process.
    enabling: Mutex<HashMap<ScopeKey, Arc<ExtensionSlot<C>>>>,
    host: Arc<dyn ExtensionHost<C>>,
    action_required: Arc<ActionRequiredManager>,
    default_client: Arc<ClientContext>,
    clients: std::sync::Mutex<HashMap<String, Arc<ClientContext>>>,
}

const TOOL_EXTENSION_META_KEY: &str = "goose_extension";

pub fn get_tool_owner(tool: &Tool) -> Option<String> {
    tool.meta
        .as_ref()
        .and_then(|m| m.0.get(TOOL_EXTENSION_META_KEY))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

pub fn is_tool_owned_by_extension(tool: &Tool, extension_name: &str) -> bool {
    let expected_owner = name_to_key(extension_name);
    get_tool_owner(tool).is_some_and(|owner| name_to_key(&owner) == expected_owner)
}

/// `tools` pairs each advertised public tool name with its owning extension's
/// key, when known (`None` for tools with no owner metadata, e.g. those
/// appended outside the extension manager).
pub fn recover_mangled_tool_name<'a>(
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

pub fn get_tool_resource_uri(tool: &Tool) -> Option<String> {
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

impl<C: ExtensionSpec> ExtensionManager<C> {
    pub fn new(
        host: Arc<dyn ExtensionHost<C>>,
        action_required: Arc<ActionRequiredManager>,
        default_client: ClientContext,
    ) -> Self {
        Self {
            scopes: Mutex::new(HashMap::new()),
            enabling: Mutex::new(HashMap::new()),
            host,
            action_required,
            default_client: Arc::new(default_client),
            clients: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// The latest client to attach to a session is the one its extensions start for from
    /// then on.
    pub fn attach_client(&self, session_id: &str, client: ClientContext) {
        self.clients
            .lock()
            .expect("clients mutex poisoned")
            .insert(session_id.to_string(), Arc::new(client));
    }

    pub fn client(&self, session_id: &str) -> Arc<ClientContext> {
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
        slots: Vec<Arc<ExtensionSlot<C>>>,
        client: &ClientContext,
    ) -> ExtensionLease<C> {
        ExtensionLease::new(
            scope_id,
            working_dir,
            slots,
            Arc::clone(&self.host),
            Arc::clone(&self.action_required),
            client.hydrates_mcp_apps(),
        )
    }

    fn new_slot(
        self: &Arc<Self>,
        scope_id: &str,
        config: &C,
        working_dir: Option<&Path>,
        container: Option<&Container>,
        client: Arc<ClientContext>,
    ) -> ExtensionSlot<C> {
        ExtensionSlot {
            key: config.key(),
            config: config.clone(),
            placement: Some(Placement::new(
                working_dir
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| self.host.default_working_dir()),
                container,
            )),
            scope_id: scope_id.to_string(),
            manager: Arc::downgrade(self),
            client,
            config_generation: AtomicU64::new(self.host.config_generation()),
            runtime: OnceCell::new(),
        }
    }

    pub async fn resolve(self: &Arc<Self>, set: &ExtensionSet<C>) -> ExtensionLease<C> {
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
                    if slot.reusable_for(config, set.working_dir(), set.container(), &client)
                        && slot.config_is_current().await
                    {
                        reusable = Some(slot);
                        break;
                    }
                }
                let slot = reusable.unwrap_or_else(|| {
                    Arc::new(self.new_slot(
                        set.scope_id(),
                        config,
                        set.working_dir(),
                        set.container(),
                        Arc::clone(&client),
                    ))
                });
                selected.insert(key, slot);
            }
            let slots = selected.values().cloned().collect();
            scopes.insert(set.scope_id().to_string(), selected);
            slots
        };
        self.lease(
            set.scope_id(),
            set.working_dir().map(Path::to_path_buf),
            slots,
            &client,
        )
    }

    async fn resolve_transport(&self, config: &C) -> ExtensionResult<Option<TransportConfig>> {
        match config.transport() {
            Some(transport) => Ok(Some(
                transport
                    .clone()
                    .resolve(&host::Secrets(self.host.as_ref()))
                    .await?,
            )),
            None => Ok(None),
        }
    }

    fn timeout(&self, timeout: Option<u64>) -> Duration {
        Duration::from_secs(timeout.unwrap_or_else(|| self.host.default_timeout()))
    }

    async fn start(
        self: &Arc<Self>,
        config: &C,
        placement: Option<&Placement>,
        session_id: &str,
        client: &ClientContext,
    ) -> ExtensionResult<Extension<C>> {
        let resolved_config = self.resolve_transport(config).await?;
        let client_working_dir = match &resolved_config {
            Some(TransportConfig::Stdio { cwd: Some(cwd), .. }) => PathBuf::from(cwd),
            _ => placement
                .map(|placement| placement.working_dir.clone())
                .unwrap_or_default(),
        };
        let container = placement.and_then(|placement| placement.container.as_ref());

        let tools_version = Arc::new(AtomicU64::new(0));
        let ctx = |timeout: Option<u64>, working_dir: PathBuf| ConnectContext {
            timeout: self.timeout(timeout),
            client_name: client.name.clone(),
            client_version: client.version.clone(),
            capabilities: client.capabilities.clone(),
            working_dir,
            docker_container: None,
            action_required: Arc::clone(&self.action_required),
            tools_version: Arc::clone(&tools_version),
            trace_context: self.host.trace_context(),
        };

        let mcp_client: Box<dyn McpClientTrait> = match &resolved_config {
            Some(TransportConfig::StreamableHttp {
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
            }) => {
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
                    auth: self.host.auth(),
                    ctx: ctx(*timeout, client_working_dir),
                };
                streamable_http::connect(params, socket.as_deref()).await?
            }
            Some(TransportConfig::Stdio {
                cmd,
                args,
                envs,
                timeout,
                ..
            }) => {
                let mut envs = envs.get_env();
                envs.insert("AGENT_SESSION_ID".to_string(), session_id.to_string());
                Box::new(
                    stdio::connect(
                        self.host.as_ref(),
                        cmd,
                        args,
                        envs,
                        container,
                        ctx(*timeout, client_working_dir),
                    )
                    .await?,
                )
            }
            None => {
                self.host
                    .connect(config, container, ctx(None, client_working_dir))
                    .await?
            }
        };

        let server_info = mcp_client.get_info().cloned();
        Ok(Extension {
            key: config.key(),
            config: config.clone(),
            resolved_config,
            client: Arc::from(mcp_client),
            server_info,
            tools_version,
            tools: Mutex::new(None),
        })
    }

    /// Start the extension and keep it pending until `commit` selects it, so a
    /// failure reaches whoever asked and the selection is only changed when it
    /// starts. A concurrent enable of the same config waits on the same start.
    pub async fn enable(
        self: &Arc<Self>,
        scope_id: &str,
        config: C,
        working_dir: &Path,
        container: Option<&Container>,
        commit: impl std::future::Future<Output = ExtensionResult<()>>,
    ) -> ExtensionResult<()> {
        let client = self.client(scope_id);
        let pending_key = (scope_id.to_string(), config.key());
        let slot = {
            let scopes = self.scopes.lock().await;
            let mut enabling = self.enabling.lock().await;
            let selected = scopes
                .get(scope_id)
                .and_then(|slots| slots.get(&pending_key.1))
                .cloned();
            let mut reusable = None;
            for slot in enabling
                .get(&pending_key)
                .cloned()
                .into_iter()
                .chain(selected)
            {
                if slot.reusable_for(&config, Some(working_dir), container, &client)
                    && slot.config_is_current().await
                    && !matches!(slot.runtime.get(), Some(Runtime::Failed(_)))
                {
                    reusable = Some(slot);
                    break;
                }
            }
            let slot = reusable.unwrap_or_else(|| {
                Arc::new(self.new_slot(scope_id, &config, Some(working_dir), container, client))
            });
            enabling.insert(pending_key.clone(), Arc::clone(&slot));
            slot
        };
        let result = match slot.runtime().await {
            Runtime::Failed(error) => Err(ExtensionError::StartFailed(error.clone())),
            Runtime::Running(_) => commit.await,
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
}

#[cfg(any(test, feature = "test-support"))]
impl<C: ExtensionSpec> ExtensionManager<C> {
    pub async fn scope_lease(&self, scope_id: &str) -> ExtensionLease<C> {
        let slots = self
            .scopes
            .lock()
            .await
            .get(scope_id)
            .into_iter()
            .flat_map(IndexMap::values)
            .cloned()
            .collect();
        self.lease(scope_id, None, slots, &self.client(scope_id))
    }

    /// Select an already-running client without starting anything.
    pub async fn install_client(
        self: &Arc<Self>,
        scope_id: &str,
        config: C,
        client: Arc<dyn McpClientTrait>,
        info: Option<ServerConfig>,
    ) {
        let mut slot = self.new_slot(scope_id, &config, None, None, self.client(scope_id));
        slot.placement = None;
        slot.runtime = OnceCell::new_with(Some(Runtime::Running(Arc::new(Extension {
            key: config.key(),
            resolved_config: config.transport().cloned(),
            config,
            client,
            server_info: info,
            tools_version: Arc::new(AtomicU64::new(0)),
            tools: Mutex::new(None),
        }))));
        self.scopes
            .lock()
            .await
            .entry(scope_id.to_string())
            .or_default()
            .insert(slot.key.clone(), Arc::new(slot));
    }

    pub async fn slots(&self, scope_id: &str) -> Slots<C> {
        self.scopes
            .lock()
            .await
            .get(scope_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Hold the slot table, blocking resolves until the guard drops.
    pub async fn lock_slots(&self) -> impl Drop + '_ {
        self.scopes.lock().await
    }
}

#[cfg(any(test, feature = "test-support"))]
impl<C: ExtensionSpec> ExtensionSlot<C> {
    pub async fn tools_version(&self) -> Option<Arc<AtomicU64>> {
        self.start()
            .await
            .map(|extension| Arc::clone(&extension.tools_version))
    }
}
