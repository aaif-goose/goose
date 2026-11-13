use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::Stream;
use indexmap::IndexMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::task::{Context, Poll};
use tokio::sync::{Mutex, OnceCell};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::action_required_manager::ActionRequiredManager;
use crate::agents::mcp_client::{GooseMcpClientCapabilities, McpClientTrait};
use goose_agent::extension::{name_to_key, ExtensionError, ExtensionResult};
use host::ExtensionServices;
use rmcp::model::{CallToolResult, MetaObject, ServerConfig, Tool};
use serde_json::Value;

pub mod host;
mod lease;
mod registration;
mod stdio;
pub mod tool_execution;
pub use stdio::spawn as spawn_stdio;
mod streamable_http;

pub use lease::{CallRequest, ExtensionLease, ExtensionSet, LeaseId};
pub use registration::StdioCommand;
pub use registration::{ConnectRequest, Connector, Registration};

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

pub(super) struct Extension {
    pub(super) key: String,
    registration: Registration,
    resolved_config: Option<ResolvedConfig>,
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

struct ResolvedConfig {
    original: goose_agent::extension::ExtensionConfig,
    resolved: goose_agent::extension::ExtensionConfig,
}

struct CachedTools {
    scope_id: String,
    version: u64,
    tools: Arc<Vec<Tool>>,
}

enum Runtime {
    Running(Arc<Extension>),
    /// A connector can decline rather than register with no tools.
    Declined,
    Failed(String),
}

pub(super) struct ExtensionSlot {
    key: String,
    pub(super) registration: Registration,
    /// Trusted in-process connectors take the directory from each call.
    working_dir: Option<PathBuf>,
    scope_id: String,
    manager: Weak<ExtensionManager>,
    client: Arc<ClientContext>,
    config_generation: AtomicU64,
    runtime: OnceCell<Runtime>,
}

impl ExtensionSlot {
    fn reusable_for(
        &self,
        registration: &Registration,
        working_dir: Option<&Path>,
        client: &ClientContext,
    ) -> bool {
        if !self.registration.is_equivalent(registration) {
            return false;
        }
        self.working_dir.as_ref().is_none_or(|directory| {
            working_dir.is_none_or(|working_dir| directory == working_dir)
                && self.client.negotiates_like(client)
        })
    }

    /// Whether the secrets and environment the extension started with still hold. Only
    /// checked after this process wrote config, so a lease does not read the keyring.
    async fn config_is_current(&self) -> bool {
        let Some(manager) = self.manager.upgrade() else {
            return self
                .runtime
                .get()
                .is_some_and(|runtime| matches!(runtime, Runtime::Running(_)));
        };
        let generation = manager.services.environment.config_generation();
        if self.config_generation.load(Ordering::SeqCst) == generation {
            return true;
        }
        let current = match self.runtime.get() {
            None | Some(Runtime::Declined) => true,
            Some(Runtime::Failed(_)) => false,
            Some(Runtime::Running(extension)) => match &extension.resolved_config {
                Some(resolved) => manager
                    .services
                    .environment
                    .resolve_config(resolved.original.clone())
                    .await
                    .is_ok_and(|config| config == resolved.resolved),
                None => true,
            },
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
                        &self.registration,
                        self.working_dir.as_deref(),
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
            name: self.registration.name.clone(),
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

    pub(super) fn is_trusted(&self) -> bool {
        self.registration.trusted
    }

    /// The extension's tools as the model sees them: filtered by
    /// `available_tools`, prefixed unless first-class, tagged with the owner,
    /// schema-normalized.
    pub(super) async fn public_tools(&self, scope_id: &str) -> Result<Arc<Vec<Tool>>> {
        let version;
        {
            let cache = self.tools.lock().await;
            version = self.tools_version.load(Ordering::SeqCst);
            if let Some(cached) = &*cache {
                if cached.version == version && cached.scope_id == scope_id {
                    let tools = Arc::clone(&cached.tools);
                    if self.tools_version.load(Ordering::SeqCst) == version {
                        return Ok(tools);
                    }
                }
            }
        }

        let tools = Arc::new(self.fetch_public_tools(scope_id).await?);

        let mut cache = self.tools.lock().await;
        if self.tools_version.load(Ordering::SeqCst) == version {
            *cache = Some(CachedTools {
                scope_id: scope_id.to_string(),
                version,
                tools: Arc::clone(&tools),
            });
        }
        Ok(tools)
    }

    async fn fetch_public_tools(&self, session_id: &str) -> Result<Vec<Tool>> {
        if self
            .server_info
            .as_ref()
            .is_some_and(|info| info.capabilities.tools.is_none())
        {
            return Ok(Vec::new());
        }
        let cancel_token = CancellationToken::default();
        let expose_unprefixed = self.registration.unprefixed;
        let mut tools = Vec::new();
        let mut cursor = None;
        loop {
            let page = self
                .client
                .list_tools(session_id, cursor, cancel_token.clone())
                .await?;
            for mut tool in page.tools {
                if !self.registration.is_tool_available(&tool.name) {
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
                if crate::agents::tool_schema_normalize::normalize_input_schema(&mut schema) {
                    tool.input_schema = Arc::new(schema);
                }
                tools.push(tool);
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        Ok(tools)
    }
}

/// The client identity and capabilities used to negotiate with MCP servers.
#[derive(Clone)]
pub struct ClientContext {
    pub name: String,
    pub capabilities: GooseMcpClientCapabilities,
}

impl ClientContext {
    fn negotiates_like(&self, other: &ClientContext) -> bool {
        let (a, b) = (&self.capabilities, &other.capabilities);
        self.name == other.name
            && a.mcpui == b.mcpui
            && a.host_info == b.host_info
            && a.protocol_version == b.protocol_version
            && a.action_required_elicitation == b.action_required_elicitation
            && match (&a.elicitation_handler, &b.elicitation_handler) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}

/// Manages goose extensions / MCP clients and their interactions
pub struct ExtensionManager {
    scopes: Mutex<HashMap<String, IndexMap<String, Arc<ExtensionSlot>>>>,
    /// Slots `enable` has started but not yet selected. A lease resolved from
    /// the old selection in the meantime must not lose them, or the next
    /// lease would start a second process.
    enabling: Mutex<HashMap<(String, String), Arc<ExtensionSlot>>>,
    services: ExtensionServices,
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

pub fn get_tool_resource_uri(tool: &Tool) -> Option<String> {
    tool.meta
        .as_ref()
        .and_then(|meta| meta.0.get("ui"))
        .and_then(Value::as_object)
        .and_then(|ui| ui.get("resourceUri"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn remove_reserved_meta(result: &mut CallToolResult, keys: &[&str]) {
    let Some(meta) = result.meta.as_mut() else {
        return;
    };
    for key in keys {
        meta.0.remove(*key);
    }
    if meta.0.is_empty() {
        result.meta = None;
    }
}

impl ExtensionManager {
    pub fn with_services(services: ExtensionServices, default_client: ClientContext) -> Self {
        Self {
            scopes: Mutex::new(HashMap::new()),
            enabling: Mutex::new(HashMap::new()),
            services,
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
        slots: Vec<Arc<ExtensionSlot>>,
        client: &ClientContext,
    ) -> ExtensionLease {
        ExtensionLease::new(
            scope_id,
            working_dir,
            slots,
            Arc::clone(&self.services.tool_policy),
            Arc::clone(&self.services.settings.action_required),
            Arc::new(client.clone()),
        )
    }

    /// A registration for an external stdio or HTTP transport config.
    pub fn registration(&self, config: goose_agent::extension::ExtensionConfig) -> Registration {
        Registration::from_config(config, None)
    }

    fn new_slot(
        self: &Arc<Self>,
        scope_id: &str,
        registration: Registration,
        working_dir: Option<&Path>,
        client: Arc<ClientContext>,
    ) -> ExtensionSlot {
        ExtensionSlot {
            key: registration.key.clone(),
            working_dir: registration
                .placed
                .then(|| working_dir.map(Path::to_path_buf).unwrap_or_default()),
            registration,
            scope_id: scope_id.to_string(),
            manager: Arc::downgrade(self),
            client,
            config_generation: AtomicU64::new(self.services.environment.config_generation()),
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
            for registration in set.registrations() {
                let key = registration.key.clone();
                let pending = enabling
                    .get(&(set.scope_id().to_string(), key.clone()))
                    .cloned();
                let mut reusable = None;
                for slot in current.swap_remove(&key).into_iter().chain(pending) {
                    if slot.reusable_for(&registration, set.working_dir.as_deref(), &client)
                        && slot.config_is_current().await
                    {
                        reusable = Some(slot);
                        break;
                    }
                }
                let slot = reusable.unwrap_or_else(|| {
                    Arc::new(self.new_slot(
                        set.scope_id(),
                        registration,
                        set.working_dir.as_deref(),
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

    pub async fn scope_lease(&self, scope_id: &str) -> ExtensionLease {
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

    async fn start(
        self: &Arc<Self>,
        registration: &Registration,
        working_dir: Option<&Path>,
        scope_id: &str,
        client: &Arc<ClientContext>,
    ) -> ExtensionResult<Option<Extension>> {
        let request = ConnectRequest::new(
            scope_id,
            working_dir.map(Path::to_path_buf).unwrap_or_default(),
            Arc::clone(client),
            self,
        );
        let (mcp_client, resolved_config) = match &registration.connection {
            registration::Connection::Config { config, command } => {
                let resolved = self
                    .services
                    .environment
                    .resolve_config(config.clone())
                    .await?;
                let client =
                    registration::connect_config(&resolved, command.as_deref(), &request).await?;
                (
                    client,
                    Some(ResolvedConfig {
                        original: config.clone(),
                        resolved,
                    }),
                )
            }
            registration::Connection::Custom(connector) => {
                let Some(client) = connector.connect(&request).await? else {
                    return Ok(None);
                };
                (client, None)
            }
        };
        let server_info = mcp_client.get_info().cloned();
        Ok(Some(Extension {
            key: registration.key.clone(),
            registration: registration.clone(),
            client: Arc::from(mcp_client),
            server_info,
            tools_version: request.tools_version(),
            resolved_config,
            tools: Mutex::new(None),
        }))
    }

    /// Coalesce concurrent starts and keep the slot pending until the host commits
    /// its selection. Resolves must not discard a connection while it is committing.
    pub async fn enable(
        self: &Arc<Self>,
        session_id: &str,
        registration: Registration,
        working_dir: &Path,
        commit: impl std::future::Future<Output = ExtensionResult<()>>,
    ) -> ExtensionResult<()> {
        let client = self.client(session_id);
        let pending_key = (session_id.to_string(), registration.key.clone());
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
                if slot.reusable_for(&registration, Some(working_dir), &client)
                    && slot.config_is_current().await
                    && !matches!(slot.runtime.get(), Some(Runtime::Failed(_)))
                {
                    reusable = Some(slot);
                    break;
                }
            }
            let slot = reusable.unwrap_or_else(|| {
                Arc::new(self.new_slot(session_id, registration.clone(), Some(working_dir), client))
            });
            enabling.insert(pending_key.clone(), Arc::clone(&slot));
            slot
        };
        let result = match slot.runtime().await {
            Runtime::Failed(error) => Err(ExtensionError::StartFailed(error.clone())),
            Runtime::Running(_) | Runtime::Declined => commit.await,
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

    #[cfg(test)]
    pub(super) async fn install_client(
        &self,
        session_id: &str,
        registration: Registration,
        client: McpClientBox,
        info: Option<ServerConfig>,
    ) {
        let key = registration.key.clone();
        let negotiation = self.client(session_id);
        let slot = ExtensionSlot {
            key: key.clone(),
            registration: registration.clone(),
            working_dir: None,
            scope_id: session_id.to_string(),
            manager: Weak::new(),
            client: negotiation,
            config_generation: AtomicU64::new(self.services.environment.config_generation()),
            runtime: OnceCell::new_with(Some(Runtime::Running(Arc::new(Extension {
                key,
                registration,
                resolved_config: None,
                client,
                server_info: info,
                tools_version: Arc::new(AtomicU64::new(0)),
                tools: Mutex::new(None),
            })))),
        };
        self.scopes
            .lock()
            .await
            .entry(session_id.to_string())
            .or_default()
            .insert(slot.key.clone(), Arc::new(slot));
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

#[cfg(test)]
mod tests;
