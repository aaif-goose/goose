use crate::acp::custom_requests::*;
use crate::acp::fs::AcpTools;
use crate::acp::tools::AcpAwareToolMeta;
use crate::acp::{PermissionDecision, ACP_CURRENT_MODEL};
use crate::agents::extension::{Envs, PLATFORM_EXTENSIONS};
use crate::agents::extension_manager::TRUSTED_TOOL_UPDATE_META_KEY;
use crate::agents::mcp_client::{GooseMcpHostInfo, McpClientTrait};
use crate::agents::platform_extensions::developer::DeveloperClient;
use crate::agents::{Agent, AgentConfig, ExtensionConfig, GoosePlatform, SessionConfig};
use crate::config::base::CONFIG_YAML_NAME;
use crate::config::extensions::get_enabled_extensions_with_config;
use crate::config::paths::Paths;
use crate::config::permission::PermissionManager;
use crate::config::{Config, GooseMode};
use crate::conversation::message::{ActionRequiredData, Message, MessageContent, ToolRequest};
use crate::mcp_utils::ToolResult;
use crate::permission::permission_confirmation::PrincipalType;
use crate::permission::{Permission, PermissionConfirmation};
use crate::providers::base::Provider;
use crate::providers::inventory::{
    InventoryIdentity, ProviderInventoryEntry, ProviderInventoryService, RefreshJobPlan,
    RefreshPlan, RefreshSkipReason,
};
use crate::session::session_manager::SessionType;
use crate::session::{EnabledExtensionsState, Session, SessionManager};
use crate::utils::sanitize_unicode_tags;
use agent_client_protocol::schema::{
    AgentCapabilities, Annotations, AuthMethod, AuthMethodAgent, AuthenticateRequest,
    AuthenticateResponse, BlobResourceContents, CancelNotification, CloseSessionRequest,
    CloseSessionResponse, ConfigOptionUpdate, Content, ContentBlock, ContentChunk,
    CurrentModeUpdate, EmbeddedResource, EmbeddedResourceResource, FileSystemCapabilities,
    ForkSessionRequest, ForkSessionResponse, ImageContent, InitializeRequest, InitializeResponse,
    ListSessionsRequest, ListSessionsResponse, LoadSessionRequest, LoadSessionResponse,
    McpCapabilities, McpServer, Meta, ModelId, ModelInfo, NewSessionRequest, NewSessionResponse,
    PermissionOption, PermissionOptionKind, PromptCapabilities, PromptRequest, PromptResponse,
    RequestPermissionOutcome, RequestPermissionRequest, ResourceLink, SessionCapabilities,
    SessionCloseCapabilities, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelectOption, SessionId, SessionInfo, SessionInfoUpdate, SessionListCapabilities,
    SessionMode, SessionModeId, SessionModeState, SessionModelState, SessionNotification,
    SessionUpdate, SetSessionConfigOptionRequest, SetSessionConfigOptionResponse,
    SetSessionModeRequest, SetSessionModeResponse, SetSessionModelRequest, SetSessionModelResponse,
    StopReason, TextContent, TextResourceContents, ToolCall, ToolCallContent, ToolCallId,
    ToolCallLocation, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind, Usage,
    UsageUpdate,
};
use agent_client_protocol::util::MatchDispatchFrom;
use agent_client_protocol::{
    Agent as SacpAgent, ByteStreams, Client, ConnectionTo, Dispatch, HandleDispatchFrom, Handled,
    Responder,
};
use anyhow::Result;
use fs_err as fs;
use futures::future::{BoxFuture, Either};
use futures::stream::{self, StreamExt};
use futures::FutureExt;
use rmcp::model::{
    AnnotateAble, CallToolResult, RawContent, RawTextContent, ResourceContents, Role,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use strum::{EnumMessage, VariantNames};
use tokio::sync::{Mutex, OnceCell};
use tokio_util::compat::{TokioAsyncReadCompatExt as _, TokioAsyncWriteCompatExt as _};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use url::Url;

mod config;
mod custom_dispatch;
mod dictation;
mod dispatch;
mod extensions;
mod onboarding;
mod providers;
mod resources;
mod sessions;
mod sources;
mod tools;

pub type AcpProviderFactory = Arc<
    dyn Fn(
            String,
            crate::model::ModelConfig,
            Vec<ExtensionConfig>,
        ) -> BoxFuture<'static, Result<Arc<dyn Provider>>>
        + Send
        + Sync,
>;

/// Convenience conversions from any `Display` error into an `agent_client_protocol::Error`.
///
/// Replaces the repetitive `.internal_err()`
/// pattern. Use `.internal_err()?` for server-side failures and `.invalid_params_err()?`
/// for bad client input. For custom messages use `.internal_err_ctx("context")?`.
#[allow(dead_code)]
trait ResultExt<T> {
    fn internal_err(self) -> Result<T, agent_client_protocol::Error>;
    fn invalid_params_err(self) -> Result<T, agent_client_protocol::Error>;
    fn internal_err_ctx(self, context: &str) -> Result<T, agent_client_protocol::Error>;
    fn invalid_params_err_ctx(self, context: &str) -> Result<T, agent_client_protocol::Error>;
}

impl<T, E: std::fmt::Display> ResultExt<T> for Result<T, E> {
    fn internal_err(self) -> Result<T, agent_client_protocol::Error> {
        self.map_err(|e| agent_client_protocol::Error::internal_error().data(e.to_string()))
    }
    fn invalid_params_err(self) -> Result<T, agent_client_protocol::Error> {
        self.map_err(|e| agent_client_protocol::Error::invalid_params().data(e.to_string()))
    }
    fn internal_err_ctx(self, context: &str) -> Result<T, agent_client_protocol::Error> {
        self.map_err(|e| {
            agent_client_protocol::Error::internal_error().data(format!("{context}: {e}"))
        })
    }
    fn invalid_params_err_ctx(self, context: &str) -> Result<T, agent_client_protocol::Error> {
        self.map_err(|e| {
            agent_client_protocol::Error::invalid_params().data(format!("{context}: {e}"))
        })
    }
}

const DEFAULT_PROVIDER_ID: &str = "goose";
const DEFAULT_PROVIDER_LABEL: &str = "Goose (Default)";
const PROVIDER_CONFIG_STATUS_CHECK_CONCURRENCY: usize = 16;

async fn ensure_refresh_identity_current(
    provider_id: &str,
    planned_identity: &InventoryIdentity,
) -> Result<()> {
    let current_identity = crate::providers::inventory_identity(provider_id)
        .await?
        .into_identity()?;
    if current_identity != *planned_identity {
        anyhow::bail!("provider inventory identity changed before refresh completed");
    }

    Ok(())
}

/// In-memory state for an active ACP session.
///
/// ## Terminology (temporary, until all clients migrate to ACP)
///
/// The ACP protocol uses "session" to mean the conversation as the human sees it —
/// a durable, append-only exchange of messages. Internally, goose also has a concept
/// called "Session" (the `sessions` DB table) which represents the agent's working
/// state: the message list the LLM sees, compaction state, provider binding, etc.
///
/// To bridge these two worlds without rewriting the existing Session model:
/// - **Thread** (`threads` table) = the ACP session. The `sessionId` that ACP clients
///   see is actually a thread ID. Threads own the human-visible message log.
/// - **Session** (`sessions` table) = an internal execution context. A thread may have
///   many sessions over its lifetime (e.g. when the provider or persona changes).
///   Clients never see or manage these directly.
///
/// The `sessions` HashMap below is keyed by **thread ID** (= ACP session ID).
/// The `internal_session_id` field tracks which goose Session is currently active.
struct GooseAcpSession {
    agent: AgentHandle,
    internal_session_id: String,
    tool_requests: HashMap<String, crate::conversation::message::ToolRequest>,
    /// For each tool_call_id that belongs to a multi-tool chain (run of
    /// consecutive ToolRequest blocks within one assistant message), the chain
    /// it belongs to. Populated when the assistant message is processed.
    /// Used by `handle_tool_response` to detect when a chain has fully
    /// completed and fire a single LLM summary covering the run.
    chain_membership: HashMap<String, Arc<ToolChain>>,
    /// Set of tool_call_ids whose ToolResponse has already been processed.
    /// Drives the "all responses present" check for chain completion.
    responded_tool_ids: HashSet<String>,
    /// Tool_call_ids of chains that have already had a summary task fired.
    /// Idempotence guard so we summarize each chain at most once.
    summarized_chains: HashSet<String>,
    cancel_token: Option<CancellationToken>,
    /// Working directory set while the agent was still loading.
    /// Applied once the agent becomes ready.
    pending_working_dir: Option<std::path::PathBuf>,
}

/// A run of consecutive ToolRequest blocks within one assistant message,
/// tracked by [`GooseAcpSession::chain_membership`]. Used to drive a single
/// LLM summary for the whole run once every step has a recorded ToolResponse.
#[derive(Debug, Clone)]
struct ToolChain {
    /// Assistant message id where every tool request in this chain lives.
    /// This is also the row we patch when persisting the chain summary.
    message_id: String,
    /// Tool call ids in document order. Always `len() >= 2`.
    ids: Vec<String>,
}

/// Progress stages signalled by the background agent setup task via the watch
/// channel.  `ProviderReady` fires as soon as the provider (and goose-mode)
/// are initialized — before extensions finish loading.  `FullyReady` fires
/// once every extension has been loaded (or failed).
#[derive(Clone)]
enum AgentSetupProgress {
    /// Provider is initialized; extensions are still loading in the background.
    ProviderReady(Arc<Agent>),
    /// Provider *and* all extensions are initialized.
    FullyReady(Arc<Agent>),
}

type AgentSetupSignal = Option<Result<AgentSetupProgress, String>>;

/// The agent may still be initializing in the background (extension loading,
/// provider setup).  Callers that need the live agent (e.g. `on_prompt`) await
/// the handle; callers that only need the session metadata can proceed without it.
enum AgentHandle {
    Ready(Arc<Agent>),
    Loading(tokio::sync::watch::Receiver<AgentSetupSignal>),
}

struct AgentSetupRequest {
    session_id: SessionId,
    goose_session: Session,
    mcp_servers: Vec<McpServer>,
    /// Pre-resolved provider name + model config (from config, no network).
    /// When present the spawn skips re-deriving these from config.
    resolved_provider: Option<(String, crate::model::ModelConfig)>,
    /// Pre-instantiated provider reused from synchronous session initialization.
    prebuilt_provider: Option<Arc<dyn Provider>>,
}

pub struct GooseAcpAgent {
    sessions: Arc<Mutex<HashMap<String, GooseAcpSession>>>,
    provider_factory: AcpProviderFactory,
    builtins: Vec<String>,
    client_fs_capabilities: OnceCell<FileSystemCapabilities>,
    client_terminal: OnceCell<bool>,
    client_mcp_host_info: OnceCell<GooseMcpHostInfo>,
    config_dir: std::path::PathBuf,
    session_manager: Arc<SessionManager>,
    thread_manager: Arc<crate::session::ThreadManager>,
    permission_manager: Arc<PermissionManager>,
    goose_mode: GooseMode,
    disable_session_naming: bool,
    provider_inventory: ProviderInventoryService,
    goose_platform: GoosePlatform,
}

/// Shorten a session/thread id for perf log correlation.
/// All `perf:` logs use `sid=<8-char-prefix>` so a single session's activity
/// can be extracted with `grep 'perf:' <log> | grep 'sid=abc12345'`.
fn sid_short(id: &str) -> String {
    id.chars().take(8).collect()
}

fn thread_session_meta(
    thread: &crate::session::Thread,
) -> serde_json::Map<String, serde_json::Value> {
    let mut meta = serde_json::Map::new();
    meta.insert(
        "messageCount".to_string(),
        serde_json::Value::Number(thread.message_count.into()),
    );
    meta.insert(
        "createdAt".to_string(),
        serde_json::Value::String(thread.created_at.to_rfc3339()),
    );
    if let Some(ref archived_at) = thread.archived_at {
        meta.insert(
            "archivedAt".to_string(),
            serde_json::Value::String(archived_at.to_rfc3339()),
        );
    }
    meta.insert(
        "userSetName".to_string(),
        serde_json::Value::Bool(thread.user_set_name),
    );
    if let Some(ref pid) = thread.metadata.project_id {
        meta.insert(
            "projectId".to_string(),
            serde_json::Value::String(pid.clone()),
        );
    }
    if let Some(ref provider_id) = thread.metadata.provider_id {
        meta.insert(
            "providerId".to_string(),
            serde_json::Value::String(provider_id.clone()),
        );
    }
    if let Some(ref model_id) = thread.metadata.model_id {
        meta.insert(
            "modelId".to_string(),
            serde_json::Value::String(model_id.clone()),
        );
    }
    if let Some(ref persona_id) = thread.metadata.persona_id {
        meta.insert(
            "personaId".to_string(),
            serde_json::Value::String(persona_id.clone()),
        );
    }
    meta
}

fn spawn_session_name_update_notifier(
    cx: ConnectionTo<Client>,
) -> tokio::sync::mpsc::UnboundedSender<crate::session::SessionNameUpdate> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<crate::session::SessionNameUpdate>();
    tokio::spawn(async move {
        while let Some(update) = rx.recv().await {
            let thread = update.thread;
            let thread_id = thread.id.clone();
            let meta = thread_session_meta(&thread);
            let notification = SessionNotification::new(
                SessionId::new(thread_id.clone()),
                SessionUpdate::SessionInfoUpdate(
                    SessionInfoUpdate::new()
                        .title(thread.name)
                        .updated_at(thread.updated_at.to_rfc3339())
                        .meta(meta),
                ),
            );
            if let Err(error) = cx.send_notification(notification) {
                warn!(
                    thread_id = %thread_id,
                    error = %error,
                    "Failed to send generated session name update"
                );
            }
        }
    });
    tx
}

fn extract_timeout_from_meta(meta: &Option<Meta>) -> Option<u64> {
    meta.as_ref()
        .and_then(|m| m.get("timeout"))
        .and_then(|v| v.as_u64())
}

#[derive(Debug, Default, Deserialize)]
struct GooseClientMetaEnvelope {
    #[serde(default)]
    goose: Option<GooseClientMeta>,
}

#[derive(Debug, Default, Deserialize)]
struct GooseClientMeta {
    #[serde(rename = "mcpHostCapabilities", default)]
    mcp_host_capabilities: Option<GooseMcpHostCapabilities>,
}

#[derive(Debug, Default, Deserialize)]
struct GooseMcpHostCapabilities {
    #[serde(default)]
    extensions: Option<rmcp::model::ExtensionCapabilities>,
}

fn extract_goose_client_meta(meta: &Meta) -> Option<GooseClientMetaEnvelope> {
    serde_json::from_value(serde_json::Value::Object(meta.clone())).ok()
}

fn extract_client_mcp_host_info(args: &InitializeRequest) -> GooseMcpHostInfo {
    let host_capabilities = args
        .client_capabilities
        .meta
        .as_ref()
        .and_then(extract_goose_client_meta)
        .and_then(|meta| meta.goose)
        .and_then(|goose| goose.mcp_host_capabilities);
    let explicit_extensions = host_capabilities
        .as_ref()
        .and_then(|capabilities| capabilities.extensions.as_ref())
        .is_some();
    let extensions = host_capabilities
        .and_then(|capabilities| capabilities.extensions)
        .unwrap_or_default();

    GooseMcpHostInfo {
        explicit_extensions,
        extensions,
        client_name: args.client_info.as_ref().map(|info| info.name.clone()),
        client_version: args.client_info.as_ref().map(|info| info.version.clone()),
    }
}

fn mcp_server_to_extension_config(mcp_server: McpServer) -> Result<ExtensionConfig, String> {
    match mcp_server {
        McpServer::Stdio(stdio) => {
            let timeout = extract_timeout_from_meta(&stdio.meta);
            Ok(ExtensionConfig::Stdio {
                name: stdio.name,
                description: String::new(),
                cmd: stdio.command.to_string_lossy().to_string(),
                args: stdio.args,
                envs: Envs::new(stdio.env.into_iter().map(|e| (e.name, e.value)).collect()),
                env_keys: vec![],
                timeout,
                bundled: Some(false),
                available_tools: vec![],
            })
        }
        McpServer::Http(http) => {
            let timeout = extract_timeout_from_meta(&http.meta);
            Ok(ExtensionConfig::StreamableHttp {
                name: http.name,
                description: String::new(),
                uri: http.url,
                envs: Envs::default(),
                env_keys: vec![],
                headers: http
                    .headers
                    .into_iter()
                    .map(|h| (h.name, h.value))
                    .collect(),
                timeout,
                socket: None,
                bundled: Some(false),
                available_tools: vec![],
            })
        }
        McpServer::Sse(_) => Err("SSE is unsupported, migrate to streamable_http".to_string()),
        _ => Err("Unknown MCP server type".to_string()),
    }
}

fn get_requested_line(arguments: Option<&rmcp::model::JsonObject>) -> Option<u32> {
    arguments
        .and_then(|args| args.get("line"))
        .and_then(|v| v.as_u64())
        .map(|l| l as u32)
}

fn is_developer_file_tool(tool_name: &str) -> bool {
    matches!(tool_name, "read" | "write" | "edit")
}

fn extract_locations_from_meta(
    tool_response: &crate::conversation::message::ToolResponse,
) -> Option<Vec<ToolCallLocation>> {
    let result = tool_response.tool_result.as_ref().ok()?;
    let meta = result.meta.as_ref()?;
    let locations_val = meta.get("tool_locations")?;
    let entries: Vec<serde_json::Value> = serde_json::from_value(locations_val.clone()).ok()?;
    let locations = entries
        .into_iter()
        .filter_map(|entry| {
            let path = entry.get("path")?.as_str()?;
            let line = entry.get("line").and_then(|v| v.as_u64()).map(|l| l as u32);
            Some(ToolCallLocation::new(path).line(line))
        })
        .collect::<Vec<_>>();
    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
}

fn extract_tool_locations(
    tool_request: &crate::conversation::message::ToolRequest,
    tool_response: &crate::conversation::message::ToolResponse,
) -> Vec<ToolCallLocation> {
    let mut locations = Vec::new();

    if let Ok(tool_call) = &tool_request.tool_call {
        if !is_developer_file_tool(tool_call.name.as_ref()) {
            return locations;
        }

        let tool_name = tool_call.name.as_ref();
        let path_str = tool_call
            .arguments
            .as_ref()
            .and_then(|args| args.get("path"))
            .and_then(|p| p.as_str());

        if let Some(path_str) = path_str {
            if matches!(tool_name, "read") {
                let line = get_requested_line(tool_call.arguments.as_ref());
                locations.push(ToolCallLocation::new(path_str).line(line));
                return locations;
            }

            if matches!(tool_name, "write" | "edit") {
                locations.push(ToolCallLocation::new(path_str).line(1));
                return locations;
            }

            let command = tool_call
                .arguments
                .as_ref()
                .and_then(|args| args.get("command"))
                .and_then(|c| c.as_str());

            if let Ok(result) = &tool_response.tool_result {
                for content in &result.content {
                    if let RawContent::Text(text_content) = &content.raw {
                        let text = &text_content.text;

                        match command {
                            Some("view") => {
                                let line = extract_view_line_range(text)
                                    .map(|range| range.0 as u32)
                                    .or(Some(1));
                                locations.push(ToolCallLocation::new(path_str).line(line));
                            }
                            Some("str_replace") | Some("insert") => {
                                let line = extract_first_line_number(text)
                                    .map(|l| l as u32)
                                    .or(Some(1));
                                locations.push(ToolCallLocation::new(path_str).line(line));
                            }
                            Some("write") => {
                                locations.push(ToolCallLocation::new(path_str).line(1));
                            }
                            _ => {
                                locations.push(ToolCallLocation::new(path_str).line(1));
                            }
                        }
                        break;
                    }
                }
            }

            if locations.is_empty() {
                locations.push(ToolCallLocation::new(path_str).line(1));
            }
        }
    }

    locations
}

fn extract_view_line_range(text: &str) -> Option<(usize, usize)> {
    let re = regex::Regex::new(r"\(lines (\d+)-(\d+|end)\)").ok()?;
    if let Some(caps) = re.captures(text) {
        let start = caps.get(1)?.as_str().parse::<usize>().ok()?;
        let end = if caps.get(2)?.as_str() == "end" {
            start
        } else {
            caps.get(2)?.as_str().parse::<usize>().ok()?
        };
        return Some((start, end));
    }
    None
}

fn extract_first_line_number(text: &str) -> Option<usize> {
    let re = regex::Regex::new(r"```[^\n]*\n(\d+):").ok()?;
    if let Some(caps) = re.captures(text) {
        return caps.get(1)?.as_str().parse::<usize>().ok();
    }
    None
}

fn read_resource_link(link: ResourceLink) -> Option<String> {
    let url = Url::parse(&link.uri).ok()?;
    if url.scheme() == "file" {
        let path = url.to_file_path().ok()?;
        let contents = fs::read_to_string(&path).ok()?;

        Some(format!(
            "\n\n# {}\n```\n{}\n```",
            path.to_string_lossy(),
            contents
        ))
    } else {
        None
    }
}

fn format_tool_name(tool_name: &str) -> String {
    if let Some((extension, tool)) = tool_name.split_once("__") {
        format!(
            "{}: {}",
            extension.replace('_', " "),
            tool.replace('_', " ")
        )
    } else {
        tool_name.replace('_', " ")
    }
}

/// Build a short fallback title from the tool name and arguments by extracting
/// the most useful value (file path, command, query, url, etc.).
fn summarize_tool_call(tool_name: &str, arguments: Option<&serde_json::Value>) -> String {
    let base = format_tool_name(tool_name);

    let detail = arguments.and_then(|args| {
        let obj = args.as_object()?;
        let keys = [
            "path", "file", "command", "query", "url", "uri", "name", "pattern", "source",
        ];
        for key in &keys {
            if let Some(v) = obj.get(*key) {
                let s = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                if !s.is_empty() {
                    let first_line = s.lines().next().unwrap_or(&s);
                    if first_line.len() > 60 {
                        return Some(format!("{}…", crate::utils::safe_truncate(first_line, 57)));
                    }
                    return Some(first_line.to_string());
                }
            }
        }
        None
    });

    match detail {
        Some(d) => format!("{base} · {d}"),
        None => base,
    }
}

fn tool_call_identity_meta(tool_request: &ToolRequest) -> Option<Meta> {
    let tool_call = tool_request.tool_call.as_ref().ok()?;
    let tool_name = tool_call.name.to_string();
    let extension_name = tool_request
        .tool_meta
        .as_ref()
        .and_then(|meta| meta.get("goose_extension"))
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            tool_name
                .split_once("__")
                .map(|(extension_name, _)| extension_name.to_string())
        });

    let mut tool_call_meta = serde_json::Map::new();
    tool_call_meta.insert("toolName".to_string(), serde_json::Value::String(tool_name));
    if let Some(extension_name) = extension_name {
        tool_call_meta.insert(
            "extensionName".to_string(),
            serde_json::Value::String(extension_name),
        );
    }

    let mut goose_meta = serde_json::Map::new();
    goose_meta.insert(
        "toolCall".to_string(),
        serde_json::Value::Object(tool_call_meta),
    );

    let mut meta = serde_json::Map::new();
    meta.insert("goose".to_string(), serde_json::Value::Object(goose_meta));
    Some(meta)
}

/// Add `goose.toolChainSummary = { summary, count }` to a `Meta` blob,
/// preserving any existing `goose.*` keys (e.g. `goose.toolCall` set by
/// [`tool_call_identity_meta`]).
fn with_tool_chain_summary_meta(base: Option<Meta>, summary: &str, count: usize) -> Option<Meta> {
    let mut meta = base.unwrap_or_default();
    let goose_entry = meta
        .entry("goose".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    let goose_obj = match goose_entry {
        serde_json::Value::Object(obj) => obj,
        other => {
            *other = serde_json::Value::Object(serde_json::Map::new());
            match other {
                serde_json::Value::Object(obj) => obj,
                _ => unreachable!(),
            }
        }
    };
    let mut chain = serde_json::Map::new();
    chain.insert(
        "summary".to_string(),
        serde_json::Value::String(summary.to_string()),
    );
    chain.insert(
        "count".to_string(),
        serde_json::Value::Number(serde_json::Number::from(count)),
    );
    goose_obj.insert(
        "toolChainSummary".to_string(),
        serde_json::Value::Object(chain),
    );
    Some(meta)
}

struct PendingToolCall {
    tool_call: ToolCall,
    identity_meta: Option<Meta>,
    fallback_title: String,
}

/// Extract chains (runs of consecutive `MessageContent::ToolRequest` blocks)
/// from a single message's content. Mirrors the frontend's chain detection in
/// `MessageBubble.groupContentSections`: any non-tool block (text, thinking,
/// image, etc.) breaks the run.
///
/// Returns one inner Vec per detected chain, holding the tool_call_ids in
/// document order. Single-tool runs are included; callers (chain
/// summarization) gate on `chain.len() >= 2`.
///
/// Note: this is the per-message view, kept around for tests and potential
/// replay use. The live runtime path uses a streaming buffer fed by
/// [`register_chain_buffer`] so chains that span multiple `AgentEvent::Message`
/// events (e.g. Bedrock-style streaming, where one LLM message is split across
/// rows — see `f087fa63c`) are still detected.
#[allow(dead_code)]
fn extract_tool_chains(
    content: &[crate::conversation::message::MessageContent],
) -> Vec<Vec<String>> {
    use crate::conversation::message::MessageContent;
    let mut chains: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();

    for block in content {
        match block {
            MessageContent::ToolRequest(tr) => current.push(tr.id.clone()),
            MessageContent::ToolResponse(_) => {
                // Server-side, assistant messages don't carry responses;
                // responses arrive in subsequent messages. Treat as
                // chain-neutral so a stray response doesn't split a chain
                // if the data shape ever changes.
            }
            _ => {
                if !current.is_empty() {
                    chains.push(std::mem::take(&mut current));
                }
            }
        }
    }
    if !current.is_empty() {
        chains.push(current);
    }
    chains
}

/// If `buffer` holds a multi-tool run (≥ 2 tool requests), (re)register a
/// [`ToolChain`] in `chain_membership` anchored on the **first** tool's
/// message_id (the row [`ThreadManager::update_tool_request_meta`] will patch
/// when persisting the LLM-generated summary). Does **not** clear the buffer
/// — chains can grow as more tools arrive (sequential tool use), so callers
/// keep accumulating and re-registering with the larger set of ids.
///
/// The buffer contains `(tool_call_id, message_id)` pairs in arrival order,
/// fed by the prompt stream loop. Sequential tool use (Bedrock/Anthropic)
/// interleaves request → response → request → response across separate
/// `AgentEvent::Message` events, so per-event `extract_tool_chains` only
/// sees length-1 chains and would miss the run. Tool responses are
/// chain-neutral (they don't split the run); only non-tool content (text,
/// thinking, image, etc.) does, matching the frontend's
/// `groupContentSections` behavior.
fn extend_chain_membership(
    buffer: &[(String, String)],
    chain_membership: &mut HashMap<String, Arc<ToolChain>>,
) {
    if buffer.len() >= 2 {
        let anchor_message_id = buffer[0].1.clone();
        let ids: Vec<String> = buffer.iter().map(|(id, _)| id.clone()).collect();
        let chain = Arc::new(ToolChain {
            message_id: anchor_message_id,
            ids: ids.clone(),
        });
        for id in ids {
            chain_membership.insert(id, chain.clone());
        }
    }
}

fn pending_tool_call_from_request(tool_request: &ToolRequest) -> PendingToolCall {
    let tool_name = match &tool_request.tool_call {
        Ok(tool_call) => tool_call.name.to_string(),
        Err(_) => "error".to_string(),
    };
    let args_value = tool_request
        .tool_call
        .as_ref()
        .ok()
        .and_then(|tc| tc.arguments.as_ref())
        .map(|a| serde_json::Value::Object(a.clone()));
    let fallback_title = summarize_tool_call(&tool_name, args_value.as_ref());
    let identity_meta = tool_call_identity_meta(tool_request);

    // Prefer the persisted LLM-generated title when available so replay (and
    // any subsequent live initial ToolCall after the title task has already
    // resolved) emits the nice title up front, with no flash of the
    // deterministic fallback.
    let initial_title = tool_request
        .persisted_title()
        .map(|s| s.to_string())
        .unwrap_or_else(|| fallback_title.clone());

    let mut tool_call = ToolCall::new(ToolCallId::new(tool_request.id.clone()), initial_title)
        .status(ToolCallStatus::Pending);
    if let Some(args) = args_value {
        tool_call = tool_call.raw_input(args);
    }

    PendingToolCall {
        tool_call,
        identity_meta,
        fallback_title,
    }
}

fn builtin_to_extension_config(name: &str) -> ExtensionConfig {
    if let Some(def) = PLATFORM_EXTENSIONS.get(name) {
        ExtensionConfig::Platform {
            name: def.name.into(),
            description: def.description.into(),
            display_name: Some(def.display_name.into()),
            bundled: Some(true),
            available_tools: vec![],
        }
    } else {
        ExtensionConfig::Builtin {
            name: name.into(),
            display_name: None,
            timeout: None,
            bundled: Some(true),
            description: name.into(),
            available_tools: vec![],
        }
    }
}

fn build_model_state(current_model: &str, inventory: &ProviderInventoryEntry) -> SessionModelState {
    let mut available_models = inventory
        .models
        .iter()
        .map(|model| ModelInfo::new(ModelId::new(model.id.as_str()), model.name.as_str()))
        .collect::<Vec<_>>();
    if !available_models
        .iter()
        .any(|model| model.model_id.0.as_ref() == current_model)
    {
        available_models.insert(
            0,
            ModelInfo::new(ModelId::new(current_model), current_model),
        );
    }
    SessionModelState::new(ModelId::new(current_model), available_models)
}

struct ProviderOptionEntry {
    id: String,
    label: String,
}

async fn list_provider_entries(current_provider: Option<&str>) -> Vec<ProviderOptionEntry> {
    let mut providers = crate::providers::providers()
        .await
        .into_iter()
        .map(|(metadata, _)| ProviderOptionEntry {
            id: metadata.name,
            label: metadata.display_name,
        })
        .collect::<Vec<_>>();
    providers.sort_by(|left, right| left.id.cmp(&right.id));
    providers.dedup_by(|left, right| left.id == right.id);

    if let Some(current_provider) = current_provider {
        if current_provider != DEFAULT_PROVIDER_ID
            && !providers
                .iter()
                .any(|provider| provider.id == current_provider)
        {
            providers.push(ProviderOptionEntry {
                id: current_provider.to_string(),
                label: current_provider.to_string(),
            });
            providers.sort_by(|left, right| left.id.cmp(&right.id));
        }
    }

    let mut entries = Vec::with_capacity(providers.len() + 1);
    entries.push(ProviderOptionEntry {
        id: DEFAULT_PROVIDER_ID.to_string(),
        label: DEFAULT_PROVIDER_LABEL.to_string(),
    });
    entries.extend(providers);
    entries
}

async fn build_provider_options(current_provider: Option<&str>) -> Vec<SessionConfigSelectOption> {
    list_provider_entries(current_provider)
        .await
        .into_iter()
        .map(|provider| SessionConfigSelectOption::new(provider.id, provider.label))
        .collect()
}

fn session_provider_selection(session: &Session) -> &str {
    session
        .provider_name
        .as_deref()
        .unwrap_or(DEFAULT_PROVIDER_ID)
}

/// Resolve the provider name and model config for a session from an
/// already-loaded `Config`.
async fn resolve_provider_and_model_from_config(
    config: &Config,
    goose_session: &Session,
) -> Result<(String, crate::model::ModelConfig), String> {
    let global_provider = config.get_goose_provider().ok();
    let provider_override = goose_session
        .provider_name
        .as_deref()
        .filter(|p| *p != DEFAULT_PROVIDER_ID);
    let provider_name = provider_override
        .map(ToOwned::to_owned)
        .or_else(|| global_provider.clone())
        .ok_or_else(|| "Missing provider".to_string())?;
    let explicitly_switched =
        provider_override.is_some() && provider_override != global_provider.as_deref();
    let model_config = match &goose_session.model_config {
        Some(mc) => mc.clone(),
        None if explicitly_switched => {
            let entry = crate::providers::get_from_registry(&provider_name)
                .await
                .map_err(|e| e.to_string())?;
            let default_model = &entry.metadata().default_model;
            crate::model::ModelConfig::new(default_model)
                .map_err(|e| e.to_string())?
                .with_canonical_limits(&provider_name)
        }
        None => {
            let model_id = config.get_goose_model().map_err(|e| e.to_string())?;
            crate::model::ModelConfig::new(&model_id)
                .map_err(|e| e.to_string())?
                .with_canonical_limits(&provider_name)
        }
    };
    Ok((provider_name, model_config))
}

/// Convenience wrapper: reads config from disk, then resolves provider + model.
/// Cheap enough to call from `on_new_session` (file + registry reads, no network).
async fn resolve_provider_and_model(
    config_dir: &std::path::Path,
    goose_session: &Session,
) -> Result<(String, crate::model::ModelConfig), String> {
    let config =
        Config::new(config_dir.join(CONFIG_YAML_NAME), "goose").map_err(|e| e.to_string())?;
    resolve_provider_and_model_from_config(&config, goose_session).await
}

fn build_mode_state(
    current_mode: GooseMode,
) -> Result<SessionModeState, agent_client_protocol::Error> {
    let mut available = Vec::with_capacity(GooseMode::VARIANTS.len());
    for &name in GooseMode::VARIANTS {
        let goose_mode: GooseMode = name.parse().map_err(|_| {
            agent_client_protocol::Error::internal_error() // impossible but satisfy linters
                .data(format!("Failed to parse GooseMode variant: {}", name))
        })?;
        let mut mode = SessionMode::new(SessionModeId::new(name), name);
        mode.description = goose_mode.get_message().map(Into::into);
        available.push(mode);
    }
    Ok(SessionModeState::new(
        SessionModeId::new(current_mode.to_string()),
        available,
    ))
}

fn should_refresh_inventory_for_session_init(entry: &ProviderInventoryEntry) -> bool {
    entry.configured
        && entry.supports_refresh
        && (entry.last_updated_at.is_none() || ProviderInventoryService::is_stale(entry))
}

async fn build_eager_config_from_inventory(
    provider_name: &str,
    current_model: &str,
    inventory: &ProviderInventoryEntry,
    mode_state: &SessionModeState,
    goose_session: &Session,
) -> (SessionModelState, Vec<SessionConfigOption>) {
    let ms = build_model_state(current_model, inventory);
    let provider_selection = session_provider_selection(goose_session);
    let provider_options = build_provider_options(Some(provider_name)).await;
    let config_options =
        build_config_options(mode_state, &ms, provider_selection, provider_options);
    (ms, config_options)
}

fn build_config_options(
    mode_state: &SessionModeState,
    model_state: &SessionModelState,
    provider_selection: &str,
    provider_options: Vec<SessionConfigSelectOption>,
) -> Vec<SessionConfigOption> {
    let mode_options: Vec<SessionConfigSelectOption> = mode_state
        .available_modes
        .iter()
        .map(|m| {
            SessionConfigSelectOption::new(m.id.0.clone(), m.name.clone())
                .description(m.description.clone())
        })
        .collect();
    let model_options: Vec<SessionConfigSelectOption> = model_state
        .available_models
        .iter()
        .map(|m| SessionConfigSelectOption::new(m.model_id.0.clone(), m.name.clone()))
        .collect();
    vec![
        SessionConfigOption::select(
            "provider",
            "Provider",
            provider_selection.to_string(),
            provider_options,
        ),
        SessionConfigOption::select(
            "mode",
            "Mode",
            mode_state.current_mode_id.0.clone(),
            mode_options,
        )
        .category(SessionConfigOptionCategory::Mode),
        SessionConfigOption::select(
            "model",
            "Model",
            model_state.current_model_id.0.clone(),
            model_options,
        )
        .category(SessionConfigOptionCategory::Model),
    ]
}

fn to_nonnegative_u64(value: Option<i32>) -> Option<u64> {
    value.and_then(|v| u64::try_from(v).ok())
}

fn build_prompt_usage(session: &Session) -> Option<Usage> {
    let total = to_nonnegative_u64(session.total_tokens)?;
    let input = to_nonnegative_u64(session.input_tokens).unwrap_or(0);
    let output = to_nonnegative_u64(session.output_tokens).unwrap_or(0);
    Some(Usage::new(total, input, output))
}

fn build_usage_update(session: &Session, context_limit: usize) -> UsageUpdate {
    let used = session.total_tokens.unwrap_or(0).max(0) as u64;
    UsageUpdate::new(used, context_limit as u64)
}

impl GooseAcpAgent {
    pub fn permission_manager(&self) -> Arc<PermissionManager> {
        Arc::clone(&self.permission_manager)
    }

    // TODO: goose reads Paths::in_state_dir globally (e.g. RequestLog), ignoring this data_dir.
    pub async fn new(
        provider_factory: AcpProviderFactory,
        builtins: Vec<String>,
        data_dir: std::path::PathBuf,
        config_dir: std::path::PathBuf,
        goose_mode: GooseMode,
        disable_session_naming: bool,
        goose_platform: GoosePlatform,
    ) -> Result<Self> {
        let session_manager = Arc::new(SessionManager::new(data_dir));

        // Eagerly initialize the SQLite pool so it's ready when providers/sessions need it.
        let storage_clone = session_manager.storage().clone();
        tokio::spawn(async move {
            let _ = storage_clone.pool().await;
        });

        let thread_manager = Arc::new(crate::session::ThreadManager::new(
            session_manager.storage().clone(),
        ));
        let permission_manager = Arc::new(PermissionManager::new(config_dir.clone()));
        let provider_inventory = ProviderInventoryService::new(session_manager.storage().clone());

        Ok(Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            provider_factory,
            builtins,
            client_fs_capabilities: OnceCell::new(),
            client_terminal: OnceCell::new(),
            client_mcp_host_info: OnceCell::new(),
            config_dir,
            session_manager,
            thread_manager,
            permission_manager,
            goose_mode,
            disable_session_naming,
            provider_inventory,
            goose_platform,
        })
    }

    fn load_config(&self) -> Result<Config> {
        Config::new(self.config_dir.join(CONFIG_YAML_NAME), "goose").map_err(Into::into)
    }

    fn config(&self) -> Result<Config, agent_client_protocol::Error> {
        self.load_config().internal_err_ctx("Failed to read config")
    }

    async fn create_provider(
        &self,
        provider_name: &str,
        model_config: crate::model::ModelConfig,
        extensions: Vec<ExtensionConfig>,
    ) -> Result<Arc<dyn Provider>> {
        (self.provider_factory)(provider_name.to_string(), model_config, extensions).await
    }

    async fn prepare_session_init_config(
        &self,
        resolved: &Result<(String, crate::model::ModelConfig), String>,
        mode_state: &SessionModeState,
        goose_session: &Session,
    ) -> (
        Option<SessionModelState>,
        Option<Vec<SessionConfigOption>>,
        Option<Arc<dyn Provider>>,
    ) {
        let Ok((provider_name, model_config)) = resolved else {
            return (None, None, None);
        };

        let Some(mut inventory) = self
            .provider_inventory
            .entry_for_provider(provider_name)
            .await
            .ok()
            .flatten()
        else {
            return (None, None, None);
        };

        let mut prebuilt_provider = None;
        if should_refresh_inventory_for_session_init(&inventory) {
            match self.load_config() {
                Ok(config) => {
                    let ext_state = EnabledExtensionsState::extensions_or_default(
                        Some(&goose_session.extension_data),
                        &config,
                    );
                    Config::global().invalidate_secrets_cache();
                    match self
                        .create_provider(provider_name, model_config.clone(), ext_state)
                        .await
                    {
                        Ok(provider) => {
                            let provider_id = provider_name.clone();
                            prebuilt_provider = Some(provider.clone());
                            match self
                                .provider_inventory
                                .plan_refresh_jobs(std::slice::from_ref(&provider_id))
                                .await
                            {
                                Ok(plan)
                                    if plan
                                        .started
                                        .iter()
                                        .any(|job| job.provider_id == provider_id) =>
                                {
                                    let refresh_job = plan
                                        .started
                                        .into_iter()
                                        .find(|job| job.provider_id == provider_id);
                                    if let Some(refresh_job) = refresh_job {
                                        let mut refresh_guard = self
                                            .provider_inventory
                                            .refresh_guard(&refresh_job.identity);
                                        let fetch_result: Result<Vec<String>> =
                                            match ensure_refresh_identity_current(
                                                &provider_id,
                                                &refresh_job.identity,
                                            )
                                            .await
                                            {
                                                Ok(()) => match AssertUnwindSafe(
                                                    provider.fetch_recommended_models(),
                                                )
                                                .catch_unwind()
                                                .await
                                                {
                                                    Ok(Ok(models)) => Ok(models),
                                                    Ok(Err(error)) => {
                                                        Err(anyhow::anyhow!(error.to_string()))
                                                    }
                                                    Err(_) => Err(anyhow::anyhow!(
                                                        "provider inventory refresh task panicked"
                                                    )),
                                                },
                                                Err(error) => Err(error),
                                            };
                                        match fetch_result {
                                            Ok(models) => {
                                                if let Err(error) = self
                                                    .provider_inventory
                                                    .store_refreshed_models_for_identity(
                                                        &refresh_job.identity,
                                                        &models,
                                                    )
                                                    .await
                                                {
                                                    warn!(
                                                        provider = %provider_id,
                                                        error = %error,
                                                        "failed to store refreshed provider inventory during session init"
                                                    );
                                                } else {
                                                    refresh_guard.complete();
                                                }
                                            }
                                            Err(error) => {
                                                let error_message = error.to_string();
                                                if let Err(store_error) = self
                                                    .provider_inventory
                                                    .store_refresh_error_for_identity(
                                                        &refresh_job.identity,
                                                        error_message.clone(),
                                                    )
                                                    .await
                                                {
                                                    warn!(
                                                        provider = %provider_id,
                                                        error = %store_error,
                                                        "failed to store provider inventory refresh error during session init"
                                                    );
                                                } else {
                                                    refresh_guard.complete();
                                                }
                                                warn!(
                                                    provider = %provider_id,
                                                    error = %error_message,
                                                    "provider inventory refresh failed during session init"
                                                );
                                            }
                                        }
                                    }
                                }
                                Ok(_) => {}
                                Err(error) => warn!(
                                    provider = %provider_id,
                                    error = %error,
                                    "failed to plan provider inventory refresh during session init"
                                ),
                            }

                            if let Ok(Some(refreshed_inventory)) = self
                                .provider_inventory
                                .entry_for_provider(provider_name)
                                .await
                            {
                                inventory = refreshed_inventory;
                            }
                        }
                        Err(error) => warn!(
                            provider = %provider_name,
                            error = %error,
                            "failed to initialize provider during synchronous inventory refresh"
                        ),
                    }
                }
                Err(error) => warn!(
                    provider = %provider_name,
                    error = %error,
                    "failed to load config during synchronous inventory refresh"
                ),
            }
        }

        let (model_state, config_options) = build_eager_config_from_inventory(
            provider_name,
            model_config.model_name.as_str(),
            &inventory,
            mode_state,
            goose_session,
        )
        .await;
        (Some(model_state), Some(config_options), prebuilt_provider)
    }

    fn spawn_agent_setup(
        &self,
        cx: &ConnectionTo<Client>,
        agent_tx: tokio::sync::watch::Sender<AgentSetupSignal>,
        req: AgentSetupRequest,
    ) {
        let AgentSetupRequest {
            session_id,
            goose_session,
            mcp_servers,
            resolved_provider,
            prebuilt_provider,
        } = req;

        let goose_mode = goose_session.goose_mode;
        let internal_session_id = goose_session.id.clone();
        let agent_session_id = SessionId::new(internal_session_id.clone());
        let sid = sid_short(session_id.0.as_ref());

        let cx = cx.clone();
        let sessions = Arc::clone(&self.sessions);
        let session_manager = Arc::clone(&self.session_manager);
        let permission_manager = Arc::clone(&self.permission_manager);
        let config_dir = self.config_dir.clone();
        let builtins = self.builtins.clone();
        let client_fs_capabilities = self
            .client_fs_capabilities
            .get()
            .cloned()
            .unwrap_or_default();
        let client_terminal = self.client_terminal.get().copied().unwrap_or(false);
        let client_mcp_host_info = self.client_mcp_host_info.get().cloned();
        let provider_factory = Arc::clone(&self.provider_factory);
        let disable_session_naming = self.disable_session_naming;
        let goose_platform = self.goose_platform.clone();

        tokio::spawn(async move {
            let t_setup = std::time::Instant::now();
            debug!(target: "perf", sid = %sid, "perf: agent_setup start (background)");
            // Shared config — read once, used by both phases.
            let config = match Config::new(config_dir.join(CONFIG_YAML_NAME), "goose") {
                Ok(c) => c,
                Err(e) => {
                    let msg = e.to_string();
                    error!(error = %msg, "Background agent setup failed (config)");
                    let _ = agent_tx.send(Some(Err(msg)));
                    return;
                }
            };

            let session_name_update_tx =
                (!disable_session_naming).then(|| spawn_session_name_update_notifier(cx.clone()));

            // ── Phase 1: create agent + init provider (fast, ~55ms) ──────
            let phase1: Result<Arc<Agent>, String> = async {
                let agent = Arc::new(Agent::with_config(
                    AgentConfig::new(
                        session_manager,
                        permission_manager,
                        None,
                        goose_mode,
                        disable_session_naming,
                        goose_platform,
                    )
                    .with_mcp_host_info(client_mcp_host_info)
                    .with_session_name_update_tx(session_name_update_tx),
                ));

                // Init provider — reuse the pre-resolved name + model when
                // available (already computed in on_new_session), otherwise
                // fall back to reading config (e.g. load_session path).
                let (provider_name, model_config) = match resolved_provider {
                    Some(resolved) => resolved,
                    None => resolve_provider_and_model_from_config(&config, &goose_session).await?,
                };
                let ext_state = EnabledExtensionsState::extensions_or_default(
                    Some(&goose_session.extension_data),
                    &config,
                );
                let provider = match prebuilt_provider {
                    Some(provider) => provider,
                    None => provider_factory(provider_name.to_string(), model_config, ext_state)
                        .await
                        .map_err(|e| e.to_string())?,
                };
                agent
                    .update_provider(provider.clone(), &goose_session.id)
                    .await
                    .map_err(|e| e.to_string())?;

                agent
                    .update_goose_mode(goose_mode, &internal_session_id)
                    .await
                    .map_err(|e| e.to_string())?;

                Ok(agent)
            }
            .await;

            let agent = match phase1 {
                Ok(agent) => {
                    // Signal ProviderReady — unblocks setProvider / update_provider
                    // while extensions continue loading below.
                    let _ =
                        agent_tx.send(Some(Ok(AgentSetupProgress::ProviderReady(agent.clone()))));
                    debug!(target: "perf", sid = %sid, ms = t_setup.elapsed().as_millis() as u64, "perf: agent_setup provider_ready (signalled)");
                    agent
                }
                Err(e) => {
                    error!(error = %e, "Background agent setup failed (provider init)");
                    debug!(target: "perf", sid = %sid, ms = t_setup.elapsed().as_millis() as u64, "perf: agent_setup failed (provider)");
                    let _ = agent_tx.send(Some(Err(e)));
                    return;
                }
            };

            // ── Phase 2: load extensions (slow, may take seconds) ────────
            let phase2: Result<(), String> = async {
                let mut extensions = get_enabled_extensions_with_config(&config);
                extensions.extend(builtins.iter().map(|b| builtin_to_extension_config(b)));

                let acp_developer = if (client_fs_capabilities.read_text_file
                    || client_fs_capabilities.write_text_file
                    || client_terminal)
                    && extensions.iter().any(|e| e.name() == "developer")
                {
                    let context = agent.extension_manager.get_context().clone();
                    match DeveloperClient::new(context) {
                        Ok(dev_client) => {
                            let client: Arc<dyn McpClientTrait> = Arc::new(AcpTools {
                                inner: Arc::new(dev_client),
                                cx: cx.clone(),
                                session_id: session_id.clone(),
                                fs_read: client_fs_capabilities.read_text_file,
                                fs_write: client_fs_capabilities.write_text_file,
                                terminal: client_terminal,
                            });
                            let dev_ext = extensions.iter().find(|e| e.name() == "developer");
                            let available_tools = dev_ext
                                .and_then(|e| match e {
                                    ExtensionConfig::Platform {
                                        available_tools, ..
                                    } => Some(available_tools.clone()),
                                    _ => None,
                                })
                                .unwrap_or_default();
                            let def = &PLATFORM_EXTENSIONS["developer"];
                            let config = ExtensionConfig::Platform {
                                name: def.name.into(),
                                description: def.description.into(),
                                display_name: Some(def.display_name.into()),
                                bundled: Some(true),
                                available_tools,
                            };
                            Some((client, config))
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to create developer client");
                            None
                        }
                    }
                } else {
                    None
                };

                let skip_developer = acp_developer.is_some();
                let sid_str = Some(agent_session_id.0.to_string());

                if skip_developer {
                    extensions.retain(|ext| ext.name() != "developer");
                }

                let ext_manager = &agent.extension_manager;
                let extension_futures = extensions
                    .into_iter()
                    .map(|ext| {
                        let ext_manager = Arc::clone(ext_manager);
                        let sid_inner = sid_str.clone();
                        async move {
                            let name = ext.name().to_string();
                            if let Err(e) = ext_manager
                                .add_extension(ext, None, None, sid_inner.as_deref())
                                .await
                            {
                                warn!(extension = %name, error = %e, "extension load failed");
                            }
                        }
                    })
                    .collect::<Vec<_>>();
                futures::future::join_all(extension_futures).await;

                if let Some((client, config)) = acp_developer {
                    let info = client.get_info().cloned();
                    agent
                        .extension_manager
                        .add_client("developer".into(), config, client, info, None)
                        .await;
                }

                GooseAcpAgent::add_mcp_extensions(&agent, mcp_servers, &internal_session_id)
                    .await
                    .map_err(|e| e.to_string())?;

                Ok(())
            }
            .await;

            if let Err(e) = &phase2 {
                // Extension failures are non-fatal — individual failures are
                // already logged as warnings.  Log the top-level error but
                // don't block the session: the provider is ready and the agent
                // is usable.
                error!(error = %e, "Background agent setup: extension phase had errors");
            }

            // Promote the handle to Ready and apply any working directory that
            // was set while we were loading — regardless of phase-2 outcome,
            // since the agent (with its provider) is fully usable.
            {
                let mut locked = sessions.lock().await;
                if let Some(session) = locked.get_mut(session_id.0.as_ref()) {
                    if let Some(dir) = session.pending_working_dir.take() {
                        agent.extension_manager.update_working_dir(&dir).await;
                    }
                    session.agent = AgentHandle::Ready(agent.clone());
                }
            }

            let _ = agent_tx.send(Some(Ok(AgentSetupProgress::FullyReady(agent))));
            debug!(
                target: "perf",
                sid = %sid,
                ms = t_setup.elapsed().as_millis() as u64,
                "perf: agent_setup done{}",
                if phase2.is_err() { " (with extension errors)" } else { "" }
            );
        });
    }

    pub async fn has_session(&self, session_id: &str) -> bool {
        self.sessions.lock().await.contains_key(session_id)
    }

    /// Convert ACP prompt content blocks into a user message.
    fn convert_acp_prompt_to_message(prompt: &[ContentBlock]) -> Message {
        let mut message = Message::user();
        for block in prompt {
            match block {
                ContentBlock::Text(text) => {
                    let annotated = if let Some(ref ann) = text.annotations {
                        let audience: Vec<Role> = ann
                            .audience
                            .as_ref()
                            .map(|roles| {
                                roles
                                    .iter()
                                    .filter_map(|r| match r {
                                        agent_client_protocol::schema::Role::Assistant => {
                                            Some(Role::Assistant)
                                        }
                                        agent_client_protocol::schema::Role::User => {
                                            Some(Role::User)
                                        }
                                        _ => None,
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        let raw = RawTextContent {
                            text: sanitize_unicode_tags(&text.text),
                            meta: None,
                        };
                        if audience.is_empty() {
                            raw.no_annotation()
                        } else {
                            raw.no_annotation().with_audience(audience)
                        }
                    } else {
                        // No annotations — regular user text.
                        let sanitized = sanitize_unicode_tags(&text.text);
                        RawTextContent {
                            text: sanitized,
                            meta: None,
                        }
                        .no_annotation()
                    };
                    message = message.with_content(MessageContent::Text(annotated));
                }
                ContentBlock::Image(image) => {
                    message = message.with_image(&image.data, &image.mime_type);
                }
                ContentBlock::Resource(resource) => {
                    if let EmbeddedResourceResource::TextResourceContents(text_resource) =
                        &resource.resource
                    {
                        let header = format!("--- Resource: {} ---\n", text_resource.uri);
                        let content = format!("{}{}\n---\n", header, text_resource.text);
                        message = message.with_text(&content);
                    }
                }
                ContentBlock::ResourceLink(link) => {
                    if let Some(text) = read_resource_link(link.clone()) {
                        message = message.with_text(text);
                    }
                }
                ContentBlock::Audio(..) | _ => (),
            }
        }
        message
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_message_content(
        &self,
        content_item: &MessageContent,
        session_id: &SessionId,
        thread_id: &str,
        message_id: Option<&str>,
        agent: &Arc<Agent>,
        session: &mut GooseAcpSession,
        cx: &ConnectionTo<Client>,
    ) -> Result<(), agent_client_protocol::Error> {
        match content_item {
            MessageContent::Text(text) => {
                cx.send_notification(SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                        TextContent::new(text.text.clone()),
                    ))),
                ))?;
            }
            MessageContent::ToolRequest(tool_request) => {
                self.handle_tool_request(
                    tool_request,
                    session_id,
                    thread_id,
                    message_id,
                    session,
                    cx,
                )
                .await?;
            }
            MessageContent::ToolResponse(tool_response) => {
                self.handle_tool_response(
                    tool_response,
                    session_id,
                    thread_id,
                    message_id,
                    session,
                    cx,
                )
                .await?;
            }
            MessageContent::Thinking(thinking) => {
                cx.send_notification(SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(
                        TextContent::new(thinking.thinking.clone()),
                    ))),
                ))?;
            }
            MessageContent::ActionRequired(action_required) => {
                if let ActionRequiredData::ToolConfirmation {
                    id,
                    tool_name,
                    arguments,
                    prompt,
                } = &action_required.data
                {
                    self.handle_tool_permission_request(
                        cx,
                        agent,
                        session_id,
                        id.clone(),
                        tool_name.clone(),
                        arguments.clone(),
                        prompt.clone(),
                    )?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_tool_request(
        &self,
        tool_request: &crate::conversation::message::ToolRequest,
        session_id: &SessionId,
        thread_id: &str,
        message_id: Option<&str>,
        session: &mut GooseAcpSession,
        cx: &ConnectionTo<Client>,
    ) -> Result<(), agent_client_protocol::Error> {
        session
            .tool_requests
            .insert(tool_request.id.clone(), tool_request.clone());

        let pending_tool_call = pending_tool_call_from_request(tool_request);
        let initial_tool_call = pending_tool_call
            .tool_call
            .meta(pending_tool_call.identity_meta.clone());
        cx.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::ToolCall(initial_tool_call),
        ))?;

        if let Ok(tool_call) = &tool_request.tool_call {
            let agent = match &session.agent {
                AgentHandle::Ready(a) => a.clone(),
                AgentHandle::Loading(_) => return Ok(()),
            };
            let sid = session_id.clone();
            let request_id = tool_request.id.clone();
            let cx = cx.clone();
            let name = tool_call.name.to_string();
            let identity_meta = pending_tool_call.identity_meta.clone();
            let fallback_title = pending_tool_call.fallback_title.clone();
            let args_json = tool_call
                .arguments
                .as_ref()
                .map(|a| {
                    let s = serde_json::to_string(a).unwrap_or_default();
                    if s.len() > 300 {
                        format!("{}…", crate::utils::safe_truncate(&s, 300))
                    } else {
                        s
                    }
                })
                .unwrap_or_default();

            let thread_id_for_persist = thread_id.to_string();
            let message_id_for_persist = message_id.map(|s| s.to_string());
            let thread_manager = self.thread_manager.clone();

            tokio::spawn(async move {
                let (title, from_llm) = match agent.provider().await {
                    Ok(provider) => {
                        if provider.manages_own_context() {
                            return;
                        }

                        let system =
                            "Summarize this tool call in a short lowercase phrase (3-8 words). \
                             No punctuation. No quotes. Examples: reading project configuration, \
                             checking network connectivity, listing files in src directory";
                        let user_text = format!("Tool: {name}\nArguments: {args_json}");
                        let message = Message::user().with_text(&user_text);
                        // The fast model occasionally returns an empty response
                        // under load (rate limiting, transient network). One
                        // retry with a short backoff is enough to recover the
                        // common cases without paying for the regular model.
                        let mut llm_outcome: Option<String> = None;
                        for attempt in 0..2 {
                            match provider
                                .complete_fast(&sid.0, system, std::slice::from_ref(&message), &[])
                                .await
                            {
                                Ok((response, _)) => {
                                    let summary: String = response
                                        .content
                                        .iter()
                                        .filter_map(|c: &MessageContent| c.as_text())
                                        .collect::<String>()
                                        .trim()
                                        .to_string();
                                    if !summary.is_empty() {
                                        llm_outcome = Some(summary);
                                        break;
                                    }
                                    if attempt == 0 {
                                        warn!(
                                            "tool call summary: fast_complete returned empty for {request_id} ({name}), retrying once",
                                        );
                                        tokio::time::sleep(std::time::Duration::from_millis(150))
                                            .await;
                                    }
                                }
                                Err(e) => {
                                    if attempt == 0 {
                                        warn!(
                                            "tool call summary: fast_complete errored for {request_id} ({name}): {e}, retrying once",
                                        );
                                        tokio::time::sleep(std::time::Duration::from_millis(150))
                                            .await;
                                    } else {
                                        warn!(
                                            "tool call summary: fast_complete errored for {request_id} ({name}) after retry: {e}",
                                        );
                                    }
                                }
                            }
                        }
                        match llm_outcome {
                            Some(summary) => (summary, true),
                            None => {
                                warn!(
                                    "tool call summary: falling back to deterministic title for {request_id} ({name}) — replay will not show an LLM summary for this call",
                                );
                                (fallback_title.clone(), false)
                            }
                        }
                    }
                    Err(e) => {
                        warn!("tool call summary: failed to get provider: {e}");
                        (fallback_title.clone(), false)
                    }
                };

                let fields = ToolCallUpdateFields::new().title(title.clone());
                let _ = cx.send_notification(SessionNotification::new(
                    sid,
                    SessionUpdate::ToolCallUpdate(
                        ToolCallUpdate::new(ToolCallId::new(request_id.clone()), fields)
                            .meta(identity_meta),
                    ),
                ));

                // Best-effort persistence: only persist the LLM-generated title
                // (not the deterministic fallback) so reload uses fallback_title
                // for older or failed cases just like today. Surface persist
                // errors at warn level so the "occasional bad replay" symptom is
                // diagnosable from logs alone.
                if from_llm {
                    if let Some(msg_id) = message_id_for_persist {
                        let patch = serde_json::json!({
                            crate::conversation::message::TOOL_META_TITLE_KEY: title,
                        });
                        if let Err(e) = thread_manager
                            .update_tool_request_meta(
                                &thread_id_for_persist,
                                &msg_id,
                                &request_id,
                                patch,
                            )
                            .await
                        {
                            warn!(
                                "tool call summary: persist failed for {request_id} in {msg_id}: {e}",
                            );
                        }
                    } else {
                        warn!(
                            "tool call summary: missing message_id for {request_id} — title will not survive reload",
                        );
                    }
                }
            });
        }

        Ok(())
    }

    async fn handle_tool_response(
        &self,
        tool_response: &crate::conversation::message::ToolResponse,
        session_id: &SessionId,
        thread_id: &str,
        message_id: Option<&str>,
        session: &mut GooseAcpSession,
        cx: &ConnectionTo<Client>,
    ) -> Result<(), agent_client_protocol::Error> {
        let status = match &tool_response.tool_result {
            Ok(result) if result.is_error == Some(true) => ToolCallStatus::Failed,
            Ok(_) => ToolCallStatus::Completed,
            Err(_) => ToolCallStatus::Failed,
        };

        let mut fields = ToolCallUpdateFields::new().status(status);
        if let Some(raw_output) = extract_tool_raw_output(&tool_response.tool_result) {
            fields = fields.raw_output(raw_output);
        }
        if !tool_response
            .tool_result
            .as_ref()
            .is_ok_and(|r| r.is_acp_aware())
        {
            let content = build_tool_call_content(&tool_response.tool_result);
            fields = fields.content(content);

            let locations = extract_locations_from_meta(tool_response).unwrap_or_else(|| {
                if let Some(tool_request) = session.tool_requests.get(&tool_response.id) {
                    extract_tool_locations(tool_request, tool_response)
                } else {
                    Vec::new()
                }
            });
            if !locations.is_empty() {
                fields = fields.locations(locations);
            }
        }

        let update = ToolCallUpdate::new(ToolCallId::new(tool_response.id.clone()), fields)
            .meta(extract_tool_call_update_meta(tool_response));
        cx.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::ToolCallUpdate(update),
        ))?;

        // Chain summarization: when this response completes a multi-tool
        // chain, fire one LLM summary covering the run.
        session.responded_tool_ids.insert(tool_response.id.clone());
        self.maybe_summarize_chain(&tool_response.id, session_id, thread_id, session, cx);
        let _ = message_id;

        Ok(())
    }

    /// If `tool_call_id` belongs to a multi-tool chain and every step in that
    /// chain has now had its response processed, spawn a single LLM
    /// summarization task that persists the chain summary on the first tool
    /// request and notifies the client. Idempotent — fires at most once per
    /// chain.
    fn maybe_summarize_chain(
        &self,
        tool_call_id: &str,
        session_id: &SessionId,
        thread_id: &str,
        session: &mut GooseAcpSession,
        cx: &ConnectionTo<Client>,
    ) {
        let Some(chain) = session.chain_membership.get(tool_call_id).cloned() else {
            warn!(
                "tool chain summary: skipped — no chain registered for tool_call_id {tool_call_id}",
            );
            return;
        };
        if !chain
            .ids
            .iter()
            .all(|id| session.responded_tool_ids.contains(id))
        {
            let total = chain.ids.len();
            let responded = chain
                .ids
                .iter()
                .filter(|id| session.responded_tool_ids.contains(*id))
                .count();
            let missing: Vec<&String> = chain
                .ids
                .iter()
                .filter(|id| !session.responded_tool_ids.contains(*id))
                .collect();
            warn!(
                "tool chain summary: waiting on {pending}/{total} responses for chain anchored at {anchor:?} (missing: {missing:?})",
                pending = total - responded,
                anchor = chain.ids.first(),
            );
            return;
        }
        let Some(first_id) = chain.ids.first() else {
            warn!("tool chain summary: skipped — empty chain.ids for tool_call_id {tool_call_id}");
            return;
        };
        if !session.summarized_chains.insert(first_id.clone()) {
            debug!("tool chain summary: chain anchored at {first_id} already summarized; skipping");
            return;
        }

        let agent = match &session.agent {
            AgentHandle::Ready(a) => a.clone(),
            AgentHandle::Loading(_) => {
                warn!(
                    "tool chain summary: agent still loading; skipping chain anchored at {first_id}",
                );
                return;
            }
        };

        // Snapshot (name, args_json) for each step in document order.
        let steps: Vec<(String, String)> = chain
            .ids
            .iter()
            .filter_map(|id| {
                let req = session.tool_requests.get(id)?;
                let tool_call = req.tool_call.as_ref().ok()?;
                let name = tool_call.name.to_string();
                let args = tool_call
                    .arguments
                    .as_ref()
                    .map(|a| serde_json::to_string(a).unwrap_or_default())
                    .unwrap_or_default();
                let args = if args.len() > 200 {
                    format!("{}…", crate::utils::safe_truncate(&args, 200))
                } else {
                    args
                };
                Some((name, args))
            })
            .collect();
        if steps.len() < 2 {
            return;
        }

        let identity_meta = session
            .tool_requests
            .get(first_id)
            .and_then(tool_call_identity_meta);

        let sid = session_id.clone();
        let thread_id_for_persist = thread_id.to_string();
        let chain_for_task = chain.clone();
        let cx = cx.clone();
        let thread_manager = self.thread_manager.clone();

        let first_id = first_id.clone();
        tokio::spawn(async move {
            let provider = match agent.provider().await {
                Ok(p) => p,
                Err(e) => {
                    warn!(
                        "tool chain summary: failed to get provider for chain anchored at {first_id}: {e}",
                    );
                    return;
                }
            };
            if provider.manages_own_context() {
                warn!(
                    "tool chain summary: provider manages own context; skipping chain anchored at {first_id}",
                );
                return;
            }

            let system = "Summarize this sequence of tool calls in a short lowercase phrase \
                 (3-8 words). No punctuation. No quotes. \
                 Examples: applied dark mode polish, scanned for security issues, \
                 refactored config loading";

            let mut user_text = String::from("Tool call sequence:\n");
            for (i, (name, args)) in steps.iter().enumerate() {
                user_text.push_str(&format!("Step {}: {} {}\n", i + 1, name, args));
            }
            let message = Message::user().with_text(&user_text);

            // Match the per-tool retry policy: one retry on empty/error keeps
            // the chain header reliable when the fast model is rate-limited or
            // momentarily flaky, without escalating to the regular model.
            let mut summary: Option<String> = None;
            for attempt in 0..2 {
                match provider
                    .complete_fast(&sid.0, system, std::slice::from_ref(&message), &[])
                    .await
                {
                    Ok((response, _)) => {
                        let s = response
                            .content
                            .iter()
                            .filter_map(|c: &MessageContent| c.as_text())
                            .collect::<String>()
                            .trim()
                            .to_string();
                        if !s.is_empty() {
                            summary = Some(s);
                            break;
                        }
                        if attempt == 0 {
                            warn!(
                                "tool chain summary: fast_complete returned empty for chain anchored at {first_id} ({} steps), retrying once",
                                steps.len(),
                            );
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        }
                    }
                    Err(e) => {
                        if attempt == 0 {
                            warn!(
                                "tool chain summary: fast_complete errored for chain anchored at {first_id}: {e}, retrying once",
                            );
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        } else {
                            warn!(
                                "tool chain summary: fast_complete errored for chain anchored at {first_id} after retry: {e}",
                            );
                        }
                    }
                }
            }
            let Some(summary) = summary else {
                warn!(
                    "tool chain summary: no LLM summary produced for chain anchored at {first_id} — replay will fall back to the deterministic phrase",
                );
                return;
            };

            let count = chain_for_task.ids.len();
            let patch = serde_json::json!({
                crate::conversation::message::TOOL_META_CHAIN_SUMMARY_KEY: {
                    "summary": &summary,
                    "count": count,
                },
            });
            if let Err(e) = thread_manager
                .update_tool_request_meta(
                    &thread_id_for_persist,
                    &chain_for_task.message_id,
                    &first_id,
                    patch,
                )
                .await
            {
                warn!(
                    "tool chain summary: persist failed for chain anchored at {first_id} in {}: {e}",
                    chain_for_task.message_id,
                );
            }

            let meta = with_tool_chain_summary_meta(identity_meta, &summary, count);
            let fields = ToolCallUpdateFields::new();
            let _ = cx.send_notification(SessionNotification::new(
                sid,
                SessionUpdate::ToolCallUpdate(
                    ToolCallUpdate::new(ToolCallId::new(first_id), fields).meta(meta),
                ),
            ));
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_tool_permission_request(
        &self,
        cx: &ConnectionTo<Client>,
        agent: &Arc<Agent>,
        session_id: &SessionId,
        request_id: String,
        tool_name: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        prompt: Option<String>,
    ) -> Result<(), agent_client_protocol::Error> {
        let cx = cx.clone();
        let agent = agent.clone();
        let session_id = session_id.clone();

        let formatted_name = format_tool_name(&tool_name);

        let mut fields = ToolCallUpdateFields::new()
            .title(formatted_name)
            .kind(ToolKind::default())
            .status(ToolCallStatus::Pending)
            .raw_input(serde_json::Value::Object(arguments));
        if let Some(p) = prompt {
            fields = fields.content(vec![ToolCallContent::Content(Content::new(
                ContentBlock::Text(TextContent::new(p)),
            ))]);
        }
        let tool_call_update = ToolCallUpdate::new(ToolCallId::new(request_id.clone()), fields);

        fn option(kind: PermissionOptionKind) -> PermissionOption {
            let id = serde_json::to_value(kind)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            PermissionOption::new(id.clone(), id, kind)
        }
        let options = vec![
            option(PermissionOptionKind::AllowAlways),
            option(PermissionOptionKind::AllowOnce),
            option(PermissionOptionKind::RejectOnce),
            option(PermissionOptionKind::RejectAlways),
        ];

        let permission_request =
            RequestPermissionRequest::new(session_id, tool_call_update, options);

        cx.send_request(permission_request)
            .on_receiving_result(move |result| async move {
                match result {
                    Ok(response) => {
                        agent
                            .handle_confirmation(
                                request_id,
                                outcome_to_confirmation(&response.outcome),
                            )
                            .await;
                        Ok(())
                    }
                    Err(e) => {
                        error!(error = ?e, "permission request failed");
                        agent
                            .handle_confirmation(
                                request_id,
                                PermissionConfirmation {
                                    principal_type: PrincipalType::Tool,
                                    permission: Permission::Cancel,
                                },
                            )
                            .await;
                        Ok(())
                    }
                }
            })?;

        Ok(())
    }
}

fn outcome_to_confirmation(outcome: &RequestPermissionOutcome) -> PermissionConfirmation {
    PermissionConfirmation {
        principal_type: PrincipalType::Tool,
        permission: Permission::from(PermissionDecision::from(outcome)),
    }
}

fn extract_tool_call_update_meta(
    tool_response: &crate::conversation::message::ToolResponse,
) -> Option<Meta> {
    let tool_result = tool_response.tool_result.as_ref().ok()?;
    let goose_meta = tool_result
        .meta
        .as_ref()?
        .0
        .get(TRUSTED_TOOL_UPDATE_META_KEY)?
        .clone();
    let mut meta_map = serde_json::Map::new();
    meta_map.insert("goose".to_string(), goose_meta);
    Some(meta_map)
}

fn replay_message_meta(message: &Message) -> Meta {
    let mut meta = serde_json::Map::new();
    meta.insert(
        "goose".to_string(),
        serde_json::Value::Object(replay_message_goose_meta(message)),
    );
    meta
}

fn replay_message_goose_meta(message: &Message) -> serde_json::Map<String, serde_json::Value> {
    let mut goose = serde_json::Map::new();
    goose.insert("created".to_string(), serde_json::json!(message.created));
    if let Some(id) = &message.id {
        goose.insert("messageId".to_string(), serde_json::json!(id));
    }
    goose
}

fn merge_replay_message_meta(meta: Option<Meta>, message: &Message) -> Meta {
    let replay_goose = replay_message_goose_meta(message);
    let mut meta = meta.unwrap_or_default();
    let goose_value = meta
        .entry("goose".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

    if let serde_json::Value::Object(goose) = goose_value {
        for (key, value) in replay_goose {
            goose.insert(key, value);
        }
    } else {
        *goose_value = serde_json::Value::Object(replay_goose);
    }

    meta
}

fn build_tool_call_content(tool_result: &ToolResult<CallToolResult>) -> Vec<ToolCallContent> {
    match tool_result {
        Ok(result) => result
            .content
            .iter()
            .filter_map(|content| match &content.raw {
                RawContent::Text(val) => Some(ToolCallContent::Content(Content::new(
                    ContentBlock::Text(TextContent::new(val.text.clone())),
                ))),
                RawContent::Image(val) => Some(ToolCallContent::Content(Content::new(
                    ContentBlock::Image(ImageContent::new(val.data.clone(), val.mime_type.clone())),
                ))),
                RawContent::Resource(val) => {
                    let resource = match &val.resource {
                        ResourceContents::TextResourceContents {
                            mime_type,
                            text,
                            uri,
                            ..
                        } => EmbeddedResourceResource::TextResourceContents(
                            TextResourceContents::new(text.clone(), uri.clone())
                                .mime_type(mime_type.clone()),
                        ),
                        ResourceContents::BlobResourceContents {
                            mime_type,
                            blob,
                            uri,
                            ..
                        } => EmbeddedResourceResource::BlobResourceContents(
                            BlobResourceContents::new(blob.clone(), uri.clone())
                                .mime_type(mime_type.clone()),
                        ),
                    };
                    Some(ToolCallContent::Content(Content::new(
                        ContentBlock::Resource(EmbeddedResource::new(resource)),
                    )))
                }
                RawContent::Audio(_) | RawContent::ResourceLink(_) => None,
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn extract_tool_raw_output(tool_result: &ToolResult<CallToolResult>) -> Option<serde_json::Value> {
    tool_result
        .as_ref()
        .ok()
        .and_then(|result| result.structured_content.clone())
}

impl GooseAcpAgent {
    async fn on_initialize(
        &self,
        args: InitializeRequest,
    ) -> Result<InitializeResponse, agent_client_protocol::Error> {
        debug!(?args, "initialize request");

        let _ = self
            .client_fs_capabilities
            .set(args.client_capabilities.fs.clone());
        let _ = self.client_terminal.set(args.client_capabilities.terminal);
        let _ = self
            .client_mcp_host_info
            .set(extract_client_mcp_host_info(&args));

        let capabilities = AgentCapabilities::new()
            .load_session(true)
            .session_capabilities(
                SessionCapabilities::new()
                    .list(SessionListCapabilities::new())
                    .close(SessionCloseCapabilities::new()),
            )
            .prompt_capabilities(
                PromptCapabilities::new()
                    .image(true)
                    .audio(false)
                    .embedded_context(true),
            )
            .mcp_capabilities(McpCapabilities::new().http(true));
        Ok(InitializeResponse::new(args.protocol_version)
            .agent_capabilities(capabilities)
            .auth_methods(vec![AuthMethod::Agent(
                AuthMethodAgent::new("goose-provider", "Configure Provider")
                    .description("Run `goose configure` to set up your AI provider and API key"),
            )]))
    }

    async fn on_new_session(
        &self,
        cx: &ConnectionTo<Client>,
        args: NewSessionRequest,
    ) -> Result<NewSessionResponse, agent_client_protocol::Error> {
        debug!(?args, "new session request");
        let t_start = std::time::Instant::now();

        let requested_provider = args
            .meta
            .as_ref()
            .and_then(|m| m.get("provider"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let project_id = args
            .meta
            .as_ref()
            .and_then(|m| m.get("projectId"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let persona_id = args
            .meta
            .as_ref()
            .and_then(|m| m.get("personaId"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Create the Thread — this IS the ACP session from the client's perspective.
        let thread_metadata = crate::session::ThreadMetadata {
            provider_id: requested_provider.clone(),
            project_id,
            persona_id,
            mode: Some(self.goose_mode.to_string()),
            ..Default::default()
        };
        let t0 = std::time::Instant::now();
        let thread = self
            .thread_manager
            .create_thread(
                None,
                Some(thread_metadata),
                Some(args.cwd.display().to_string()),
            )
            .await
            .internal_err_ctx("Failed to create thread")?;
        let thread_id = thread.id.clone();
        let sid = sid_short(&thread_id);
        debug!(target: "perf", sid = %sid, ms = t0.elapsed().as_millis() as u64, "perf: new_session create_thread");

        // Create the first internal Session linked to this thread.
        let t1 = std::time::Instant::now();
        let goose_session = self
            .create_internal_session(
                &thread_id,
                args.cwd.clone(),
                requested_provider.as_deref(),
                None,
            )
            .await?;
        debug!(target: "perf", sid = %sid, ms = t1.elapsed().as_millis() as u64, "perf: new_session create_internal_session");

        let internal_session_id = goose_session.id.clone();

        let (agent_tx, agent_rx) = tokio::sync::watch::channel::<AgentSetupSignal>(None);

        let session = GooseAcpSession {
            agent: AgentHandle::Loading(agent_rx),
            internal_session_id: internal_session_id.clone(),
            tool_requests: HashMap::new(),
            chain_membership: HashMap::new(),
            responded_tool_ids: HashSet::new(),
            summarized_chains: HashSet::new(),
            cancel_token: None,
            pending_working_dir: None,
        };
        self.sessions
            .lock()
            .await
            .insert(thread_id.clone(), session);

        let mode_state = build_mode_state(self.goose_mode)?;

        // Resolve provider + model from config so we can include the current
        // model in the response without waiting for the full agent setup.
        let resolved = resolve_provider_and_model(&self.config_dir, &goose_session).await;
        let initial_usage_update = resolved
            .as_ref()
            .ok()
            .map(|(_, mc)| build_usage_update(&goose_session, mc.context_limit()));
        let session_id = SessionId::new(thread_id.clone());
        let (model_state, config_options, prebuilt_provider) = self
            .prepare_session_init_config(&resolved, &mode_state, &goose_session)
            .await;

        self.spawn_agent_setup(
            cx,
            agent_tx,
            AgentSetupRequest {
                session_id: session_id.clone(),
                goose_session,
                mcp_servers: args.mcp_servers,
                resolved_provider: resolved.as_ref().ok().cloned(),
                prebuilt_provider,
            },
        );

        let mut response = NewSessionResponse::new(session_id.clone()).modes(mode_state);
        if let Some(ms) = model_state {
            response = response.models(ms);
        }
        if let Some(co) = config_options {
            response = response.config_options(co);
        }
        if let Some(usage_update) = initial_usage_update {
            cx.send_notification(SessionNotification::new(
                session_id,
                SessionUpdate::UsageUpdate(usage_update),
            ))?;
        }
        debug!(
            target: "perf",
            sid = %sid,
            ms = t_start.elapsed().as_millis() as u64,
            "perf: new_session done (agent setup continues in background)"
        );
        Ok(response)
    }

    /// Create a new internal goose Session linked to a thread.
    /// This is the agent's working state — invisible to ACP clients.
    async fn create_internal_session(
        &self,
        thread_id: &str,
        cwd: std::path::PathBuf,
        provider_name: Option<&str>,
        model_name: Option<&str>,
    ) -> Result<Session, agent_client_protocol::Error> {
        let goose_session = self
            .session_manager
            .create_session(
                cwd,
                "ACP Session".to_string(),
                SessionType::Acp,
                self.goose_mode,
            )
            .await
            .internal_err_ctx("Failed to create session")?;

        let mut builder = self.session_manager.update(&goose_session.id);
        builder = builder.thread_id(Some(thread_id.to_string()));
        if let Some(provider) = provider_name {
            builder = builder.provider_name(provider);
        }
        if let Some(model) = model_name {
            if let Ok(mc) = crate::model::ModelConfig::new(model) {
                builder = builder.model_config(mc);
            }
        }
        builder
            .apply()
            .await
            .internal_err_ctx("Failed to link session to thread")?;

        self.session_manager
            .get_session(&goose_session.id, false)
            .await
            .internal_err_ctx("Failed to reload session")
    }

    /// Look up the session and return the agent if already ready, or the watch
    /// receiver if still loading.  Optionally sets a cancellation token on the
    /// session (needed by `on_prompt`).
    async fn get_agent_or_receiver(
        &self,
        thread_id: &str,
        cancel_token: Option<CancellationToken>,
    ) -> Result<
        Either<Arc<Agent>, tokio::sync::watch::Receiver<AgentSetupSignal>>,
        agent_client_protocol::Error,
    > {
        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(thread_id).ok_or_else(|| {
            agent_client_protocol::Error::resource_not_found(Some(thread_id.to_string()))
                .data(format!("Session not found: {}", thread_id))
        })?;
        if let Some(token) = cancel_token {
            session.cancel_token = Some(token);
        }
        match &session.agent {
            AgentHandle::Ready(agent) => Ok(Either::Left(agent.clone())),
            AgentHandle::Loading(rx) => Ok(Either::Right(rx.clone())),
        }
    }

    /// Wait until the agent is **fully ready** (provider + all extensions).
    /// Most callers (e.g. `on_prompt`, `on_get_tools`) should use this.
    async fn get_session_agent(
        &self,
        thread_id: &str,
        cancel_token: Option<CancellationToken>,
    ) -> Result<Arc<Agent>, agent_client_protocol::Error> {
        let mut rx = match self.get_agent_or_receiver(thread_id, cancel_token).await? {
            Either::Left(agent) => return Ok(agent),
            Either::Right(rx) => rx,
        };
        // Wait specifically for FullyReady (not just ProviderReady).
        let guard = rx
            .wait_for(|v| {
                matches!(
                    v,
                    Some(Ok(AgentSetupProgress::FullyReady(_))) | Some(Err(_))
                )
            })
            .await
            .map_err(|_| {
                agent_client_protocol::Error::internal_error()
                    .data("Agent setup task was dropped".to_string())
            })?;
        match guard.as_ref().unwrap() {
            Ok(AgentSetupProgress::FullyReady(agent)) => Ok(agent.clone()),
            Err(e) => Err(agent_client_protocol::Error::internal_error().data(e.clone())),
            // wait_for predicate excludes ProviderReady
            _ => unreachable!(),
        }
    }

    /// Wait only until the **provider** is initialized.  Extensions may still
    /// be loading in the background.  Use this for operations that only touch
    /// the provider (e.g. `update_provider`, `set_model`, `build_config_update`).
    async fn get_session_agent_provider_ready(
        &self,
        thread_id: &str,
    ) -> Result<Arc<Agent>, agent_client_protocol::Error> {
        let mut rx = match self.get_agent_or_receiver(thread_id, None).await? {
            Either::Left(agent) => return Ok(agent),
            Either::Right(rx) => rx,
        };
        // Any signal (ProviderReady, FullyReady, or Err) unblocks us.
        let guard = rx.wait_for(|v| v.is_some()).await.map_err(|_| {
            agent_client_protocol::Error::internal_error()
                .data("Agent setup task was dropped".to_string())
        })?;
        match guard.as_ref().unwrap() {
            Ok(progress) => match progress {
                AgentSetupProgress::ProviderReady(agent)
                | AgentSetupProgress::FullyReady(agent) => Ok(agent.clone()),
            },
            Err(e) => Err(agent_client_protocol::Error::internal_error().data(e.clone())),
        }
    }

    async fn add_mcp_extensions(
        agent: &Arc<Agent>,
        mcp_servers: Vec<McpServer>,
        internal_session_id: &str,
    ) -> Result<(), agent_client_protocol::Error> {
        let mut configs = Vec::with_capacity(mcp_servers.len());
        for mcp_server in mcp_servers {
            let config = match mcp_server_to_extension_config(mcp_server) {
                Ok(c) => c,
                Err(msg) => {
                    return Err(agent_client_protocol::Error::invalid_params().data(msg));
                }
            };
            configs.push(config);
        }

        if configs.is_empty() {
            return Ok(());
        }

        let results = agent
            .add_extensions_bulk(configs, internal_session_id)
            .await
            .internal_err()?;
        for result in &results {
            if !result.success {
                let error_msg = result.error.as_deref().unwrap_or("unknown error");
                return Err(agent_client_protocol::Error::internal_error().data(format!(
                    "Failed to add MCP server '{}': {}",
                    result.name, error_msg
                )));
            }
        }
        Ok(())
    }

    async fn on_load_session(
        &self,
        cx: &ConnectionTo<Client>,
        args: LoadSessionRequest,
    ) -> Result<LoadSessionResponse, agent_client_protocol::Error> {
        debug!(?args, "load session request");

        // The ACP session_id IS the thread ID.
        let thread_id = args.session_id.0.to_string();
        let sid = sid_short(&thread_id);
        let t_start = std::time::Instant::now();

        let t0 = std::time::Instant::now();
        let thread = self
            .thread_manager
            .get_thread(&thread_id)
            .await
            .map_err(|_| {
                agent_client_protocol::Error::resource_not_found(Some(thread_id.clone()))
                    .data(format!("Session not found: {}", thread_id))
            })?;
        debug!(target: "perf", sid = %sid, ms = t0.elapsed().as_millis() as u64, "perf: load_session get_thread");

        // Reuse the thread's current internal session so the agent retains
        // conversation context (compaction state, full message history, etc.).
        // The internal session is the source of truth for provider/mode.
        let internal_session_id = thread.current_session_id.clone().ok_or_else(|| {
            agent_client_protocol::Error::internal_error()
                .data(format!("Thread {} has no internal session", thread_id))
        })?;
        let t1 = std::time::Instant::now();
        let goose_session = self
            .session_manager
            .get_session(&internal_session_id, false)
            .await
            .internal_err_ctx("Failed to load internal session")?;
        debug!(target: "perf", sid = %sid, ms = t1.elapsed().as_millis() as u64, "perf: load_session get_session");
        let loaded_mode = goose_session.goose_mode;

        // ── REPLAY MESSAGES FIRST ──
        // Stream the thread's human-visible message history back to the client
        // immediately, before the slow agent/provider/extension setup. The
        // replay only needs the thread_manager (SQLite reads) so the UI gets
        // messages while the agent is still booting.
        let t2 = std::time::Instant::now();
        let thread_messages = self
            .thread_manager
            .list_messages(&thread_id)
            .await
            .internal_err_ctx("Failed to load thread messages")?;
        debug!(
            target: "perf",
            sid = %sid,
            ms = t2.elapsed().as_millis() as u64,
            messages = thread_messages.len(),
            "perf: load_session list_messages"
        );

        // Lightweight tool_requests map for the replay loop — we only need it
        // so that handle_tool_response can extract file locations from the
        // matching request. No GooseAcpSession required.
        let mut replay_tool_requests =
            HashMap::<String, crate::conversation::message::ToolRequest>::new();

        for message in &thread_messages {
            if !message.metadata.user_visible {
                continue;
            }

            for content_item in &message.content {
                match content_item {
                    MessageContent::Text(text) => {
                        let mut tc = TextContent::new(text.text.clone());
                        if let Some(audience) = text.audience() {
                            tc = tc.annotations(
                                Annotations::new().audience(
                                    audience
                                        .iter()
                                        .map(|r| match r {
                                            Role::Assistant => {
                                                agent_client_protocol::schema::Role::Assistant
                                            }
                                            Role::User => agent_client_protocol::schema::Role::User,
                                        })
                                        .collect::<Vec<_>>(),
                                ),
                            );
                        }
                        let chunk = ContentChunk::new(ContentBlock::Text(tc))
                            .meta(replay_message_meta(message));
                        let update = match message.role {
                            Role::User => SessionUpdate::UserMessageChunk(chunk),
                            Role::Assistant => SessionUpdate::AgentMessageChunk(chunk),
                        };
                        cx.send_notification(SessionNotification::new(
                            args.session_id.clone(),
                            update,
                        ))?;
                    }
                    MessageContent::ToolRequest(tool_request) => {
                        // Replay-only: emit the ToolCall notification and
                        // stash the request for location extraction, but
                        // don't require a full GooseAcpSession.
                        replay_tool_requests.insert(tool_request.id.clone(), tool_request.clone());

                        let pending_tool_call = pending_tool_call_from_request(tool_request);
                        let mut meta = pending_tool_call.identity_meta;
                        // If this tool request is the first of a chain whose
                        // summary was persisted at completion time, attach the
                        // chain summary to the initial ToolCall so the chain
                        // header is correct on first paint after reload.
                        if let Some(chain_summary) = tool_request.persisted_chain_summary() {
                            meta = with_tool_chain_summary_meta(
                                meta,
                                &chain_summary.summary,
                                chain_summary.count,
                            );
                        }
                        let tool_call = pending_tool_call
                            .tool_call
                            .meta(merge_replay_message_meta(meta, message));

                        cx.send_notification(SessionNotification::new(
                            args.session_id.clone(),
                            SessionUpdate::ToolCall(tool_call),
                        ))?;
                    }
                    MessageContent::ToolResponse(tool_response) => {
                        // Replay-only: emit the ToolCallUpdate notification,
                        // using the stashed replay_tool_requests for location
                        // extraction.
                        let status = match &tool_response.tool_result {
                            Ok(result) if result.is_error == Some(true) => ToolCallStatus::Failed,
                            Ok(_) => ToolCallStatus::Completed,
                            Err(_) => ToolCallStatus::Failed,
                        };

                        let mut fields = ToolCallUpdateFields::new().status(status);
                        if let Some(raw_output) =
                            extract_tool_raw_output(&tool_response.tool_result)
                        {
                            fields = fields.raw_output(raw_output);
                        }
                        if !tool_response
                            .tool_result
                            .as_ref()
                            .is_ok_and(|r| r.is_acp_aware())
                        {
                            let content = build_tool_call_content(&tool_response.tool_result);
                            fields = fields.content(content);

                            let locations = extract_locations_from_meta(tool_response)
                                .unwrap_or_else(|| {
                                    if let Some(tool_request) =
                                        replay_tool_requests.get(&tool_response.id)
                                    {
                                        extract_tool_locations(tool_request, tool_response)
                                    } else {
                                        Vec::new()
                                    }
                                });
                            if !locations.is_empty() {
                                fields = fields.locations(locations);
                            }
                        }

                        let update =
                            ToolCallUpdate::new(ToolCallId::new(tool_response.id.clone()), fields)
                                .meta(merge_replay_message_meta(
                                    extract_tool_call_update_meta(tool_response),
                                    message,
                                ));
                        cx.send_notification(SessionNotification::new(
                            args.session_id.clone(),
                            SessionUpdate::ToolCallUpdate(update),
                        ))?;
                    }
                    MessageContent::Thinking(thinking) => {
                        cx.send_notification(SessionNotification::new(
                            args.session_id.clone(),
                            SessionUpdate::AgentThoughtChunk(
                                ContentChunk::new(ContentBlock::Text(TextContent::new(
                                    thinking.thinking.clone(),
                                )))
                                .meta(replay_message_meta(message)),
                            ),
                        ))?;
                    }
                    _ => {}
                }
            }
        }

        // ── Lightweight DB updates (fast) ──
        self.session_manager
            .update(&internal_session_id)
            .working_dir(args.cwd.clone())
            .apply()
            .await
            .internal_err_ctx("Failed to update session working directory")?;

        self.thread_manager
            .update_working_dir(&thread_id, &args.cwd.display().to_string())
            .await
            .internal_err_ctx("Failed to update thread working directory")?;

        // ── Register the session immediately with a Loading handle ──
        let (agent_tx, agent_rx) = tokio::sync::watch::channel::<AgentSetupSignal>(None);

        let session = GooseAcpSession {
            agent: AgentHandle::Loading(agent_rx),
            internal_session_id: internal_session_id.clone(),
            tool_requests: replay_tool_requests,
            chain_membership: HashMap::new(),
            responded_tool_ids: HashSet::new(),
            summarized_chains: HashSet::new(),
            cancel_token: None,
            pending_working_dir: None,
        };
        self.sessions
            .lock()
            .await
            .insert(thread_id.clone(), session);

        let mode_state = build_mode_state(loaded_mode)?;

        let resolved = resolve_provider_and_model(&self.config_dir, &goose_session).await;
        let initial_usage_update = resolved
            .as_ref()
            .ok()
            .map(|(_, mc)| build_usage_update(&goose_session, mc.context_limit()))
            .or_else(|| {
                goose_session
                    .model_config
                    .as_ref()
                    .map(|mc| build_usage_update(&goose_session, mc.context_limit()))
            });
        let (model_state, config_options, prebuilt_provider) = self
            .prepare_session_init_config(&resolved, &mode_state, &goose_session)
            .await;

        self.spawn_agent_setup(
            cx,
            agent_tx,
            AgentSetupRequest {
                session_id: args.session_id.clone(),
                goose_session,
                mcp_servers: args.mcp_servers,
                resolved_provider: None,
                prebuilt_provider,
            },
        );

        let mut response = LoadSessionResponse::new().modes(mode_state);
        if let Some(ms) = model_state {
            response = response.models(ms);
        }
        if let Some(co) = config_options {
            response = response.config_options(co);
        }
        if let Some(usage_update) = initial_usage_update {
            cx.send_notification(SessionNotification::new(
                args.session_id.clone(),
                SessionUpdate::UsageUpdate(usage_update),
            ))?;
        }
        debug!(
            target: "perf",
            sid = %sid,
            ms = t_start.elapsed().as_millis() as u64,
            "perf: load_session done (agent setup continues in background)"
        );
        Ok(response)
    }

    async fn on_prompt(
        &self,
        cx: &ConnectionTo<Client>,
        args: PromptRequest,
    ) -> Result<PromptResponse, agent_client_protocol::Error> {
        // The ACP session_id IS the thread ID.
        let thread_id = args.session_id.0.to_string();
        let sid = sid_short(&thread_id);
        let t_start = std::time::Instant::now();

        // Update persona_id on the thread if the client sent one in _meta.
        let prompt_persona_id = args
            .meta
            .as_ref()
            .and_then(|m| m.get("personaId"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if let Some(ref pid) = prompt_persona_id {
            let pid = pid.clone();
            self.update_thread_metadata(&thread_id, move |meta| {
                meta.persona_id = Some(pid);
            })
            .await?;
        }

        let cancel_token = CancellationToken::new();
        let internal_session_id = self.internal_session_id(&thread_id).await?;

        let agent = self
            .get_session_agent(&thread_id, Some(cancel_token.clone()))
            .await?;

        let user_message = Self::convert_acp_prompt_to_message(&args.prompt);

        // Persist user message (may contain assistant-only annotated blocks)
        self.thread_manager
            .append_message(&thread_id, Some(&internal_session_id), &user_message)
            .await
            .internal_err_ctx("Failed to persist message")?;

        let session_config = SessionConfig {
            id: internal_session_id.clone(),
            schedule_id: None,
            max_turns: None,
            retry_config: None,
        };

        let mut stream = agent
            .reply(user_message, session_config, Some(cancel_token.clone()))
            .await
            .internal_err_ctx("Error getting agent reply")?;

        let mut was_cancelled = false;
        let mut first_event_logged = false;
        let mut event_count: u32 = 0;
        // Streaming chain buffer: tracks consecutive tool requests across
        // `AgentEvent::Message` events so chains that span multiple rows are
        // still registered. Sequential tool use (Bedrock/Anthropic) yields
        // request → response → request → response across separate
        // assistant/user messages, so tool responses are chain-neutral; only
        // non-tool content (text, thinking, image, etc.) breaks the run.
        // Holds `(tool_call_id, message_id_of_owning_row)` in arrival order;
        // re-registered eagerly each time a request arrives so
        // `handle_tool_response` finds the chain when subsequent responses
        // are processed.
        let mut chain_buffer: Vec<(String, String)> = Vec::new();

        while let Some(event) = stream.next().await {
            if cancel_token.is_cancelled() {
                was_cancelled = true;
                break;
            }
            event_count += 1;
            if !first_event_logged {
                debug!(
                    target: "perf",
                    sid = %sid,
                    ttft_ms = t_start.elapsed().as_millis() as u64,
                    "perf: prompt first stream event (time-to-first-token from prompt start)"
                );
                first_event_logged = true;
            }

            match event {
                Ok(crate::agents::AgentEvent::Message(message)) => {
                    let stored_message = self
                        .thread_manager
                        .append_message(&thread_id, Some(&internal_session_id), &message)
                        .await
                        .internal_err_ctx("Failed to persist message")?;
                    let stored_message_id = stored_message.id.clone();

                    let mut sessions = self.sessions.lock().await;
                    let session = sessions.get_mut(&thread_id).ok_or_else(|| {
                        agent_client_protocol::Error::invalid_params()
                            .data(format!("Session not found: {}", thread_id))
                    })?;

                    for content_item in &stored_message.content {
                        match content_item {
                            MessageContent::ToolRequest(tr) => {
                                if let Some(msg_id) = stored_message_id.as_deref() {
                                    chain_buffer.push((tr.id.clone(), msg_id.to_string()));
                                    // Re-register eagerly so the chain is in
                                    // place by the time the matching
                                    // `tool_response` triggers
                                    // `maybe_summarize_chain` (sequential
                                    // tool use interleaves request/response
                                    // events).
                                    extend_chain_membership(
                                        &chain_buffer,
                                        &mut session.chain_membership,
                                    );
                                }
                            }
                            MessageContent::ToolResponse(_) => {
                                // Chain-neutral: a response between two
                                // requests doesn't break the run, matching
                                // the frontend's `groupContentSections`.
                            }
                            _ => {
                                // Text, thinking, image, etc. end the run.
                                chain_buffer.clear();
                            }
                        }

                        self.handle_message_content(
                            content_item,
                            &args.session_id,
                            &thread_id,
                            stored_message_id.as_deref(),
                            &agent,
                            session,
                            cx,
                        )
                        .await?;
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    return Err(agent_client_protocol::Error::internal_error()
                        .data(format!("Error in agent response stream: {}", e)));
                }
            }
        }

        {
            let mut sessions = self.sessions.lock().await;
            if let Some(session) = sessions.get_mut(&thread_id) {
                // Final safety net: in case the stream ended without any
                // chain-breaking content, make sure a multi-tool buffer is
                // registered. (Eager registration during the loop usually
                // covers this.)
                extend_chain_membership(&chain_buffer, &mut session.chain_membership);
                session.cancel_token = None;
            }
        }

        let session = self
            .session_manager
            .get_session(&internal_session_id, false)
            .await
            .internal_err_ctx("Failed to load session")?;
        let provider = agent
            .provider()
            .await
            .internal_err_ctx("Failed to get provider")?;
        let usage_update =
            build_usage_update(&session, provider.get_model_config().context_limit());
        cx.send_notification(SessionNotification::new(
            args.session_id.clone(),
            SessionUpdate::UsageUpdate(usage_update),
        ))?;

        debug!(
            target: "perf",
            sid = %sid,
            ms = t_start.elapsed().as_millis() as u64,
            events = event_count,
            cancelled = was_cancelled,
            "perf: prompt done"
        );
        let stop_reason = if was_cancelled {
            StopReason::Cancelled
        } else {
            StopReason::EndTurn
        };

        let mut response = PromptResponse::new(stop_reason);
        if let Some(usage) = build_prompt_usage(&session) {
            response = response.usage(usage);
        }
        Ok(response)
    }

    async fn on_cancel(
        &self,
        args: CancelNotification,
    ) -> Result<(), agent_client_protocol::Error> {
        debug!(?args, "cancel request");

        let thread_id = args.session_id.0.to_string();
        let mut sessions = self.sessions.lock().await;

        if let Some(session) = sessions.get_mut(&thread_id) {
            if let Some(ref token) = session.cancel_token {
                info!(thread_id = %thread_id, "prompt cancelled");
                token.cancel();
            }
        } else {
            warn!(thread_id = %thread_id, "cancel request for unknown session");
        }

        Ok(())
    }

    async fn on_set_model(
        &self,
        thread_id: &str,
        model_id: &str,
    ) -> Result<SetSessionModelResponse, agent_client_protocol::Error> {
        let internal_id = self.internal_session_id(thread_id).await?;
        let config = self.config()?;
        let agent = self.get_session_agent_provider_ready(thread_id).await?;
        let current_provider = agent
            .provider()
            .await
            .internal_err_ctx("Failed to get provider")?;
        let provider_name = current_provider.get_name().to_string();
        let extensions =
            EnabledExtensionsState::for_session(&self.session_manager, &internal_id, &config).await;
        let model_config = crate::model::ModelConfig::new(model_id)
            .invalid_params_err_ctx("Invalid model config")?
            .with_canonical_limits(&provider_name);
        let provider = self
            .create_provider(&provider_name, model_config, extensions)
            .await
            .internal_err_ctx("Failed to create provider")?;
        agent
            .update_provider(provider, &internal_id)
            .await
            .internal_err_ctx("Failed to update provider")?;
        let mode = agent.goose_mode().await;
        agent
            .update_goose_mode(mode, &internal_id)
            .await
            .internal_err_ctx("Failed to propagate mode")?;
        let model_id_owned = model_id.to_string();
        self.update_thread_metadata(thread_id, move |meta| {
            meta.model_id = Some(model_id_owned);
        })
        .await?;
        Ok(SetSessionModelResponse::new())
    }

    async fn internal_session_id(
        &self,
        thread_id: &str,
    ) -> Result<String, agent_client_protocol::Error> {
        self.sessions
            .lock()
            .await
            .get(thread_id)
            .map(|s| s.internal_session_id.clone())
            .ok_or_else(|| {
                agent_client_protocol::Error::resource_not_found(Some(thread_id.to_string()))
                    .data(format!("Session not found: {}", thread_id))
            })
    }

    pub(super) async fn update_thread_metadata(
        &self,
        thread_id: &str,
        f: impl FnOnce(&mut crate::session::ThreadMetadata),
    ) -> Result<(), agent_client_protocol::Error> {
        self.thread_manager
            .update_metadata(thread_id, f)
            .await
            .internal_err()?;
        Ok(())
    }

    async fn build_config_update(
        &self,
        thread_id: &SessionId,
    ) -> Result<(SessionNotification, Vec<SessionConfigOption>), agent_client_protocol::Error> {
        let internal_id = self.internal_session_id(&thread_id.0).await?;
        let session = self
            .session_manager
            .get_session(&internal_id, false)
            .await
            .internal_err()?;
        let agent = self.get_session_agent_provider_ready(&thread_id.0).await?;
        let provider = agent
            .provider()
            .await
            .internal_err_ctx("Failed to get provider")?;
        let provider_name = provider.get_name().to_string();
        let current_model = provider.get_model_config().model_name.clone();
        let goose_mode = agent.goose_mode().await;
        let inventory = self
            .provider_inventory
            .entry_for_provider(&provider_name)
            .await
            .internal_err()?;
        let Some(inventory) = inventory else {
            return Err(agent_client_protocol::Error::internal_error()
                .data(format!("Unknown provider inventory: {}", provider_name)));
        };
        let model_state = build_model_state(current_model.as_str(), &inventory);
        let mode_state = build_mode_state(goose_mode)?;
        let provider_options = build_provider_options(Some(&provider_name)).await;
        let config_options = build_config_options(
            &mode_state,
            &model_state,
            session_provider_selection(&session),
            provider_options,
        );
        let notification = SessionNotification::new(
            thread_id.clone(),
            SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(config_options.clone())),
        );
        Ok((notification, config_options))
    }

    async fn on_set_mode(
        &self,
        thread_id: &str,
        mode_id: &str,
    ) -> Result<SetSessionModeResponse, agent_client_protocol::Error> {
        let internal_id = self.internal_session_id(thread_id).await?;
        let mode = mode_id.parse::<GooseMode>().map_err(|_| {
            agent_client_protocol::Error::invalid_params()
                .data(format!("Invalid mode: {}", mode_id))
        })?;

        let agent = self.get_session_agent_provider_ready(thread_id).await?;
        agent
            .update_goose_mode(mode, &internal_id)
            .await
            .internal_err_ctx("Failed to update mode")?;

        let mode_id = mode_id.to_string();
        self.update_thread_metadata(thread_id, move |meta| {
            meta.mode = Some(mode_id);
        })
        .await?;

        Ok(SetSessionModeResponse::new())
    }

    async fn update_provider(
        &self,
        thread_id: &str,
        provider_name: &str,
        model_name: Option<&str>,
        context_limit: Option<usize>,
        request_params: Option<std::collections::HashMap<String, serde_json::Value>>,
    ) -> Result<(), agent_client_protocol::Error> {
        let internal_id = self.internal_session_id(thread_id).await?;
        let config = self.config()?;
        let agent = self.get_session_agent_provider_ready(thread_id).await?;
        let current_provider = agent
            .provider()
            .await
            .internal_err_ctx("Failed to get provider")?;
        let current_provider_name = current_provider.get_name();
        let current_model = current_provider.get_model_config().model_name;
        let has_default_overrides =
            model_name.is_some() || context_limit.is_some() || request_params.is_some();
        let use_default_provider = provider_name == DEFAULT_PROVIDER_ID;
        let resolved_provider_name = if use_default_provider {
            config
                .get_goose_provider()
                .internal_err_ctx("Failed to resolve default provider from config")?
        } else {
            provider_name.to_string()
        };
        let is_changing_provider = resolved_provider_name != current_provider_name;
        let default_model = if let Some(model_name) = model_name {
            model_name.to_string()
        } else if use_default_provider {
            config
                .get_goose_model()
                .internal_err_ctx("Failed to resolve default model from config")?
        } else if is_changing_provider {
            ACP_CURRENT_MODEL.to_string()
        } else {
            current_model
        };
        let model = model_name.unwrap_or(&default_model);
        let model_config = crate::model::ModelConfig::new(model)
            .invalid_params_err_ctx("Invalid model config")?
            .with_canonical_limits(&resolved_provider_name)
            .with_context_limit(context_limit)
            .with_request_params(request_params);

        let extensions =
            EnabledExtensionsState::for_session(&self.session_manager, &internal_id, &config).await;
        let new_provider = self
            .create_provider(&resolved_provider_name, model_config, extensions)
            .await
            .internal_err_ctx("Failed to create provider")?;
        agent
            .update_provider(new_provider, &internal_id)
            .await
            .internal_err_ctx("Failed to update provider")?;
        let mode = agent.goose_mode().await;
        agent
            .update_goose_mode(mode, &internal_id)
            .await
            .internal_err_ctx("Failed to propagate mode")?;
        let provider = agent
            .provider()
            .await
            .internal_err_ctx("Failed to get provider")?;

        let provider_name_owned = provider_name.to_string();
        self.update_thread_metadata(thread_id, move |meta| {
            meta.provider_id = Some(provider_name_owned);
            meta.model_id = None;
        })
        .await?;

        if use_default_provider {
            let update = self
                .session_manager
                .update(&internal_id)
                .provider_name(DEFAULT_PROVIDER_ID);
            if has_default_overrides {
                update
                    .model_config(provider.get_model_config())
                    .apply()
                    .await
                    .internal_err_ctx("Failed to persist default provider selection overrides")?;
            } else {
                update
                    .clear_model_config()
                    .apply()
                    .await
                    .internal_err_ctx("Failed to persist default provider selection")?;
            }
        }
        Ok(())
    }

    async fn on_list_sessions(&self) -> Result<ListSessionsResponse, agent_client_protocol::Error> {
        // Return threads (= ACP sessions), not internal goose sessions.
        let threads = self
            .thread_manager
            .list_threads(false)
            .await
            .internal_err()?;
        let session_infos: Vec<SessionInfo> = threads
            .into_iter()
            .map(|t| {
                let cwd = t
                    .working_dir
                    .as_deref()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_default();
                let meta = thread_session_meta(&t);
                SessionInfo::new(SessionId::new(t.id), cwd)
                    .title(t.name)
                    .updated_at(t.updated_at.to_rfc3339())
                    .meta(meta)
            })
            .collect();
        Ok(ListSessionsResponse::new(session_infos))
    }

    async fn on_fork_session(
        &self,
        cx: &ConnectionTo<Client>,
        args: ForkSessionRequest,
    ) -> Result<ForkSessionResponse, agent_client_protocol::Error> {
        let source_thread_id = &*args.session_id.0;

        // Fork the thread (copies metadata + messages).
        let new_thread = self
            .thread_manager
            .fork_thread(source_thread_id)
            .await
            .internal_err()?;
        let new_thread_id = new_thread.id.clone();

        // Create an internal session for the new thread.
        let goose_session = self
            .create_internal_session(&new_thread_id, args.cwd, None, None)
            .await?;

        let internal_session_id = goose_session.id.clone();

        let (agent_tx, agent_rx) = tokio::sync::watch::channel::<AgentSetupSignal>(None);

        let session = GooseAcpSession {
            agent: AgentHandle::Loading(agent_rx),
            internal_session_id: internal_session_id.clone(),
            tool_requests: HashMap::new(),
            chain_membership: HashMap::new(),
            responded_tool_ids: HashSet::new(),
            summarized_chains: HashSet::new(),
            cancel_token: None,
            pending_working_dir: None,
        };
        self.sessions
            .lock()
            .await
            .insert(new_thread_id.clone(), session);

        let mode_state = build_mode_state(self.goose_mode)?;
        let resolved = resolve_provider_and_model(&self.config_dir, &goose_session).await;
        let (model_state, config_options, prebuilt_provider) = self
            .prepare_session_init_config(&resolved, &mode_state, &goose_session)
            .await;

        self.spawn_agent_setup(
            cx,
            agent_tx,
            AgentSetupRequest {
                session_id: SessionId::new(new_thread_id.clone()),
                goose_session,
                mcp_servers: args.mcp_servers,
                resolved_provider: resolved.ok(),
                prebuilt_provider,
            },
        );

        let meta = thread_session_meta(&new_thread);

        let mut response = ForkSessionResponse::new(SessionId::new(new_thread_id))
            .modes(mode_state)
            .meta(meta);
        if let Some(ms) = model_state {
            response = response.models(ms);
        }
        if let Some(co) = config_options {
            response = response.config_options(co);
        }
        Ok(response)
    }

    async fn on_close_session(
        &self,
        thread_id: &str,
    ) -> Result<CloseSessionResponse, agent_client_protocol::Error> {
        // Tear down the in-memory agent. The thread persists for later session/load.
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(thread_id) {
            if let Some(ref token) = session.cancel_token {
                token.cancel();
            }
        }
        sessions.remove(thread_id);
        info!(thread_id = %thread_id, "ACP session closed (thread preserved)");
        Ok(CloseSessionResponse::new())
    }
}

pub struct GooseAcpHandler {
    pub agent: Arc<GooseAcpAgent>,
}

pub fn serve<R, W>(
    agent: Arc<GooseAcpAgent>,
    read: R,
    write: W,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>
where
    R: futures::AsyncRead + Unpin + Send + 'static,
    W: futures::AsyncWrite + Unpin + Send + 'static,
{
    Box::pin(async move {
        let handler = GooseAcpHandler { agent };

        SacpAgent
            .builder()
            .name("goose-acp")
            .with_handler(handler)
            .connect_to(ByteStreams::new(write, read))
            .await?;

        Ok(())
    })
}

pub async fn run(builtins: Vec<String>) -> Result<()> {
    info!("listening on stdio");

    let outgoing = tokio::io::stdout().compat_write();
    let incoming = tokio::io::stdin().compat();

    let server = crate::acp::server_factory::AcpServer::new(
        crate::acp::server_factory::AcpServerFactoryConfig {
            builtins,
            data_dir: Paths::data_dir(),
            config_dir: Paths::config_dir(),
            goose_platform: GoosePlatform::GooseCli,
        },
    );
    let agent = server.create_agent().await?;
    serve(agent, incoming, outgoing).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::{ToolRequest, ToolResponse};
    use agent_client_protocol::schema::{
        EnvVariable, HttpHeader, McpServer, McpServerHttp, McpServerSse, McpServerStdio,
        PermissionOptionId, ResourceLink, SelectedPermissionOutcome, SessionConfigSelectOption,
        SessionMode, SessionModeId, SessionModeState,
    };
    use rmcp::model::{CallToolRequestParams, Content as RmcpContent};
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;
    use test_case::test_case;

    #[test_case(
        McpServer::Stdio(
            McpServerStdio::new("github", "/path/to/github-mcp-server")
                .args(vec!["stdio".into()])
                .env(vec![EnvVariable::new("GITHUB_PERSONAL_ACCESS_TOKEN", "ghp_xxxxxxxxxxxx")])
        ),
        Ok(ExtensionConfig::Stdio {
            name: "github".into(),
            description: String::new(),
            cmd: "/path/to/github-mcp-server".into(),
            args: vec!["stdio".into()],
            envs: Envs::new(
                [(
                    "GITHUB_PERSONAL_ACCESS_TOKEN".into(),
                    "ghp_xxxxxxxxxxxx".into()
                )]
                .into()
            ),
            env_keys: vec![],
            timeout: None,
            bundled: Some(false),
            available_tools: vec![],
        })
    )]
    #[test_case(
        McpServer::Http(
            McpServerHttp::new("github", "https://api.githubcopilot.com/mcp/")
                .headers(vec![HttpHeader::new("Authorization", "Bearer ghp_xxxxxxxxxxxx")])
        ),
        Ok(ExtensionConfig::StreamableHttp {
            name: "github".into(),
            description: String::new(),
            uri: "https://api.githubcopilot.com/mcp/".into(),
            envs: Envs::default(),
            env_keys: vec![],
            headers: HashMap::from([(
                "Authorization".into(),
                "Bearer ghp_xxxxxxxxxxxx".into()
            )]),
            timeout: None,
            socket: None,
            bundled: Some(false),
            available_tools: vec![],
        })
    )]
    #[test_case(
        McpServer::Sse(McpServerSse::new("test-sse", "https://agent-fin.biodnd.com/sse")),
        Err("SSE is unsupported, migrate to streamable_http".to_string())
    )]
    fn test_mcp_server_to_extension_config(
        input: McpServer,
        expected: Result<ExtensionConfig, String>,
    ) {
        assert_eq!(mcp_server_to_extension_config(input), expected);
    }

    fn new_resource_link(content: &str) -> anyhow::Result<(ResourceLink, NamedTempFile)> {
        let mut file = NamedTempFile::new()?;
        file.write_all(content.as_bytes())?;

        let name = file
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let uri = format!("file://{}", file.path().to_str().unwrap());
        let link = ResourceLink::new(name, uri);
        Ok((link, file))
    }

    #[test]
    fn test_read_resource_link_non_file_scheme() {
        let (link, file) = new_resource_link("print(\"hello, world\")").unwrap();

        let result = read_resource_link(link).unwrap();
        let expected = format!(
            "

# {}
```
print(\"hello, world\")
```",
            file.path().to_str().unwrap(),
        );

        assert_eq!(result, expected,)
    }

    #[test]
    fn test_format_tool_name_with_extension() {
        assert_eq!(format_tool_name("developer__edit"), "developer: edit");
        assert_eq!(
            format_tool_name("platform__manage_extensions"),
            "platform: manage extensions"
        );
        assert_eq!(format_tool_name("todo__write"), "todo: write");
    }

    #[test]
    fn test_format_tool_name_without_extension() {
        assert_eq!(format_tool_name("simple_tool"), "simple tool");
        assert_eq!(format_tool_name("another_name"), "another name");
        assert_eq!(format_tool_name("single"), "single");
    }

    #[test]
    fn test_summarize_tool_call_no_args() {
        assert_eq!(
            summarize_tool_call("developer__shell", None),
            "developer: shell"
        );
    }

    #[test]
    fn test_summarize_tool_call_with_path() {
        let args = serde_json::json!({"path": "/src/main.rs", "content": "fn main() {}"});
        assert_eq!(
            summarize_tool_call("developer__edit", Some(&args)),
            "developer: edit · /src/main.rs"
        );
    }

    #[test]
    fn test_summarize_tool_call_with_command() {
        let args = serde_json::json!({"command": "cargo build"});
        assert_eq!(
            summarize_tool_call("developer__shell", Some(&args)),
            "developer: shell · cargo build"
        );
    }

    #[test]
    fn test_tool_call_identity_meta_uses_goose_extension_metadata() {
        let request = ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("context7__query-docs")),
            metadata: None,
            tool_meta: Some(serde_json::json!({"goose_extension": "context7"})),
        };

        let meta = tool_call_identity_meta(&request).expect("expected metadata");

        assert_eq!(
            meta.get("goose"),
            Some(&serde_json::json!({
                "toolCall": {
                    "toolName": "context7__query-docs",
                    "extensionName": "context7",
                },
            })),
        );
    }

    fn tool_request_block(id: &str) -> crate::conversation::message::MessageContent {
        crate::conversation::message::MessageContent::ToolRequest(ToolRequest {
            id: id.to_string(),
            tool_call: Ok(CallToolRequestParams::new("dummy")),
            metadata: None,
            tool_meta: None,
        })
    }

    fn text_block(text: &str) -> crate::conversation::message::MessageContent {
        crate::conversation::message::MessageContent::text(text)
    }

    #[test]
    fn extract_tool_chains_returns_empty_for_no_tool_blocks() {
        let content = vec![text_block("hello"), text_block("world")];
        assert!(extract_tool_chains(&content).is_empty());
    }

    #[test]
    fn extract_tool_chains_returns_single_chain_when_only_tools() {
        let content = vec![
            tool_request_block("a"),
            tool_request_block("b"),
            tool_request_block("c"),
        ];
        let chains = extract_tool_chains(&content);
        assert_eq!(
            chains,
            vec![vec!["a".to_string(), "b".to_string(), "c".to_string()]]
        );
    }

    #[test]
    fn extract_tool_chains_breaks_on_text_block() {
        let content = vec![
            tool_request_block("a"),
            tool_request_block("b"),
            text_block("interlude"),
            tool_request_block("c"),
            tool_request_block("d"),
        ];
        let chains = extract_tool_chains(&content);
        assert_eq!(
            chains,
            vec![
                vec!["a".to_string(), "b".to_string()],
                vec!["c".to_string(), "d".to_string()],
            ]
        );
    }

    #[test]
    fn extract_tool_chains_includes_singletons() {
        let content = vec![
            tool_request_block("a"),
            text_block("split"),
            tool_request_block("b"),
            text_block("split"),
            tool_request_block("c"),
        ];
        let chains = extract_tool_chains(&content);
        assert_eq!(
            chains,
            vec![
                vec!["a".to_string()],
                vec!["b".to_string()],
                vec!["c".to_string()],
            ]
        );
    }

    #[test]
    fn extract_tool_chains_keeps_run_when_text_leads_or_trails() {
        let content = vec![
            text_block("intro"),
            tool_request_block("a"),
            tool_request_block("b"),
            text_block("outro"),
        ];
        let chains = extract_tool_chains(&content);
        assert_eq!(chains, vec![vec!["a".to_string(), "b".to_string()]]);
    }

    fn buf_entry(tool_id: &str, msg_id: &str) -> (String, String) {
        (tool_id.to_string(), msg_id.to_string())
    }

    #[test]
    fn extend_chain_membership_skips_singleton_and_leaves_buffer() {
        let mut membership: HashMap<String, Arc<ToolChain>> = HashMap::new();
        let buffer = vec![buf_entry("a", "row_1")];

        extend_chain_membership(&buffer, &mut membership);

        assert_eq!(buffer.len(), 1, "buffer is left intact for caller");
        assert!(
            membership.is_empty(),
            "single-tool runs should not register a chain",
        );
    }

    #[test]
    fn extend_chain_membership_registers_each_id_against_shared_chain() {
        let mut membership: HashMap<String, Arc<ToolChain>> = HashMap::new();
        let buffer = vec![
            buf_entry("a", "row_first"),
            buf_entry("b", "row_second"),
            buf_entry("c", "row_third"),
        ];

        extend_chain_membership(&buffer, &mut membership);

        assert_eq!(membership.len(), 3);
        let chain_a = membership.get("a").expect("a registered");
        let chain_b = membership.get("b").expect("b registered");
        let chain_c = membership.get("c").expect("c registered");
        assert!(
            Arc::ptr_eq(chain_a, chain_b) && Arc::ptr_eq(chain_b, chain_c),
            "every id in the run must point at the same ToolChain Arc",
        );
        assert_eq!(chain_a.message_id, "row_first");
        assert_eq!(
            chain_a.ids,
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
        );
    }

    #[test]
    fn extend_chain_membership_anchors_on_first_row_for_split_messages() {
        // Sequential tool use (Bedrock/Anthropic) emits each tool request as
        // its own assistant message, with the tool response interleaved in
        // between. The chain should still form, anchored on the *first*
        // tool's row id so `update_tool_request_meta` can find that
        // ToolRequest when persisting the summary.
        let mut membership: HashMap<String, Arc<ToolChain>> = HashMap::new();
        let buffer = vec![
            buf_entry("toolu_bdrk_1", "row_for_tool_1"),
            buf_entry("toolu_bdrk_2", "row_for_tool_2"),
        ];

        extend_chain_membership(&buffer, &mut membership);

        let chain = membership
            .get("toolu_bdrk_1")
            .expect("first tool registered");
        assert_eq!(chain.message_id, "row_for_tool_1");
        assert_eq!(
            chain.ids,
            vec!["toolu_bdrk_1".to_string(), "toolu_bdrk_2".to_string()],
        );
        let chain_via_second = membership
            .get("toolu_bdrk_2")
            .expect("second tool registered");
        assert!(Arc::ptr_eq(chain, chain_via_second));
    }

    #[test]
    fn extend_chain_membership_grows_chain_as_more_requests_arrive() {
        // The streaming loop re-registers eagerly each time a new request
        // arrives, so a chain that started at length 2 must grow to include
        // a third tool whose response is yet to come. Both the original
        // members and the new member must point at the new (extended) chain.
        let mut membership: HashMap<String, Arc<ToolChain>> = HashMap::new();
        let mut buffer = vec![buf_entry("a", "row_1"), buf_entry("b", "row_2")];
        extend_chain_membership(&buffer, &mut membership);

        buffer.push(buf_entry("c", "row_3"));
        extend_chain_membership(&buffer, &mut membership);

        let chain_a = membership.get("a").expect("a present");
        let chain_b = membership.get("b").expect("b present");
        let chain_c = membership.get("c").expect("c present");
        assert!(Arc::ptr_eq(chain_a, chain_b) && Arc::ptr_eq(chain_b, chain_c));
        assert_eq!(chain_a.message_id, "row_1");
        assert_eq!(
            chain_a.ids,
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
        );
    }

    #[test]
    fn with_tool_chain_summary_meta_creates_fresh_when_none() {
        let meta = with_tool_chain_summary_meta(None, "applied dark mode", 4)
            .expect("meta should be created");
        assert_eq!(
            meta.get("goose"),
            Some(&serde_json::json!({
                "toolChainSummary": { "summary": "applied dark mode", "count": 4 },
            })),
        );
    }

    #[test]
    fn with_tool_chain_summary_meta_preserves_existing_tool_call_identity() {
        let existing = tool_call_identity_meta(&ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("developer__shell")),
            metadata: None,
            tool_meta: None,
        });
        let meta = with_tool_chain_summary_meta(existing, "ran two commands", 2)
            .expect("meta should be created");
        let goose = meta.get("goose").expect("goose key");
        assert_eq!(
            goose.get("toolCall"),
            Some(
                &serde_json::json!({ "toolName": "developer__shell", "extensionName": "developer" })
            )
        );
        assert_eq!(
            goose.get("toolChainSummary"),
            Some(&serde_json::json!({ "summary": "ran two commands", "count": 2 }))
        );
    }

    #[test]
    fn replay_attaches_chain_summary_meta_for_first_tool_request_with_persisted_summary() {
        let tool_request = ToolRequest {
            id: "req_first".to_string(),
            tool_call: Ok(CallToolRequestParams::new("developer__shell")),
            metadata: None,
            tool_meta: Some(serde_json::json!({
                crate::conversation::message::TOOL_META_CHAIN_SUMMARY_KEY: {
                    "summary": "applied dark mode polish",
                    "count": 3,
                },
            })),
        };

        let pending_tool_call = pending_tool_call_from_request(&tool_request);
        let mut meta = pending_tool_call.identity_meta;
        let chain_summary = tool_request
            .persisted_chain_summary()
            .expect("chain summary should be present");
        meta = with_tool_chain_summary_meta(meta, &chain_summary.summary, chain_summary.count);

        let goose = meta
            .as_ref()
            .and_then(|m| m.get("goose"))
            .expect("replay meta must include a goose namespace");
        assert_eq!(
            goose.get("toolCall"),
            Some(
                &serde_json::json!({ "toolName": "developer__shell", "extensionName": "developer" })
            ),
            "replay must preserve identity meta alongside the chain summary",
        );
        assert_eq!(
            goose.get("toolChainSummary"),
            Some(&serde_json::json!({ "summary": "applied dark mode polish", "count": 3 })),
            "replay must attach toolChainSummary so the chain header renders on first paint",
        );
    }

    #[test]
    fn replay_does_not_attach_chain_summary_for_tool_requests_without_persisted_summary() {
        let tool_request = ToolRequest {
            id: "req_second".to_string(),
            tool_call: Ok(CallToolRequestParams::new("developer__shell")),
            metadata: None,
            tool_meta: None,
        };

        let chain_summary = tool_request.persisted_chain_summary();
        assert!(
            chain_summary.is_none(),
            "non-first tool requests must not carry chain summaries",
        );
    }

    #[test]
    fn test_summarize_tool_call_long_value_truncated() {
        let long_path = "a".repeat(80);
        let args = serde_json::json!({"path": long_path});
        let result = summarize_tool_call("developer__read_file", Some(&args));
        assert!(result.ends_with('…'));
        assert!(result.len() < 90);
    }

    #[test_case(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(PermissionOptionId::from("allow_once".to_string()))),
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::AllowOnce };
        "allow_once_maps_to_allow_once"
    )]
    #[test_case(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(PermissionOptionId::from("allow_always".to_string()))),
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::AlwaysAllow };
        "allow_always_maps_to_always_allow"
    )]
    #[test_case(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(PermissionOptionId::from("reject_once".to_string()))),
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::DenyOnce };
        "reject_once_maps_to_deny_once"
    )]
    #[test_case(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(PermissionOptionId::from("reject_always".to_string()))),
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::AlwaysDeny };
        "reject_always_maps_to_always_deny"
    )]
    #[test_case(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(PermissionOptionId::from("unknown".to_string()))),
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::Cancel };
        "unknown_option_maps_to_cancel"
    )]
    #[test_case(
        RequestPermissionOutcome::Cancelled,
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::Cancel };
        "cancelled_maps_to_cancel"
    )]
    fn test_outcome_to_confirmation(
        input: RequestPermissionOutcome,
        expected: PermissionConfirmation,
    ) {
        assert_eq!(outcome_to_confirmation(&input), expected);
    }

    #[test_case(
        vec!["model-a".into(), "model-b".into()]
        => SessionModelState::new(
            ModelId::new("unused"),
            vec![ModelInfo::new(ModelId::new("unused"), "unused"),
                 ModelInfo::new(ModelId::new("model-a"), "model-a"),
                 ModelInfo::new(ModelId::new("model-b"), "model-b")],
        )
        ; "returns current and available models"
    )]
    #[test_case(
        vec![]
        => SessionModelState::new(
            ModelId::new("unused"),
            vec![ModelInfo::new(ModelId::new("unused"), "unused")],
        )
        ; "empty model list"
    )]
    fn test_build_model_state(models: Vec<String>) -> SessionModelState {
        let inventory = ProviderInventoryEntry {
            provider_id: "mock".to_string(),
            provider_name: "Mock".to_string(),
            description: "Mock".to_string(),
            default_model: "unused".to_string(),
            configured: true,
            provider_type: crate::providers::base::ProviderType::Builtin,
            category: crate::providers::catalog::ProviderSetupCategory::Model,
            config_keys: vec![],
            setup_steps: vec![],
            supports_refresh: true,
            refreshing: false,
            models: models
                .into_iter()
                .map(|id| crate::providers::inventory::InventoryModel {
                    name: id.clone(),
                    id,
                    family: None,
                    context_limit: None,
                    reasoning: None,
                    recommended: false,
                })
                .collect(),
            last_updated_at: None,
            last_refresh_attempt_at: None,
            last_refresh_error: None,
            model_selection_hint: None,
        };
        build_model_state("unused", &inventory)
    }

    fn json_object(pairs: Vec<(&str, serde_json::Value)>) -> rmcp::model::JsonObject {
        pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    #[test_case(None => None ; "none arguments")]
    #[test_case(Some(json_object(vec![])) => None ; "missing line key")]
    #[test_case(Some(json_object(vec![("line", serde_json::json!(5))])) => Some(5) ; "line present")]
    #[test_case(Some(json_object(vec![("line", serde_json::json!("not_a_number"))])) => None ; "line not a number")]
    fn test_get_requested_line(arguments: Option<rmcp::model::JsonObject>) -> Option<u32> {
        get_requested_line(arguments.as_ref())
    }

    #[test_case("read", true ; "read is developer file tool")]
    #[test_case("write", true ; "write is developer file tool")]
    #[test_case("edit", true ; "edit is developer file tool")]
    #[test_case("shell", false ; "shell is not developer file tool")]
    #[test_case("analyze", false ; "analyze is not developer file tool")]
    fn test_is_developer_file_tool(tool_name: &str, expected: bool) {
        assert_eq!(is_developer_file_tool(tool_name), expected);
    }

    #[test_case(
        ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("read").with_arguments(serde_json::json!({"path": "/tmp/f.txt", "line": 5}).as_object().unwrap().clone())),
            metadata: None, tool_meta: None,
        },
        ToolResponse {
            id: "req_1".to_string(),
            tool_result: Ok(CallToolResult::success(vec![RmcpContent::text("")])),
            metadata: None,
        }
        => vec![(PathBuf::from("/tmp/f.txt"), Some(5))]
        ; "read returns requested line"
    )]
    #[test_case(
        ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("read").with_arguments(serde_json::json!({"path": "/tmp/f.txt"}).as_object().unwrap().clone())),
            metadata: None, tool_meta: None,
        },
        ToolResponse {
            id: "req_1".to_string(),
            tool_result: Ok(CallToolResult::success(vec![RmcpContent::text("")])),
            metadata: None,
        }
        => vec![(PathBuf::from("/tmp/f.txt"), None)]
        ; "read without line"
    )]
    #[test_case(
        ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("write").with_arguments(serde_json::json!({"path": "/tmp/f.txt", "content": "hi"}).as_object().unwrap().clone())),
            metadata: None, tool_meta: None,
        },
        ToolResponse {
            id: "req_1".to_string(),
            tool_result: Ok(CallToolResult::success(vec![RmcpContent::text("")])),
            metadata: None,
        }
        => vec![(PathBuf::from("/tmp/f.txt"), Some(1))]
        ; "write returns line 1"
    )]
    #[test_case(
        ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("edit").with_arguments(serde_json::json!({"path": "/tmp/f.txt", "before": "a", "after": "b"}).as_object().unwrap().clone())),
            metadata: None, tool_meta: None,
        },
        ToolResponse {
            id: "req_1".to_string(),
            tool_result: Ok(CallToolResult::success(vec![RmcpContent::text("")])),
            metadata: None,
        }
        => vec![(PathBuf::from("/tmp/f.txt"), Some(1))]
        ; "edit returns line 1"
    )]
    #[test_case(
        ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("shell").with_arguments(serde_json::json!({"command": "ls"}).as_object().unwrap().clone())),
            metadata: None, tool_meta: None,
        },
        ToolResponse {
            id: "req_1".to_string(),
            tool_result: Ok(CallToolResult::success(vec![RmcpContent::text("")])),
            metadata: None,
        }
        => Vec::<(PathBuf, Option<u32>)>::new()
        ; "non file tool returns empty"
    )]
    fn test_extract_tool_locations(
        request: ToolRequest,
        response: ToolResponse,
    ) -> Vec<(PathBuf, Option<u32>)> {
        extract_tool_locations(&request, &response)
            .into_iter()
            .map(|loc| (loc.path, loc.line))
            .collect()
    }

    fn response_with_meta(meta: Option<serde_json::Value>) -> ToolResponse {
        let mut result = CallToolResult::success(vec![RmcpContent::text("")]);
        result.meta = meta.map(|v| serde_json::from_value(v).unwrap());
        ToolResponse {
            id: "req_1".to_string(),
            tool_result: Ok(result),
            metadata: None,
        }
    }

    #[test_case(
        response_with_meta(Some(serde_json::json!({"tool_locations": [{"path": "/tmp/f.txt", "line": 5}]})))
        => Some(vec![(PathBuf::from("/tmp/f.txt"), Some(5))])
        ; "meta with path and line"
    )]
    #[test_case(
        response_with_meta(Some(serde_json::json!({"tool_locations": [{"path": "/tmp/f.txt"}]})))
        => Some(vec![(PathBuf::from("/tmp/f.txt"), None)])
        ; "meta with path no line"
    )]
    #[test_case(
        response_with_meta(Some(serde_json::json!({})))
        => None
        ; "meta without tool_locations key"
    )]
    #[test_case(
        response_with_meta(None)
        => None
        ; "no meta"
    )]
    fn test_extract_locations_from_meta(
        response: ToolResponse,
    ) -> Option<Vec<(PathBuf, Option<u32>)>> {
        extract_locations_from_meta(&response)
            .map(|locs| locs.into_iter().map(|loc| (loc.path, loc.line)).collect())
    }

    #[test]
    fn test_extract_tool_call_update_meta_ignores_untrusted_goose_meta() {
        let response = response_with_meta(Some(serde_json::json!({
            "goose": {
                "mcpApp": {
                    "resourceUri": "ui://spoofed/app",
                },
            },
        })));

        assert_eq!(extract_tool_call_update_meta(&response), None);
    }

    #[test]
    fn test_extract_tool_call_update_meta_uses_trusted_meta_only() {
        let response = response_with_meta(Some(serde_json::json!({
            "goose": {
                "mcpApp": {
                    "resourceUri": "ui://spoofed/app",
                },
            },
            TRUSTED_TOOL_UPDATE_META_KEY: {
                "mcpApp": {
                    "resourceUri": "ui://trusted/app",
                    "extensionName": "weather",
                    "toolName": "weather__render",
                },
            },
        })));

        let extracted = extract_tool_call_update_meta(&response).expect("expected trusted meta");
        assert_eq!(
            extracted.get("goose"),
            Some(&serde_json::json!({
                "mcpApp": {
                    "resourceUri": "ui://trusted/app",
                    "extensionName": "weather",
                    "toolName": "weather__render",
                },
            })),
        );
    }

    #[test]
    fn test_merge_replay_message_meta_preserves_existing_goose_meta() {
        let message = Message::new(Role::Assistant, 1_700_000_000, vec![]).with_id("msg_1");
        let existing = serde_json::from_value(serde_json::json!({
            "goose": {
                "mcpApp": {
                    "resourceUri": "ui://trusted/app",
                    "extensionName": "weather",
                    "toolName": "weather__render",
                },
            },
        }))
        .unwrap();

        let merged = merge_replay_message_meta(Some(existing), &message);

        assert_eq!(
            merged.get("goose"),
            Some(&serde_json::json!({
                "created": 1_700_000_000,
                "messageId": "msg_1",
                "mcpApp": {
                    "resourceUri": "ui://trusted/app",
                    "extensionName": "weather",
                    "toolName": "weather__render",
                },
            })),
        );
    }

    #[test]
    fn test_merge_replay_message_meta_creates_fresh_when_none() {
        let message = Message::new(Role::Assistant, 1_700_000_000, vec![]).with_id("msg_2");

        let merged = merge_replay_message_meta(None, &message);

        assert_eq!(
            merged.get("goose"),
            Some(&serde_json::json!({
                "created": 1_700_000_000,
                "messageId": "msg_2",
            })),
        );
    }

    #[test]
    fn test_merge_replay_message_meta_omits_message_id_when_none() {
        let message = Message::new(Role::Assistant, 1_700_000_000, vec![]);

        let merged = merge_replay_message_meta(None, &message);

        assert_eq!(
            merged.get("goose"),
            Some(&serde_json::json!({
                "created": 1_700_000_000,
            })),
        );
    }

    #[test]
    fn test_extract_tool_raw_output_preserves_structured_content() {
        let mut result = CallToolResult::success(vec![RmcpContent::text("fallback")]);
        result.structured_content = Some(serde_json::json!({
            "restaurants": [
                {
                    "name": "Coffee Shop",
                    "unitToken": "unit-1",
                },
            ],
        }));

        assert_eq!(
            extract_tool_raw_output(&Ok(result)),
            Some(serde_json::json!({
                "restaurants": [
                    {
                        "name": "Coffee Shop",
                        "unitToken": "unit-1",
                    },
                ],
            })),
        );
    }

    fn make_session_with_usage(
        total_tokens: Option<i32>,
        input_tokens: Option<i32>,
        output_tokens: Option<i32>,
        accumulated_total_tokens: Option<i32>,
        accumulated_input_tokens: Option<i32>,
        accumulated_output_tokens: Option<i32>,
    ) -> Session {
        Session {
            id: "session-1".to_string(),
            working_dir: PathBuf::from("/tmp"),
            name: "ACP Session".to_string(),
            user_set_name: false,
            session_type: SessionType::Acp,
            created_at: Default::default(),
            updated_at: Default::default(),
            extension_data: crate::session::ExtensionData::default(),
            total_tokens,
            input_tokens,
            output_tokens,
            accumulated_total_tokens,
            accumulated_input_tokens,
            accumulated_output_tokens,
            schedule_id: None,
            recipe: None,
            user_recipe_values: None,
            conversation: None,
            message_count: 0,
            provider_name: None,
            model_config: None,
            goose_mode: GooseMode::default(),
            thread_id: None,
        }
    }

    #[test]
    fn test_build_prompt_usage_uses_current_turn_tokens() {
        let session = make_session_with_usage(
            Some(120),
            Some(80),
            Some(40),
            Some(360),
            Some(210),
            Some(150),
        );
        let usage = build_prompt_usage(&session).expect("usage should be present");
        assert_eq!(usage.total_tokens, 120);
        assert_eq!(usage.input_tokens, 80);
        assert_eq!(usage.output_tokens, 40);
    }

    #[test]
    fn test_build_prompt_usage_falls_back_to_current_tokens() {
        let session = make_session_with_usage(Some(120), Some(80), Some(40), None, None, None);
        let usage = build_prompt_usage(&session).expect("usage should be present");
        assert_eq!(usage.total_tokens, 120);
        assert_eq!(usage.input_tokens, 80);
        assert_eq!(usage.output_tokens, 40);
    }

    #[test]
    fn test_build_prompt_usage_requires_total_tokens() {
        let session = make_session_with_usage(None, Some(80), Some(40), None, None, None);
        assert!(build_prompt_usage(&session).is_none());
    }

    #[test]
    fn test_build_usage_update_clamps_negative_used_to_zero() {
        let session = make_session_with_usage(Some(-7), Some(0), Some(0), None, None, None);
        let usage = build_usage_update(&session, 258_000);
        assert_eq!(usage.used, 0);
        assert_eq!(usage.size, 258_000);
    }

    #[test_case(
        GooseMode::Auto
        => Ok(SessionModeState::new(
            SessionModeId::new("auto"),
            vec![
                SessionMode::new(SessionModeId::new("auto"), "auto")
                    .description("Automatically approve tool calls"),
                SessionMode::new(SessionModeId::new("approve"), "approve")
                    .description("Ask before every tool call"),
                SessionMode::new(SessionModeId::new("smart_approve"), "smart_approve")
                    .description("Ask only for sensitive tool calls"),
                SessionMode::new(SessionModeId::new("chat"), "chat")
                    .description("Chat only, no tool calls"),
            ],
        ))
        ; "auto mode"
    )]
    #[test_case(
        GooseMode::Approve
        => Ok(SessionModeState::new(
            SessionModeId::new("approve"),
            vec![
                SessionMode::new(SessionModeId::new("auto"), "auto")
                    .description("Automatically approve tool calls"),
                SessionMode::new(SessionModeId::new("approve"), "approve")
                    .description("Ask before every tool call"),
                SessionMode::new(SessionModeId::new("smart_approve"), "smart_approve")
                    .description("Ask only for sensitive tool calls"),
                SessionMode::new(SessionModeId::new("chat"), "chat")
                    .description("Chat only, no tool calls"),
            ],
        ))
        ; "approve mode"
    )]
    fn test_build_mode_state(
        current_mode: GooseMode,
    ) -> Result<SessionModeState, agent_client_protocol::Error> {
        build_mode_state(current_mode)
    }

    #[test_case(
        build_mode_state(GooseMode::Auto).unwrap(),
        "openai",
        vec![
            SessionConfigSelectOption::new("anthropic", "anthropic"),
            SessionConfigSelectOption::new("openai", "openai"),
        ],
        SessionModelState::new(
            ModelId::new("gpt-4"),
            vec![ModelInfo::new(ModelId::new("gpt-4"), "gpt-4"), ModelInfo::new(ModelId::new("gpt-3.5"), "gpt-3.5")],
        )
        => vec![
            SessionConfigOption::select(
                "provider", "Provider", "openai",
                vec![
                    SessionConfigSelectOption::new("anthropic", "anthropic"),
                    SessionConfigSelectOption::new("openai", "openai"),
                ],
            ),
            SessionConfigOption::select(
                "mode", "Mode", "auto",
                vec![
                    SessionConfigSelectOption::new("auto", "auto").description("Automatically approve tool calls"),
                    SessionConfigSelectOption::new("approve", "approve").description("Ask before every tool call"),
                    SessionConfigSelectOption::new("smart_approve", "smart_approve").description("Ask only for sensitive tool calls"),
                    SessionConfigSelectOption::new("chat", "chat").description("Chat only, no tool calls"),
                ],
            ).category(SessionConfigOptionCategory::Mode),
            SessionConfigOption::select(
                "model", "Model", "gpt-4",
                vec![
                    SessionConfigSelectOption::new("gpt-4", "gpt-4"),
                    SessionConfigSelectOption::new("gpt-3.5", "gpt-3.5"),
                ],
            ).category(SessionConfigOptionCategory::Model),
        ]
        ; "auto mode with multiple models"
    )]
    #[test_case(
        build_mode_state(GooseMode::Approve).unwrap(),
        "openai",
        vec![SessionConfigSelectOption::new("openai", "openai")],
        SessionModelState::new(ModelId::new("only-model"), vec![ModelInfo::new(ModelId::new("only-model"), "only-model")])
        => vec![
            SessionConfigOption::select(
                "provider", "Provider", "openai",
                vec![SessionConfigSelectOption::new("openai", "openai")],
            ),
            SessionConfigOption::select(
                "mode", "Mode", "approve",
                vec![
                    SessionConfigSelectOption::new("auto", "auto").description("Automatically approve tool calls"),
                    SessionConfigSelectOption::new("approve", "approve").description("Ask before every tool call"),
                    SessionConfigSelectOption::new("smart_approve", "smart_approve").description("Ask only for sensitive tool calls"),
                    SessionConfigSelectOption::new("chat", "chat").description("Chat only, no tool calls"),
                ],
            ).category(SessionConfigOptionCategory::Mode),
            SessionConfigOption::select(
                "model", "Model", "only-model",
                vec![SessionConfigSelectOption::new("only-model", "only-model")],
            ).category(SessionConfigOptionCategory::Model),
        ]
        ; "approve mode with single model"
    )]
    fn test_build_config_options(
        mode_state: SessionModeState,
        provider_name: &'static str,
        provider_options: Vec<SessionConfigSelectOption>,
        model_state: SessionModelState,
    ) -> Vec<SessionConfigOption> {
        build_config_options(&mode_state, &model_state, provider_name, provider_options)
    }
}
