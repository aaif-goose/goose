use super::{Catalog, CatalogEntry, Client, CollisionPolicy, Manager, Runtime, Selection};
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
    pub id: String,
    pub error: Option<String>,
}

pub struct Slot {
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
                self.registration
                    .connector
                    .connect(scope, cancel)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
    }
}

impl Manager<Slot> {
    /// Atomically select registrations. Existing leases continue owning the old selection.
    pub async fn resolve(
        &self,
        scope: impl Into<String>,
        registrations: Vec<Registration>,
    ) -> Result<Lease> {
        let scope = scope.into();
        let mut ids = HashSet::new();
        for registration in &registrations {
            if registration.id.is_empty() || !ids.insert(&registration.id) {
                bail!("empty or duplicate MCP server ID: {}", registration.id);
            }
        }
        let mut scopes = self.scopes.lock().await;
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
        })
    }
}

type PinnedCatalog = OnceCell<std::result::Result<Catalog<Arc<dyn Client>>, String>>;

#[derive(Clone)]
pub struct Lease {
    id: String,
    selection: Selection<Slot>,
    catalog: Arc<PinnedCatalog>,
}
impl Lease {
    /// Create the next inference binding without restarting its connections.
    /// The previous binding remains valid for its outstanding requests.
    pub fn refresh(&self) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            selection: Selection::new(self.selection.scope.clone(), self.selection.slots.clone()),
            catalog: Arc::new(OnceCell::new()),
        }
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn scope(&self) -> &str {
        &self.selection.scope
    }
    pub async fn start(&self, cancel: CancellationToken) -> Vec<LoadResult> {
        futures::future::join_all(self.selection.slots.iter().map(|slot| {
            let cancel = cancel.clone();
            async move {
                LoadResult {
                    id: slot.registration.id.clone(),
                    error: slot
                        .start(&self.selection.scope, cancel)
                        .await
                        .as_ref()
                        .err()
                        .cloned(),
                }
            }
        }))
        .await
    }

    pub async fn catalog(&self, cancel: CancellationToken) -> Result<&Catalog<Arc<dyn Client>>> {
        self.catalog
            .get_or_init(|| async {
                let mut entries = Vec::new();
                for slot in &self.selection.slots {
                    let Ok(client) = slot.start(&self.selection.scope, cancel.clone()).await else {
                        continue;
                    };
                    let tools = super::discovery::tools(
                        client.as_ref(),
                        &self.selection.scope,
                        cancel.clone(),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    for tool in tools {
                        if !slot.registration.available_tools.is_empty()
                            && !slot
                                .registration
                                .available_tools
                                .iter()
                                .any(|name| name == tool.name.as_ref())
                        {
                            continue;
                        }
                        let server_name = tool.name.to_string();
                        let tool = super::discovery::expose_tool(
                            tool,
                            &slot.registration.id,
                            slot.registration.unprefixed,
                        );
                        entries.push(CatalogEntry {
                            tool,
                            owner: Arc::clone(client),
                            server_name,
                        });
                    }
                }
                Catalog::new(entries, CollisionPolicy::Reject).map_err(|e| e.to_string())
            })
            .await
            .as_ref()
            .map_err(|error| anyhow::anyhow!("{error}"))
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
        let mut parts = Vec::new();
        for slot in &self.selection.slots {
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
    pub fn attached(id: impl Into<String>, revision: u64, client: Arc<dyn Client>) -> Self {
        Self {
            id: id.into(),
            revision,
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
