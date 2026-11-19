//! A lease keeps the slots and tool catalog it was resolved with, so a held
//! lease is unaffected by selection changes and tool-list changes; only a newly
//! resolved lease sees them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures::stream;
use futures::{FutureExt, Stream};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ErrorCode, ErrorData, GetPromptResult,
    ListResourcesResult, ListToolsResult, Prompt, ReadResourceResult, Resource, ServerNotification,
    Tool,
};
use rmcp::service::ServiceError;
use serde_json::Value;
use tokio::sync::{mpsc, OnceCell};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::{
    get_tool_meta_value, get_tool_owner, get_tool_resource_uri, insert_trusted_tool_update_meta,
    recover_mangled_tool_name, remove_untrusted_mcp_app_meta, ActionRequiredStream, Extension,
    ExtensionHost, ExtensionLoadResult, ExtensionSlot, ExtensionSpec, GooseMcpAppToolAttachment,
    ToolCallNotificationEmitter, ToolCallResult, TOOL_CALL_NOTIFICATION_CHANNEL_CAPACITY,
};
use crate::action_required_manager::ActionRequiredManager;
use crate::extension::{name_to_key, ExtensionError, ExtensionInfo};
use crate::mcp::client::McpCallContext;
use crate::mcp::client::McpClientTrait;
use crate::mcp::container::Container;
use crate::tool_visibility::is_tool_visible_to_app;
use oi_kit_provider_types::conversation::message::Message;

#[derive(Debug)]
pub struct ExtensionSet<C> {
    scope_id: String,
    working_dir: Option<PathBuf>,
    container: Option<Container>,
    extensions: Vec<C>,
}

impl<C: ExtensionSpec> ExtensionSet<C> {
    pub fn new(
        scope_id: impl Into<String>,
        working_dir: Option<PathBuf>,
        extensions: Vec<C>,
    ) -> Result<Self, ExtensionError> {
        let mut keys = std::collections::HashSet::new();
        for config in &extensions {
            if !keys.insert(config.key()) {
                return Err(ExtensionError::ConfigError(format!(
                    "extension '{}' appears twice in the selection",
                    config.name()
                )));
            }
        }
        Ok(Self {
            scope_id: scope_id.into(),
            working_dir,
            container: None,
            extensions,
        })
    }

    pub fn with_container(mut self, container: Option<Container>) -> Self {
        self.container = container;
        self
    }

    pub fn scope_id(&self) -> &str {
        &self.scope_id
    }

    pub fn working_dir(&self) -> Option<&Path> {
        self.working_dir.as_deref()
    }

    pub fn container(&self) -> Option<&Container> {
        self.container.as_ref()
    }

    pub fn extensions(&self) -> &[C] {
        &self.extensions
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LeaseId(u64);

impl LeaseId {
    fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

struct CatalogEntry<C> {
    tool: Tool,
    extension: Arc<Extension<C>>,
    server_name: String,
}

struct ToolCatalog<C> {
    entries: Vec<CatalogEntry<C>>,
    by_name: HashMap<String, usize>,
}

impl<C: ExtensionSpec> ToolCatalog<C> {
    async fn build(scope_id: &str, extensions: &[Arc<Extension<C>>]) -> Self {
        let lists = futures::future::join_all(
            extensions
                .iter()
                .map(|extension| extension.public_tools(scope_id)),
        )
        .await;

        let mut entries = Vec::new();
        let mut by_name = HashMap::new();
        for (extension, tools) in extensions.iter().zip(lists) {
            for tool in tools.iter() {
                let name = provider_safe_tool_name(&tool.name);
                if by_name.contains_key(&name) {
                    warn!(
                        tool = %name,
                        extension = %extension.key,
                        "duplicate tool name, keeping the earlier extension's"
                    );
                    continue;
                }
                let server_name = tool
                    .name
                    .strip_prefix(&format!("{}__", extension.key))
                    .unwrap_or(&tool.name)
                    .to_string();
                let mut tool = tool.clone();
                tool.name = name.clone().into();
                by_name.insert(name, entries.len());
                entries.push(CatalogEntry {
                    tool,
                    extension: Arc::clone(extension),
                    server_name,
                });
            }
        }
        Self { entries, by_name }
    }

    fn get(&self, name: &str) -> Option<&CatalogEntry<C>> {
        self.by_name.get(name).map(|&i| &self.entries[i])
    }
}

/// MCP allows tool names that providers reject (OpenAI requires
/// `^[a-zA-Z0-9_-]{1,64}$`). A rewritten name carries a hash of the original
/// so that `db.query` and `db_query`, or two names sharing their first 64
/// characters, stay distinct tools.
fn provider_safe_tool_name(name: &str) -> String {
    let is_safe = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    if name.len() <= 64 && name.chars().all(is_safe) {
        return name.to_string();
    }
    let sanitized: String = name
        .chars()
        .map(|c| if is_safe(c) { c } else { '_' })
        .take(55)
        .collect();
    let hash = blake3::hash(name.as_bytes()).to_hex();
    format!("{sanitized}_{:.8}", hash.as_str())
}

pub struct ExtensionLease<C: ExtensionSpec> {
    id: LeaseId,
    scope_id: String,
    working_dir: Option<PathBuf>,
    slots: Vec<Arc<ExtensionSlot<C>>>,
    tool_catalog: Arc<OnceCell<ToolCatalog<C>>>,
    host: Arc<dyn ExtensionHost<C>>,
    action_required: Arc<ActionRequiredManager>,
    hydrate_mcp_apps: bool,
}

impl<C: ExtensionSpec> Clone for ExtensionLease<C> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            scope_id: self.scope_id.clone(),
            working_dir: self.working_dir.clone(),
            slots: self.slots.clone(),
            tool_catalog: Arc::clone(&self.tool_catalog),
            host: Arc::clone(&self.host),
            action_required: Arc::clone(&self.action_required),
            hydrate_mcp_apps: self.hydrate_mcp_apps,
        }
    }
}

struct ResolvedTool<'a, C> {
    extension: &'a Arc<Extension<C>>,
    server_name: &'a str,
    tool: &'a Tool,
}

impl<C: ExtensionSpec> ExtensionLease<C> {
    pub(super) fn new(
        scope_id: impl Into<String>,
        working_dir: Option<PathBuf>,
        slots: Vec<Arc<ExtensionSlot<C>>>,
        host: Arc<dyn ExtensionHost<C>>,
        action_required: Arc<ActionRequiredManager>,
        hydrate_mcp_apps: bool,
    ) -> Self {
        Self {
            id: LeaseId::next(),
            scope_id: scope_id.into(),
            working_dir,
            slots,
            tool_catalog: Arc::new(OnceCell::new()),
            host,
            action_required,
            hydrate_mcp_apps,
        }
    }

    async fn tool_catalog(&self) -> &ToolCatalog<C> {
        self.tool_catalog
            .get_or_init(|| async {
                ToolCatalog::build(&self.scope_id, &self.start_extensions().await).await
            })
            .await
    }

    async fn start_extensions(&self) -> Vec<Arc<Extension<C>>> {
        futures::future::join_all(self.slots.iter().map(|slot| slot.start()))
            .await
            .into_iter()
            .flatten()
            .cloned()
            .collect()
    }

    async fn start_extension(&self, extension_name: &str) -> Option<Arc<Extension<C>>> {
        let key = name_to_key(extension_name);
        let slot = self.slots.iter().find(|slot| slot.key == key)?;
        slot.start().await.cloned()
    }

    pub async fn start(&self) -> Vec<ExtensionLoadResult> {
        futures::future::join_all(self.slots.iter().map(|slot| slot.load_result())).await
    }

    pub fn id(&self) -> LeaseId {
        self.id
    }

    pub fn scope_id(&self) -> &str {
        &self.scope_id
    }

    pub fn working_dir(&self) -> Option<&Path> {
        self.working_dir.as_deref()
    }

    pub async fn tools(&self) -> Vec<Tool> {
        self.tool_catalog()
            .await
            .entries
            .iter()
            .map(|entry| entry.tool.clone())
            .collect()
    }

    pub fn is_enabled(&self, extension: &str) -> bool {
        let key = name_to_key(extension);
        self.slots.iter().any(|slot| slot.key == key)
    }

    pub fn configs(&self) -> Vec<C> {
        self.slots.iter().map(|slot| slot.config.clone()).collect()
    }

    pub async fn read_resource(
        &self,
        uri: &str,
        extension_name: &str,
        cancellation_token: CancellationToken,
    ) -> Result<ReadResourceResult, ErrorData> {
        let client = self
            .start_extension(extension_name)
            .await
            .map(|extension| Arc::clone(&extension.client))
            .ok_or_else(|| {
                let available = self
                    .slots
                    .iter()
                    .map(|slot| slot.key.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!(
                        "Extension '{extension_name}' not found. Here are the available extensions: {available}"
                    ),
                    None,
                )
            })?;

        client
            .read_resource(&self.scope_id, uri, cancellation_token)
            .await
            .map_err(|_| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Could not read resource with uri: {uri}"),
                    None,
                )
            })
    }

    async fn client(&self, extension_name: &str) -> Result<Arc<dyn McpClientTrait>, ErrorData> {
        self.start_extension(extension_name)
            .await
            .map(|extension| Arc::clone(&extension.client))
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("Extension {extension_name} is not valid"),
                    None,
                )
            })
    }

    pub async fn list_tools_from_extension(
        &self,
        extension_name: &str,
        cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, ErrorData> {
        self.client(extension_name)
            .await?
            .list_tools(&self.scope_id, None, cancellation_token)
            .await
            .map_err(|error| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Unable to list tools for {extension_name}, {error:?}"),
                    None,
                )
            })
    }

    pub async fn list_prompts_from_extension(
        &self,
        extension_name: &str,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<Prompt>, ErrorData> {
        self.client(extension_name)
            .await?
            .list_prompts(&self.scope_id, None, cancellation_token)
            .await
            .map(|result| result.prompts)
            .map_err(|error| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Unable to list prompts for {extension_name}, {error:?}"),
                    None,
                )
            })
    }

    pub async fn list_prompts(
        &self,
        cancellation_token: CancellationToken,
    ) -> HashMap<String, Vec<Prompt>> {
        let running = self.start_extensions().await;
        let results = futures::future::join_all(running.iter().map(|extension| {
            let token = cancellation_token.clone();
            async move {
                (
                    extension.key.clone(),
                    self.list_prompts_from_extension(&extension.key, token)
                        .await,
                )
            }
        }))
        .await;
        let mut prompts = HashMap::new();
        for (key, result) in results {
            match result {
                Ok(listed) => {
                    prompts.insert(key, listed);
                }
                Err(error) => tracing::debug!(?error, "failed to list prompts"),
            }
        }
        prompts
    }

    pub async fn get_prompt(
        &self,
        extension_name: &str,
        name: &str,
        arguments: Value,
        cancellation_token: CancellationToken,
    ) -> anyhow::Result<GetPromptResult> {
        self.client(extension_name)
            .await?
            .get_prompt(&self.scope_id, name, arguments, cancellation_token)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get prompt: {}", e))
    }

    pub async fn ui_resources(&self) -> Vec<(String, Resource)> {
        let mut ui_resources = Vec::new();
        for extension in self.start_extensions().await {
            match extension
                .client
                .list_resources(&self.scope_id, None, CancellationToken::default())
                .await
            {
                Ok(listed) => ui_resources.extend(
                    listed
                        .resources
                        .into_iter()
                        .filter(|resource| resource.uri.starts_with("ui://"))
                        .map(|resource| (extension.key.clone(), resource)),
                ),
                Err(error) => warn!(extension = %extension.key, ?error, "failed to list resources"),
            }
        }
        ui_resources
    }

    pub async fn list_resources_result_from_extension(
        &self,
        extension_name: &str,
        cancellation_token: CancellationToken,
    ) -> Result<ListResourcesResult, ErrorData> {
        self.client(extension_name)
            .await?
            .list_resources(&self.scope_id, None, cancellation_token)
            .await
            .map_err(|error| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Unable to list resources for {extension_name}, {error:?}"),
                    None,
                )
            })
    }

    pub async fn instructions(&self) -> Vec<ExtensionInfo> {
        let working_dir = self
            .working_dir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let placeholder = working_dir.to_string_lossy();
        let mut infos = Vec::new();
        for extension in self.start_extensions().await {
            let instructions = extension
                .server_info
                .as_ref()
                .and_then(|info| info.instructions.clone())
                .unwrap_or_default();
            infos.push(ExtensionInfo::new(
                &extension.key,
                &instructions.replace("{{WORKING_DIR}}", &placeholder),
                extension.supports_resources(),
            ));
        }
        infos
    }

    async fn resolve_tool(
        &self,
        tool_name: &str,
        calling_app: Option<&str>,
    ) -> Result<ResolvedTool<'_, C>, ErrorData> {
        let catalog = self.tool_catalog().await;
        let extension_key = calling_app.map(name_to_key);
        let belongs_to_extension = |entry: &&CatalogEntry<C>| {
            extension_key
                .as_ref()
                .is_none_or(|key| entry.extension.key == *key)
        };
        let entry = catalog
            .get(tool_name)
            .filter(belongs_to_extension)
            .or_else(|| {
                let tool_owners = catalog
                    .entries
                    .iter()
                    .filter(belongs_to_extension)
                    .map(|entry| (entry.tool.name.as_ref(), get_tool_owner(&entry.tool)))
                    .collect::<Vec<_>>();
                recover_mangled_tool_name(
                    tool_name,
                    tool_owners
                        .iter()
                        .map(|(name, owner)| (*name, owner.as_deref())),
                )
                .and_then(|recovered| catalog.get(&recovered))
            });

        let Some(entry) = entry else {
            let available = catalog
                .entries
                .iter()
                .filter(belongs_to_extension)
                .map(|entry| entry.tool.name.as_ref())
                .collect::<Vec<&str>>()
                .join(", ");
            return Err(ErrorData::new(
                ErrorCode::RESOURCE_NOT_FOUND,
                format!(
                    "Tool '{}' not found. Available tools: [{}]",
                    tool_name, available
                ),
                None,
            ));
        };

        if calling_app.is_some() && !is_tool_visible_to_app(&entry.tool) {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "Tool is not visible to app clients",
                None,
            ));
        }

        Ok(ResolvedTool {
            extension: &entry.extension,
            server_name: &entry.server_name,
            tool: &entry.tool,
        })
    }

    pub async fn call(
        &self,
        tool_call: CallToolRequestParams,
        request: CallRequest,
        cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ErrorData> {
        let resolved = self.resolve_tool(&tool_call.name, None).await?;
        Ok(self
            .call_resolved(resolved, tool_call.arguments, request, cancellation_token)
            .await)
    }

    pub async fn call_for_app(
        &self,
        tool_call: CallToolRequestParams,
        app_extension: &str,
        request: CallRequest,
        cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ErrorData> {
        let resolved = self
            .resolve_tool(&tool_call.name, Some(app_extension))
            .await?;
        Ok(self
            .call_resolved(resolved, tool_call.arguments, request, cancellation_token)
            .await)
    }

    async fn call_resolved(
        &self,
        resolved: ResolvedTool<'_, C>,
        arguments: Option<rmcp::model::JsonObject>,
        request: CallRequest,
        cancellation_token: CancellationToken,
    ) -> ToolCallResult {
        let CallRequest {
            tool_call_id,
            notification_emitter,
            container: _,
        } = request;
        let client = resolved.extension.client.clone();
        let action_required_stream = self.action_required_stream(tool_call_id.as_deref()).await;
        let (emitter, notification_stream) = Self::notifications_for_call(
            client.subscribe().await,
            notification_emitter,
            tool_call_id.as_deref(),
        );
        let call_context = McpCallContext::new(
            self.scope_id.clone(),
            self.working_dir.clone(),
            tool_call_id,
        );
        let call_context = match emitter {
            Some(emitter) => call_context.with_notification_emitter(emitter),
            None => call_context,
        };

        let mcp_app_call = McpAppCall::from_resolved(&resolved, self.hydrate_mcp_apps);
        let server_name = resolved.server_name.to_string();
        let session_id = self.scope_id.clone();
        let host = Arc::clone(&self.host);
        let result = async move {
            let mut result = client
                .call_tool(
                    &call_context,
                    &server_name,
                    arguments,
                    cancellation_token.clone(),
                )
                .await
                .map_err(|e| match e {
                    ServiceError::McpError(error_data) => error_data,
                    _ => ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None),
                })?;
            remove_untrusted_mcp_app_meta(&mut result);
            host.sanitize_result(&mut result);
            if let Some(mcp_app_call) = mcp_app_call {
                mcp_app_call
                    .attach_resource(&*client, &session_id, &mut result, cancellation_token)
                    .await;
            }
            Ok(result)
        };

        ToolCallResult {
            result: Box::new(result.boxed()),
            notification_stream: Some(notification_stream),
            action_required_stream,
        }
    }

    async fn action_required_stream(
        &self,
        tool_call_id: Option<&str>,
    ) -> Option<Box<dyn Stream<Item = Message> + Send + Unpin>> {
        let tool_call_id = tool_call_id?;
        if self
            .action_required
            .has_action_required_stream(&self.scope_id, tool_call_id)
            .await
        {
            return None;
        }
        let receiver = self
            .action_required
            .register_action_required_stream(self.scope_id.clone(), tool_call_id.to_string())
            .await;
        Some(Box::new(ActionRequiredStream::new(
            receiver,
            self.action_required.clone(),
            self.scope_id.clone(),
            tool_call_id.to_string(),
        )))
    }

    fn notifications_for_call(
        client_notifications: mpsc::Receiver<ServerNotification>,
        emitter: Option<ToolCallNotificationEmitter>,
        tool_call_id: Option<&str>,
    ) -> (
        Option<ToolCallNotificationEmitter>,
        Box<dyn Stream<Item = ServerNotification> + Send + Unpin>,
    ) {
        if emitter.is_some() || tool_call_id.is_none() {
            return (emitter, Box::new(ReceiverStream::new(client_notifications)));
        }
        let (sender, receiver) = mpsc::channel(TOOL_CALL_NOTIFICATION_CHANNEL_CAPACITY);
        (
            Some(ToolCallNotificationEmitter::new(sender)),
            Box::new(stream::select(
                ReceiverStream::new(client_notifications),
                ReceiverStream::new(receiver),
            )),
        )
    }
}

#[derive(Default)]
pub struct CallRequest {
    pub tool_call_id: Option<String>,
    pub notification_emitter: Option<ToolCallNotificationEmitter>,
    pub container: Option<Container>,
}

impl CallRequest {
    pub fn new(tool_call_id: impl Into<String>) -> Self {
        Self {
            tool_call_id: Some(tool_call_id.into()),
            ..Default::default()
        }
    }

    pub fn with_container(mut self, container: Option<Container>) -> Self {
        self.container = container;
        self
    }
}

struct McpAppCall {
    server_tool_name: String,
    extension_key: String,
    resource_uri: String,
    tool_meta: Option<serde_json::Value>,
}

impl McpAppCall {
    fn from_resolved<C>(resolved: &ResolvedTool<'_, C>, host_supports_apps: bool) -> Option<Self> {
        if !host_supports_apps {
            return None;
        }
        Some(Self {
            server_tool_name: resolved.server_name.to_string(),
            extension_key: resolved.extension.key.clone(),
            resource_uri: get_tool_resource_uri(resolved.tool)?,
            tool_meta: get_tool_meta_value(resolved.tool),
        })
    }

    async fn attach_resource(
        self,
        client: &dyn McpClientTrait,
        session_id: &str,
        result: &mut CallToolResult,
        cancellation_token: CancellationToken,
    ) {
        if result.is_error == Some(true) {
            return;
        }
        let mut attachment = GooseMcpAppToolAttachment {
            tool_name: self.server_tool_name,
            tool_name_is_actual: true,
            extension_name: self.extension_key,
            resource_uri: self.resource_uri.clone(),
            tool_meta: self.tool_meta,
            resource_result: None,
            read_error: None,
        };
        match client
            .read_resource(session_id, &self.resource_uri, cancellation_token)
            .await
        {
            Ok(resource_result) => {
                attachment.resource_result = serde_json::to_value(&resource_result).ok();
            }
            Err(error) => attachment.read_error = Some(error.to_string()),
        }
        insert_trusted_tool_update_meta(result, &attachment);
    }
}

impl From<&McpCallContext> for CallRequest {
    fn from(context: &McpCallContext) -> Self {
        Self {
            tool_call_id: context.tool_call_request_id.clone(),
            notification_emitter: context.notification_emitter().cloned(),
            container: None,
        }
    }
}
