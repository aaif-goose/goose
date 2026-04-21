use crate::acp::custom_requests::*;
use crate::acp::fs::AcpTools;
use crate::acp::tools::AcpAwareToolMeta;
use crate::acp::{PermissionDecision, ACP_CURRENT_MODEL};
use crate::agents::extension::{Envs, PLATFORM_EXTENSIONS};
use crate::agents::mcp_client::McpClientTrait;
use crate::agents::platform_extensions::developer::DeveloperClient;
use crate::agents::{Agent, AgentConfig, ExtensionConfig, GoosePlatform, SessionConfig};
use crate::config::base::CONFIG_YAML_NAME;
use crate::config::extensions::get_enabled_extensions_with_config;
use crate::config::paths::Paths;
use crate::config::permission::PermissionManager;
use crate::config::{Config, GooseMode};
use crate::conversation::message::{ActionRequiredData, Message, MessageContent};
#[cfg(feature = "local-inference")]
use crate::dictation::providers::transcribe_local;
use crate::dictation::providers::{
    all_providers, is_configured, transcribe_with_provider, DictationProvider,
};
#[cfg(feature = "local-inference")]
use crate::dictation::whisper;
use crate::mcp_utils::ToolResult;
use crate::permission::permission_confirmation::PrincipalType;
use crate::permission::{Permission, PermissionConfirmation};
use crate::providers::base::Provider;
use crate::providers::inventory::{
    ProviderInventoryEntry, ProviderInventoryService, RefreshSkipReason,
};
use crate::session::session_manager::SessionType;
use crate::session::{EnabledExtensionsState, Session, SessionManager};
use anyhow::Result;
use fs_err as fs;
use futures::future::{BoxFuture, Either};
use goose_acp_macros::custom_methods;
use rmcp::model::{CallToolResult, RawContent, ResourceContents, Role};
use sacp::schema::{
    AgentCapabilities, AuthMethod, AuthMethodAgent, AuthenticateRequest, AuthenticateResponse,
    BlobResourceContents, CancelNotification, CloseSessionRequest, CloseSessionResponse,
    ConfigOptionUpdate, Content, ContentBlock, ContentChunk, CurrentModeUpdate, EmbeddedResource,
    EmbeddedResourceResource, FileSystemCapabilities, ForkSessionRequest, ForkSessionResponse,
    ImageContent, InitializeRequest, InitializeResponse, ListSessionsRequest, ListSessionsResponse,
    LoadSessionRequest, LoadSessionResponse, McpCapabilities, McpServer, Meta, ModelId, ModelInfo,
    NewSessionRequest, NewSessionResponse, PermissionOption, PermissionOptionKind,
    PromptCapabilities, PromptRequest, PromptResponse, RequestPermissionOutcome,
    RequestPermissionRequest, ResourceLink, SessionCapabilities, SessionCloseCapabilities,
    SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOption, SessionId,
    SessionInfo, SessionListCapabilities, SessionMode, SessionModeId, SessionModeState,
    SessionModelState, SessionNotification, SessionUpdate, SetSessionConfigOptionRequest,
    SetSessionConfigOptionResponse, SetSessionModeRequest, SetSessionModeResponse,
    SetSessionModelRequest, SetSessionModelResponse, StopReason, TextContent, TextResourceContents,
    ToolCall, ToolCallContent, ToolCallId, ToolCallLocation, ToolCallStatus, ToolCallUpdate,
    ToolCallUpdateFields, ToolKind, Usage, UsageUpdate,
};
use sacp::util::MatchDispatchFrom;
use sacp::{
    Agent as SacpAgent, ByteStreams, Client, ConnectionTo, Dispatch, HandleDispatchFrom, Handled,
    Responder,
};
use std::collections::HashMap;
use std::sync::Arc;
use strum::{EnumMessage, VariantNames};
use tokio::sync::{Mutex, OnceCell};
use tokio_util::compat::{TokioAsyncReadCompatExt as _, TokioAsyncWriteCompatExt as _};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use url::Url;

pub type AcpProviderFactory = Arc<
    dyn Fn(
            String,
            crate::model::ModelConfig,
            Vec<ExtensionConfig>,
        ) -> BoxFuture<'static, Result<Arc<dyn Provider>>>
        + Send
        + Sync,
>;

const DEFAULT_PROVIDER_ID: &str = "goose";
const DEFAULT_PROVIDER_LABEL: &str = "Goose (Default)";
const OPENAI_TRANSCRIPTION_MODEL_CONFIG_KEY: &str = "OPENAI_TRANSCRIPTION_MODEL";
const GROQ_TRANSCRIPTION_MODEL_CONFIG_KEY: &str = "GROQ_TRANSCRIPTION_MODEL";
const ELEVENLABS_TRANSCRIPTION_MODEL_CONFIG_KEY: &str = "ELEVENLABS_TRANSCRIPTION_MODEL";
const OPENAI_TRANSCRIPTION_MODEL: &str = "whisper-1";
const GROQ_TRANSCRIPTION_MODEL: &str = "whisper-large-v3-turbo";
const ELEVENLABS_TRANSCRIPTION_MODEL: &str = "scribe_v1";

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
    cancel_token: Option<CancellationToken>,
    /// Working directory set while the agent was still loading.
    /// Applied once the agent becomes ready.
    pending_working_dir: Option<std::path::PathBuf>,
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
    config_dir: std::path::PathBuf,
    session_manager: Arc<SessionManager>,
    thread_manager: Arc<crate::session::ThreadManager>,
    permission_manager: Arc<PermissionManager>,
    goose_mode: GooseMode,
    disable_session_naming: bool,
    provider_inventory: ProviderInventoryService,
}

/// Shorten a session/thread id for perf log correlation.
/// All `perf:` logs use `sid=<8-char-prefix>` so a single session's activity
/// can be extracted with `grep 'perf:' <log> | grep 'sid=abc12345'`.
fn sid_short(id: &str) -> String {
    id.chars().take(8).collect()
}

fn thread_session_meta(
    message_count: i64,
    metadata: &crate::session::ThreadMetadata,
) -> serde_json::Map<String, serde_json::Value> {
    let mut meta = serde_json::Map::new();
    meta.insert(
        "messageCount".to_string(),
        serde_json::Value::Number(message_count.into()),
    );
    if let Some(ref pid) = metadata.project_id {
        meta.insert(
            "projectId".to_string(),
            serde_json::Value::String(pid.clone()),
        );
    }
    meta
}

fn extract_timeout_from_meta(meta: &Option<Meta>) -> Option<u64> {
    meta.as_ref()
        .and_then(|m| m.get("timeout"))
        .and_then(|v| v.as_u64())
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

fn create_tool_location(path: &str, line: Option<u32>) -> ToolCallLocation {
    let mut loc = ToolCallLocation::new(path);
    if let Some(l) = line {
        loc = loc.line(l);
    }
    loc
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
            Some(create_tool_location(path, line))
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
                locations.push(create_tool_location(path_str, line));
                return locations;
            }

            if matches!(tool_name, "write" | "edit") {
                locations.push(create_tool_location(path_str, Some(1)));
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
                                locations.push(create_tool_location(path_str, line));
                            }
                            Some("str_replace") | Some("insert") => {
                                let line = extract_first_line_number(text)
                                    .map(|l| l as u32)
                                    .or(Some(1));
                                locations.push(create_tool_location(path_str, line));
                            }
                            Some("write") => {
                                locations.push(create_tool_location(path_str, Some(1)));
                            }
                            _ => {
                                locations.push(create_tool_location(path_str, Some(1)));
                            }
                        }
                        break;
                    }
                }
            }

            if locations.is_empty() {
                locations.push(create_tool_location(path_str, Some(1)));
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

fn inventory_entry_to_dto(entry: ProviderInventoryEntry) -> ProviderInventoryEntryDto {
    let stale = ProviderInventoryService::is_stale(&entry);
    ProviderInventoryEntryDto {
        provider_id: entry.provider_id,
        provider_name: entry.provider_name,
        description: entry.description,
        default_model: entry.default_model,
        configured: entry.configured,
        provider_type: format!("{:?}", entry.provider_type),
        config_keys: entry
            .config_keys
            .into_iter()
            .map(provider_config_key_to_dto)
            .collect(),
        setup_steps: entry.setup_steps,
        supports_refresh: entry.supports_refresh,
        refreshing: entry.refreshing,
        models: entry
            .models
            .into_iter()
            .map(|m| ProviderInventoryModelDto {
                id: m.id,
                name: m.name,
                family: m.family,
                context_limit: m.context_limit,
                reasoning: m.reasoning,
                recommended: m.recommended,
            })
            .collect(),
        last_updated_at: entry.last_updated_at.map(|t| t.to_rfc3339()),
        last_refresh_attempt_at: entry.last_refresh_attempt_at.map(|t| t.to_rfc3339()),
        last_refresh_error: entry.last_refresh_error,
        stale,
        model_selection_hint: entry.model_selection_hint,
    }
}

fn provider_config_key_to_dto(key: crate::providers::base::ConfigKey) -> ProviderConfigKey {
    ProviderConfigKey {
        name: key.name,
        required: key.required,
        secret: key.secret,
        default: key.default,
        oauth_flow: key.oauth_flow,
        device_code_flow: key.device_code_flow,
        primary: key.primary,
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

fn build_mode_state(current_mode: GooseMode) -> Result<SessionModeState, sacp::Error> {
    let mut available = Vec::with_capacity(GooseMode::VARIANTS.len());
    for &name in GooseMode::VARIANTS {
        let goose_mode: GooseMode = name.parse().map_err(|_| {
            sacp::Error::internal_error() // impossible but satisfy linters
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
    ) -> Result<Self> {
        let session_manager = Arc::new(SessionManager::new(data_dir));
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
            config_dir,
            session_manager,
            thread_manager,
            permission_manager,
            goose_mode,
            disable_session_naming,
            provider_inventory,
        })
    }

    fn load_config(&self) -> Result<Config> {
        Config::new(self.config_dir.join(CONFIG_YAML_NAME), "goose").map_err(Into::into)
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
                    match self
                        .create_provider(provider_name, model_config.clone(), ext_state)
                        .await
                    {
                        Ok(provider) => {
                            let provider_id = provider_name.clone();
                            prebuilt_provider = Some(provider.clone());
                            match self
                                .provider_inventory
                                .plan_refresh(std::slice::from_ref(&provider_id))
                                .await
                            {
                                Ok(plan) if plan.started.iter().any(|id| id == &provider_id) => {
                                    match provider.fetch_recommended_models().await {
                                        Ok(models) => {
                                            if let Err(error) = self
                                                .provider_inventory
                                                .store_refreshed_models(&provider_id, &models)
                                                .await
                                            {
                                                warn!(
                                                    provider = %provider_id,
                                                    error = %error,
                                                    "failed to store refreshed provider inventory during session init"
                                                );
                                            }
                                        }
                                        Err(error) => {
                                            if let Err(store_error) = self
                                                .provider_inventory
                                                .store_refresh_error(
                                                    &provider_id,
                                                    error.to_string(),
                                                )
                                                .await
                                            {
                                                warn!(
                                                    provider = %provider_id,
                                                    error = %store_error,
                                                    "failed to store provider inventory refresh error during session init"
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
        let provider_factory = Arc::clone(&self.provider_factory);
        let disable_session_naming = self.disable_session_naming;

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

            // ── Phase 1: create agent + init provider (fast, ~55ms) ──────
            let phase1: Result<Arc<Agent>, String> = async {
                let agent = Arc::new(Agent::with_config(AgentConfig::new(
                    session_manager,
                    permission_manager,
                    None,
                    goose_mode,
                    disable_session_naming,
                    GoosePlatform::GooseCli,
                )));

                // Init provider — reuse the pre-resolved name + model when
                // available (already computed in on_new_session), otherwise
                // fall back to reading config (e.g. load_session path).
                let t_prov = std::time::Instant::now();
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
                debug!(target: "perf", sid = %sid, ms = t_prov.elapsed().as_millis() as u64, "perf: agent_setup provider_init");

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
                let ext_count = extensions.len();
                let t_ext = std::time::Instant::now();
                let extension_futures = extensions
                    .into_iter()
                    .map(|ext| {
                        let ext_manager = Arc::clone(ext_manager);
                        let sid_inner = sid_str.clone();
                        let sid_log = sid.clone();
                        async move {
                            let name = ext.name().to_string();
                            let t_one = std::time::Instant::now();
                            match ext_manager
                                .add_extension(ext, None, None, sid_inner.as_deref())
                                .await
                            {
                                Ok(_) => debug!(
                                    target: "perf",
                                    sid = %sid_log,
                                    extension = %name,
                                    ms = t_one.elapsed().as_millis() as u64,
                                    "perf: agent_setup extension_loaded"
                                ),
                                Err(e) => {
                                    warn!(extension = %name, error = %e, "extension load failed")
                                }
                            }
                        }
                    })
                    .collect::<Vec<_>>();
                futures::future::join_all(extension_futures).await;
                debug!(
                    target: "perf",
                    sid = %sid,
                    ms = t_ext.elapsed().as_millis() as u64,
                    extensions = ext_count,
                    "perf: agent_setup extensions_total"
                );

                if let Some((client, config)) = acp_developer {
                    let info = client.get_info().cloned();
                    agent
                        .extension_manager
                        .add_client("developer".into(), config, client, info, None)
                        .await;
                }

                let t_mcp = std::time::Instant::now();
                let mcp_count = mcp_servers.len();
                GooseAcpAgent::add_mcp_extensions(&agent, mcp_servers, &internal_session_id)
                    .await
                    .map_err(|e| e.to_string())?;
                debug!(
                    target: "perf",
                    sid = %sid,
                    ms = t_mcp.elapsed().as_millis() as u64,
                    mcp_servers = mcp_count,
                    "perf: agent_setup mcp_extensions"
                );

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

    fn convert_acp_prompt_to_message(&self, prompt: Vec<ContentBlock>) -> Message {
        let mut user_message = Message::user();

        for block in prompt {
            match block {
                ContentBlock::Text(text) => {
                    user_message = user_message.with_text(&text.text);
                }
                ContentBlock::Image(image) => {
                    user_message = user_message.with_image(&image.data, &image.mime_type);
                }
                ContentBlock::Resource(resource) => {
                    if let EmbeddedResourceResource::TextResourceContents(text_resource) =
                        &resource.resource
                    {
                        let header = format!("--- Resource: {} ---\n", text_resource.uri);
                        let content = format!("{}{}\n---\n", header, text_resource.text);
                        user_message = user_message.with_text(&content);
                    }
                }
                ContentBlock::ResourceLink(link) => {
                    if let Some(text) = read_resource_link(link) {
                        user_message = user_message.with_text(text)
                    }
                }
                ContentBlock::Audio(..) | _ => (),
            }
        }

        user_message
    }

    async fn handle_message_content(
        &self,
        content_item: &MessageContent,
        session_id: &SessionId,
        agent: &Arc<Agent>,
        session: &mut GooseAcpSession,
        cx: &ConnectionTo<Client>,
    ) -> Result<(), sacp::Error> {
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
                self.handle_tool_request(tool_request, session_id, session, cx)
                    .await?;
            }
            MessageContent::ToolResponse(tool_response) => {
                self.handle_tool_response(tool_response, session_id, session, cx)
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
        session: &mut GooseAcpSession,
        cx: &ConnectionTo<Client>,
    ) -> Result<(), sacp::Error> {
        session
            .tool_requests
            .insert(tool_request.id.clone(), tool_request.clone());

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

        let mut initial_tool_call = ToolCall::new(
            ToolCallId::new(tool_request.id.clone()),
            fallback_title.clone(),
        )
        .status(ToolCallStatus::Pending);
        if let Some(args) = args_value.clone() {
            initial_tool_call = initial_tool_call.raw_input(args);
        }
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

            tokio::spawn(async move {
                let provider: Arc<dyn Provider> = match agent.provider().await {
                    Ok(p) => p,
                    Err(e) => {
                        warn!("tool call summary: failed to get provider: {e}");
                        let fields = ToolCallUpdateFields::new().title(fallback_title);
                        let _ = cx.send_notification(SessionNotification::new(
                            sid,
                            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                                ToolCallId::new(request_id),
                                fields,
                            )),
                        ));
                        return;
                    }
                };

                // in these case, the title summarization request would
                // be added to the conversation which we don't want
                if provider.manages_own_context() {
                    return;
                }

                let system = "Summarize this tool call in a short lowercase phrase (3-8 words). \
                              No punctuation. No quotes. Examples: reading project configuration, \
                              checking network connectivity, listing files in src directory";
                let user_text = format!("Tool: {name}\nArguments: {args_json}");
                let message = Message::user().with_text(&user_text);
                match provider
                    .complete_fast(&sid.0, system, &[message], &[])
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
                        let title = if summary.is_empty() {
                            fallback_title
                        } else {
                            summary
                        };
                        let fields = ToolCallUpdateFields::new().title(title);
                        let _ = cx.send_notification(SessionNotification::new(
                            sid,
                            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                                ToolCallId::new(request_id),
                                fields,
                            )),
                        ));
                    }
                    Err(e) => {
                        warn!("tool call summary: fast_complete failed: {e}");
                        let fields = ToolCallUpdateFields::new().title(fallback_title);
                        let _ = cx.send_notification(SessionNotification::new(
                            sid,
                            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                                ToolCallId::new(request_id),
                                fields,
                            )),
                        ));
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
        session: &mut GooseAcpSession,
        cx: &ConnectionTo<Client>,
    ) -> Result<(), sacp::Error> {
        let status = match &tool_response.tool_result {
            Ok(result) if result.is_error == Some(true) => ToolCallStatus::Failed,
            Ok(_) => ToolCallStatus::Completed,
            Err(_) => ToolCallStatus::Failed,
        };

        let mut fields = ToolCallUpdateFields::new().status(status);
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

        cx.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(tool_response.id.clone()),
                fields,
            )),
        ))?;

        Ok(())
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
    ) -> Result<(), sacp::Error> {
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

impl GooseAcpAgent {
    async fn on_initialize(
        &self,
        args: InitializeRequest,
    ) -> Result<InitializeResponse, sacp::Error> {
        debug!(?args, "initialize request");

        let _ = self
            .client_fs_capabilities
            .set(args.client_capabilities.fs.clone());
        let _ = self.client_terminal.set(args.client_capabilities.terminal);

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
    ) -> Result<NewSessionResponse, sacp::Error> {
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

        // Create the Thread — this IS the ACP session from the client's perspective.
        let thread_metadata = crate::session::ThreadMetadata {
            provider_id: requested_provider.clone(),
            project_id,
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
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to create thread: {}", e))
            })?;
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
    ) -> Result<Session, sacp::Error> {
        let goose_session = self
            .session_manager
            .create_session(
                cwd,
                "ACP Session".to_string(),
                SessionType::Acp,
                self.goose_mode,
            )
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to create session: {}", e))
            })?;

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
        builder.apply().await.map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to link session to thread: {}", e))
        })?;

        self.session_manager
            .get_session(&goose_session.id, false)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to reload session: {}", e))
            })
    }

    /// Look up the session and return the agent if already ready, or the watch
    /// receiver if still loading.  Optionally sets a cancellation token on the
    /// session (needed by `on_prompt`).
    async fn get_agent_or_receiver(
        &self,
        thread_id: &str,
        cancel_token: Option<CancellationToken>,
    ) -> Result<Either<Arc<Agent>, tokio::sync::watch::Receiver<AgentSetupSignal>>, sacp::Error>
    {
        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(thread_id).ok_or_else(|| {
            sacp::Error::resource_not_found(Some(thread_id.to_string()))
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
    ) -> Result<Arc<Agent>, sacp::Error> {
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
                sacp::Error::internal_error().data("Agent setup task was dropped".to_string())
            })?;
        match guard.as_ref().unwrap() {
            Ok(AgentSetupProgress::FullyReady(agent)) => Ok(agent.clone()),
            Err(e) => Err(sacp::Error::internal_error().data(e.clone())),
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
    ) -> Result<Arc<Agent>, sacp::Error> {
        let mut rx = match self.get_agent_or_receiver(thread_id, None).await? {
            Either::Left(agent) => return Ok(agent),
            Either::Right(rx) => rx,
        };
        // Any signal (ProviderReady, FullyReady, or Err) unblocks us.
        let guard = rx.wait_for(|v| v.is_some()).await.map_err(|_| {
            sacp::Error::internal_error().data("Agent setup task was dropped".to_string())
        })?;
        match guard.as_ref().unwrap() {
            Ok(progress) => match progress {
                AgentSetupProgress::ProviderReady(agent)
                | AgentSetupProgress::FullyReady(agent) => Ok(agent.clone()),
            },
            Err(e) => Err(sacp::Error::internal_error().data(e.clone())),
        }
    }

    async fn add_mcp_extensions(
        agent: &Arc<Agent>,
        mcp_servers: Vec<McpServer>,
        internal_session_id: &str,
    ) -> Result<(), sacp::Error> {
        let mut configs = Vec::with_capacity(mcp_servers.len());
        for mcp_server in mcp_servers {
            let config = match mcp_server_to_extension_config(mcp_server) {
                Ok(c) => c,
                Err(msg) => {
                    return Err(sacp::Error::invalid_params().data(msg));
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
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        for result in &results {
            if !result.success {
                let error_msg = result.error.as_deref().unwrap_or("unknown error");
                return Err(sacp::Error::internal_error().data(format!(
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
    ) -> Result<LoadSessionResponse, sacp::Error> {
        debug!(?args, "load session request");

        // The ACP session_id IS the thread ID.
        let thread_id = args.session_id.0.to_string();
        let sid = sid_short(&thread_id);
        let t_start = std::time::Instant::now();
        debug!(target: "perf", sid = %sid, "perf: load_session start");

        let t0 = std::time::Instant::now();
        let thread = self
            .thread_manager
            .get_thread(&thread_id)
            .await
            .map_err(|_| {
                sacp::Error::resource_not_found(Some(thread_id.clone()))
                    .data(format!("Session not found: {}", thread_id))
            })?;
        debug!(target: "perf", sid = %sid, ms = t0.elapsed().as_millis() as u64, "perf: load_session get_thread");

        // Reuse the thread's current internal session so the agent retains
        // conversation context (compaction state, full message history, etc.).
        // The internal session is the source of truth for provider/mode.
        let internal_session_id = thread.current_session_id.clone().ok_or_else(|| {
            sacp::Error::internal_error()
                .data(format!("Thread {} has no internal session", thread_id))
        })?;
        let t1 = std::time::Instant::now();
        let goose_session = self
            .session_manager
            .get_session(&internal_session_id, false)
            .await
            .map_err(|e| {
                sacp::Error::internal_error()
                    .data(format!("Failed to load internal session: {}", e))
            })?;
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
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to load thread messages: {}", e))
            })?;
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

        let t_replay = std::time::Instant::now();
        let mut replay_notifications: u32 = 0;
        for message in &thread_messages {
            if !message.metadata.user_visible {
                continue;
            }

            for content_item in &message.content {
                match content_item {
                    MessageContent::Text(text) => {
                        let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(
                            text.text.clone(),
                        )));
                        let update = match message.role {
                            Role::User => SessionUpdate::UserMessageChunk(chunk),
                            Role::Assistant => SessionUpdate::AgentMessageChunk(chunk),
                        };
                        cx.send_notification(SessionNotification::new(
                            args.session_id.clone(),
                            update,
                        ))?;
                        replay_notifications += 1;
                    }
                    MessageContent::ToolRequest(tool_request) => {
                        // Replay-only: emit the ToolCall notification and
                        // stash the request for location extraction, but
                        // don't require a full GooseAcpSession.
                        replay_tool_requests.insert(tool_request.id.clone(), tool_request.clone());

                        let tool_name = match &tool_request.tool_call {
                            Ok(tool_call) => tool_call.name.to_string(),
                            Err(_) => "error".to_string(),
                        };

                        cx.send_notification(SessionNotification::new(
                            args.session_id.clone(),
                            SessionUpdate::ToolCall(
                                ToolCall::new(
                                    ToolCallId::new(tool_request.id.clone()),
                                    format_tool_name(&tool_name),
                                )
                                .status(ToolCallStatus::Pending),
                            ),
                        ))?;
                        replay_notifications += 1;
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

                        cx.send_notification(SessionNotification::new(
                            args.session_id.clone(),
                            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                                ToolCallId::new(tool_response.id.clone()),
                                fields,
                            )),
                        ))?;
                        replay_notifications += 1;
                    }
                    MessageContent::Thinking(thinking) => {
                        cx.send_notification(SessionNotification::new(
                            args.session_id.clone(),
                            SessionUpdate::AgentThoughtChunk(ContentChunk::new(
                                ContentBlock::Text(TextContent::new(thinking.thinking.clone())),
                            )),
                        ))?;
                        replay_notifications += 1;
                    }
                    _ => {}
                }
            }
        }
        debug!(
            target: "perf",
            sid = %sid,
            ms = t_replay.elapsed().as_millis() as u64,
            notifications = replay_notifications,
            "perf: load_session replay_loop"
        );

        // ── Lightweight DB updates (fast) ──
        let t_db = std::time::Instant::now();
        self.session_manager
            .update(&internal_session_id)
            .working_dir(args.cwd.clone())
            .apply()
            .await
            .map_err(|e| {
                sacp::Error::internal_error()
                    .data(format!("Failed to update session working directory: {}", e))
            })?;

        self.thread_manager
            .update_working_dir(&thread_id, &args.cwd.display().to_string())
            .await
            .map_err(|e| {
                sacp::Error::internal_error()
                    .data(format!("Failed to update thread working directory: {}", e))
            })?;
        debug!(target: "perf", sid = %sid, ms = t_db.elapsed().as_millis() as u64, "perf: load_session db_updates");

        // ── Register the session immediately with a Loading handle ──
        let (agent_tx, agent_rx) = tokio::sync::watch::channel::<AgentSetupSignal>(None);

        let session = GooseAcpSession {
            agent: AgentHandle::Loading(agent_rx),
            internal_session_id: internal_session_id.clone(),
            tool_requests: replay_tool_requests,
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
    ) -> Result<PromptResponse, sacp::Error> {
        // The ACP session_id IS the thread ID.
        let thread_id = args.session_id.0.to_string();
        let sid = sid_short(&thread_id);
        let t_start = std::time::Instant::now();
        debug!(target: "perf", sid = %sid, "perf: prompt start");

        let cancel_token = CancellationToken::new();
        let internal_session_id = self.internal_session_id(&thread_id).await?;

        let t_agent = std::time::Instant::now();
        let agent = self
            .get_session_agent(&thread_id, Some(cancel_token.clone()))
            .await?;
        debug!(target: "perf", sid = %sid, ms = t_agent.elapsed().as_millis() as u64, "perf: prompt get_session_agent (waits for agent setup)");

        let user_message = self.convert_acp_prompt_to_message(args.prompt);

        let t_persist = std::time::Instant::now();
        self.thread_manager
            .append_message(&thread_id, Some(&internal_session_id), &user_message)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to persist message: {}", e))
            })?;
        debug!(target: "perf", sid = %sid, ms = t_persist.elapsed().as_millis() as u64, "perf: prompt append_user_message");

        let session_config = SessionConfig {
            id: internal_session_id.clone(),
            schedule_id: None,
            max_turns: None,
            retry_config: None,
        };

        let t_reply = std::time::Instant::now();
        let mut stream = agent
            .reply(user_message, session_config, Some(cancel_token.clone()))
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Error getting agent reply: {}", e))
            })?;
        debug!(target: "perf", sid = %sid, ms = t_reply.elapsed().as_millis() as u64, "perf: prompt agent.reply() setup");

        use futures::StreamExt;

        let mut was_cancelled = false;
        let mut first_event_logged = false;
        let mut event_count: u32 = 0;

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
                    self.thread_manager
                        .append_message(&thread_id, Some(&internal_session_id), &message)
                        .await
                        .map_err(|e| {
                            sacp::Error::internal_error()
                                .data(format!("Failed to persist message: {}", e))
                        })?;

                    let mut sessions = self.sessions.lock().await;
                    let session = sessions.get_mut(&thread_id).ok_or_else(|| {
                        sacp::Error::invalid_params()
                            .data(format!("Session not found: {}", thread_id))
                    })?;

                    for content_item in &message.content {
                        self.handle_message_content(
                            content_item,
                            &args.session_id,
                            &agent,
                            session,
                            cx,
                        )
                        .await?;
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    return Err(sacp::Error::internal_error()
                        .data(format!("Error in agent response stream: {}", e)));
                }
            }
        }

        {
            let mut sessions = self.sessions.lock().await;
            if let Some(session) = sessions.get_mut(&thread_id) {
                session.cancel_token = None;
            }
        }

        let session = self
            .session_manager
            .get_session(&internal_session_id, false)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to load session: {}", e))
            })?;
        let provider = agent.provider().await.map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to get provider: {}", e))
        })?;
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

    async fn on_cancel(&self, args: CancelNotification) -> Result<(), sacp::Error> {
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
    ) -> Result<SetSessionModelResponse, sacp::Error> {
        let sid = sid_short(thread_id);
        let t_total = std::time::Instant::now();
        debug!(target: "perf", sid = %sid, model = %model_id, "perf: set_model start");

        let t_step = std::time::Instant::now();
        let internal_id = self.internal_session_id(thread_id).await?;
        debug!(target: "perf", sid = %sid, ms = t_step.elapsed().as_millis() as u64, "perf: set_model internal_session_id");

        let t_step = std::time::Instant::now();
        let config = self.load_config().map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to read config: {}", e))
        })?;
        debug!(target: "perf", sid = %sid, ms = t_step.elapsed().as_millis() as u64, "perf: set_model load_config");

        let t_step = std::time::Instant::now();
        let agent = self.get_session_agent_provider_ready(thread_id).await?;
        debug!(target: "perf", sid = %sid, ms = t_step.elapsed().as_millis() as u64, "perf: set_model get_session_agent_provider_ready");

        let t_step = std::time::Instant::now();
        let current_provider = agent.provider().await.map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to get provider: {}", e))
        })?;
        let provider_name = current_provider.get_name().to_string();
        let extensions =
            EnabledExtensionsState::for_session(&self.session_manager, &internal_id, &config).await;
        let model_config = crate::model::ModelConfig::new(model_id)
            .map_err(|e| {
                sacp::Error::invalid_params().data(format!("Invalid model config: {}", e))
            })?
            .with_canonical_limits(&provider_name);
        debug!(target: "perf", sid = %sid, ms = t_step.elapsed().as_millis() as u64, provider = %provider_name, "perf: set_model build_model_config");

        let t_step = std::time::Instant::now();
        let provider = self
            .create_provider(&provider_name, model_config, extensions)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to create provider: {}", e))
            })?;
        debug!(target: "perf", sid = %sid, ms = t_step.elapsed().as_millis() as u64, provider = %provider_name, "perf: set_model create_provider");

        let t_step = std::time::Instant::now();
        agent
            .update_provider(provider, &internal_id)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to update provider: {}", e))
            })?;
        debug!(target: "perf", sid = %sid, ms = t_step.elapsed().as_millis() as u64, "perf: set_model agent.update_provider");

        let t_step = std::time::Instant::now();
        let mode = agent.goose_mode().await;
        agent
            .update_goose_mode(mode, &internal_id)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to propagate mode: {}", e))
            })?;
        debug!(target: "perf", sid = %sid, ms = t_step.elapsed().as_millis() as u64, "perf: set_model update_goose_mode");

        let t_step = std::time::Instant::now();
        let model_id_owned = model_id.to_string();
        self.update_thread_metadata(thread_id, move |meta| {
            meta.model_name = Some(model_id_owned);
        })
        .await?;
        debug!(target: "perf", sid = %sid, ms = t_step.elapsed().as_millis() as u64, "perf: set_model update_thread_metadata");

        debug!(target: "perf", sid = %sid, ms = t_total.elapsed().as_millis() as u64, model = %model_id, "perf: set_model done");
        Ok(SetSessionModelResponse::new())
    }

    async fn internal_session_id(&self, thread_id: &str) -> Result<String, sacp::Error> {
        self.sessions
            .lock()
            .await
            .get(thread_id)
            .map(|s| s.internal_session_id.clone())
            .ok_or_else(|| {
                sacp::Error::resource_not_found(Some(thread_id.to_string()))
                    .data(format!("Session not found: {}", thread_id))
            })
    }

    async fn update_thread_metadata(
        &self,
        thread_id: &str,
        f: impl FnOnce(&mut crate::session::ThreadMetadata),
    ) -> Result<(), sacp::Error> {
        self.thread_manager
            .update_metadata(thread_id, f)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        Ok(())
    }

    async fn build_config_update(
        &self,
        thread_id: &SessionId,
    ) -> Result<(SessionNotification, Vec<SessionConfigOption>), sacp::Error> {
        let internal_id = self.internal_session_id(&thread_id.0).await?;
        let session = self
            .session_manager
            .get_session(&internal_id, false)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        let agent = self.get_session_agent_provider_ready(&thread_id.0).await?;
        let provider = agent.provider().await.map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to get provider: {}", e))
        })?;
        let provider_name = provider.get_name().to_string();
        let current_model = provider.get_model_config().model_name.clone();
        let goose_mode = agent.goose_mode().await;
        let inventory = self
            .provider_inventory
            .entry_for_provider(&provider_name)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        let Some(inventory) = inventory else {
            return Err(sacp::Error::internal_error()
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
    ) -> Result<SetSessionModeResponse, sacp::Error> {
        let internal_id = self.internal_session_id(thread_id).await?;
        let mode = mode_id.parse::<GooseMode>().map_err(|_| {
            sacp::Error::invalid_params().data(format!("Invalid mode: {}", mode_id))
        })?;

        let agent = self.get_session_agent_provider_ready(thread_id).await?;
        agent
            .update_goose_mode(mode, &internal_id)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to update mode: {}", e))
            })?;

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
    ) -> Result<(), sacp::Error> {
        let sid = sid_short(thread_id);
        let t_total = std::time::Instant::now();
        debug!(target: "perf", sid = %sid, provider = %provider_name, "perf: update_provider start");

        let t_step = std::time::Instant::now();
        let internal_id = self.internal_session_id(thread_id).await?;
        debug!(target: "perf", sid = %sid, ms = t_step.elapsed().as_millis() as u64, "perf: update_provider internal_session_id");

        let t_step = std::time::Instant::now();
        let config = self.load_config().map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to read config: {}", e))
        })?;
        debug!(target: "perf", sid = %sid, ms = t_step.elapsed().as_millis() as u64, "perf: update_provider load_config");

        let t_step = std::time::Instant::now();
        let agent = self.get_session_agent_provider_ready(thread_id).await?;
        debug!(target: "perf", sid = %sid, ms = t_step.elapsed().as_millis() as u64, "perf: update_provider get_session_agent_provider_ready");

        let t_step = std::time::Instant::now();
        let current_provider = agent.provider().await.map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to get provider: {}", e))
        })?;
        let current_provider_name = current_provider.get_name();
        let current_model = current_provider.get_model_config().model_name;
        let has_default_overrides =
            model_name.is_some() || context_limit.is_some() || request_params.is_some();
        let use_default_provider = provider_name == DEFAULT_PROVIDER_ID;
        let resolved_provider_name = if use_default_provider {
            config.get_goose_provider().map_err(|e| {
                sacp::Error::internal_error().data(format!(
                    "Failed to resolve default provider from config: {}",
                    e
                ))
            })?
        } else {
            provider_name.to_string()
        };
        let is_changing_provider = resolved_provider_name != current_provider_name;
        let default_model = if let Some(model_name) = model_name {
            model_name.to_string()
        } else if use_default_provider {
            config.get_goose_model().map_err(|e| {
                sacp::Error::internal_error().data(format!(
                    "Failed to resolve default model from config: {}",
                    e
                ))
            })?
        } else if is_changing_provider {
            ACP_CURRENT_MODEL.to_string()
        } else {
            current_model
        };
        let model = model_name.unwrap_or(&default_model);
        let model_config = crate::model::ModelConfig::new(model)
            .map_err(|e| {
                sacp::Error::invalid_params().data(format!("Invalid model config: {}", e))
            })?
            .with_canonical_limits(&resolved_provider_name)
            .with_context_limit(context_limit)
            .with_request_params(request_params);
        debug!(
            target: "perf",
            sid = %sid,
            ms = t_step.elapsed().as_millis() as u64,
            resolved_provider = %resolved_provider_name,
            current_provider = %current_provider_name,
            changing = is_changing_provider,
            has_overrides = has_default_overrides,
            "perf: update_provider resolve_defaults"
        );

        let t_step = std::time::Instant::now();
        let extensions =
            EnabledExtensionsState::for_session(&self.session_manager, &internal_id, &config).await;
        debug!(target: "perf", sid = %sid, ms = t_step.elapsed().as_millis() as u64, "perf: update_provider build_extensions");

        let t_step = std::time::Instant::now();
        let new_provider = self
            .create_provider(&resolved_provider_name, model_config, extensions)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to create provider: {}", e))
            })?;
        debug!(target: "perf", sid = %sid, ms = t_step.elapsed().as_millis() as u64, provider = %resolved_provider_name, "perf: update_provider create_provider");

        let t_step = std::time::Instant::now();
        agent
            .update_provider(new_provider, &internal_id)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to update provider: {}", e))
            })?;
        debug!(target: "perf", sid = %sid, ms = t_step.elapsed().as_millis() as u64, "perf: update_provider agent.update_provider");

        let t_step = std::time::Instant::now();
        let mode = agent.goose_mode().await;
        agent
            .update_goose_mode(mode, &internal_id)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to propagate mode: {}", e))
            })?;
        debug!(target: "perf", sid = %sid, ms = t_step.elapsed().as_millis() as u64, "perf: update_provider update_goose_mode");

        let provider = agent.provider().await.map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to get provider: {}", e))
        })?;

        let t_step = std::time::Instant::now();
        let provider_name_owned = provider_name.to_string();
        self.update_thread_metadata(thread_id, move |meta| {
            meta.provider_id = Some(provider_name_owned);
        })
        .await?;
        debug!(target: "perf", sid = %sid, ms = t_step.elapsed().as_millis() as u64, "perf: update_provider update_thread_metadata");

        let t_step = std::time::Instant::now();
        if use_default_provider {
            let update = self
                .session_manager
                .update(&internal_id)
                .provider_name(DEFAULT_PROVIDER_ID);
            if has_default_overrides {
                let provider_model_config = provider.get_model_config();
                update
                    .model_config(provider_model_config)
                    .apply()
                    .await
                    .map_err(|e| {
                        sacp::Error::internal_error().data(format!(
                            "Failed to persist default provider selection overrides: {}",
                            e
                        ))
                    })?;
            } else {
                update.clear_model_config().apply().await.map_err(|e| {
                    sacp::Error::internal_error().data(format!(
                        "Failed to persist default provider selection: {}",
                        e
                    ))
                })?;
            }
        }
        debug!(
            target: "perf",
            sid = %sid,
            ms = t_step.elapsed().as_millis() as u64,
            persisted = use_default_provider,
            "perf: update_provider persist_session"
        );

        debug!(
            target: "perf",
            sid = %sid,
            ms = t_total.elapsed().as_millis() as u64,
            provider = %provider_name,
            resolved_provider = %resolved_provider_name,
            changing = is_changing_provider,
            "perf: update_provider done"
        );
        Ok(())
    }

    async fn on_list_sessions(&self) -> Result<ListSessionsResponse, sacp::Error> {
        // Return threads (= ACP sessions), not internal goose sessions.
        let threads = self
            .thread_manager
            .list_threads(false)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        let session_infos: Vec<SessionInfo> = threads
            .into_iter()
            .map(|t| {
                let cwd = t
                    .working_dir
                    .as_deref()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_default();
                let meta = thread_session_meta(t.message_count, &t.metadata);
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
    ) -> Result<ForkSessionResponse, sacp::Error> {
        let source_thread_id = &*args.session_id.0;

        // Fork the thread (copies metadata + messages).
        let new_thread = self
            .thread_manager
            .fork_thread(source_thread_id)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
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

        let meta = thread_session_meta(new_thread.message_count, &new_thread.metadata);

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

    async fn on_close_session(&self, thread_id: &str) -> Result<CloseSessionResponse, sacp::Error> {
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

#[custom_methods]
impl GooseAcpAgent {
    #[custom_method(AddExtensionRequest)]
    async fn on_add_extension(
        &self,
        req: AddExtensionRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let internal_id = self.internal_session_id(&req.session_id).await?;
        let config: ExtensionConfig = serde_json::from_value(req.config)
            .map_err(|e| sacp::Error::invalid_params().data(format!("bad config: {e}")))?;
        let agent = self.get_session_agent(&req.session_id, None).await?;
        agent
            .add_extension(config, &internal_id)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        Ok(EmptyResponse {})
    }

    #[custom_method(RemoveExtensionRequest)]
    async fn on_remove_extension(
        &self,
        req: RemoveExtensionRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let internal_id = self.internal_session_id(&req.session_id).await?;
        let agent = self.get_session_agent(&req.session_id, None).await?;
        agent
            .remove_extension(&req.name, &internal_id)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        Ok(EmptyResponse {})
    }

    #[custom_method(GetToolsRequest)]
    async fn on_get_tools(&self, req: GetToolsRequest) -> Result<GetToolsResponse, sacp::Error> {
        let internal_id = self.internal_session_id(&req.session_id).await?;
        let agent = self.get_session_agent(&req.session_id, None).await?;
        let tools = agent.list_tools(&internal_id, None).await;
        let tools_json = tools
            .into_iter()
            .map(|t| serde_json::to_value(&t))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        Ok(GetToolsResponse { tools: tools_json })
    }

    #[custom_method(ReadResourceRequest)]
    async fn on_read_resource(
        &self,
        req: ReadResourceRequest,
    ) -> Result<ReadResourceResponse, sacp::Error> {
        let internal_id = self.internal_session_id(&req.session_id).await?;
        let agent = self.get_session_agent(&req.session_id, None).await?;
        let cancel_token = CancellationToken::new();
        let result = agent
            .extension_manager
            .read_resource(&internal_id, &req.uri, &req.extension_name, cancel_token)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        let result_json = serde_json::to_value(&result)
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        Ok(ReadResourceResponse {
            result: result_json,
        })
    }

    #[custom_method(UpdateWorkingDirRequest)]
    async fn on_update_working_dir(
        &self,
        req: UpdateWorkingDirRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let working_dir = req.working_dir.trim().to_string();
        if working_dir.is_empty() {
            return Err(sacp::Error::invalid_params().data("working directory cannot be empty"));
        }
        let path = std::path::PathBuf::from(&working_dir);
        if !path.exists() || !path.is_dir() {
            return Err(sacp::Error::invalid_params().data("invalid directory path"));
        }
        let internal_id = self.internal_session_id(&req.session_id).await?;
        self.session_manager
            .update(&internal_id)
            .working_dir(path.clone())
            .apply()
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;

        self.thread_manager
            .update_working_dir(&req.session_id, &working_dir)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;

        if let Some(session) = self.sessions.lock().await.get_mut(&req.session_id) {
            match &session.agent {
                AgentHandle::Ready(agent) => {
                    agent.extension_manager.update_working_dir(&path).await;
                }
                AgentHandle::Loading(_) => {
                    session.pending_working_dir = Some(path);
                }
            }
        }

        Ok(EmptyResponse {})
    }

    #[custom_method(DeleteSessionRequest)]
    async fn on_delete_session(
        &self,
        req: DeleteSessionRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        // Delete the thread and all its internal sessions + messages.
        self.thread_manager
            .delete_thread(&req.session_id)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        self.sessions.lock().await.remove(&req.session_id);
        Ok(EmptyResponse {})
    }

    #[custom_method(GetExtensionsRequest)]
    async fn on_get_extensions(&self) -> Result<GetExtensionsResponse, sacp::Error> {
        let extensions = crate::config::extensions::get_all_extensions();
        let warnings = crate::config::extensions::get_warnings();
        let extensions_json = extensions
            .into_iter()
            .map(|e| {
                let config_key = e.config.key();
                let mut value = serde_json::to_value(&e)?;
                if let Some(obj) = value.as_object_mut() {
                    obj.insert(
                        "config_key".to_string(),
                        serde_json::Value::String(config_key),
                    );
                }
                Ok::<_, serde_json::Error>(value)
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        Ok(GetExtensionsResponse {
            extensions: extensions_json,
            warnings,
        })
    }

    #[custom_method(AddConfigExtensionRequest)]
    async fn on_add_config_extension(
        &self,
        req: AddConfigExtensionRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let mut obj = match req.extension_config {
            serde_json::Value::Object(obj) => obj,
            _ => {
                return Err(
                    sacp::Error::invalid_params().data("extensionConfig must be a JSON object")
                );
            }
        };
        obj.insert(
            "name".to_string(),
            serde_json::Value::String(req.name.clone()),
        );

        let config: crate::agents::ExtensionConfig =
            serde_json::from_value(serde_json::Value::Object(obj))
                .map_err(|e| sacp::Error::invalid_params().data(format!("bad config: {e}")))?;

        crate::config::extensions::set_extension(crate::config::extensions::ExtensionEntry {
            enabled: req.enabled,
            config,
        });
        Ok(EmptyResponse {})
    }

    #[custom_method(RemoveConfigExtensionRequest)]
    async fn on_remove_config_extension(
        &self,
        req: RemoveConfigExtensionRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let keys = crate::config::extensions::get_all_extension_names();
        if !keys.iter().any(|k| k == &req.config_key) {
            return Err(sacp::Error::invalid_params()
                .data(format!("Extension '{}' not found", req.config_key)));
        }
        crate::config::extensions::remove_extension(&req.config_key);
        Ok(EmptyResponse {})
    }

    #[custom_method(ToggleConfigExtensionRequest)]
    async fn on_toggle_config_extension(
        &self,
        req: ToggleConfigExtensionRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let keys = crate::config::extensions::get_all_extension_names();
        if !keys.iter().any(|k| k == &req.config_key) {
            return Err(sacp::Error::invalid_params()
                .data(format!("Extension '{}' not found", req.config_key)));
        }
        crate::config::extensions::set_extension_enabled(&req.config_key, req.enabled);
        Ok(EmptyResponse {})
    }

    #[custom_method(GetSessionExtensionsRequest)]
    async fn on_get_session_extensions(
        &self,
        req: GetSessionExtensionsRequest,
    ) -> Result<GetSessionExtensionsResponse, sacp::Error> {
        let internal_id = self.internal_session_id(&req.session_id).await?;
        let session = self
            .session_manager
            .get_session(&internal_id, false)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;

        let extensions = EnabledExtensionsState::extensions_or_default(
            Some(&session.extension_data),
            crate::config::Config::global(),
        );

        let extensions_json = extensions
            .into_iter()
            .map(|e| serde_json::to_value(&e))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;

        Ok(GetSessionExtensionsResponse {
            extensions: extensions_json,
        })
    }

    #[custom_method(ListProvidersRequest)]
    async fn on_list_providers(
        &self,
        req: ListProvidersRequest,
    ) -> Result<ListProvidersResponse, sacp::Error> {
        let entries = self
            .provider_inventory
            .entries(&req.provider_ids)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        Ok(ListProvidersResponse {
            entries: entries.into_iter().map(inventory_entry_to_dto).collect(),
        })
    }

    #[custom_method(RefreshProviderInventoryRequest)]
    async fn on_refresh_provider_inventory(
        &self,
        req: RefreshProviderInventoryRequest,
    ) -> Result<RefreshProviderInventoryResponse, sacp::Error> {
        let refresh_plan = self
            .provider_inventory
            .plan_refresh(&req.provider_ids)
            .await;
        let refresh_plan =
            refresh_plan.map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        for provider_id in &refresh_plan.started {
            let provider_inventory = self.provider_inventory.clone();
            let provider_factory = Arc::clone(&self.provider_factory);
            let provider_id = provider_id.clone();
            tokio::spawn(async move {
                let result = async {
                    let metadata = crate::providers::get_from_registry(&provider_id).await?;
                    let model_config =
                        crate::model::ModelConfig::new(&metadata.metadata().default_model)?
                            .with_canonical_limits(&provider_id);
                    let provider =
                        provider_factory(provider_id.clone(), model_config, Vec::new()).await?;
                    let models = provider.fetch_recommended_models().await?;
                    provider_inventory
                        .store_refreshed_models(&provider_id, &models)
                        .await
                }
                .await;
                if let Err(error) = result {
                    let _ = provider_inventory
                        .store_refresh_error(&provider_id, error.to_string())
                        .await;
                    warn!(provider = %provider_id, error = %error, "provider inventory refresh failed");
                }
            });
        }
        Ok(RefreshProviderInventoryResponse {
            started: refresh_plan.started,
            skipped: refresh_plan
                .skipped
                .into_iter()
                .map(|entry| RefreshProviderInventorySkipDto {
                    provider_id: entry.provider_id,
                    reason: match entry.reason {
                        RefreshSkipReason::UnknownProvider => {
                            RefreshProviderInventorySkipReasonDto::UnknownProvider
                        }
                        RefreshSkipReason::NotConfigured => {
                            RefreshProviderInventorySkipReasonDto::NotConfigured
                        }
                        RefreshSkipReason::DoesNotSupportRefresh => {
                            RefreshProviderInventorySkipReasonDto::DoesNotSupportRefresh
                        }
                        RefreshSkipReason::AlreadyRefreshing => {
                            RefreshProviderInventorySkipReasonDto::AlreadyRefreshing
                        }
                    },
                })
                .collect(),
        })
    }

    #[custom_method(ReadConfigRequest)]
    async fn on_read_config(
        &self,
        req: ReadConfigRequest,
    ) -> Result<ReadConfigResponse, sacp::Error> {
        let config = self.load_config().map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to read config: {}", e))
        })?;
        let response = match config.get_param::<serde_json::Value>(&req.key) {
            Ok(value) => ReadConfigResponse { value },
            Err(crate::config::ConfigError::NotFound(_)) => ReadConfigResponse {
                value: serde_json::Value::Null,
            },
            Err(e) => return Err(sacp::Error::internal_error().data(e.to_string())),
        };
        Ok(response)
    }

    #[custom_method(UpsertConfigRequest)]
    async fn on_upsert_config(
        &self,
        req: UpsertConfigRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let config = self.load_config().map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to read config: {}", e))
        })?;
        config
            .set_param(&req.key, &req.value)
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        Ok(EmptyResponse {})
    }

    #[custom_method(RemoveConfigRequest)]
    async fn on_remove_config(
        &self,
        req: RemoveConfigRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let config = self.load_config().map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to read config: {}", e))
        })?;
        config
            .delete(&req.key)
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        Ok(EmptyResponse {})
    }

    #[custom_method(CheckSecretRequest)]
    async fn on_check_secret(
        &self,
        req: CheckSecretRequest,
    ) -> Result<CheckSecretResponse, sacp::Error> {
        let config = self.load_config().map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to read config: {}", e))
        })?;
        let exists = config.get_secret::<serde_json::Value>(&req.key).is_ok();
        Ok(CheckSecretResponse { exists })
    }

    #[custom_method(UpsertSecretRequest)]
    async fn on_upsert_secret(
        &self,
        req: UpsertSecretRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let config = self.load_config().map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to read config: {}", e))
        })?;
        config
            .set_secret(&req.key, &req.value)
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        Ok(EmptyResponse {})
    }

    #[custom_method(RemoveSecretRequest)]
    async fn on_remove_secret(
        &self,
        req: RemoveSecretRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let config = self.load_config().map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to read config: {}", e))
        })?;
        config
            .delete_secret(&req.key)
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        Ok(EmptyResponse {})
    }

    #[custom_method(ExportSessionRequest)]
    async fn on_export_session(
        &self,
        req: ExportSessionRequest,
    ) -> Result<ExportSessionResponse, sacp::Error> {
        let thread = self
            .thread_manager
            .get_thread(&req.session_id)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        let internal_id = thread
            .current_session_id
            .ok_or_else(|| sacp::Error::internal_error().data("Thread has no internal session"))?;
        let data = self
            .session_manager
            .export_session(&internal_id)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        Ok(ExportSessionResponse { data })
    }

    #[custom_method(ImportSessionRequest)]
    async fn on_import_session(
        &self,
        req: ImportSessionRequest,
    ) -> Result<ImportSessionResponse, sacp::Error> {
        let session = self
            .session_manager
            .import_session(&req.data, Some(SessionType::Acp))
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;

        // Create a thread for the imported session.
        let thread = self
            .thread_manager
            .create_thread(
                Some(session.name.clone()),
                None,
                Some(session.working_dir.display().to_string()),
            )
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;

        // Link the internal session to the thread.
        self.session_manager
            .update(&session.id)
            .thread_id(Some(thread.id.clone()))
            .apply()
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;

        // Copy conversation messages into thread_messages so they appear in the thread.
        if let Some(ref conversation) = session.conversation {
            for msg in conversation.messages() {
                self.thread_manager
                    .append_message(&thread.id, Some(&session.id), msg)
                    .await
                    .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
            }
        }

        // Re-fetch thread to get accurate message_count.
        let thread = self
            .thread_manager
            .get_thread(&thread.id)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;

        Ok(ImportSessionResponse {
            session_id: thread.id,
            title: Some(thread.name),
            updated_at: Some(thread.updated_at.to_rfc3339()),
            message_count: thread.message_count as u64,
        })
    }

    #[custom_method(UpdateSessionProjectRequest)]
    async fn on_update_session_project(
        &self,
        req: UpdateSessionProjectRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let project_id = req.project_id;
        self.update_thread_metadata(&req.session_id, move |meta| {
            meta.project_id = project_id;
        })
        .await?;
        Ok(EmptyResponse {})
    }

    #[custom_method(ArchiveSessionRequest)]
    async fn on_archive_session(
        &self,
        req: ArchiveSessionRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        self.thread_manager
            .archive_thread(&req.session_id)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        self.sessions.lock().await.remove(&req.session_id);
        Ok(EmptyResponse {})
    }

    #[custom_method(UnarchiveSessionRequest)]
    async fn on_unarchive_session(
        &self,
        req: UnarchiveSessionRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        self.thread_manager
            .unarchive_thread(&req.session_id)
            .await
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        Ok(EmptyResponse {})
    }

    #[custom_method(CreateSourceRequest)]
    async fn on_create_source(
        &self,
        req: CreateSourceRequest,
    ) -> Result<CreateSourceResponse, sacp::Error> {
        let source = crate::sources::create_source(
            req.source_type,
            &req.name,
            &req.description,
            &req.content,
            req.global,
            req.project_dir.as_deref(),
        )?;
        Ok(CreateSourceResponse { source })
    }

    #[custom_method(ListSourcesRequest)]
    async fn on_list_sources(
        &self,
        req: ListSourcesRequest,
    ) -> Result<ListSourcesResponse, sacp::Error> {
        let sources = crate::sources::list_sources(req.source_type, req.project_dir.as_deref())?;
        Ok(ListSourcesResponse { sources })
    }

    #[custom_method(UpdateSourceRequest)]
    async fn on_update_source(
        &self,
        req: UpdateSourceRequest,
    ) -> Result<UpdateSourceResponse, sacp::Error> {
        let source = crate::sources::update_source(
            req.source_type,
            &req.name,
            &req.description,
            &req.content,
            req.global,
            req.project_dir.as_deref(),
        )?;
        Ok(UpdateSourceResponse { source })
    }

    #[custom_method(DeleteSourceRequest)]
    async fn on_delete_source(
        &self,
        req: DeleteSourceRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        crate::sources::delete_source(
            req.source_type,
            &req.name,
            req.global,
            req.project_dir.as_deref(),
        )?;
        Ok(EmptyResponse {})
    }

    #[custom_method(ExportSourceRequest)]
    async fn on_export_source(
        &self,
        req: ExportSourceRequest,
    ) -> Result<ExportSourceResponse, sacp::Error> {
        let (json, filename) = crate::sources::export_source(
            req.source_type,
            &req.name,
            req.global,
            req.project_dir.as_deref(),
        )?;
        Ok(ExportSourceResponse { json, filename })
    }

    #[custom_method(ImportSourcesRequest)]
    async fn on_import_sources(
        &self,
        req: ImportSourcesRequest,
    ) -> Result<ImportSourcesResponse, sacp::Error> {
        let sources =
            crate::sources::import_sources(&req.data, req.global, req.project_dir.as_deref())?;
        Ok(ImportSourcesResponse { sources })
    }

    #[custom_method(DictationTranscribeRequest)]
    async fn on_dictation_transcribe(
        &self,
        req: DictationTranscribeRequest,
    ) -> Result<DictationTranscribeResponse, sacp::Error> {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        let config = crate::config::Config::global();

        #[cfg(not(feature = "local-inference"))]
        if req.provider == "local" {
            return Err(sacp::Error::invalid_params()
                .data("Local inference is not available in this build"));
        }

        let provider: DictationProvider = serde_json::from_value(serde_json::Value::String(
            req.provider.clone(),
        ))
        .map_err(|_| {
            sacp::Error::invalid_params().data(format!("Unknown provider: {}", req.provider))
        })?;

        let audio_bytes = BASE64
            .decode(&req.audio)
            .map_err(|_| sacp::Error::invalid_params().data("Invalid base64 audio data"))?;

        if audio_bytes.len() > 50 * 1024 * 1024 {
            return Err(sacp::Error::invalid_params().data("Audio too large (max 50MB)"));
        }

        let extension = match req.mime_type.as_str() {
            "audio/webm" | "audio/webm;codecs=opus" => "webm",
            "audio/mp4" => "mp4",
            "audio/mpeg" | "audio/mpga" => "mp3",
            "audio/m4a" => "m4a",
            "audio/wav" | "audio/x-wav" => "wav",
            other => {
                return Err(
                    sacp::Error::invalid_params().data(format!("Unsupported format: {other}"))
                );
            }
        };

        let text = match provider {
            DictationProvider::OpenAI => {
                let model = dictation_selected_model(config, DictationProvider::OpenAI)
                    .unwrap_or_else(|| OPENAI_TRANSCRIPTION_MODEL.to_string());
                transcribe_with_provider(
                    DictationProvider::OpenAI,
                    "model".to_string(),
                    model,
                    audio_bytes,
                    extension,
                    &req.mime_type,
                )
                .await
            }
            DictationProvider::Groq => {
                let model = dictation_selected_model(config, DictationProvider::Groq)
                    .unwrap_or_else(|| GROQ_TRANSCRIPTION_MODEL.to_string());
                transcribe_with_provider(
                    DictationProvider::Groq,
                    "model".to_string(),
                    model,
                    audio_bytes,
                    extension,
                    &req.mime_type,
                )
                .await
            }
            DictationProvider::ElevenLabs => {
                let model = dictation_selected_model(config, DictationProvider::ElevenLabs)
                    .unwrap_or_else(|| ELEVENLABS_TRANSCRIPTION_MODEL.to_string());
                transcribe_with_provider(
                    DictationProvider::ElevenLabs,
                    "model_id".to_string(),
                    model,
                    audio_bytes,
                    extension,
                    &req.mime_type,
                )
                .await
            }
            #[cfg(feature = "local-inference")]
            DictationProvider::Local => transcribe_local(audio_bytes).await,
        }
        .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;

        Ok(DictationTranscribeResponse { text })
    }

    #[custom_method(DictationConfigRequest)]
    async fn on_dictation_config(
        &self,
        _req: DictationConfigRequest,
    ) -> Result<DictationConfigResponse, sacp::Error> {
        let config = crate::config::Config::global();
        let mut providers = std::collections::HashMap::new();

        for def in all_providers() {
            let provider = def.provider;
            let host = if let Some(host_key) = def.host_key {
                config
                    .get(host_key, false)
                    .ok()
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            } else {
                None
            };

            let provider_key = serde_json::to_value(provider)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| format!("{:?}", provider).to_lowercase());
            providers.insert(
                provider_key,
                DictationProviderStatusEntry {
                    configured: is_configured(provider),
                    host,
                    description: def.description.to_string(),
                    uses_provider_config: def.uses_provider_config,
                    settings_path: def.settings_path.map(|s| s.to_string()),
                    config_key: if !def.uses_provider_config {
                        Some(def.config_key.to_string())
                    } else {
                        None
                    },
                    model_config_key: dictation_model_config_key(provider),
                    default_model: dictation_default_model(provider),
                    selected_model: dictation_selected_model(config, provider),
                    available_models: dictation_available_models(provider),
                },
            );
        }

        Ok(DictationConfigResponse { providers })
    }

    #[custom_method(DictationModelsListRequest)]
    async fn on_dictation_models_list(
        &self,
        _req: DictationModelsListRequest,
    ) -> Result<DictationModelsListResponse, sacp::Error> {
        #[cfg(feature = "local-inference")]
        {
            use crate::download_manager::{get_download_manager, DownloadStatus};

            let manager = get_download_manager();
            let models = whisper::available_models()
                .iter()
                .map(|model| DictationLocalModelStatus {
                    id: model.id.to_string(),
                    label: model.id.to_string(),
                    description: model.description.to_string(),
                    size_mb: model.size_mb,
                    downloaded: model.is_downloaded(),
                    download_in_progress: manager
                        .get_progress(model.id)
                        .map(|progress| progress.status == DownloadStatus::Downloading)
                        .unwrap_or(false),
                })
                .collect();

            Ok(DictationModelsListResponse { models })
        }

        #[cfg(not(feature = "local-inference"))]
        Ok(DictationModelsListResponse::default())
    }

    #[custom_method(DictationModelDownloadRequest)]
    async fn on_dictation_model_download(
        &self,
        _req: DictationModelDownloadRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        #[cfg(feature = "local-inference")]
        {
            use crate::download_manager::get_download_manager;

            let model = whisper::get_model(&_req.model_id)
                .ok_or_else(|| sacp::Error::invalid_params().data("Unknown model id"))?;
            let manager = get_download_manager();
            let model_id_for_config = model.id.to_string();

            manager
                .download_model(
                    model.id.to_string(),
                    model.url.to_string(),
                    model.local_path(),
                    Some(Box::new(move || {
                        let config = crate::config::Config::global();
                        // Only auto-select this model if the user has no model
                        // currently selected. This prevents silently switching
                        // the active model mid-session when a user downloads an
                        // additional model while one is already in use.
                        let already_selected = config
                            .get(whisper::LOCAL_WHISPER_MODEL_CONFIG_KEY, false)
                            .ok()
                            .and_then(|value| value.as_str().map(str::to_owned))
                            .filter(|model_id| {
                                // Treat a deleted model file as no active selection
                                // so a fresh download can auto-select cleanly.
                                whisper::get_model(model_id)
                                    .is_some_and(|model| model.is_downloaded())
                            });
                        if already_selected.is_none() {
                            if let Err(e) = config.set_param(
                                whisper::LOCAL_WHISPER_MODEL_CONFIG_KEY,
                                model_id_for_config.clone(),
                            ) {
                                error!("Failed to save LOCAL_WHISPER_MODEL after download: {}", e);
                            }
                        }
                    })),
                )
                .await
                .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;

            Ok(EmptyResponse {})
        }

        #[cfg(not(feature = "local-inference"))]
        Err(sacp::Error::invalid_params().data("Local inference not enabled"))
    }

    #[custom_method(DictationModelDownloadProgressRequest)]
    async fn on_dictation_model_download_progress(
        &self,
        _req: DictationModelDownloadProgressRequest,
    ) -> Result<DictationModelDownloadProgressResponse, sacp::Error> {
        #[cfg(feature = "local-inference")]
        {
            use crate::download_manager::get_download_manager;

            let manager = get_download_manager();
            let progress =
                manager
                    .get_progress(&_req.model_id)
                    .map(|progress| DictationDownloadProgress {
                        bytes_downloaded: progress.bytes_downloaded,
                        total_bytes: progress.total_bytes,
                        progress_percent: progress.progress_percent,
                        status: serde_json::to_value(&progress.status)
                            .ok()
                            .and_then(|value| value.as_str().map(ToOwned::to_owned))
                            .unwrap_or_else(|| "unknown".to_string()),
                        error: progress.error,
                    });

            Ok(DictationModelDownloadProgressResponse { progress })
        }

        #[cfg(not(feature = "local-inference"))]
        Ok(DictationModelDownloadProgressResponse { progress: None })
    }

    #[custom_method(DictationModelCancelRequest)]
    async fn on_dictation_model_cancel(
        &self,
        _req: DictationModelCancelRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        #[cfg(feature = "local-inference")]
        {
            use crate::download_manager::get_download_manager;

            let manager = get_download_manager();
            manager
                .cancel_download(&_req.model_id)
                .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;

            Ok(EmptyResponse {})
        }

        #[cfg(not(feature = "local-inference"))]
        Err(sacp::Error::invalid_params().data("Local inference not enabled"))
    }

    #[custom_method(DictationModelDeleteRequest)]
    async fn on_dictation_model_delete(
        &self,
        _req: DictationModelDeleteRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        #[cfg(feature = "local-inference")]
        {
            let model = whisper::get_model(&_req.model_id)
                .ok_or_else(|| sacp::Error::invalid_params().data("Unknown model id"))?;
            let path = model.local_path();

            if !path.exists() {
                return Err(sacp::Error::invalid_params().data("Model not downloaded"));
            }

            std::fs::remove_file(path)
                .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;

            Ok(EmptyResponse {})
        }

        #[cfg(not(feature = "local-inference"))]
        Err(sacp::Error::invalid_params().data("Local inference not enabled"))
    }

    #[custom_method(DictationModelSelectRequest)]
    async fn on_dictation_model_select(
        &self,
        req: DictationModelSelectRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        #[cfg(not(feature = "local-inference"))]
        if req.provider == "local" {
            return Err(sacp::Error::invalid_params().data("Local inference not enabled"));
        }

        let provider: DictationProvider = serde_json::from_value(serde_json::Value::String(
            req.provider.clone(),
        ))
        .map_err(|_| {
            sacp::Error::invalid_params().data(format!("Unknown provider: {}", req.provider))
        })?;

        let key = match provider {
            DictationProvider::OpenAI => OPENAI_TRANSCRIPTION_MODEL_CONFIG_KEY,
            DictationProvider::Groq => GROQ_TRANSCRIPTION_MODEL_CONFIG_KEY,
            DictationProvider::ElevenLabs => ELEVENLABS_TRANSCRIPTION_MODEL_CONFIG_KEY,
            #[cfg(feature = "local-inference")]
            DictationProvider::Local => {
                let model = whisper::get_model(&req.model_id)
                    .ok_or_else(|| sacp::Error::invalid_params().data("Unknown model id"))?;
                if !model.is_downloaded() {
                    return Err(
                        sacp::Error::invalid_params().data("Local Whisper model is not downloaded")
                    );
                }
                whisper::LOCAL_WHISPER_MODEL_CONFIG_KEY
            }
        };

        crate::config::Config::global()
            .set_param(key, req.model_id)
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;

        Ok(EmptyResponse {})
    }
}

fn dictation_model_config_key(provider: DictationProvider) -> Option<String> {
    match provider {
        DictationProvider::OpenAI => Some(OPENAI_TRANSCRIPTION_MODEL_CONFIG_KEY.to_string()),
        DictationProvider::Groq => Some(GROQ_TRANSCRIPTION_MODEL_CONFIG_KEY.to_string()),
        DictationProvider::ElevenLabs => {
            Some(ELEVENLABS_TRANSCRIPTION_MODEL_CONFIG_KEY.to_string())
        }
        #[cfg(feature = "local-inference")]
        DictationProvider::Local => Some(whisper::LOCAL_WHISPER_MODEL_CONFIG_KEY.to_string()),
    }
}

fn dictation_default_model(provider: DictationProvider) -> Option<String> {
    match provider {
        DictationProvider::OpenAI => Some(OPENAI_TRANSCRIPTION_MODEL.to_string()),
        DictationProvider::Groq => Some(GROQ_TRANSCRIPTION_MODEL.to_string()),
        DictationProvider::ElevenLabs => Some(ELEVENLABS_TRANSCRIPTION_MODEL.to_string()),
        #[cfg(feature = "local-inference")]
        DictationProvider::Local => Some(whisper::recommend_model().to_string()),
    }
}

fn dictation_selected_model(config: &Config, provider: DictationProvider) -> Option<String> {
    #[cfg(feature = "local-inference")]
    if provider == DictationProvider::Local {
        return config
            .get(whisper::LOCAL_WHISPER_MODEL_CONFIG_KEY, false)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .filter(|model_id| whisper::get_model(model_id).is_some())
            .or_else(|| dictation_default_model(provider));
    }

    dictation_model_config_key(provider)
        .and_then(|key| {
            config
                .get(&key, false)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
        })
        .or_else(|| dictation_default_model(provider))
}

fn dictation_available_models(provider: DictationProvider) -> Vec<DictationModelOption> {
    match provider {
        DictationProvider::OpenAI => vec![DictationModelOption {
            id: OPENAI_TRANSCRIPTION_MODEL.to_string(),
            label: "Whisper-1".to_string(),
            description: "OpenAI's hosted Whisper transcription model.".to_string(),
        }],
        DictationProvider::Groq => vec![DictationModelOption {
            id: GROQ_TRANSCRIPTION_MODEL.to_string(),
            label: "Whisper Large V3 Turbo".to_string(),
            description: "Groq's fast hosted Whisper transcription model.".to_string(),
        }],
        DictationProvider::ElevenLabs => vec![DictationModelOption {
            id: ELEVENLABS_TRANSCRIPTION_MODEL.to_string(),
            label: "Scribe v1".to_string(),
            description: "ElevenLabs' hosted speech-to-text model.".to_string(),
        }],
        #[cfg(feature = "local-inference")]
        DictationProvider::Local => whisper::available_models()
            .iter()
            .map(|model| DictationModelOption {
                id: model.id.to_string(),
                label: model.id.to_string(),
                description: model.description.to_string(),
            })
            .collect(),
    }
}

pub struct GooseAcpHandler {
    pub agent: Arc<GooseAcpAgent>,
}

impl HandleDispatchFrom<Client> for GooseAcpHandler {
    fn describe_chain(&self) -> impl std::fmt::Debug {
        "goose-acp"
    }

    fn handle_dispatch_from(
        &mut self,
        message: Dispatch,
        cx: ConnectionTo<Client>,
    ) -> impl std::future::Future<Output = Result<Handled<Dispatch>, sacp::Error>> + Send {
        let agent = self.agent.clone();

        // The MatchDispatchFrom chain produces an ~85KB async state machine.
        // Box::pin moves it to the heap so it doesn't overflow the tokio worker stack.
        Box::pin(async move {
            MatchDispatchFrom::new(message, &cx)
                .if_request(
                    |req: InitializeRequest, responder: Responder<InitializeResponse>| async {
                        responder.respond_with_result(agent.on_initialize(req).await)
                    },
                )
                .await
                .if_request(
                    |_req: AuthenticateRequest, responder: Responder<AuthenticateResponse>| async {
                        responder.respond(AuthenticateResponse::new())
                    },
                )
                .await
                .if_request(
                    |req: NewSessionRequest, responder: Responder<NewSessionResponse>| async {
                        responder.respond_with_result(agent.on_new_session(&cx, req).await)
                    },
                )
                .await
                .if_request(
                    |req: LoadSessionRequest, responder: Responder<LoadSessionResponse>| async {
                        let agent = agent.clone();
                        let cx_clone = cx.clone();
                        cx.spawn(async move {
                            match agent.on_load_session(&cx_clone, req).await {
                                Ok(response) => {
                                    responder.respond(response)?;
                                }
                                Err(e) => {
                                    responder.respond_with_error(e)?;
                                }
                            }
                            Ok(())
                        })?;
                        Ok(())
                    },
                )
                .await
                .if_request(
                    |req: PromptRequest, responder: Responder<PromptResponse>| async {
                        let agent = agent.clone();
                        let cx_clone = cx.clone();
                        cx.spawn(async move {
                            match agent.on_prompt(&cx_clone, req).await {
                                Ok(response) => {
                                    responder.respond(response)?;
                                }
                                Err(e) => {
                                    responder.respond_with_error(e)?;
                                }
                            }
                            Ok(())
                        })?;
                        Ok(())
                    },
                )
                .await
                .if_notification(|notif: CancelNotification| async { agent.on_cancel(notif).await })
                .await
                // set_config_option (SACP 11) and legacy set_mode/set_model; custom _goose/* in otherwise.
                .if_request({
                    let agent = agent.clone();
                    let cx = cx.clone();
                    |req: SetSessionConfigOptionRequest, responder: Responder<SetSessionConfigOptionResponse>| async move {
                        let value_id = req.value.as_value_id()
                            .ok_or_else(|| sacp::Error::invalid_params().data("Expected a value ID"))?
                            .clone();
                        let session_id = req.session_id.clone();
                        let sid = sid_short(session_id.0.as_ref());
                        let config_id = req.config_id.0.to_string();
                        let t_handler = std::time::Instant::now();
                        debug!(target: "perf", sid = %sid, config_id = %config_id, value = %value_id.0, "perf: set_config_option start");
                        match config_id.as_ref() {
                            "provider" => {
                                match agent.update_provider(&session_id.0, &value_id.0, None, None, None).await {
                                    Ok(_) => {}
                                    Err(e) => { responder.respond_with_error(e)?; return Ok(()); }
                                }
                            }
                            "mode" => {
                                match agent.on_set_mode(&session_id.0, &value_id.0).await {
                                    Ok(_) => {}
                                    Err(e) => { responder.respond_with_error(e)?; return Ok(()); }
                                }
                            }
                            "model" => {
                                match agent.on_set_model(&session_id.0, &value_id.0).await {
                                    Ok(_) => {}
                                    Err(e) => { responder.respond_with_error(e)?; return Ok(()); }
                                }
                            }
                            other => {
                                responder.respond_with_error(
                                    sacp::Error::invalid_params().data(format!("Unsupported config option: {}", other))
                                )?;
                                return Ok(());
                            }
                        }
                        // Respond immediately using the current provider inventory snapshot.
                        let t_tail = std::time::Instant::now();
                        let (notification, config_options) = agent.build_config_update(&session_id).await?;
                        cx.send_notification(notification)?;
                        responder.respond(SetSessionConfigOptionResponse::new(config_options))?;
                        debug!(target: "perf", sid = %sid, ms = t_tail.elapsed().as_millis() as u64, "perf: set_config_option inventory_respond");

                        let maybe_refresh = if config_id == "provider" {
                            let provider_id = value_id.0.to_string();
                            agent
                                .provider_inventory
                                .plan_refresh(std::slice::from_ref(&provider_id))
                                .await
                                .ok()
                                .filter(|plan| plan.started.iter().any(|id| id == &provider_id))
                        } else {
                            None
                        };
                        if maybe_refresh.is_some() {
                            let agent_bg = agent.clone();
                            let cx_bg = cx.clone();
                            let session_id_bg = session_id.clone();
                            let sid_bg = sid.clone();
                            tokio::spawn(async move {
                                let t_bg = std::time::Instant::now();
                                let refreshed = async {
                                    let session_agent =
                                        agent_bg.get_session_agent(&session_id_bg.0, None).await?;
                                    let provider = session_agent
                                        .provider()
                                        .await
                                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                                    let provider_name = provider.get_name().to_string();
                                    let models = provider
                                        .fetch_recommended_models()
                                        .await
                                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                                    agent_bg
                                        .provider_inventory
                                        .store_refreshed_models(&provider_name, &models)
                                        .await?;
                                    agent_bg
                                        .build_config_update(&session_id_bg)
                                        .await
                                        .map_err(|e| anyhow::anyhow!(e.to_string()))
                                }
                                .await;

                                match refreshed {
                                    Ok((fresh_notification, _)) => {
                                        let _ = cx_bg.send_notification(fresh_notification);
                                        debug!(target: "perf", sid = %sid_bg, ms = t_bg.elapsed().as_millis() as u64, "perf: set_config_option background_refresh done");
                                    }
                                    Err(e) => {
                                        if let Ok(session_agent) =
                                            agent_bg.get_session_agent(&session_id_bg.0, None).await
                                        {
                                            if let Ok(provider) = session_agent.provider().await {
                                                let provider_name = provider.get_name().to_string();
                                                let _ = agent_bg
                                                    .provider_inventory
                                                    .store_refresh_error(&provider_name, e.to_string())
                                                    .await;
                                            }
                                        }
                                        debug!(target: "perf", sid = %sid_bg, error = %e, ms = t_bg.elapsed().as_millis() as u64, "perf: set_config_option background_refresh failed");
                                    }
                                }
                            });
                        }

                        debug!(target: "perf", sid = %sid, ms = t_handler.elapsed().as_millis() as u64, config_id = %config_id, "perf: set_config_option done");
                        Ok(())
                    }
                })
                .await
                .if_request({
                    let agent = agent.clone();
                    let cx = cx.clone();
                    |req: SetSessionModeRequest, responder: Responder<SetSessionModeResponse>| async move {
                        let session_id = req.session_id.clone();
                        let mode_id = req.mode_id.clone();
                        match agent.on_set_mode(&session_id.0, &mode_id.0).await {
                            Ok(resp) => {
                                // Notify before responding so clients see the mode update before block_task unblocks.
                                cx.send_notification(SessionNotification::new(
                                    session_id,
                                    SessionUpdate::CurrentModeUpdate(
                                        CurrentModeUpdate::new(mode_id),
                                    ),
                                ))?;
                                responder.respond(resp)?;
                            }
                            Err(e) => {
                                responder.respond_with_error(e)?;
                            }
                        }
                        Ok(())
                    }
                })
                .await
                .if_request({
                    let agent = agent.clone();
                    let cx = cx.clone();
                    |req: SetSessionModelRequest, responder: Responder<SetSessionModelResponse>| async move {
                        let session_id = req.session_id.clone();
                        match agent.on_set_model(&session_id.0, &req.model_id.0).await {
                            Ok(resp) => {
                                let (notification, _) = agent.build_config_update(&session_id).await?;
                                cx.send_notification(notification)?;
                                responder.respond(resp)?;
                            }
                            Err(e) => responder.respond_with_error(e)?,
                        }
                        Ok(())
                    }
                })
                .await
                .if_request({
                    let agent = agent.clone();
                    |_req: ListSessionsRequest, responder: Responder<ListSessionsResponse>| async move {
                        responder.respond(agent.on_list_sessions().await?)
                    }
                })
                .await
                .if_request({
                    let agent = agent.clone();
                    |req: CloseSessionRequest, responder: Responder<CloseSessionResponse>| async move {
                        responder.respond(agent.on_close_session(&req.session_id.0).await?)
                    }
                })
                .await
                .if_request({
                    let agent = agent.clone();
                    let cx = cx.clone();
                    |req: ForkSessionRequest, responder: Responder<ForkSessionResponse>| async move {
                        responder.respond_with_result(agent.on_fork_session(&cx, req).await)
                    }
                })
                .await
                .otherwise({
                    let agent = agent.clone();
                    |message: Dispatch| async move {
                        match message {
                            Dispatch::Request(req, responder) => {
                                match agent.handle_custom_request(&req.method, req.params).await {
                                    Ok(json) => responder.respond(json)?,
                                    Err(e) => responder.respond_with_error(e)?,
                                }
                                Ok(())
                            }
                            Dispatch::Response(result, router) => {
                                debug!(method = %router.method(), id = %router.id(), ok = result.is_ok(), "routing response");
                                router.respond_with_result(result)?;
                                Ok(())
                            }
                            Dispatch::Notification(notif) => {
                                debug!(method = %notif.method, "unhandled notification");
                                Ok(())
                            }
                        }
                    }
                })
                .await
                .map(|()| Handled::Yes)
        })
    }
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
        },
    );
    let agent = server.create_agent().await?;
    serve(agent, incoming, outgoing).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::{ToolRequest, ToolResponse};
    use rmcp::model::{CallToolRequestParams, Content as RmcpContent};
    use sacp::schema::{
        EnvVariable, HttpHeader, McpServer, McpServerHttp, McpServerSse, McpServerStdio,
        PermissionOptionId, ResourceLink, SelectedPermissionOutcome, SessionConfigSelectOption,
        SessionMode, SessionModeId, SessionModeState,
    };
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
    fn test_build_mode_state(current_mode: GooseMode) -> Result<SessionModeState, sacp::Error> {
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
