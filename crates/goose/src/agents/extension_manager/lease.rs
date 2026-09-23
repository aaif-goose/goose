//! An `ExtensionSet` describes the extensions requested by one scope and the
//! working directory they share. Resolving a set snapshots matching running
//! extensions into an `ExtensionLease`. The lease builds its public tool
//! catalog on first use and keeps both its extensions and catalog stable, so an
//! existing lease survives manager changes while a newly resolved lease sees
//! replacements, removals, and tool-list changes. Tool calls, resource
//! operations, and extension prompt context use the lease's snapshot. Calls
//! also use its scope and working directory and carry their notification and
//! action-required streams with them.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures::stream;
use futures::{FutureExt, Stream};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ErrorCode, ErrorData, ListResourcesResult,
    ReadResourceResult, ResourceContents, ServerNotification, Tool,
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
    ExtensionMutation, GooseMcpAppToolAttachment, TOOL_CALL_NOTIFICATION_CHANNEL_CAPACITY,
};
use crate::action_required_manager::ActionRequiredManager;
use crate::agents::container::Container;
use crate::agents::extension::{ExtensionConfig, ExtensionError, ExtensionInfo};
use crate::agents::mcp_client::McpClientTrait;
use crate::agents::reply_parts::is_tool_visible_to_app;
use crate::agents::tool_execution::{ToolCallContext, ToolCallNotificationEmitter, ToolCallResult};
use crate::config::extensions::name_to_key;
use crate::conversation::message::Message;

fn require_str_parameter<'a>(value: &'a Value, name: &str) -> Result<&'a str, ErrorData> {
    let value = value.get(name).ok_or_else(|| {
        ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!("The parameter {name} is required"),
            None,
        )
    })?;
    value.as_str().ok_or_else(|| {
        ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            format!("The parameter {name} must be a string"),
            None,
        )
    })
}

#[derive(Debug)]
pub struct ExtensionSet {
    scope_id: String,
    working_dir: Option<PathBuf>,
    extensions: Vec<ExtensionConfig>,
}

impl ExtensionSet {
    pub fn new(
        scope_id: impl Into<String>,
        working_dir: Option<PathBuf>,
        extensions: Vec<ExtensionConfig>,
    ) -> Result<Self, ExtensionError> {
        let mut seen = std::collections::HashSet::new();
        for config in &extensions {
            if !seen.insert(config.key()) {
                return Err(ExtensionError::ConfigError(format!(
                    "extension '{}' appears twice in the set",
                    config.name()
                )));
            }
        }
        Ok(Self {
            scope_id: scope_id.into(),
            working_dir,
            extensions,
        })
    }

    pub fn scope_id(&self) -> &str {
        &self.scope_id
    }

    pub fn extensions(&self) -> &[ExtensionConfig] {
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

struct CatalogEntry {
    tool: Tool,
    extension: Arc<Extension>,
    server_name: String,
}

struct ToolCatalog {
    entries: Vec<CatalogEntry>,
    by_name: HashMap<String, usize>,
}

impl ToolCatalog {
    async fn build(scope_id: &str, extensions: &[Arc<Extension>]) -> Self {
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
                let name = tool.name.to_string();
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
                by_name.insert(name, entries.len());
                entries.push(CatalogEntry {
                    tool: tool.clone(),
                    extension: Arc::clone(extension),
                    server_name,
                });
            }
        }
        Self { entries, by_name }
    }

    fn get(&self, name: &str) -> Option<&CatalogEntry> {
        self.by_name.get(name).map(|&i| &self.entries[i])
    }
}

#[derive(Clone)]
pub struct ExtensionLease {
    id: LeaseId,
    scope_id: String,
    working_dir: Option<PathBuf>,
    extensions: Vec<Arc<Extension>>,
    tool_catalog: Arc<OnceCell<ToolCatalog>>,
    action_required: Arc<ActionRequiredManager>,
    hydrate_mcp_apps: bool,
}

struct ResolvedTool<'a> {
    extension: &'a Arc<Extension>,
    server_name: &'a str,
    tool: &'a Tool,
}

impl ExtensionLease {
    pub(super) fn new(
        set: &ExtensionSet,
        extensions: Vec<Arc<Extension>>,
        action_required: Arc<ActionRequiredManager>,
        hydrate_mcp_apps: bool,
    ) -> Self {
        Self {
            id: LeaseId::next(),
            scope_id: set.scope_id.clone(),
            working_dir: set.working_dir.clone(),
            extensions,
            tool_catalog: Arc::new(OnceCell::new()),
            action_required,
            hydrate_mcp_apps,
        }
    }

    async fn tool_catalog(&self) -> &ToolCatalog {
        self.tool_catalog
            .get_or_init(|| ToolCatalog::build(&self.scope_id, &self.extensions))
            .await
    }

    pub fn id(&self) -> LeaseId {
        self.id
    }

    pub fn scope_id(&self) -> &str {
        &self.scope_id
    }

    pub async fn tools(&self) -> Vec<Tool> {
        self.tool_catalog()
            .await
            .entries
            .iter()
            .map(|entry| entry.tool.clone())
            .collect()
    }

    pub async fn tools_for(&self, extension: &str) -> Vec<Tool> {
        let key = name_to_key(extension);
        self.tool_catalog()
            .await
            .entries
            .iter()
            .filter(|entry| entry.extension.key == key)
            .map(|entry| entry.tool.clone())
            .collect()
    }

    pub async fn tools_excluding(&self, extension: &str) -> Vec<Tool> {
        let key = name_to_key(extension);
        self.tool_catalog()
            .await
            .entries
            .iter()
            .filter(|entry| entry.extension.key != key)
            .map(|entry| entry.tool.clone())
            .collect()
    }

    pub fn is_enabled(&self, extension: &str) -> bool {
        let key = name_to_key(extension);
        self.extensions.iter().any(|extension| extension.key == key)
    }

    pub fn configs(&self) -> Vec<ExtensionConfig> {
        self.extensions
            .iter()
            .map(|extension| extension.config.clone())
            .collect()
    }

    pub fn supports_resources(&self) -> bool {
        self.extensions
            .iter()
            .any(|extension| extension.supports_resources())
    }

    pub async fn read_resource_tool(
        &self,
        params: Value,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<ContentBlock>, ErrorData> {
        let uri = require_str_parameter(&params, "uri")?;
        let extension_name = require_str_parameter(&params, "extension_name")?;
        let read_result = self
            .read_resource(uri, extension_name, cancellation_token)
            .await?;

        Ok(read_result
            .contents
            .into_iter()
            .filter_map(|content| match content {
                ResourceContents::TextResourceContents { text, .. } => {
                    Some(ContentBlock::text(format!("{uri}\n\n{text}")))
                }
                _ => None,
            })
            .collect())
    }

    pub async fn read_resource(
        &self,
        uri: &str,
        extension_name: &str,
        cancellation_token: CancellationToken,
    ) -> Result<ReadResourceResult, ErrorData> {
        let key = name_to_key(extension_name);
        let client = self
            .extensions
            .iter()
            .find(|extension| extension.key == key)
            .map(|extension| Arc::clone(&extension.client))
            .ok_or_else(|| {
                let available = self
                    .extensions
                    .iter()
                    .map(|extension| extension.key.as_str())
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

    pub async fn list_resources_result_from_extension(
        &self,
        extension_name: &str,
        cancellation_token: CancellationToken,
    ) -> Result<ListResourcesResult, ErrorData> {
        let key = name_to_key(extension_name);
        let client = self
            .extensions
            .iter()
            .find(|extension| extension.key == key)
            .map(|extension| Arc::clone(&extension.client))
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("Extension {extension_name} is not valid"),
                    None,
                )
            })?;

        client
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

    async fn list_resources_from_extension(
        &self,
        extension_name: &str,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<ContentBlock>, ErrorData> {
        self.list_resources_result_from_extension(extension_name, cancellation_token)
            .await
            .map(|result| {
                let resources = result
                    .resources
                    .into_iter()
                    .map(|resource| {
                        format!(
                            "{extension_name} - {}, uri: ({})",
                            resource.name, resource.uri
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                vec![ContentBlock::text(resources)]
            })
    }

    pub async fn list_resources(
        &self,
        params: Value,
        cancellation_token: CancellationToken,
    ) -> Result<Vec<ContentBlock>, ErrorData> {
        if let Some(extension_name) = params.get("extension_name").and_then(Value::as_str) {
            return self
                .list_resources_from_extension(extension_name, cancellation_token)
                .await;
        }

        let results = futures::future::join_all(
            self.extensions
                .iter()
                .filter(|extension| extension.supports_resources())
                .map(|extension| {
                    self.list_resources_from_extension(&extension.key, cancellation_token.clone())
                }),
        )
        .await;
        let mut resources = Vec::new();
        for result in results {
            match result {
                Ok(content) => resources.extend(content),
                Err(error) => warn!(?error, "failed to list resources"),
            }
        }
        Ok(resources)
    }

    pub fn instructions(&self) -> Vec<ExtensionInfo> {
        let working_dir = self
            .working_dir
            .as_deref()
            .unwrap_or(std::path::Path::new("."))
            .to_string_lossy();
        self.extensions
            .iter()
            .map(|extension| {
                let instructions = extension.client.get_instructions().unwrap_or_default();
                ExtensionInfo::new(
                    &extension.key,
                    &instructions.replace("{{WORKING_DIR}}", &working_dir),
                    extension.supports_resources(),
                )
            })
            .collect()
    }

    pub async fn moim(&self) -> Vec<String> {
        let mut content = Vec::new();
        for extension in self
            .extensions
            .iter()
            .filter(|extension| extension.is_platform())
        {
            let tools = self.tools_excluding(&extension.key).await;
            if let Some(part) = extension.client.get_moim(&self.scope_id, &tools).await {
                content.push(part);
            }
        }
        content
    }

    async fn resolve_tool(
        &self,
        tool_name: &str,
        calling_app: Option<&str>,
    ) -> Result<ResolvedTool<'_>, ErrorData> {
        let catalog = self.tool_catalog().await;
        let extension_key = calling_app.map(name_to_key);
        let belongs_to_extension = |entry: &&CatalogEntry| {
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
        resolved: ResolvedTool<'_>,
        arguments: Option<rmcp::model::JsonObject>,
        request: CallRequest,
        cancellation_token: CancellationToken,
    ) -> ToolCallResult {
        let CallRequest {
            tool_call_id,
            notification_emitter,
            container,
        } = request;
        let client = resolved.extension.client.clone();
        let action_required_stream = self.action_required_stream(tool_call_id.as_deref()).await;
        let (emitter, notification_stream) = Self::notifications_for_call(
            client.subscribe().await,
            notification_emitter,
            tool_call_id.as_deref(),
        );
        let mut call_context = ToolCallContext::new(
            self.scope_id.clone(),
            self.working_dir.clone(),
            tool_call_id,
        )
        .with_container(container)
        .with_extension_lease(Arc::new(self.clone()));
        if let Some(emitter) = emitter {
            call_context = call_context.with_notification_emitter(emitter);
        }

        let mcp_app_call = McpAppCall::from_resolved(&resolved, self.hydrate_mcp_apps);
        let server_name = resolved.server_name.to_string();
        let is_platform_extension = resolved.extension.is_platform();
        let session_id = self.scope_id.clone();
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
            if !is_platform_extension {
                ExtensionMutation::take(&mut result);
            }
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
    pub(crate) tool_call_id: Option<String>,
    pub(crate) notification_emitter: Option<ToolCallNotificationEmitter>,
    pub(crate) container: Option<Container>,
}

impl CallRequest {
    pub fn new(tool_call_id: impl Into<String>) -> Self {
        Self {
            tool_call_id: Some(tool_call_id.into()),
            notification_emitter: None,
            container: None,
        }
    }

    pub(crate) fn with_container(mut self, container: Option<Container>) -> Self {
        self.container = container;
        self
    }
}

impl From<&ToolCallContext> for CallRequest {
    fn from(ctx: &ToolCallContext) -> Self {
        Self {
            tool_call_id: ctx.tool_call_request_id.clone(),
            notification_emitter: ctx.notification_emitter().cloned(),
            container: ctx.container().cloned(),
        }
    }
}

struct McpAppCall {
    server_tool_name: String,
    extension_key: String,
    resource_uri: String,
    tool_meta: Option<serde_json::Value>,
}

impl McpAppCall {
    fn from_resolved(resolved: &ResolvedTool<'_>, host_supports_apps: bool) -> Option<Self> {
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
