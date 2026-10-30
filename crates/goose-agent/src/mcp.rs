//! Tool snapshots for already-connected MCP clients. Connection setup, server
//! lifetime, authentication, and client notification handlers belong to the caller.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{bail, Result};
use async_trait::async_trait;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, CancelledNotificationParam, ClientRequest, ErrorData,
    ListToolsResult, Notification, PaginatedRequestParams, ServerResult, Tool,
};
use rmcp::service::PeerRequestOptions;
use rmcp::{Peer, RoleClient, ServiceError};
use tokio_util::sync::CancellationToken;

use crate::operation::Emitter;
use crate::tool::ToolProvider;

/// A tool definition paired with its original server name and caller-owned route.
/// Keeping these together avoids rediscovery or name guessing during dispatch.
#[derive(Clone)]
pub struct McpTool<T> {
    pub tool: Tool,
    pub server_name: String,
    pub target: T,
}

/// An immutable routing snapshot. Build a replacement to refresh discovery; retain
/// the old snapshot until calls from its inference have been handled.
pub struct McpToolCatalog<T> {
    entries: Vec<McpTool<T>>,
    by_name: HashMap<String, usize>,
}

impl<T> McpToolCatalog<T> {
    pub fn new(entries: Vec<McpTool<T>>) -> Result<Self> {
        let mut by_name = HashMap::new();
        for (index, entry) in entries.iter().enumerate() {
            if by_name.insert(entry.tool.name.to_string(), index).is_some() {
                bail!("duplicate MCP tool name '{}'", entry.tool.name);
            }
        }
        Ok(Self { entries, by_name })
    }

    pub fn entries(&self) -> &[McpTool<T>] {
        &self.entries
    }

    pub fn get(&self, name: &str) -> Option<&McpTool<T>> {
        self.by_name.get(name).map(|&index| &self.entries[index])
    }
}

/// Deterministically map an MCP name to a provider-compatible identifier while
/// distinguishing names that sanitize or truncate to the same prefix.
pub fn provider_safe_tool_name(name: &str) -> String {
    let is_safe = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    if !name.is_empty() && name.len() <= 64 && name.chars().all(is_safe) {
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

/// Only discovery and invocation are required. Applications can adapt an existing
/// client here without giving the agent access to their extension lifecycle.
#[async_trait]
pub trait McpToolClient<S>: Send + Sync {
    async fn list_tools(
        &self,
        session: &S,
        cursor: Option<String>,
        cancel: CancellationToken,
    ) -> Result<ListToolsResult, ErrorData>;

    async fn call_tool(
        &self,
        session: &S,
        request_id: &str,
        call: CallToolRequestParams,
        cancel: CancellationToken,
    ) -> Result<CallToolResult, ErrorData>;
}

fn mcp_error(error: ServiceError) -> ErrorData {
    match error {
        ServiceError::McpError(error) => error,
        other => ErrorData::internal_error(other.to_string(), None),
    }
}

// A connected rmcp peer is sufficient for basic tools/list and tools/call. MRTR
// input-required rounds and request-scoped elicitation are not driven here.
#[async_trait]
impl<S: Send + Sync> McpToolClient<S> for Peer<RoleClient> {
    async fn list_tools(
        &self,
        _session: &S,
        cursor: Option<String>,
        cancel: CancellationToken,
    ) -> Result<ListToolsResult, ErrorData> {
        let params = cursor.map(|cursor| {
            let mut params = PaginatedRequestParams::default();
            params.cursor = Some(cursor);
            params
        });
        let request = ClientRequest::ListToolsRequest(rmcp::model::ListToolsRequest {
            method: Default::default(),
            params,
            extensions: Default::default(),
        });
        match request_with_cancellation(self, request, cancel).await? {
            ServerResult::ListToolsResult(result) => Ok(result),
            _ => Err(ErrorData::internal_error(
                "expected tools/list result",
                None,
            )),
        }
    }

    async fn call_tool(
        &self,
        _session: &S,
        _request_id: &str,
        call: CallToolRequestParams,
        cancel: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        match request_with_cancellation(
            self,
            ClientRequest::CallToolRequest(rmcp::model::Request::new(call)),
            cancel,
        )
        .await?
        {
            ServerResult::CallToolResult(result) => Ok(result),
            _ => Err(ErrorData::internal_error(
                "expected tools/call result",
                None,
            )),
        }
    }
}

async fn request_with_cancellation(
    peer: &Peer<RoleClient>,
    request: ClientRequest,
    cancel: CancellationToken,
) -> Result<ServerResult, ErrorData> {
    let handle = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(ErrorData::internal_error("MCP request cancelled", None)),
        handle = peer.send_cancellable_request(request, PeerRequestOptions::no_options()) => handle.map_err(mcp_error)?,
    };
    tokio::select! {
        result = handle.rx => result.map_err(|_| ErrorData::internal_error("MCP connection closed", None))?.map_err(mcp_error),
        _ = cancel.cancelled() => {
            peer.send_notification(Notification::new(CancelledNotificationParam::new(
                Some(handle.id), Some("operation cancelled".to_string()),
            )).into()).await.map_err(mcp_error)?;
            Err(ErrorData::internal_error("MCP request cancelled", None))
        }
    }
}

/// Discovers a fixed set of namespaced clients. The same snapshot implements
/// ToolProvider, so ToolOperation and its pre/post-tool hooks need no MCP special case.
pub struct McpTools<S> {
    catalog: McpToolCatalog<Arc<dyn McpToolClient<S>>>,
}

impl<S: Send + Sync> McpTools<S> {
    pub async fn discover(
        session: &S,
        clients: impl IntoIterator<Item = (String, Arc<dyn McpToolClient<S>>)>,
        cancel: CancellationToken,
    ) -> Result<Self> {
        let mut entries = Vec::new();
        for (namespace, client) in clients {
            if namespace.is_empty() {
                bail!("MCP namespace must not be empty");
            }
            let mut cursor = None;
            let mut seen = HashSet::new();
            loop {
                let page = client.list_tools(session, cursor, cancel.clone()).await?;
                for mut tool in page.tools {
                    let server_name = tool.name.to_string();
                    tool.name =
                        provider_safe_tool_name(&format!("{namespace}__{server_name}")).into();
                    entries.push(McpTool {
                        tool,
                        server_name,
                        target: client.clone(),
                    });
                }
                cursor = page.next_cursor;
                match &cursor {
                    Some(cursor) if !seen.insert(cursor.clone()) => {
                        bail!("MCP server repeated discovery cursor '{cursor}'")
                    }
                    Some(_) => {}
                    None => break,
                }
            }
        }
        Ok(Self {
            catalog: McpToolCatalog::new(entries)?,
        })
    }

    pub fn catalog(&self) -> &McpToolCatalog<Arc<dyn McpToolClient<S>>> {
        &self.catalog
    }

    pub async fn call(
        &self,
        session: &S,
        request_id: &str,
        mut call: CallToolRequestParams,
        cancel: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        let route = self.catalog.get(&call.name).ok_or_else(|| {
            ErrorData::invalid_params(format!("unknown MCP tool '{}'", call.name), None)
        })?;
        call.name = route.server_name.clone().into();
        route
            .target
            .call_tool(session, request_id, call, cancel)
            .await
    }
}

#[async_trait]
impl<S: Send + Sync> ToolProvider<S> for McpTools<S> {
    async fn tools(&self, _session: &S) -> Result<Vec<Tool>> {
        Ok(self
            .catalog
            .entries()
            .iter()
            .map(|entry| entry.tool.clone())
            .collect())
    }

    async fn call(
        &self,
        session: &S,
        request_id: &str,
        call: CallToolRequestParams,
        emit: &Emitter,
    ) -> Result<CallToolResult, ErrorData> {
        self.call(session, request_id, call, emit.cancel_token().clone())
            .await
    }
}
