use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures::stream;
use futures::{FutureExt, Stream};
use rmcp::model::{CallToolRequestParams, ErrorCode, ErrorData, ServerNotification, Tool};
use rmcp::service::ServiceError;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::{
    get_tool_meta_value, get_tool_owner, get_tool_resource_uri, insert_trusted_tool_update_meta,
    recover_mangled_tool_name, remove_untrusted_mcp_app_meta, ActionRequiredStream, Extension,
    GooseMcpAppToolAttachment, TOOL_CALL_NOTIFICATION_CHANNEL_CAPACITY,
};
use crate::action_required_manager::ActionRequiredManager;
use crate::agents::extension::{ExtensionConfig, ExtensionInfo};
use crate::agents::reply_parts::is_tool_visible_to_app;
use crate::agents::tool_execution::{ToolCallContext, ToolCallNotificationEmitter, ToolCallResult};
use crate::config::extensions::name_to_key;

/// What a scope wants: which extensions, rooted where.
pub struct ExtensionSet {
    pub id: String,
    pub working_dir: PathBuf,
    pub extensions: Vec<ExtensionConfig>,
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
    working_dir: PathBuf,
    members: Vec<Arc<Extension>>,
    catalog: ToolCatalog,
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
        let catalog = ToolCatalog::build(&set.id, &members).await;
        Self {
            id: LeaseId::next(),
            scope_id: set.id.clone(),
            working_dir: set.working_dir.clone(),
            members,
            catalog,
            action_required,
            hydrate_mcp_apps,
        }
    }

    pub fn id(&self) -> LeaseId {
        self.id
    }

    pub fn scope_id(&self) -> &str {
        &self.scope_id
    }

    pub fn tools(&self) -> Vec<Tool> {
        self.catalog
            .entries
            .iter()
            .map(|e| e.tool.clone())
            .collect()
    }

    pub fn tools_for(&self, extension: &str) -> Vec<Tool> {
        let key = name_to_key(extension);
        self.catalog
            .entries
            .iter()
            .filter(|e| e.extension.key == key)
            .map(|e| e.tool.clone())
            .collect()
    }

    pub fn tools_excluding(&self, extension: &str) -> Vec<Tool> {
        let key = name_to_key(extension);
        self.catalog
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
        let working_dir = self.working_dir.to_string_lossy();
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

    pub(super) fn resolve(
        &self,
        tool_name: &str,
        expected_extension: Option<&str>,
        require_app_visibility: bool,
    ) -> Result<ResolvedTool<'_>, ErrorData> {
        let entry = self.catalog.get(tool_name).or_else(|| {
            let owners = self
                .catalog
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
            .and_then(|recovered| self.catalog.get(&recovered))
        });

        let Some(entry) = entry else {
            let available = self
                .catalog
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

        if expected_extension.is_some_and(|expected| name_to_key(expected) != entry.extension.key) {
            return Err(ErrorData::new(
                ErrorCode::RESOURCE_NOT_FOUND,
                format!("Tool '{}' not found for extension", tool_name),
                None,
            ));
        }
        if require_app_visibility && !is_tool_visible_to_app(&entry.tool) {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "Tool is not visible to app clients",
                None,
            ));
        }

        Ok(ResolvedTool {
            extension: &entry.extension,
            actual_name: &entry.actual_name,
            tool: &entry.tool,
        })
    }

    pub async fn call(
        &self,
        ctx: &ToolCallContext,
        tool_call: CallToolRequestParams,
        expected_extension: Option<&str>,
        require_app_visibility: bool,
        cancellation_token: CancellationToken,
    ) -> Result<ToolCallResult, ErrorData> {
        let resolved = self.resolve(&tool_call.name, expected_extension, require_app_visibility)?;
        let client = resolved.extension.client.clone();
        let actual_name = resolved.actual_name.to_string();
        let extension_key = resolved.extension.key.clone();
        let tool_meta = get_tool_meta_value(resolved.tool);
        let resource_uri = get_tool_resource_uri(resolved.tool);

        let client_notifications = client.subscribe().await;
        let session_id = ctx.session_id.clone();
        let action_required = self.action_required.clone();
        let action_required_receiver = match ctx.tool_call_request_id.clone() {
            Some(request_id)
                if !action_required
                    .has_action_required_stream(&session_id, &request_id)
                    .await =>
            {
                let receiver = action_required
                    .register_action_required_stream(session_id.clone(), request_id.clone())
                    .await;
                Some((receiver, request_id))
            }
            _ => None,
        };

        let owned_ctx = ToolCallContext::new(
            ctx.session_id.clone(),
            ctx.working_dir.clone(),
            ctx.tool_call_request_id.clone(),
        );
        let (owned_ctx, tool_call_notifications) =
            if let Some(emitter) = ctx.notification_emitter().cloned() {
                (owned_ctx.with_notification_emitter(emitter), None)
            } else if owned_ctx.tool_call_request_id.is_some() {
                let (sender, receiver) = mpsc::channel(TOOL_CALL_NOTIFICATION_CHANNEL_CAPACITY);
                (
                    owned_ctx.with_notification_emitter(ToolCallNotificationEmitter::new(sender)),
                    Some(receiver),
                )
            } else {
                (owned_ctx, None)
            };
        let notification_stream: Box<dyn Stream<Item = ServerNotification> + Send + Unpin> =
            match tool_call_notifications {
                Some(receiver) => Box::new(stream::select(
                    ReceiverStream::new(client_notifications),
                    ReceiverStream::new(receiver),
                )),
                None => Box::new(ReceiverStream::new(client_notifications)),
            };

        let hydrate = self.hydrate_mcp_apps;
        let read_cancellation_token = cancellation_token.clone();
        let fut = async move {
            let mut result = client
                .call_tool(
                    &owned_ctx,
                    &actual_name,
                    tool_call.arguments,
                    cancellation_token,
                )
                .await
                .map_err(|e| match e {
                    ServiceError::McpError(error_data) => error_data,
                    _ => ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None),
                })?;

            remove_untrusted_mcp_app_meta(&mut result);

            if hydrate && result.is_error != Some(true) {
                if let Some(resource_uri) = resource_uri {
                    let mut attachment = GooseMcpAppToolAttachment {
                        tool_name: actual_name,
                        tool_name_is_actual: true,
                        extension_name: extension_key,
                        resource_uri: resource_uri.clone(),
                        tool_meta,
                        resource_result: None,
                        read_error: None,
                    };
                    match client
                        .read_resource(&session_id, &resource_uri, read_cancellation_token)
                        .await
                    {
                        Ok(resource_result) => {
                            attachment.resource_result =
                                serde_json::to_value(&resource_result).ok();
                        }
                        Err(error) => attachment.read_error = Some(error.to_string()),
                    }
                    insert_trusted_tool_update_meta(&mut result, &attachment);
                }
            }

            Ok(result)
        };

        Ok(ToolCallResult {
            result: Box::new(fut.boxed()),
            notification_stream: Some(notification_stream),
            action_required_stream: action_required_receiver.map(|(rx, request_id)| {
                Box::new(ActionRequiredStream::new(
                    rx,
                    self.action_required.clone(),
                    ctx.session_id.clone(),
                    request_id,
                )) as _
            }),
        })
    }
}
