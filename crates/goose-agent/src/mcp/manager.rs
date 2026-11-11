use super::runtime::{Manager as HostManager, Runtime, Selection};
use super::{Catalog, CatalogEntry, Client, CollisionPolicy};
use anyhow::{bail, Result};
use async_trait::async_trait;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};
use rmcp::model::Tool;
use std::{collections::HashSet, future::Future, sync::Arc};
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

/// A started connection. Hosts can wrap the client with their own per-connection state.
pub trait Server: MaybeSend + MaybeSync {
    type Context: MaybeSync;
    fn client(&self) -> &dyn Client<Self::Context>;
    /// Changes whenever the server's tool list changes. `None` re-lists tools for each binding.
    fn tools_version(&self) -> Option<u64> {
        None
    }
}

impl<'a, C: MaybeSync> Server for dyn Client<C> + 'a {
    type Context = C;
    fn client(&self) -> &dyn Client<C> {
        self
    }
}

/// The state of a selected connection, as seen when deciding whether to keep it.
pub enum Started<'a, T: ?Sized> {
    NotStarted,
    Running(&'a Arc<T>),
    Declined,
    Failed(&'a str),
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait Connector<T: ?Sized + Server = dyn Client>: MaybeSend + MaybeSync {
    /// `None` declines: the server is selected but contributes nothing, and is not a failure.
    async fn connect(&self, scope: &str, cancel: CancellationToken) -> Result<Option<Arc<T>>>;

    /// Called on the connector that started a slot whose registration revision still matches.
    /// Lets hosts drop connections whose inputs changed in ways a revision cannot capture,
    /// such as secrets resolved at startup.
    async fn is_current(&self, _started: Started<'_, T>) -> bool {
        true
    }
}

/// Resolved host input. `revision` decides reuse: a selected connection is kept only while
/// it compares equal, so it must change whenever credentials, placement, or capabilities do.
pub struct Registration<T: ?Sized + Server = dyn Client, R = u64> {
    pub id: String,
    pub revision: R,
    pub connector: Arc<dyn Connector<T>>,
    pub available_tools: Vec<String>,
    pub unprefixed: bool,
}

impl<T: ?Sized + Server, R: Clone> Clone for Registration<T, R> {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            revision: self.revision.clone(),
            connector: Arc::clone(&self.connector),
            available_tools: self.available_tools.clone(),
            unprefixed: self.unprefixed,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LoadResult {
    pub stage: LoadStage,
    pub id: String,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadStage {
    Connect,
    Discover,
    Declined,
    Ready,
}

/// Whether a binding permits servers that failed initialization or discovery.
#[derive(Clone, Copy, Debug, Default)]
pub enum StartupPolicy {
    #[default]
    Strict,
    BestEffort,
}

type StartResult<T> = std::result::Result<Option<Arc<T>>, String>;

struct CachedTools {
    version: u64,
    tools: Arc<Vec<Tool>>,
}

struct Slot<T: ?Sized + Server, R> {
    registration: Registration<T, R>,
    client: Runtime<StartResult<T>>,
    tools: std::sync::Mutex<Option<CachedTools>>,
}

impl<T: ?Sized + Server, R> Slot<T, R> {
    fn new(registration: Registration<T, R>) -> Self {
        Self {
            registration,
            client: Runtime::new(),
            tools: std::sync::Mutex::new(None),
        }
    }

    async fn start(&self, scope: &str, cancel: CancellationToken) -> &StartResult<T> {
        self.client
            .get_or_init(|| async {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => Err("MCP connection cancelled".to_string()),
                    result = self.registration.connector.connect(scope, cancel.clone()) =>
                        result.map_err(|e| format!("{e:#}")),
                }
            })
            .await
    }

    fn started(&self) -> Started<'_, T> {
        match self.client.get() {
            None => Started::NotStarted,
            Some(Ok(Some(server))) => Started::Running(server),
            Some(Ok(None)) => Started::Declined,
            Some(Err(error)) => Started::Failed(error),
        }
    }

    async fn is_current(&self) -> bool {
        self.registration.connector.is_current(self.started()).await
    }

    async fn load_result(&self, scope: &str, cancel: CancellationToken) -> LoadResult {
        let (stage, error) = match self.start(scope, cancel).await {
            Ok(Some(_)) => (LoadStage::Ready, None),
            Ok(None) => (LoadStage::Declined, None),
            Err(error) => (LoadStage::Connect, Some(error.clone())),
        };
        LoadResult {
            stage,
            id: self.registration.id.clone(),
            error,
        }
    }

    /// Reuses the previous listing while the server reports an unchanged tools version.
    async fn tools(
        &self,
        server: &T,
        scope: &str,
        cancel: CancellationToken,
    ) -> std::result::Result<Arc<Vec<Tool>>, rmcp::ServiceError> {
        let version = server.tools_version();
        if let Some(version) = version {
            if let Some(cached) = &*self.tools.lock().unwrap() {
                if cached.version == version {
                    return Ok(Arc::clone(&cached.tools));
                }
            }
        }
        let tools = Arc::new(super::discovery::tools(server.client(), scope, cancel).await?);
        if let Some(version) = version.filter(|version| server.tools_version() == Some(*version)) {
            *self.tools.lock().unwrap() = Some(CachedTools {
                version,
                tools: Arc::clone(&tools),
            });
        }
        Ok(tools)
    }

    async fn discover(
        &self,
        scope: &str,
        cancel: CancellationToken,
    ) -> std::result::Result<(Arc<T>, Arc<Vec<Tool>>), (LoadStage, String)> {
        match self.start(scope, cancel.clone()).await {
            Ok(Some(server)) => self
                .tools(server, scope, cancel)
                .await
                .map(|tools| (Arc::clone(server), tools))
                .map_err(|error| (LoadStage::Discover, error.to_string())),
            Ok(None) => Err((LoadStage::Declined, String::new())),
            Err(error) => Err((LoadStage::Connect, error.clone())),
        }
    }
}

fn reusable<T: ?Sized + Server, R: PartialEq>(
    slot: &Slot<T, R>,
    registration: &Registration<T, R>,
) -> bool {
    let old = &slot.registration;
    old.revision == registration.revision
        && old.available_tools == registration.available_tools
        && old.unprefixed == registration.unprefixed
}

/// Scope-aware connection selection. Use `Lease::connect` for one-shot attachment.
pub struct Manager<T: ?Sized + Server = dyn Client, R = u64> {
    runtime: HostManager<Slot<T, R>>,
}

impl<T: ?Sized + Server, R> Default for Manager<T, R> {
    fn default() -> Self {
        Self {
            runtime: HostManager::new(),
        }
    }
}

impl Manager {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T: ?Sized + Server, R: PartialEq + Clone + MaybeSend + MaybeSync> Manager<T, R> {
    pub async fn release(&self, scope: &str) {
        self.runtime.release(scope).await;
    }

    pub async fn resolve(
        &self,
        scope: impl Into<String>,
        registrations: Vec<Registration<T, R>>,
    ) -> Result<Lease<T, R>> {
        self.resolve_with_policy(scope, registrations, StartupPolicy::Strict)
            .await
    }

    /// Atomically select registrations. Existing leases continue owning the old selection.
    pub async fn resolve_with_policy(
        &self,
        scope: impl Into<String>,
        registrations: Vec<Registration<T, R>>,
        policy: StartupPolicy,
    ) -> Result<Lease<T, R>> {
        let scope = scope.into();
        let mut ids = HashSet::new();
        for registration in &registrations {
            if registration.id.trim().is_empty() || !ids.insert(&registration.id) {
                bail!("empty or duplicate MCP server ID: {}", registration.id);
            }
        }
        let mut scopes = self.runtime.scopes.lock().await;
        let slots = scopes
            .reconcile(
                &scope,
                registrations,
                |registration| registration.id.clone(),
                |slot, registration| {
                    let compatible = reusable(&slot, registration);
                    async move { compatible && slot.is_current().await }
                },
                |registration| Arc::new(Slot::new(registration.clone())),
            )
            .await;
        Ok(Lease::new(Selection::new(scope, slots), policy))
    }

    /// A lease over the scope's current selection, without changing it.
    pub async fn selected(&self, scope: &str, policy: StartupPolicy) -> Lease<T, R> {
        let slots = self
            .runtime
            .scopes
            .lock()
            .await
            .selected
            .get(scope)
            .map(|slots| slots.values().cloned().collect())
            .unwrap_or_default();
        Lease::new(Selection::new(scope, slots), policy)
    }

    /// Start one server and add it to the scope's selection if `commit` succeeds.
    ///
    /// The host persists its own selection in `commit`. A concurrent `resolve` that selects the
    /// same server reuses the in-flight start rather than spawning another one. A failed start
    /// is never reused: enabling again retries it.
    pub async fn enable<O, E, F, Fut>(
        &self,
        scope: &str,
        registration: Registration<T, R>,
        cancel: CancellationToken,
        commit: F,
    ) -> std::result::Result<O, E>
    where
        F: FnOnce(LoadResult) -> Fut,
        Fut: Future<Output = std::result::Result<O, E>>,
    {
        let pending_key = (scope.to_string(), registration.id.clone());
        let slot = {
            let mut scopes = self.runtime.scopes.lock().await;
            let selected = scopes
                .selected
                .get(scope)
                .and_then(|slots| slots.get(&registration.id))
                .cloned();
            let mut reuse = None;
            for slot in scopes
                .enabling
                .get(&pending_key)
                .cloned()
                .into_iter()
                .chain(selected)
            {
                if reusable(&slot, &registration)
                    && !matches!(slot.started(), Started::Failed(_))
                    && slot.is_current().await
                {
                    reuse = Some(slot);
                    break;
                }
            }
            let slot = reuse.unwrap_or_else(|| Arc::new(Slot::new(registration)));
            scopes
                .enabling
                .insert(pending_key.clone(), Arc::clone(&slot));
            slot
        };
        let result = commit(slot.load_result(scope, cancel).await).await;
        let mut scopes = self.runtime.scopes.lock().await;
        if scopes
            .enabling
            .get(&pending_key)
            .is_some_and(|pending| Arc::ptr_eq(pending, &slot))
        {
            scopes.enabling.remove(&pending_key);
            if result.is_ok() {
                scopes
                    .selected
                    .entry(pending_key.0)
                    .or_default()
                    .insert(pending_key.1, slot);
            }
        }
        result
    }
}

struct Binding<T: ?Sized> {
    catalog: Catalog<Arc<T>>,
    reports: Vec<LoadResult>,
}
type PinnedCatalog<T> = OnceCell<std::result::Result<Binding<T>, String>>;

pub struct Lease<T: ?Sized + Server = dyn Client, R = u64> {
    id: String,
    policy: StartupPolicy,
    collisions: CollisionPolicy,
    selection: Selection<Slot<T, R>>,
    catalog: Arc<PinnedCatalog<T>>,
}

impl<T: ?Sized + Server, R> Clone for Lease<T, R> {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            policy: self.policy,
            collisions: self.collisions,
            selection: self.selection.clone(),
            catalog: Arc::clone(&self.catalog),
        }
    }
}

impl Lease {
    /// Resolve, start and discover a one-shot selection. No manager or revision needed.
    pub async fn connect(
        scope: impl Into<String>,
        registrations: Vec<Registration>,
        policy: StartupPolicy,
        cancel: CancellationToken,
    ) -> Result<Self> {
        let lease = Manager::new()
            .resolve_with_policy(scope, registrations, policy)
            .await?;
        lease.ready(cancel).await?;
        Ok(lease)
    }
}

impl<T: ?Sized + Server, R> Lease<T, R> {
    fn new(selection: Selection<Slot<T, R>>, policy: StartupPolicy) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            policy,
            collisions: CollisionPolicy::default(),
            selection,
            catalog: Arc::new(OnceCell::new()),
        }
    }

    /// Create the next inference binding without restarting its connections.
    /// The previous binding remains valid for its outstanding requests.
    pub fn new_binding(&self) -> Self {
        Self {
            catalog: Arc::new(OnceCell::new()),
            id: uuid::Uuid::new_v4().to_string(),
            ..self.clone()
        }
    }

    /// Applies to a binding whose catalog has not been built yet.
    pub fn with_collision_policy(mut self, collisions: CollisionPolicy) -> Self {
        self.collisions = collisions;
        self.catalog = Arc::new(OnceCell::new());
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn scope(&self) -> &str {
        &self.selection.scope
    }

    /// The host's registrations for this selection, in order.
    pub fn registrations(&self) -> impl Iterator<Item = &Registration<T, R>> {
        self.selection.slots.iter().map(|slot| &slot.registration)
    }

    /// Start every selected server without discovering tools.
    pub async fn start(&self, cancel: CancellationToken) -> Vec<LoadResult> {
        futures::future::join_all(
            self.selection
                .slots
                .iter()
                .map(|slot| slot.load_result(self.scope(), cancel.clone())),
        )
        .await
    }

    /// The selected servers that started, in selection order.
    pub async fn servers(&self, cancel: CancellationToken) -> Vec<(&str, Arc<T>)> {
        futures::future::join_all(self.selection.slots.iter().map(|slot| {
            let cancel = cancel.clone();
            async move {
                match slot.start(self.scope(), cancel).await {
                    Ok(Some(server)) => Some((slot.registration.id.as_str(), Arc::clone(server))),
                    _ => None,
                }
            }
        }))
        .await
        .into_iter()
        .flatten()
        .collect()
    }

    pub async fn server(&self, id: &str, cancel: CancellationToken) -> Result<Arc<T>> {
        let slot = self
            .selection
            .slots
            .iter()
            .find(|slot| slot.registration.id == id)
            .ok_or_else(|| anyhow::anyhow!("unknown MCP server: {id}"))?;
        match slot.start(self.scope(), cancel).await {
            Ok(Some(server)) => Ok(Arc::clone(server)),
            Ok(None) => bail!("MCP server {id} declined to start"),
            Err(error) => bail!("{error}"),
        }
    }

    /// Initialize this inference binding using its selected policy.
    pub async fn ready(&self, cancel: CancellationToken) -> Result<&[LoadResult]> {
        Ok(&self.binding(cancel).await?.reports)
    }

    pub async fn catalog(&self, cancel: CancellationToken) -> Result<&Catalog<Arc<T>>> {
        Ok(&self.binding(cancel).await?.catalog)
    }

    async fn binding(&self, cancel: CancellationToken) -> Result<&Binding<T>> {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => bail!("MCP startup cancelled"),
            result = self.catalog.get_or_init(|| self.build_binding(cancel.clone())) =>
                result.as_ref().map_err(|error| anyhow::anyhow!("{error}")),
        }
    }

    async fn build_binding(
        &self,
        cancel: CancellationToken,
    ) -> std::result::Result<Binding<T>, String> {
        let discovered = futures::future::join_all(
            self.selection
                .slots
                .iter()
                .map(|slot| slot.discover(self.scope(), cancel.clone())),
        )
        .await;
        let mut entries = Vec::new();
        let mut reports = Vec::new();
        for (slot, result) in self.selection.slots.iter().zip(discovered) {
            let registration = &slot.registration;
            let id = &registration.id;
            let (server, tools) = match result {
                Ok(value) => value,
                Err((LoadStage::Declined, _)) => {
                    reports.push(LoadResult {
                        id: id.clone(),
                        stage: LoadStage::Declined,
                        error: None,
                    });
                    continue;
                }
                Err((stage, error)) => {
                    if matches!(self.policy, StartupPolicy::Strict) {
                        return Err(format!("MCP server {id} ({stage:?}): {error}"));
                    }
                    reports.push(LoadResult {
                        id: id.clone(),
                        stage,
                        error: Some(error),
                    });
                    continue;
                }
            };
            reports.push(LoadResult {
                id: id.clone(),
                stage: LoadStage::Ready,
                error: None,
            });
            for tool in tools.iter() {
                if !registration.available_tools.is_empty()
                    && !registration
                        .available_tools
                        .iter()
                        .any(|name| name == tool.name.as_ref())
                {
                    continue;
                }
                entries.push(CatalogEntry {
                    tool: super::discovery::expose_tool(tool.clone(), id, registration.unprefixed),
                    owner: Arc::clone(&server),
                    server_name: tool.name.to_string(),
                });
            }
        }
        let catalog = Catalog::new(entries, self.collisions).map_err(|e| e.to_string())?;
        Ok(Binding { catalog, reports })
    }

    pub async fn instructions(&self, cancel: CancellationToken) -> Vec<(String, String)> {
        let Ok(binding) = self.binding(cancel.clone()).await else {
            return Vec::new();
        };
        let mut parts = Vec::new();
        for slot in &self.selection.slots {
            let id = &slot.registration.id;
            if !binding
                .reports
                .iter()
                .any(|report| report.id == *id && report.stage == LoadStage::Ready)
            {
                continue;
            }
            if let Ok(Some(server)) = slot.start(self.scope(), cancel.clone()).await {
                if let Some(text) = server
                    .client()
                    .get_instructions(self.scope(), std::path::Path::new(""))
                    .await
                {
                    parts.push((format!("mcp:{id}"), text));
                }
            }
        }
        parts
    }

    pub async fn resources(
        &self,
        id: &str,
        cancel: CancellationToken,
    ) -> Result<Vec<rmcp::model::Resource>> {
        let server = self.server(id, cancel.clone()).await?;
        Ok(super::discovery::resources(server.client(), self.scope(), cancel).await?)
    }

    pub async fn prompts(
        &self,
        id: &str,
        cancel: CancellationToken,
    ) -> Result<Vec<rmcp::model::Prompt>> {
        let server = self.server(id, cancel.clone()).await?;
        Ok(super::discovery::prompts(server.client(), self.scope(), cancel).await?)
    }
}

struct Attached<T: ?Sized>(Arc<T>);
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<T: ?Sized + Server> Connector<T> for Attached<T> {
    async fn connect(&self, _scope: &str, _cancel: CancellationToken) -> Result<Option<Arc<T>>> {
        Ok(Some(Arc::clone(&self.0)))
    }
}

impl<T: ?Sized + Server + 'static, R> Registration<T, R> {
    /// An already-started server. The host keeps ownership of its lifecycle.
    pub fn started(id: impl Into<String>, revision: R, server: Arc<T>) -> Self {
        Self {
            id: id.into(),
            revision,
            connector: Arc::new(Attached(server)),
            available_tools: Vec::new(),
            unprefixed: false,
        }
    }
}

impl Registration {
    pub fn with_revision(mut self, revision: u64) -> Self {
        self.revision = revision;
        self
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "mcp-stdio"))]
    pub fn stdio(id: impl Into<String>, connector: super::StdioConnector) -> Self {
        Self::new(id, Arc::new(connector))
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "mcp-http"))]
    pub fn http(id: impl Into<String>, connector: super::HttpConnector) -> Self {
        Self::new(id, Arc::new(connector))
    }

    pub fn new(id: impl Into<String>, connector: Arc<dyn Connector>) -> Self {
        Self {
            id: id.into(),
            revision: 0,
            connector,
            available_tools: Vec::new(),
            unprefixed: false,
        }
    }

    pub fn attached(id: impl Into<String>, client: Arc<dyn Client>) -> Self {
        Self::started(id, 0, client)
    }
}
