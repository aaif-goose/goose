use super::runtime::{Manager as HostManager, Runtime, Selection};
use super::{Catalog, CatalogEntry, Client, CollisionPolicy};
use anyhow::{bail, Result};
use async_trait::async_trait;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};
use std::{collections::HashSet, sync::Arc};
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait Connector: MaybeSend + MaybeSync {
    async fn connect(&self, scope: &str, cancel: CancellationToken) -> Result<Arc<dyn Client>>;
}

/// Resolved host input. Bump revision whenever credentials, placement, or capabilities change.
#[derive(Clone)]
pub struct Registration {
    pub id: String,
    pub revision: u64,
    pub connector: Arc<dyn Connector>,
    pub available_tools: Vec<String>,
    pub unprefixed: bool,
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
    Ready,
}

/// Whether a binding permits servers that failed initialization or discovery.
#[derive(Clone, Copy, Debug, Default)]
pub enum StartupPolicy {
    #[default]
    Strict,
    BestEffort,
}

struct Slot {
    registration: Registration,
    client: Runtime<std::result::Result<Arc<dyn Client>, String>>,
}
impl Slot {
    async fn start(
        &self,
        scope: &str,
        cancel: CancellationToken,
    ) -> &std::result::Result<Arc<dyn Client>, String> {
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
}

/// Scope-aware connection selection. Use `Lease::connect` for one-shot attachment.
#[derive(Default)]
pub struct Manager {
    runtime: HostManager<Slot>,
}
impl Manager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn release(&self, scope: &str) {
        self.runtime.release(scope).await;
    }

    pub async fn resolve(
        &self,
        scope: impl Into<String>,
        registrations: Vec<Registration>,
    ) -> Result<Lease> {
        self.resolve_with_policy(scope, registrations, StartupPolicy::Strict)
            .await
    }

    /// Atomically select registrations. Existing leases continue owning the old selection.
    pub async fn resolve_with_policy(
        &self,
        scope: impl Into<String>,
        registrations: Vec<Registration>,
        policy: StartupPolicy,
    ) -> Result<Lease> {
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
                    let old = &slot.registration;
                    let compatible = old.revision == registration.revision
                        && old.available_tools == registration.available_tools
                        && old.unprefixed == registration.unprefixed;
                    std::future::ready(compatible)
                },
                |registration| {
                    Arc::new(Slot {
                        registration: registration.clone(),
                        client: Runtime::new(),
                    })
                },
            )
            .await;
        Ok(Lease {
            id: uuid::Uuid::new_v4().to_string(),
            selection: Selection::new(scope, slots),
            catalog: Arc::new(OnceCell::new()),
            policy,
        })
    }
}

struct Binding {
    catalog: Catalog<Arc<dyn Client>>,
    reports: Vec<LoadResult>,
}
type PinnedCatalog = OnceCell<std::result::Result<Binding, String>>;

#[derive(Clone)]
pub struct Lease {
    id: String,
    policy: StartupPolicy,
    selection: Selection<Slot>,
    catalog: Arc<PinnedCatalog>,
}
impl Lease {
    /// Create the next inference binding without restarting its connections.
    /// The previous binding remains valid for its outstanding requests.
    pub fn new_binding(&self) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            selection: Selection::new(self.selection.scope.clone(), self.selection.slots.clone()),
            catalog: Arc::new(OnceCell::new()),
            policy: self.policy,
        }
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn scope(&self) -> &str {
        &self.selection.scope
    }
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

    /// Initialize this inference binding using its selected policy.
    pub async fn ready(&self, cancel: CancellationToken) -> Result<&[LoadResult]> {
        Ok(&self.binding(cancel).await?.reports)
    }

    pub async fn catalog(&self, cancel: CancellationToken) -> Result<&Catalog<Arc<dyn Client>>> {
        Ok(&self.binding(cancel).await?.catalog)
    }

    async fn binding(&self, cancel: CancellationToken) -> Result<&Binding> {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => bail!("MCP startup cancelled"),
            result = self.catalog.get_or_init(|| async {
                let mut entries = Vec::new();
                let mut reports = Vec::new();
                for slot in &self.selection.slots {
                    let id = &slot.registration.id;
                    let (stage, result) = match slot.start(self.scope(), cancel.clone()).await {
                        Ok(client) => (LoadStage::Discover, super::discovery::tools(client.as_ref(), self.scope(), cancel.clone()).await
                            .map(|tools| (client, tools)).map_err(|error| error.to_string())),
                        Err(error) => (LoadStage::Connect, Err(error.clone())),
                    };
                    let (client, tools) = match result {
                        Ok(value) => value,
                        Err(error) => {
                            if matches!(self.policy, StartupPolicy::Strict) {
                                return Err(format!("MCP server {id} ({stage:?}): {error}"));
                            }
                            reports.push(LoadResult { id: id.clone(), stage, error: Some(error) });
                            continue;
                        }
                    };
                    reports.push(LoadResult { id: id.clone(), stage: LoadStage::Ready, error: None });
                    for tool in tools {
                        if !slot.registration.available_tools.is_empty()
                            && !slot.registration.available_tools.iter().any(|name| name == tool.name.as_ref()) {
                            continue;
                        }
                        let server_name = tool.name.to_string();
                        let tool = super::discovery::expose_tool(tool, id, slot.registration.unprefixed);
                        entries.push(CatalogEntry { tool, owner: Arc::clone(client), server_name });
                    }
                }
                let catalog = Catalog::new(entries, CollisionPolicy::Reject).map_err(|e| e.to_string())?;
                Ok(Binding { catalog, reports })
            }) => result.as_ref().map_err(|error| anyhow::anyhow!("{error}")),
        }
    }

    pub async fn client(&self, id: &str, cancel: CancellationToken) -> Result<Arc<dyn Client>> {
        let slot = self
            .selection
            .slots
            .iter()
            .find(|slot| slot.registration.id == id)
            .ok_or_else(|| anyhow::anyhow!("unknown MCP server: {id}"))?;
        slot.start(&self.selection.scope, cancel)
            .await
            .clone()
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn instructions(&self, cancel: CancellationToken) -> Vec<(String, String)> {
        let Ok(binding) = self.binding(cancel.clone()).await else {
            return Vec::new();
        };
        let mut parts = Vec::new();
        for slot in &self.selection.slots {
            if !binding
                .reports
                .iter()
                .any(|report| report.id == slot.registration.id && report.error.is_none())
            {
                continue;
            }
            if let Ok(client) = slot.start(&self.selection.scope, cancel.clone()).await {
                if let Some(text) = client
                    .get_instructions(&self.selection.scope, std::path::Path::new(""))
                    .await
                {
                    parts.push((format!("mcp:{}", slot.registration.id), text));
                }
            }
        }
        parts
    }
}

struct Attached(Arc<dyn Client>);
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Connector for Attached {
    async fn connect(&self, _scope: &str, _cancel: CancellationToken) -> Result<Arc<dyn Client>> {
        Ok(Arc::clone(&self.0))
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
        Self {
            id: id.into(),
            revision: 0,
            connector: Arc::new(Attached(client)),
            available_tools: Vec::new(),
            unprefixed: false,
        }
    }
}

impl Lease {
    pub async fn resources(
        &self,
        id: &str,
        cancel: CancellationToken,
    ) -> Result<Vec<rmcp::model::Resource>> {
        let client = self.client(id, cancel.clone()).await?;
        Ok(super::discovery::resources(client.as_ref(), &self.selection.scope, cancel).await?)
    }

    pub async fn prompts(
        &self,
        id: &str,
        cancel: CancellationToken,
    ) -> Result<Vec<rmcp::model::Prompt>> {
        let client = self.client(id, cancel.clone()).await?;
        Ok(super::discovery::prompts(client.as_ref(), &self.selection.scope, cancel).await?)
    }
}
