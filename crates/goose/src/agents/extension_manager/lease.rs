use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures::stream;
use futures::{FutureExt, Stream};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ErrorCode, ErrorData, ServerNotification, Tool,
};
use rmcp::service::ServiceError;
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
use crate::agents::extension::{ExtensionConfig, ExtensionError, ExtensionInfo};
use crate::agents::mcp_client::McpClientTrait;
use crate::agents::reply_parts::is_tool_visible_to_app;
use crate::agents::tool_execution::{ToolCallContext, ToolCallNotificationEmitter, ToolCallResult};
use crate::config::extensions::name_to_key;
use crate::conversation::message::Message;

/// What a scope wants: which extensions, rooted where.
#[derive(Debug)]
pub struct ExtensionSet {
    id: String,
    working_dir: Option<PathBuf>,
    extensions: Vec<ExtensionConfig>,
}

impl ExtensionSet {
    pub fn new(
        id: impl Into<String>,
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
            id: id.into(),
            working_dir,
            extensions,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
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

pub(super) struct CatalogEntry {
    tool: Tool,
    extension: Arc<Extension>,
    actual_name: String,
}

/// Every public tool name in a lease, built once. Precedence between
/// extensions exposing the same name follows set order.
pub struct ToolCatalog {
    entries: Vec<CatalogEntry>,
    by_name: HashMap<String, usize>,
}

impl ToolCatalog {
    async fn build(scope_id: &str, members: &[Arc<Extension>]) -> Self {
        let lists = futures::future::join_all(
            members
                .iter()
                .map(|extension| extension.public_tools(scope_id)),
        )
        .await;

        let mut entries = Vec::new();
        let mut by_name = HashMap::new();
        for (extension, tools) in members.iter().zip(lists) {
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
                let actual_name = tool
                    .name
                    .strip_prefix(&format!("{}__", extension.key))
                    .unwrap_or(&tool.name)
                    .to_string();
                by_name.insert(name, entries.len());
                entries.push(CatalogEntry {
                    tool: tool.clone(),
                    extension: Arc::clone(extension),
                    actual_name,
                });
            }
        }
        Self { entries, by_name }
    }

    fn get(&self, name: &str) -> Option<&CatalogEntry> {
        self.by_name.get(name).map(|&i| &self.entries[i])
    }
}

pub struct ExtensionLease {
    id: LeaseId,
    scope_id: String,
    working_dir: Option<PathBuf>,
    members: Vec<Arc<Extension>>,
    /// Built on first use: callers that only want instructions or MOIM never
    /// list tools.
    catalog: OnceCell<ToolCatalog>,
    action_required: Arc<ActionRequiredManager>,
    hydrate_mcp_apps: bool,
}

pub(super) struct ResolvedTool<'a> {
    pub extension: &'a Arc<Extension>,
    pub actual_name: &'a str,
    pub tool: &'a Tool,
}

impl ExtensionLease {
    pub(super) async fn new(
        set: &ExtensionSet,
        members: Vec<Arc<Extension>>,
        action_required: Arc<ActionRequiredManager>,
        hydrate_mcp_apps: bool,
    ) -> Self {
        Self {
            id: LeaseId::next(),
            scope_id: set.id.clone(),
            working_dir: set.working_dir.clone(),
            members,
            catalog: OnceCell::new(),
            action_required,
            hydrate_mcp_apps,
        }
    }

    async fn catalog(&self) -> &ToolCatalog {
        self.catalog
            .get_or_init(|| ToolCatalog::build(&self.scope_id, &self.members))
            .await
    }

    pub fn id(&self) -> LeaseId {
        self.id
    }

    pub fn scope_id(&self) -> &str {
        &self.scope_id
    }

    pub async fn tools(&self) -> Vec<Tool> {
        self.catalog()
            .await
            .entries
            .iter()
            .map(|e| e.tool.clone())
            .collect()
    }

    pub async fn tools_for(&self, extension: &str) -> Vec<Tool> {
        let key = name_to_key(extension);
        self.catalog()
            .await
            .entries
            .iter()
            .filter(|e| e.extension.key == key)
            .map(|e| e.tool.clone())
            .collect()
    }

    pub async fn tools_excluding(&self, extension: &str) -> Vec<Tool> {
        let key = name_to_key(extension);
        self.catalog()
            .await
            .entries
            .iter()
            .filter(|e| e.extension.key != key)
            .map(|e| e.tool.clone())
            .collect()
    }

    pub fn is_enabled(&self, extension: &str) -> bool {
        let key = name_to_key(extension);
        self.members.iter().any(|m| m.key == key)
    }

    pub fn configs(&self) -> Vec<ExtensionConfig> {
        self.members.iter().map(|m| m.config.clone()).collect()
    }

    pub fn supports_resources(&self) -> bool {
        self.members.iter().any(|m| m.supports_resources())
    }

    pub fn instructions(&self) -> Vec<ExtensionInfo> {
        let working_dir = self
            .working_dir
            .as_deref()
            .unwrap_or(std::path::Path::new("."))
            .to_string_lossy();
        self.members
            .iter()
            .map(|m| {
                let instructions = m.client.get_instructions().unwrap_or_default();
                ExtensionInfo::new(
                    &m.key,
                    &instructions.replace("{{WORKING_DIR}}", &working_dir),
                    m.supports_resources(),
                )
            })
            .collect()
    }

    pub async fn moim(&self) -> Vec<String> {
        let mut parts = Vec::new();
        for member in self.members.iter().filter(|m| m.is_platform()) {
            if let Some(content) = member.client.get_moim(&self.scope_id).await {
                parts.push(content);
            }
        }
        parts
    }

    /// `app_extension` is set for calls made by an MCP app: the tool must belong
    /// to that extension and be visible to apps.
    pub(super) async fn resolve(
        &self,
        tool_name: &str,
        app_extension: Option<&str>,
    ) -> Result<ResolvedTool<'_>, ErrorData> {
        let catalog = self.catalog().await;
        let entry = catalog.get(tool_name).or_else(|| {
            let owners = catalog
                .entries
                .iter()
                .map(|e| (e.tool.name.as_ref(), get_tool_owner(&e.tool)));
            recover_mangled_tool_name(
                tool_name,
                owners
                    .collect::<Vec<_>>()
                    .iter()
                    .map(|(n, o)| (*n, o.as_deref())),
            )
            .and_then(|recovered| catalog.get(&recovered))
        });

        let Some(entry) = entry else {
            let available = catalog
                .entries
                .iter()
                .map(|e| e.tool.name.as_ref())
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

        if let Some(app_extension) = app_extension {
            if name_to_key(app_extension) != entry.extension.key {
                return Err(ErrorData::new(
                    ErrorCode::RESOURCE_NOT_FOUND,
                    format!("Tool '{}' not found for extension", tool_name),
                    None,
                ));
            }
            if !is_tool_visible_to_app(&entry.tool) {
                return Err(ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    "Tool is not visible to app clients",
                    None,
                ));
            }
        }

        Ok(ResolvedTool {
            extension: &entry.extension,
            actual_name: &entry.actual_name,
            tool: &entry.tool,
        })
    }

    pub async fn call(
        &self,
        tool_call: CallToolRequestParams,
        request: CallRequest,
        cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ErrorData> {
        let resolved = self.resolve(&tool_call.name, None).await?;
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
        let resolved = self.resolve(&tool_call.name, Some(app_extension)).await?;
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
        let client = resolved.extension.client.clone();
        let action_required_stream = self.action_required_stream(request.id.as_deref()).await;
        let (emitter, notification_stream) = self.notification_stream(
            client.subscribe().await,
            request.notification_emitter,
            request.id.is_some(),
        );
        let mut ctx =
            ToolCallContext::new(self.scope_id.clone(), self.working_dir.clone(), request.id);
        if let Some(emitter) = emitter {
            ctx = ctx.with_notification_emitter(emitter);
        }

        let app_call = McpAppCall::from_resolved(&resolved, self.hydrate_mcp_apps);
        let actual_name = resolved.actual_name.to_string();
        let strip_mutation = !resolved.extension.is_platform();
        let session_id = self.scope_id.clone();
        let fut = async move {
            let mut result = client
                .call_tool(&ctx, &actual_name, arguments, cancellation_token.clone())
                .await
                .map_err(|e| match e {
                    ServiceError::McpError(error_data) => error_data,
                    _ => ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None),
                })?;
            remove_untrusted_mcp_app_meta(&mut result);
            if strip_mutation {
                ExtensionMutation::take(&mut result);
            }
            if let Some(app_call) = app_call {
                app_call
                    .hydrate(&*client, &session_id, &mut result, cancellation_token)
                    .await;
            }
            Ok(result)
        };

        ToolCallResult {
            result: Box::new(fut.boxed()),
            notification_stream: Some(notification_stream),
            action_required_stream,
        }
    }

    async fn action_required_stream(
        &self,
        request_id: Option<&str>,
    ) -> Option<Box<dyn Stream<Item = Message> + Send + Unpin>> {
        let request_id = request_id?;
        if self
            .action_required
            .has_action_required_stream(&self.scope_id, request_id)
            .await
        {
            return None;
        }
        let receiver = self
            .action_required
            .register_action_required_stream(self.scope_id.clone(), request_id.to_string())
            .await;
        Some(Box::new(ActionRequiredStream::new(
            receiver,
            self.action_required.clone(),
            self.scope_id.clone(),
            request_id.to_string(),
        )))
    }

    /// A caller that already has an emitter (a nested call) keeps it, so its
    /// notifications reach the outer stream. Otherwise a call with a request
    /// id gets a channel of its own merged with the client's server
    /// notifications; without one there is nothing to attribute them to.
    fn notification_stream(
        &self,
        client_notifications: mpsc::Receiver<ServerNotification>,
        emitter: Option<ToolCallNotificationEmitter>,
        has_request_id: bool,
    ) -> (
        Option<ToolCallNotificationEmitter>,
        Box<dyn Stream<Item = ServerNotification> + Send + Unpin>,
    ) {
        if emitter.is_some() || !has_request_id {
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

/// What a caller supplies per call. Session and working directory are the
/// lease's, not the caller's.
#[derive(Default)]
pub struct CallRequest {
    pub(crate) id: Option<String>,
    pub(crate) notification_emitter: Option<ToolCallNotificationEmitter>,
}

impl CallRequest {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: Some(id.into()),
            notification_emitter: None,
        }
    }
}

impl From<&ToolCallContext> for CallRequest {
    fn from(ctx: &ToolCallContext) -> Self {
        Self {
            id: ctx.tool_call_request_id.clone(),
            notification_emitter: ctx.notification_emitter().cloned(),
        }
    }
}

/// An MCP-app tool's result gets its UI resource attached so the host can
/// render it without a second round trip.
struct McpAppCall {
    tool_name: String,
    extension_name: String,
    resource_uri: String,
    tool_meta: Option<serde_json::Value>,
}

impl McpAppCall {
    fn from_resolved(resolved: &ResolvedTool<'_>, host_supports_apps: bool) -> Option<Self> {
        if !host_supports_apps {
            return None;
        }
        Some(Self {
            tool_name: resolved.actual_name.to_string(),
            extension_name: resolved.extension.key.clone(),
            resource_uri: get_tool_resource_uri(resolved.tool)?,
            tool_meta: get_tool_meta_value(resolved.tool),
        })
    }

    async fn hydrate(
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
            tool_name: self.tool_name,
            tool_name_is_actual: true,
            extension_name: self.extension_name,
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
