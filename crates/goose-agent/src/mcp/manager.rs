use super::{Catalog, CatalogEntry, CollisionPolicy, Runtime, ScopedRuntime};
use anyhow::{bail, Result};
use async_trait::async_trait;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ErrorData, GetPromptResult, ListPromptsResult,
    ListResourcesResult, ListToolsResult, ReadResourceResult, ServerNotification,
};
use std::{collections::HashSet, sync::Arc};
use tokio::sync::{broadcast, OnceCell};
use tokio_util::sync::CancellationToken;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait Client: MaybeSend + MaybeSync {
    async fn list_tools(
        &self,
        cursor: Option<String>,
        cancel: CancellationToken,
    ) -> Result<ListToolsResult>;
    async fn call_tool(
        &self,
        request_id: &str,
        call: CallToolRequestParams,
        cancel: CancellationToken,
    ) -> Result<CallToolResult, ErrorData>;
    async fn list_resources(
        &self,
        _cursor: Option<String>,
        _cancel: CancellationToken,
    ) -> Result<ListResourcesResult> {
        bail!("resources are not supported")
    }
    async fn read_resource(
        &self,
        _uri: &str,
        _cancel: CancellationToken,
    ) -> Result<ReadResourceResult> {
        bail!("resources are not supported")
    }
    async fn list_prompts(
        &self,
        _cursor: Option<String>,
        _cancel: CancellationToken,
    ) -> Result<ListPromptsResult> {
        bail!("prompts are not supported")
    }
    async fn get_prompt(
        &self,
        _name: &str,
        _arguments: Option<rmcp::model::JsonObject>,
        _cancel: CancellationToken,
    ) -> Result<GetPromptResult> {
        bail!("prompts are not supported")
    }
    async fn instructions(&self) -> Option<String> {
        None
    }
    fn subscribe(&self) -> Option<broadcast::Receiver<ServerNotification>> {
        None
    }
}

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
                self.registration
                    .connector
                    .connect(scope, cancel)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
    }
}

pub struct Manager {
    runtime: ScopedRuntime<Slot>,
}
impl Manager {
    pub fn new() -> Self {
        Self {
            runtime: ScopedRuntime::new(),
        }
    }

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
            scope,
            slots,
            catalog: Arc::new(OnceCell::new()),
        })
    }

    pub async fn release(&self, scope: &str) {
        self.runtime.release(scope).await;
    }
}

type PinnedCatalog = OnceCell<std::result::Result<Catalog<Arc<dyn Client>>, String>>;

#[derive(Clone)]
pub struct Lease {
    id: String,
    scope: String,
    slots: Vec<Arc<Slot>>,
    catalog: Arc<PinnedCatalog>,
}
impl Lease {
    /// Create the next inference binding without restarting its connections.
    /// The previous binding remains valid for its outstanding requests.
    pub fn refresh(&self) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            scope: self.scope.clone(),
            slots: self.slots.clone(),
            catalog: Arc::new(OnceCell::new()),
        }
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn scope(&self) -> &str {
        &self.scope
    }
    pub async fn start(&self, cancel: CancellationToken) -> Vec<LoadResult> {
        futures::future::join_all(self.slots.iter().map(|slot| {
            let cancel = cancel.clone();
            async move {
                LoadResult {
                    id: slot.registration.id.clone(),
                    error: slot
                        .start(&self.scope, cancel)
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
                for slot in &self.slots {
                    let Ok(client) = slot.start(&self.scope, cancel.clone()).await else {
                        continue;
                    };
                    let mut cursor = None;
                    let mut seen = HashSet::new();
                    loop {
                        let page = client
                            .list_tools(cursor, cancel.clone())
                            .await
                            .map_err(|e| e.to_string())?;
                        for mut tool in page.tools {
                            if !slot.registration.available_tools.is_empty()
                                && !slot
                                    .registration
                                    .available_tools
                                    .iter()
                                    .any(|name| name == tool.name.as_ref())
                            {
                                continue;
                            }
                            let mut schema = (*tool.input_schema).clone();
                            if super::schema::normalize_input_schema(&mut schema) {
                                tool.input_schema = Arc::new(schema);
                            }
                            let server_name = tool.name.to_string();
                            if !slot.registration.unprefixed {
                                tool.name =
                                    format!("{}__{}", slot.registration.id, tool.name).into();
                            }
                            entries.push(CatalogEntry {
                                tool,
                                owner: Arc::clone(client),
                                server_name,
                            });
                        }
                        cursor = page.next_cursor;
                        match &cursor {
                            None => break,
                            Some(value) if !seen.insert(value.clone()) => {
                                return Err("repeated tools pagination cursor".to_string())
                            }
                            _ => {}
                        }
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
            .slots
            .iter()
            .find(|slot| slot.registration.id == id)
            .ok_or_else(|| anyhow::anyhow!("unknown MCP server: {id}"))?;
        slot.start(&self.scope, cancel)
            .await
            .clone()
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn instructions(&self, cancel: CancellationToken) -> Vec<(String, String)> {
        let mut parts = Vec::new();
        for slot in &self.slots {
            if let Ok(client) = slot.start(&self.scope, cancel.clone()).await {
                if let Some(text) = client.instructions().await {
                    parts.push((format!("mcp:{}", slot.registration.id), text));
                }
            }
        }
        parts
    }
}

impl Default for Manager {
    fn default() -> Self {
        Self::new()
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
        let mut resources = Vec::new();
        let mut cursor = None;
        let mut seen = HashSet::new();
        loop {
            let page = client.list_resources(cursor, cancel.clone()).await?;
            resources.extend(page.resources);
            cursor = page.next_cursor;
            match &cursor {
                None => break,
                Some(value) if !seen.insert(value.clone()) => {
                    bail!("repeated resources pagination cursor")
                }
                _ => {}
            }
        }
        Ok(resources)
    }
    pub async fn prompts(
        &self,
        id: &str,
        cancel: CancellationToken,
    ) -> Result<Vec<rmcp::model::Prompt>> {
        let client = self.client(id, cancel.clone()).await?;
        let mut prompts = Vec::new();
        let mut cursor = None;
        let mut seen = HashSet::new();
        loop {
            let page = client.list_prompts(cursor, cancel.clone()).await?;
            prompts.extend(page.prompts);
            cursor = page.next_cursor;
            match &cursor {
                None => break,
                Some(value) if !seen.insert(value.clone()) => {
                    bail!("repeated prompts pagination cursor")
                }
                _ => {}
            }
        }
        Ok(prompts)
    }
}
