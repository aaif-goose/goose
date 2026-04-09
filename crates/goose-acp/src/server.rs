use crate::custom_requests::*;
use crate::fs::AcpTools;
use crate::tools::AcpAwareToolMeta;
use anyhow::Result;
use fs_err as fs;
use futures::future::BoxFuture;
use goose::acp::{PermissionDecision, ACP_CURRENT_MODEL};
use goose::agents::extension::{Envs, PLATFORM_EXTENSIONS};
use goose::agents::mcp_client::McpClientTrait;
use goose::agents::platform_extensions::developer::DeveloperClient;
use goose::agents::{Agent, AgentConfig, ExtensionConfig, GoosePlatform, SessionConfig};
use goose::builtin_extension::register_builtin_extensions;
use goose::config::base::CONFIG_YAML_NAME;
use goose::config::extensions::get_enabled_extensions_with_config;
use goose::config::paths::Paths;
use goose::config::permission::PermissionManager;
use goose::config::{Config, GooseMode};
use goose::conversation::message::{ActionRequiredData, Message, MessageContent};
use goose::mcp_utils::ToolResult;
use goose::permission::permission_confirmation::PrincipalType;
use goose::permission::{Permission, PermissionConfirmation};
use goose::providers::base::Provider;
use goose::session::session_manager::SessionType;
use goose::session::{EnabledExtensionsState, Session, SessionManager};
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
    ToolCallUpdateFields, ToolKind,
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
            goose::model::ModelConfig,
            Vec<ExtensionConfig>,
        ) -> BoxFuture<'static, Result<Arc<dyn Provider>>>
        + Send
        + Sync,
>;

const DEFAULT_PROVIDER_ID: &str = "goose";
const DEFAULT_PROVIDER_LABEL: &str = "Goose (Default)";

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
    tool_requests: HashMap<String, goose::conversation::message::ToolRequest>,
    cancel_token: Option<CancellationToken>,
    /// Working directory set while the agent was still loading.
    /// Applied once the agent becomes ready.
    pending_working_dir: Option<std::path::PathBuf>,
}

/// The agent may still be initializing in the background (extension loading,
/// provider setup).  Callers that need the live agent (e.g. `on_prompt`) await
/// the handle; callers that only need the session metadata can proceed without it.
enum AgentHandle {
    Ready(Arc<Agent>),
    Loading(tokio::sync::watch::Receiver<Option<Result<Arc<Agent>, String>>>),
}

struct AgentSetupRequest {
    session_id: SessionId,
    goose_session: Session,
    mcp_servers: Vec<McpServer>,
    /// Pre-resolved provider name + model config (from config, no network).
    /// When present the spawn skips re-deriving these from config.
    resolved_provider: Option<(String, goose::model::ModelConfig)>,
}

pub struct GooseAcpAgent {
    sessions: Arc<Mutex<HashMap<String, GooseAcpSession>>>,
    provider_factory: AcpProviderFactory,
    builtins: Vec<String>,
    client_fs_capabilities: OnceCell<FileSystemCapabilities>,
    client_terminal: OnceCell<bool>,
    config_dir: std::path::PathBuf,
    session_manager: Arc<SessionManager>,
    thread_manager: Arc<goose::session::ThreadManager>,
    permission_manager: Arc<PermissionManager>,
    goose_mode: GooseMode,
    disable_session_naming: bool,
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
    tool_response: &goose::conversation::message::ToolResponse,
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
    tool_request: &goose::conversation::message::ToolRequest,
    tool_response: &goose::conversation::message::ToolResponse,
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
                        return Some(format!("{}…", goose::utils::safe_truncate(first_line, 57)));
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

async fn build_model_state(provider: &dyn Provider) -> Result<SessionModelState, sacp::Error> {
    let models = provider
        .fetch_recommended_models()
        .await
        .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
    let current_model = &provider.get_model_config().model_name;
    Ok(SessionModelState::new(
        ModelId::new(current_model.as_str()),
        models
            .iter()
            .map(|name| ModelInfo::new(ModelId::new(&**name), &**name))
            .collect(),
    ))
}

async fn list_provider_entries(current_provider: Option<&str>) -> Vec<ProviderListEntry> {
    let mut providers = goose::providers::providers()
        .await
        .into_iter()
        .map(|(metadata, _)| ProviderListEntry {
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
            providers.push(ProviderListEntry {
                id: current_provider.to_string(),
                label: current_provider.to_string(),
            });
            providers.sort_by(|left, right| left.id.cmp(&right.id));
        }
    }

    let mut entries = Vec::with_capacity(providers.len() + 1);
    entries.push(ProviderListEntry {
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
) -> Result<(String, goose::model::ModelConfig), String> {
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
            let entry = goose::providers::get_from_registry(&provider_name)
                .await
                .map_err(|e| e.to_string())?;
            let default_model = &entry.metadata().default_model;
            goose::model::ModelConfig::new(default_model)
                .map_err(|e| e.to_string())?
                .with_canonical_limits(&provider_name)
        }
        None => {
            let model_id = config.get_goose_model().map_err(|e| e.to_string())?;
            goose::model::ModelConfig::new(&model_id)
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
) -> Result<(String, goose::model::ModelConfig), String> {
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

/// Build model state and config options eagerly from the canonical registry.
///
/// TODO: This trades speed for correctness — the canonical registry may not perfectly
/// match what the provider API returns (new models not yet in the registry, deprecated
/// models still listed, or locally-installed models for providers like Ollama). Consider
/// whether to reconcile with a live API call in the background.
async fn build_eager_config(
    resolved: &Result<(String, goose::model::ModelConfig), String>,
    mode_state: &SessionModeState,
    goose_session: &Session,
) -> (Option<SessionModelState>, Option<Vec<SessionConfigOption>>) {
    let Ok((ref provider_name, ref mc)) = resolved else {
        return (None, None);
    };
    let recommended = goose::providers::canonical::recommended_models_from_registry(provider_name);
    let available: Vec<ModelInfo> = recommended
        .iter()
        .map(|name| ModelInfo::new(ModelId::new(&**name), &**name))
        .collect();
    let ms = SessionModelState::new(ModelId::new(mc.model_name.as_str()), available);
    let provider_selection = session_provider_selection(goose_session);
    let provider_options = build_provider_options(Some(provider_name.as_str())).await;
    let config_options =
        build_config_options(mode_state, &ms, provider_selection, provider_options);
    (Some(ms), Some(config_options))
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
        let thread_manager = Arc::new(goose::session::ThreadManager::new(
            session_manager.storage().clone(),
        ));
        let permission_manager = Arc::new(PermissionManager::new(config_dir.clone()));

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
        })
    }

    fn load_config(&self) -> Result<Config> {
        Config::new(self.config_dir.join(CONFIG_YAML_NAME), "goose").map_err(Into::into)
    }

    async fn create_provider(
        &self,
        provider_name: &str,
        model_config: goose::model::ModelConfig,
        extensions: Vec<ExtensionConfig>,
    ) -> Result<Arc<dyn Provider>> {
        (self.provider_factory)(provider_name.to_string(), model_config, extensions).await
    }

    fn spawn_agent_setup(
        &self,
        cx: &ConnectionTo<Client>,
        agent_tx: tokio::sync::watch::Sender<Option<Result<Arc<Agent>, String>>>,
        req: AgentSetupRequest,
    ) {
        let AgentSetupRequest {
            session_id,
            goose_session,
            mcp_servers,
            resolved_provider,
        } = req;

        let goose_mode = goose_session.goose_mode;
        let internal_session_id = goose_session.id.clone();
        let agent_session_id = SessionId::new(internal_session_id.clone());

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
            let result: Result<(), String> = async {
                let agent = Arc::new(Agent::with_config(AgentConfig::new(
                    session_manager,
                    permission_manager,
                    None,
                    goose_mode,
                    disable_session_naming,
                    GoosePlatform::GooseCli,
                )));

                let config_path = config_dir.join(CONFIG_YAML_NAME);
                let mut extensions = Config::new(&config_path, "goose")
                    .ok()
                    .map(|c| get_enabled_extensions_with_config(&c))
                    .unwrap_or_default();
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
                        let sid = sid_str.clone();
                        async move {
                            let name = ext.name().to_string();
                            match ext_manager
                                .add_extension(ext, None, None, sid.as_deref())
                                .await
                            {
                                Ok(_) => info!(extension = %name, "extension loaded"),
                                Err(e) => {
                                    warn!(extension = %name, error = %e, "extension load failed")
                                }
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

                // Init provider — reuse the pre-resolved name + model when
                // available (already computed in on_new_session), otherwise
                // fall back to reading config (e.g. load_session path).
                let config = Config::new(config_dir.join(CONFIG_YAML_NAME), "goose")
                    .map_err(|e| e.to_string())?;
                let (provider_name, model_config) = match resolved_provider {
                    Some(resolved) => resolved,
                    None => resolve_provider_and_model_from_config(&config, &goose_session).await?,
                };
                let ext_state = EnabledExtensionsState::extensions_or_default(
                    Some(&goose_session.extension_data),
                    &config,
                );
                let provider = provider_factory(provider_name.to_string(), model_config, ext_state)
                    .await
                    .map_err(|e| e.to_string())?;
                agent
                    .update_provider(provider.clone(), &goose_session.id)
                    .await
                    .map_err(|e| e.to_string())?;

                agent
                    .update_goose_mode(goose_mode, &internal_session_id)
                    .await
                    .map_err(|e| e.to_string())?;

                GooseAcpAgent::add_mcp_extensions(&agent, mcp_servers, &internal_session_id)
                    .await
                    .map_err(|e| e.to_string())?;

                // Apply any working directory that was set while we were loading.
                {
                    let mut locked = sessions.lock().await;
                    if let Some(session) = locked.get_mut(session_id.0.as_ref()) {
                        if let Some(dir) = session.pending_working_dir.take() {
                            agent.extension_manager.update_working_dir(&dir).await;
                        }
                        session.agent = AgentHandle::Ready(agent.clone());
                    }
                }

                let _ = agent_tx.send(Some(Ok(agent)));

                Ok(())
            }
            .await;

            if let Err(e) = &result {
                error!(error = %e, "Background agent setup failed");
                let _ = agent_tx.send(Some(Err(e.clone())));
            }
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
        tool_request: &goose::conversation::message::ToolRequest,
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

        cx.send_notification(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::ToolCall(
                ToolCall::new(
                    ToolCallId::new(tool_request.id.clone()),
                    fallback_title.clone(),
                )
                .status(ToolCallStatus::Pending),
            ),
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
                        format!("{}…", goose::utils::safe_truncate(&s, 300))
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
        tool_response: &goose::conversation::message::ToolResponse,
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

        let requested_provider = args
            .meta
            .as_ref()
            .and_then(|m| m.get("provider"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Create the Thread — this IS the ACP session from the client's perspective.
        let thread_metadata = goose::session::ThreadMetadata {
            provider_id: requested_provider.clone(),
            mode: Some(self.goose_mode.to_string()),
            ..Default::default()
        };
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

        // Create the first internal Session linked to this thread.
        let goose_session = self
            .create_internal_session(
                &thread_id,
                args.cwd.clone(),
                requested_provider.as_deref(),
                None,
            )
            .await?;

        let internal_session_id = goose_session.id.clone();

        let (agent_tx, agent_rx) =
            tokio::sync::watch::channel::<Option<Result<Arc<Agent>, String>>>(None);

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
        let (model_state, config_options) =
            build_eager_config(&resolved, &mode_state, &goose_session).await;

        self.spawn_agent_setup(
            cx,
            agent_tx,
            AgentSetupRequest {
                session_id: SessionId::new(thread_id.clone()),
                goose_session,
                mcp_servers: args.mcp_servers,
                resolved_provider: resolved.ok(),
            },
        );

        let mut response = NewSessionResponse::new(SessionId::new(thread_id)).modes(mode_state);
        if let Some(ms) = model_state {
            response = response.models(ms);
        }
        if let Some(co) = config_options {
            response = response.config_options(co);
        }
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
            if let Ok(mc) = goose::model::ModelConfig::new(model) {
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

    async fn get_session_agent(
        &self,
        thread_id: &str,
        cancel_token: Option<CancellationToken>,
    ) -> Result<Arc<Agent>, sacp::Error> {
        let mut rx = {
            let mut sessions = self.sessions.lock().await;
            let session = sessions.get_mut(thread_id).ok_or_else(|| {
                sacp::Error::resource_not_found(Some(thread_id.to_string()))
                    .data(format!("Session not found: {}", thread_id))
            })?;
            if let Some(token) = cancel_token {
                session.cancel_token = Some(token);
            }
            match &session.agent {
                AgentHandle::Ready(agent) => return Ok(agent.clone()),
                AgentHandle::Loading(rx) => rx.clone(),
            }
        };
        // Drop the lock while we wait for the background setup to finish.
        // spawn_agent_setup promotes the handle to Ready before signalling.
        let agent = {
            let guard = rx.wait_for(|v| v.is_some()).await.map_err(|_| {
                sacp::Error::internal_error().data("Agent setup task was dropped".to_string())
            })?;
            guard
                .as_ref()
                .unwrap()
                .as_ref()
                .map_err(|e| sacp::Error::internal_error().data(e.clone()))?
                .clone()
        };
        Ok(agent)
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

        let thread = self
            .thread_manager
            .get_thread(&thread_id)
            .await
            .map_err(|_| {
                sacp::Error::resource_not_found(Some(thread_id.clone()))
                    .data(format!("Session not found: {}", thread_id))
            })?;

        // Reuse the thread's current internal session so the agent retains
        // conversation context (compaction state, full message history, etc.).
        // The internal session is the source of truth for provider/mode.
        let internal_session_id = thread.current_session_id.clone().ok_or_else(|| {
            sacp::Error::internal_error()
                .data(format!("Thread {} has no internal session", thread_id))
        })?;
        let goose_session = self
            .session_manager
            .get_session(&internal_session_id, false)
            .await
            .map_err(|e| {
                sacp::Error::internal_error()
                    .data(format!("Failed to load internal session: {}", e))
            })?;
        let loaded_mode = goose_session.goose_mode;

        // ── REPLAY MESSAGES FIRST ──
        // Stream the thread's human-visible message history back to the client
        // immediately, before the slow agent/provider/extension setup. The
        // replay only needs the thread_manager (SQLite reads) so the UI gets
        // messages while the agent is still booting.
        let thread_messages = self
            .thread_manager
            .list_messages(&thread_id)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to load thread messages: {}", e))
            })?;

        // Lightweight tool_requests map for the replay loop — we only need it
        // so that handle_tool_response can extract file locations from the
        // matching request. No GooseAcpSession required.
        let mut replay_tool_requests =
            HashMap::<String, goose::conversation::message::ToolRequest>::new();

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
                    }
                    MessageContent::Thinking(thinking) => {
                        cx.send_notification(SessionNotification::new(
                            args.session_id.clone(),
                            SessionUpdate::AgentThoughtChunk(ContentChunk::new(
                                ContentBlock::Text(TextContent::new(thinking.thinking.clone())),
                            )),
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

        // ── Register the session immediately with a Loading handle ──
        let (agent_tx, agent_rx) =
            tokio::sync::watch::channel::<Option<Result<Arc<Agent>, String>>>(None);

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
        let (model_state, config_options) =
            build_eager_config(&resolved, &mode_state, &goose_session).await;

        self.spawn_agent_setup(
            cx,
            agent_tx,
            AgentSetupRequest {
                session_id: args.session_id.clone(),
                goose_session,
                mcp_servers: args.mcp_servers,
                resolved_provider: None,
            },
        );

        let mut response = LoadSessionResponse::new().modes(mode_state);
        if let Some(ms) = model_state {
            response = response.models(ms);
        }
        if let Some(co) = config_options {
            response = response.config_options(co);
        }
        Ok(response)
    }

    async fn on_prompt(
        &self,
        cx: &ConnectionTo<Client>,
        args: PromptRequest,
    ) -> Result<PromptResponse, sacp::Error> {
        // The ACP session_id IS the thread ID.
        let thread_id = args.session_id.0.to_string();
        let cancel_token = CancellationToken::new();
        let internal_session_id = self.internal_session_id(&thread_id).await?;

        let agent = self
            .get_session_agent(&thread_id, Some(cancel_token.clone()))
            .await?;

        let user_message = self.convert_acp_prompt_to_message(args.prompt);

        self.thread_manager
            .append_message(&thread_id, Some(&internal_session_id), &user_message)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to persist message: {}", e))
            })?;

        let session_config = SessionConfig {
            id: internal_session_id.clone(),
            schedule_id: None,
            max_turns: None,
            retry_config: None,
        };

        let mut stream = agent
            .reply(user_message, session_config, Some(cancel_token.clone()))
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Error getting agent reply: {}", e))
            })?;

        use futures::StreamExt;

        let mut was_cancelled = false;

        while let Some(event) = stream.next().await {
            if cancel_token.is_cancelled() {
                was_cancelled = true;
                break;
            }

            match event {
                Ok(goose::agents::AgentEvent::Message(message)) => {
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

        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get_mut(&thread_id) {
            session.cancel_token = None;
        }
        Ok(PromptResponse::new(if was_cancelled {
            StopReason::Cancelled
        } else {
            StopReason::EndTurn
        }))
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
        let internal_id = self.internal_session_id(thread_id).await?;
        let config = self.load_config().map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to read config: {}", e))
        })?;
        let agent = self.get_session_agent(thread_id, None).await?;
        let current_provider = agent.provider().await.map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to get provider: {}", e))
        })?;
        let provider_name = current_provider.get_name().to_string();
        let extensions =
            EnabledExtensionsState::for_session(&self.session_manager, &internal_id, &config).await;
        let model_config = goose::model::ModelConfig::new(model_id)
            .map_err(|e| {
                sacp::Error::invalid_params().data(format!("Invalid model config: {}", e))
            })?
            .with_canonical_limits(&provider_name);
        let provider = self
            .create_provider(&provider_name, model_config, extensions)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to create provider: {}", e))
            })?;

        agent
            .update_provider(provider, &internal_id)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to update provider: {}", e))
            })?;

        let mode = agent.goose_mode().await;
        agent
            .update_goose_mode(mode, &internal_id)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to propagate mode: {}", e))
            })?;

        let model_id = model_id.to_string();
        self.update_thread_metadata(thread_id, move |meta| {
            meta.model_name = Some(model_id);
        })
        .await?;

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
        f: impl FnOnce(&mut goose::session::ThreadMetadata),
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
        let agent = self.get_session_agent(&thread_id.0, None).await?;
        let provider = agent.provider().await.map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to get provider: {}", e))
        })?;
        let goose_mode = agent.goose_mode().await;
        let model_state = build_model_state(&*provider).await?;
        let mode_state = build_mode_state(goose_mode)?;
        let provider_options = build_provider_options(Some(provider.get_name())).await;
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

        let agent = self.get_session_agent(thread_id, None).await?;
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
    ) -> Result<Vec<SessionConfigOption>, sacp::Error> {
        let internal_id = self.internal_session_id(thread_id).await?;
        let config = self.load_config().map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to read config: {}", e))
        })?;
        let agent = self.get_session_agent(thread_id, None).await?;
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
        let model_config = goose::model::ModelConfig::new(model)
            .map_err(|e| {
                sacp::Error::invalid_params().data(format!("Invalid model config: {}", e))
            })?
            .with_canonical_limits(&resolved_provider_name)
            .with_context_limit(context_limit)
            .with_request_params(request_params);
        let extensions =
            EnabledExtensionsState::for_session(&self.session_manager, &internal_id, &config).await;
        let new_provider = self
            .create_provider(&resolved_provider_name, model_config, extensions)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to create provider: {}", e))
            })?;

        agent
            .update_provider(new_provider, &internal_id)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to update provider: {}", e))
            })?;

        let mode = agent.goose_mode().await;
        agent
            .update_goose_mode(mode, &internal_id)
            .await
            .map_err(|e| {
                sacp::Error::internal_error().data(format!("Failed to propagate mode: {}", e))
            })?;

        let provider = agent.provider().await.map_err(|e| {
            sacp::Error::internal_error().data(format!("Failed to get provider: {}", e))
        })?;

        let provider_name_owned = provider_name.to_string();
        self.update_thread_metadata(thread_id, move |meta| {
            meta.provider_id = Some(provider_name_owned);
        })
        .await?;

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

        let (_, config_options) = self
            .build_config_update(&SessionId::new(thread_id.to_string()))
            .await?;
        Ok(config_options)
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
                let mut meta = serde_json::Map::new();
                meta.insert(
                    "messageCount".to_string(),
                    serde_json::Value::Number(t.message_count.into()),
                );
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

        let (agent_tx, agent_rx) =
            tokio::sync::watch::channel::<Option<Result<Arc<Agent>, String>>>(None);

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
        let (model_state, config_options) =
            build_eager_config(&resolved, &mode_state, &goose_session).await;

        self.spawn_agent_setup(
            cx,
            agent_tx,
            AgentSetupRequest {
                session_id: SessionId::new(new_thread_id.clone()),
                goose_session,
                mcp_servers: args.mcp_servers,
                resolved_provider: resolved.ok(),
            },
        );

        let mut meta = serde_json::Map::new();
        meta.insert(
            "messageCount".to_string(),
            serde_json::Value::Number(new_thread.message_count.into()),
        );

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
        let extensions = goose::config::extensions::get_all_extensions();
        let warnings = goose::config::extensions::get_warnings();
        let extensions_json = extensions
            .into_iter()
            .map(|e| serde_json::to_value(&e))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        Ok(GetExtensionsResponse {
            extensions: extensions_json,
            warnings,
        })
    }

    #[custom_method(UpdateProviderRequest)]
    async fn on_update_provider(
        &self,
        req: UpdateProviderRequest,
    ) -> Result<UpdateProviderResponse, sacp::Error> {
        let config_options = self
            .update_provider(
                &req.session_id,
                &req.provider,
                req.model.as_deref(),
                req.context_limit,
                req.request_params,
            )
            .await?;
        let config_options = config_options
            .into_iter()
            .map(|option| serde_json::to_value(&option))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| sacp::Error::internal_error().data(e.to_string()))?;
        Ok(UpdateProviderResponse { config_options })
    }

    #[custom_method(ListProvidersRequest)]
    async fn on_list_providers(
        &self,
        _req: ListProvidersRequest,
    ) -> Result<ListProvidersResponse, sacp::Error> {
        Ok(ListProvidersResponse {
            providers: list_provider_entries(None).await,
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
            Err(goose::config::ConfigError::NotFound(_)) => ReadConfigResponse {
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
                        match req.config_id.0.as_ref() {
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
                        let (notification, config_options) = agent.build_config_update(&session_id).await?;
                        cx.send_notification(notification)?;
                        responder.respond(SetSessionConfigOptionResponse::new(config_options))?;
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
    register_builtin_extensions(goose_mcp::BUILTIN_EXTENSIONS.clone());
    info!("listening on stdio");

    let outgoing = tokio::io::stdout().compat_write();
    let incoming = tokio::io::stdin().compat();

    let server =
        crate::server_factory::AcpServer::new(crate::server_factory::AcpServerFactoryConfig {
            builtins,
            data_dir: Paths::data_dir(),
            config_dir: Paths::config_dir(),
        });
    let agent = server.create_agent().await?;
    serve(agent, incoming, outgoing).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose::conversation::message::{ToolRequest, ToolResponse};
    use goose::providers::errors::ProviderError;
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

    struct MockModelProvider {
        models: Result<Vec<String>, ProviderError>,
    }

    #[async_trait::async_trait]
    impl Provider for MockModelProvider {
        fn get_name(&self) -> &str {
            "mock"
        }

        async fn stream(
            &self,
            _model_config: &goose::model::ModelConfig,
            _session_id: &str,
            _system: &str,
            _messages: &[goose::conversation::message::Message],
            _tools: &[rmcp::model::Tool],
        ) -> Result<goose::providers::base::MessageStream, ProviderError> {
            unimplemented!()
        }

        fn get_model_config(&self) -> goose::model::ModelConfig {
            goose::model::ModelConfig::new_or_fail("unused")
        }

        async fn fetch_recommended_models(&self) -> Result<Vec<String>, ProviderError> {
            self.models.clone()
        }
    }

    #[test_case(
        Ok(vec!["model-a".into(), "model-b".into()])
        => Ok(SessionModelState::new(
            ModelId::new("unused"),
            vec![ModelInfo::new(ModelId::new("model-a"), "model-a"),
                 ModelInfo::new(ModelId::new("model-b"), "model-b")],
        ))
        ; "returns current and available models"
    )]
    #[test_case(
        Ok(vec![])
        => Ok(SessionModelState::new(ModelId::new("unused"), vec![]))
        ; "empty model list"
    )]
    #[test_case(
        Err(ProviderError::ExecutionError("fail".into()))
        => Err(sacp::Error::internal_error().data("Execution error: fail".to_string()))
        ; "fetch error propagates"
    )]
    #[tokio::test]
    async fn test_build_model_state(
        models: Result<Vec<String>, ProviderError>,
    ) -> Result<SessionModelState, sacp::Error> {
        let provider = MockModelProvider { models };
        build_model_state(&provider).await
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
