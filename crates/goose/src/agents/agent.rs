use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use futures::stream::BoxStream;
use futures::{stream, FutureExt, StreamExt, TryStreamExt};
use tracing_futures::Instrument;

use super::container::Container;
use super::final_output_tool::FinalOutputTool;
use super::gen_ai_telemetry;
use super::mcp_client::GooseMcpHostInfo;
use super::tool_confirmation_router::ToolConfirmationRouter;
use super::tool_execution::{
    tool_stream, ToolCallResult, ToolStream, ToolStreamItem, CHAT_MODE_TOOL_SKIPPED_RESPONSE,
    DECLINED_RESPONSE,
};
use crate::action_required_manager::ElicitationOutcome;
use crate::agents::extension::{ExtensionConfig, ExtensionResult, ToolInfo};
use crate::agents::extension_manager::{
    get_parameter_names, ExtensionManager, ExtensionManagerCapabilities,
};
use crate::agents::final_output_tool::{FINAL_OUTPUT_CONTINUATION_MESSAGE, FINAL_OUTPUT_TOOL_NAME};
use crate::agents::platform_extensions::MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE;
use crate::agents::prompt_manager::PromptManager;
use crate::agents::retry::{RetryManager, RetryResult};
use crate::agents::state_machine::{
    run_goose, BangShellOperation, CompactionOperation, DoctorOperation, Emitter,
    EntryHookOperation, ExitOnErrorOperation, GooseEffect, InferenceRunner, MaxTurnsOperation,
    Operation, ProjectOperation, RecipeOperation, RetryOperation, SkillOperation,
    SlashCommandOperation, StateMachine, SteerOperation, SteerQueue, Step, StopHookOperation,
    ToolApprovalOperation, ToolExecutionOperation, ToolPairCompactionOperation,
    UnknownToolOperation, MAX_TURNS_MESSAGE,
};
use crate::agents::types::{
    FrontendTool, SessionConfig, SharedProvider, ToolResultReceiver,
    DEFAULT_ON_FAILURE_TIMEOUT_SECONDS, DEFAULT_RETRY_TIMEOUT_SECONDS,
};
use crate::agents::AgentEvent;
use crate::config::extensions::name_to_key;
use crate::config::permission::PermissionManager;
use crate::config::{get_enabled_extensions, Config, GooseMode};
use crate::context_mgmt::{
    check_if_compaction_needed, compact_messages, DEFAULT_COMPACTION_THRESHOLD,
};
use crate::conversation::message::{
    ActionRequiredData, InferenceMetadata, Message, MessageContent, MessageUsage, ProviderMetadata,
    SystemNotificationType, ToolRequest,
};
use crate::conversation::{
    debug_conversation_fix, fix_conversation, merge_consecutive_messages_for_request, Conversation,
};
use crate::mcp_utils::ToolResult;
use crate::permission::permission_inspector::PermissionInspector;
use crate::permission::permission_judge::PermissionCheckResult;
use crate::permission::PermissionConfirmation;
use crate::providers::base::{PermissionRouting, Provider};
use crate::recipe::{Author, Recipe, Response, Settings};
use crate::scheduler_trait::SchedulerTrait;
use crate::security::adversary_inspector::AdversaryInspector;
use crate::security::egress_inspector::EgressInspector;
use crate::security::security_inspector::SecurityInspector;
use crate::session::extension_data::{EnabledExtensionsState, ExtensionState};
use crate::session::{Session, SessionManager, SessionNameUpdate};
use crate::tool_inspection::ToolInspectionManager;
use crate::tool_monitor::RepetitionInspector;
use crate::utils::is_token_cancelled;
use goose_providers::conversation::token_usage::{ProviderUsage, Usage};
use goose_providers::errors::ProviderError;
use goose_providers::thinking::ThinkingEffort;
use regex::Regex;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ElicitationAction, ErrorCode, ErrorData,
    GetPromptResult, Prompt, Tool,
};
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

const DEFAULT_MAX_TURNS: u32 = 1000;
const DEFAULT_STOP_HOOK_BLOCK_CAP: u32 = 8;
const COMPACTION_PROGRESS_TEXT: &str = "goose is compacting the conversation...";
const MAX_EMPTY_TURN_RETRIES: u32 = 3;
const EMPTY_TURN_MESSAGE: &str =
    "The model returned an empty response. Please resend your message to continue.";
const DEFAULT_FRONTEND_INSTRUCTIONS: &str = "The following tools are provided directly by the frontend and will be executed by the frontend when called.";

fn provider_creation_error(error: anyhow::Error, context: impl fmt::Display) -> anyhow::Error {
    let message = format!("{context}: {error}");
    error.context(message)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolCategory {
    Shell,
    Read,
    Write,
    Other,
}

fn categorize_tool(tool_name: &str) -> ToolCategory {
    let local = tool_name.rsplit("__").next().unwrap_or(tool_name);
    match local {
        "shell" | "bash" | "exec" | "run" => ToolCategory::Shell,
        "read" | "view" | "cat" | "read_file" => ToolCategory::Read,
        "write" | "edit" | "patch" | "write_file" | "edit_file" => ToolCategory::Write,
        _ => ToolCategory::Other,
    }
}

fn extract_string_arg(input: &Value, keys: &[&str]) -> Option<String> {
    let obj = input.as_object()?;
    for k in keys {
        if let Some(s) = obj.get(*k).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

pub(crate) fn stop_hook_denial_context_message(plugin: &str, reason: &str) -> Message {
    let nudge = format!(
        "Stop hook `{plugin}` blocked ending this turn:

{reason}

Address this policy hook denial before trying to stop again."
    );
    Message::user()
        .with_text(nudge)
        .with_visibility(false, true)
}

pub(crate) fn stop_hook_denial_notification(plugin: &str) -> Message {
    Message::assistant().with_system_notification(
        SystemNotificationType::InlineMessage,
        format!("Stop hook `{plugin}` blocked ending this turn."),
    )
}

pub(crate) fn stop_hook_block_cap_warning(plugin: &str, cap: u32) -> Message {
    Message::assistant().with_system_notification(
        SystemNotificationType::InlineMessage,
        format!(
            "Stop hook `{plugin}` blocked the turn from ending more than {cap} consecutive times — overriding and ending turn to avoid an infinite loop. Set GOOSE_STOP_HOOK_BLOCK_CAP to raise this limit."
        ),
    )
}

/// Context needed for the reply function
pub struct ReplyContext {
    pub conversation: Conversation,
    pub tools: Vec<Tool>,
    pub toolshim_tools: Vec<Tool>,
    pub system_prompt: String,
    pub goose_mode: GooseMode,
    pub tool_call_cut_off: usize,
    pub model_config: goose_providers::model::ModelConfig,
}

pub struct ToolCategorizeResult {
    pub frontend_requests: Vec<ToolRequest>,
    pub remaining_requests: Vec<ToolRequest>,
    pub filtered_response: Message,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtensionLoadResult {
    pub name: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub enum GoosePlatform {
    GooseDesktop,
    GooseCli,
}

impl fmt::Display for GoosePlatform {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GoosePlatform::GooseCli => write!(f, "goose-cli"),
            GoosePlatform::GooseDesktop => write!(f, "goose-desktop"),
        }
    }
}

#[derive(Clone)]
pub struct AgentConfig {
    pub session_manager: Arc<SessionManager>,
    pub permission_manager: Arc<PermissionManager>,
    pub scheduler_service: Option<Arc<dyn SchedulerTrait>>,
    pub goose_mode: GooseMode,
    pub disable_session_naming: bool,
    pub goose_platform: GoosePlatform,
    pub mcp_host_info: Option<GooseMcpHostInfo>,
    pub session_name_update_tx: Option<mpsc::UnboundedSender<SessionNameUpdate>>,
    pub use_login_shell_path: Option<bool>,
    pub is_subagent: bool,
}

impl AgentConfig {
    pub fn new(
        session_manager: Arc<SessionManager>,
        permission_manager: Arc<PermissionManager>,
        scheduler_service: Option<Arc<dyn SchedulerTrait>>,
        goose_mode: GooseMode,
        disable_session_naming: bool,
        goose_platform: GoosePlatform,
    ) -> Self {
        Self {
            session_manager,
            permission_manager,
            scheduler_service,
            goose_mode,
            disable_session_naming,
            goose_platform,
            mcp_host_info: None,
            session_name_update_tx: None,
            use_login_shell_path: None,
            is_subagent: false,
        }
    }

    pub fn with_mcp_host_info(mut self, mcp_host_info: Option<GooseMcpHostInfo>) -> Self {
        self.mcp_host_info = mcp_host_info;
        self
    }

    pub fn with_session_name_update_tx(
        mut self,
        tx: Option<mpsc::UnboundedSender<SessionNameUpdate>>,
    ) -> Self {
        self.session_name_update_tx = tx;
        self
    }

    pub fn with_use_login_shell_path(mut self, use_login_shell_path: bool) -> Self {
        self.use_login_shell_path = Some(use_login_shell_path);
        self
    }

    fn resolve_use_login_shell_path(&self) -> bool {
        resolve_use_login_shell_path(self.use_login_shell_path, &self.goose_platform)
    }
}

fn resolve_use_login_shell_path(explicit: Option<bool>, platform: &GoosePlatform) -> bool {
    explicit.unwrap_or(matches!(platform, GoosePlatform::GooseDesktop))
}

fn new_extension_manager(config: &AgentConfig, provider: SharedProvider) -> Arc<ExtensionManager> {
    let explicit_mcp_host_info = config.mcp_host_info.clone();
    let mcpui = explicit_mcp_host_info
        .as_ref()
        .filter(|host_info| host_info.explicit_extensions)
        .map(GooseMcpHostInfo::mcpui_enabled)
        .unwrap_or_else(|| matches!(config.goose_platform, GoosePlatform::GooseDesktop));
    let capabilities = ExtensionManagerCapabilities {
        mcpui,
        host_info: explicit_mcp_host_info.clone(),
    };
    let client_name = explicit_mcp_host_info
        .as_ref()
        .and_then(|host_info| host_info.client_name.clone())
        .unwrap_or_else(|| config.goose_platform.to_string());

    Arc::new(ExtensionManager::new(
        provider,
        Arc::clone(&config.session_manager),
        config.scheduler_service.clone(),
        client_name,
        capabilities,
        config.resolve_use_login_shell_path(),
    ))
}

/// The main goose Agent
pub struct Agent {
    pub(super) provider: SharedProvider,
    pub config: AgentConfig,
    pub(super) current_goose_mode: Mutex<GooseMode>,

    pub extension_manager: Arc<ExtensionManager>,
    pub(super) final_output_tool: Arc<Mutex<Option<FinalOutputTool>>>,
    pub(super) frontend_extensions: Mutex<HashMap<String, ExtensionConfig>>,
    pub(super) frontend_tools: Mutex<HashMap<String, FrontendTool>>,
    pub(super) frontend_instructions: Mutex<Option<String>>,
    pub(super) prompt_manager: Mutex<PromptManager>,
    pub tool_confirmation_router: ToolConfirmationRouter,
    pub(super) tool_result_tx: mpsc::Sender<(String, ToolResult<CallToolResult>)>,
    pub(super) tool_result_rx: ToolResultReceiver,

    pub(super) retry_manager: RetryManager,
    pub(super) tool_inspection_manager: ToolInspectionManager,
    pub(super) hook_manager: crate::hooks::HookManager,
    session_start_emitted: AtomicBool,
    #[cfg(test)]
    pub(super) stop_hook_block_cap_override: Option<u32>,
    container: Mutex<Option<Container>>,
    pub(super) goal: Mutex<Option<String>>,
    pub(super) grind: Mutex<Option<String>>,
    steer_queues: Mutex<HashMap<String, SteerQueue>>,
}

pub(in crate::agents) struct StateMachineResources {
    pub provider: Arc<dyn Provider>,
    pub model_config: goose_providers::model::ModelConfig,
    pub extension_manager: Arc<ExtensionManager>,
    pub context_limit: usize,
}

fn ensure_message_event_id(event: AgentEvent) -> AgentEvent {
    match event {
        AgentEvent::Message(message) => AgentEvent::Message(message.with_generated_id_if_missing()),
        other => other,
    }
}

fn push_message_with_id(messages: &mut Conversation, message: Message) -> Message {
    let message = message.with_generated_id_if_missing();
    messages.push(message.clone());
    message
}

async fn persist_message_with_id(
    session_manager: &SessionManager,
    session_id: &str,
    message: Message,
) -> Result<Message> {
    let message = message.with_generated_id_if_missing();
    session_manager.add_message(session_id, &message).await?;
    Ok(message)
}

async fn persist_and_push_message_with_id(
    session_manager: &SessionManager,
    session_id: &str,
    conversation: &mut Conversation,
    message: Message,
) -> Result<Message> {
    let message = persist_message_with_id(session_manager, session_id, message).await?;
    conversation.push(message.clone());
    Ok(message)
}

fn project_message_for_user_event(message: &Message) -> Message {
    message.user_visible_content()
}

fn agent_visible_message_text(message: &Message) -> String {
    message.agent_visible_content().as_concat_text()
}

fn attach_turn_usage(
    messages: &mut Conversation,
    usage: &ProviderUsage,
    preferred_message_id: Option<&str>,
) -> Option<(Option<String>, MessageUsage)> {
    let message_index = preferred_message_id
        .and_then(|preferred_message_id| {
            messages.messages().iter().rposition(|message| {
                message.role == rmcp::model::Role::Assistant
                    && message.id.as_deref() == Some(preferred_message_id)
            })
        })
        .or_else(|| {
            messages
                .messages()
                .iter()
                .rposition(|message| message.role == rmcp::model::Role::Assistant)
        })?;
    let message = &mut messages.messages_mut()[message_index];
    let has_user_visible_content = !message.user_visible_content().content.is_empty();
    let message_usage = MessageUsage::from_provider_usage(usage, false);
    message.metadata.usage = Some(Box::new(message_usage.clone()));
    has_user_visible_content.then(|| (message.id.clone(), message_usage))
}

impl Default for Agent {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent {
    pub fn new() -> Self {
        let config = Config::global();
        Self::with_config(AgentConfig::new(
            Arc::new(SessionManager::instance()),
            PermissionManager::instance(),
            None,
            config.get_goose_mode().unwrap_or_default(),
            config.get_goose_disable_session_naming().unwrap_or(false),
            GoosePlatform::GooseCli,
        ))
    }

    pub fn with_config(config: AgentConfig) -> Self {
        let (tool_tx, tool_rx) = mpsc::channel(32);
        let provider = Arc::new(Mutex::new(None));
        let initial_mode = config.goose_mode;
        let extension_manager = new_extension_manager(&config, provider.clone());
        let inspection_session_manager = Arc::clone(&config.session_manager);
        let permission_manager = Arc::clone(&config.permission_manager);
        let use_login_shell_path = config.resolve_use_login_shell_path();
        let is_subagent = config.is_subagent;
        Self {
            provider: provider.clone(),
            config,
            current_goose_mode: Mutex::new(initial_mode),
            extension_manager,
            final_output_tool: Arc::new(Mutex::new(None)),
            frontend_extensions: Mutex::new(HashMap::new()),
            frontend_tools: Mutex::new(HashMap::new()),
            frontend_instructions: Mutex::new(None),
            prompt_manager: Mutex::new(PromptManager::new()),
            tool_confirmation_router: ToolConfirmationRouter::new(),
            tool_result_tx: tool_tx,
            tool_result_rx: Arc::new(Mutex::new(tool_rx)),
            retry_manager: RetryManager::new(),
            tool_inspection_manager: Self::create_tool_inspection_manager(
                permission_manager,
                provider.clone(),
                inspection_session_manager,
            ),
            hook_manager: if is_subagent {
                crate::hooks::HookManager::default()
            } else {
                crate::hooks::HookManager::load(
                    std::env::current_dir().ok().as_deref(),
                    use_login_shell_path,
                )
            },
            session_start_emitted: AtomicBool::new(false),
            #[cfg(test)]
            stop_hook_block_cap_override: None,
            container: Mutex::new(None),
            goal: Mutex::new(None),
            grind: Mutex::new(None),
            steer_queues: Mutex::new(HashMap::new()),
        }
    }

    /// Emit a lifecycle hook event with no extra context. Useful for events
    /// that have no matcher (e.g. `SessionStart`, `SessionEnd`).
    #[cfg(test)]
    pub(crate) fn set_hook_manager_for_test(&mut self, hook_manager: crate::hooks::HookManager) {
        self.hook_manager = hook_manager;
    }

    #[cfg(test)]
    pub(crate) fn set_stop_hook_block_cap_for_test(&mut self, cap: u32) {
        self.stop_hook_block_cap_override = Some(cap);
    }

    pub(crate) fn stop_hook_block_cap(&self) -> u32 {
        #[cfg(test)]
        if let Some(cap) = self.stop_hook_block_cap_override {
            return cap;
        }

        Config::global()
            .get_param::<u32>("GOOSE_STOP_HOOK_BLOCK_CAP")
            .unwrap_or(DEFAULT_STOP_HOOK_BLOCK_CAP)
    }

    pub async fn emit_hook(&self, event: crate::hooks::HookEvent, session_id: &str) {
        if !self.hook_manager.has_hooks(event) {
            return;
        }
        self.hook_manager
            .emit(event, crate::hooks::HookContext::new(event, session_id))
            .await;
    }

    pub async fn emit_hook_with_banners(
        &self,
        event: crate::hooks::HookEvent,
        session_id: &str,
    ) -> Vec<String> {
        if event == crate::hooks::HookEvent::SessionStart {
            self.session_start_emitted.store(true, Ordering::Release);
        }
        if !self.hook_manager.has_hooks(event) {
            return Vec::new();
        }
        self.hook_manager
            .emit_collecting_banners(event, crate::hooks::HookContext::new(event, session_id))
            .await
    }

    fn stop_hook_context(
        session_id: &str,
        last_assistant_message: &str,
        working_dir: &str,
    ) -> crate::hooks::HookContext {
        crate::hooks::HookContext::new(crate::hooks::HookEvent::Stop, session_id)
            .with_last_assistant_message(last_assistant_message.to_string())
            .with_working_dir(working_dir.to_string())
    }

    pub(crate) async fn emit_stop_hook(
        &self,
        session_id: &str,
        last_assistant_message: &str,
        working_dir: &str,
    ) {
        if !self.hook_manager.has_hooks(crate::hooks::HookEvent::Stop) {
            return;
        }
        self.hook_manager
            .emit(
                crate::hooks::HookEvent::Stop,
                Self::stop_hook_context(session_id, last_assistant_message, working_dir),
            )
            .await;
    }

    pub(crate) async fn emit_stop_hook_blocking(
        &self,
        session_id: &str,
        last_assistant_message: &str,
        working_dir: &str,
    ) -> crate::hooks::HookDecision {
        self.hook_manager
            .emit_blocking(
                crate::hooks::HookEvent::Stop,
                Self::stop_hook_context(session_id, last_assistant_message, working_dir),
            )
            .await
    }

    pub async fn steer(&self, session_id: &str, message: Message) {
        self.steer_queue(session_id)
            .await
            .lock()
            .await
            .push_back(message);
    }

    pub async fn discard_pending_steers(&self, session_id: &str) {
        self.steer_queues.lock().await.remove(session_id);
    }

    pub(crate) async fn has_pending_steers(&self, session_id: &str) -> bool {
        let queue = self.steer_queues.lock().await.get(session_id).cloned();
        match queue {
            Some(queue) => !queue.lock().await.is_empty(),
            None => false,
        }
    }

    pub(crate) async fn drain_pending_steers(&self, session_id: &str) -> Vec<Message> {
        let queue = self.steer_queues.lock().await.get(session_id).cloned();
        match queue {
            Some(queue) => queue
                .lock()
                .await
                .drain(..)
                .map(Message::with_steer)
                .collect(),
            None => Vec::new(),
        }
    }

    pub(super) async fn steer_queue(&self, session_id: &str) -> SteerQueue {
        self.steer_queues
            .lock()
            .await
            .entry(session_id.to_string())
            .or_default()
            .clone()
    }

    async fn emit_pre_tool_extended_hooks(
        &self,
        tool_name: &str,
        tool_input: Option<&Value>,
        session: &Session,
    ) {
        let working_dir = session.working_dir.to_string_lossy().to_string();
        match categorize_tool(tool_name) {
            ToolCategory::Shell => {
                if let Some(cmd) = tool_input.and_then(|v| extract_string_arg(v, &["command"])) {
                    self.emit_with_matcher(
                        crate::hooks::HookEvent::BeforeShellExecution,
                        &session.id,
                        &cmd,
                        tool_name,
                        tool_input.cloned(),
                        &working_dir,
                    )
                    .await;
                }
            }
            ToolCategory::Read => {
                if let Some(path) =
                    tool_input.and_then(|v| extract_string_arg(v, &["path", "file", "file_path"]))
                {
                    self.emit_with_matcher(
                        crate::hooks::HookEvent::BeforeReadFile,
                        &session.id,
                        &path,
                        tool_name,
                        tool_input.cloned(),
                        &working_dir,
                    )
                    .await;
                }
            }
            ToolCategory::Write | ToolCategory::Other => {}
        }
    }

    async fn emit_with_matcher(
        &self,
        event: crate::hooks::HookEvent,
        session_id: &str,
        matcher_context: &str,
        tool_name: &str,
        tool_input: Option<Value>,
        working_dir: &str,
    ) {
        if !self.hook_manager.has_hooks(event) {
            return;
        }
        let mut ctx = crate::hooks::HookContext::new(event, session_id)
            .with_tool(tool_name.to_string(), tool_input)
            .with_working_dir(working_dir.to_string());
        ctx.matcher_context = Some(matcher_context.to_string());
        self.hook_manager.emit(event, ctx).await;
    }

    fn with_post_tool_hook(
        &self,
        result: ToolCallResult,
        tool_call: &CallToolRequestParams,
        session: &Session,
    ) -> ToolCallResult {
        let hook_manager = self.hook_manager.clone();
        let session_id = session.id.clone();
        let working_dir = session.working_dir.to_string_lossy().to_string();
        let tool_name = tool_call.name.to_string();
        let tool_input = tool_call
            .arguments
            .as_ref()
            .map(|a| serde_json::Value::Object(a.clone()));
        let category = categorize_tool(&tool_name);
        let span = tracing::Span::current();
        let capture_message_content = gen_ai_telemetry::capture_message_content();

        let fut = async move {
            let processed_result =
                super::large_response_handler::process_tool_response(result.result.await);
            if capture_message_content {
                let output = gen_ai_telemetry::tool_result_json(&processed_result);
                span.record("output", output.as_str());
                if let Some(result) =
                    gen_ai_telemetry::successful_tool_result_json(&processed_result)
                {
                    span.record("gen_ai.tool.call.result", result.as_str());
                }
            }
            let event = match &processed_result {
                Ok(call_result) if call_result.is_error != Some(true) => {
                    crate::hooks::HookEvent::PostToolUse
                }
                _ => crate::hooks::HookEvent::PostToolUseFailure,
            };

            if hook_manager.has_hooks(event) {
                let ctx = crate::hooks::HookContext::new(event, &session_id)
                    .with_tool(tool_name.clone(), tool_input.clone())
                    .with_working_dir(working_dir.clone());
                hook_manager.emit(event, ctx).await;
            }

            if event == crate::hooks::HookEvent::PostToolUse {
                let extended = match category {
                    ToolCategory::Shell => Some((
                        crate::hooks::HookEvent::AfterShellExecution,
                        tool_input
                            .as_ref()
                            .and_then(|v| extract_string_arg(v, &["command"])),
                    )),
                    ToolCategory::Write => Some((
                        crate::hooks::HookEvent::AfterFileEdit,
                        tool_input
                            .as_ref()
                            .and_then(|v| extract_string_arg(v, &["path", "file", "file_path"])),
                    )),
                    _ => None,
                };
                if let Some((ext_event, Some(matcher))) = extended {
                    if hook_manager.has_hooks(ext_event) {
                        let mut ctx = crate::hooks::HookContext::new(ext_event, &session_id)
                            .with_tool(tool_name, tool_input)
                            .with_working_dir(working_dir);
                        ctx.matcher_context = Some(matcher);
                        hook_manager.emit(ext_event, ctx).await;
                    }
                }
            }

            processed_result
        };

        ToolCallResult {
            notification_stream: result.notification_stream,
            action_required_stream: result.action_required_stream,
            result: Box::new(fut.boxed()),
        }
    }

    /// Create a tool inspection manager with default inspectors
    fn create_tool_inspection_manager(
        permission_manager: Arc<PermissionManager>,
        provider: SharedProvider,
        session_manager: Arc<SessionManager>,
    ) -> ToolInspectionManager {
        let mut tool_inspection_manager = ToolInspectionManager::new();

        // Add security inspector (highest priority - runs first)
        tool_inspection_manager.add_inspector(Box::new(SecurityInspector::new()));
        tool_inspection_manager.add_inspector(Box::new(EgressInspector::new()));

        // Add adversary inspector (LLM-based review, enabled by ~/.config/goose/adversary.md)
        tool_inspection_manager.add_inspector(Box::new(AdversaryInspector::new(
            provider.clone(),
            session_manager.clone(),
        )));

        // Add permission inspector (medium-high priority)
        tool_inspection_manager.add_inspector(Box::new(PermissionInspector::new(
            permission_manager,
            provider,
            session_manager,
        )));

        // Add repetition inspector (lower priority - basic repetition checking)
        tool_inspection_manager.add_inspector(Box::new(RepetitionInspector::new(None)));

        tool_inspection_manager
    }

    /// Reset the retry attempts counter to 0
    pub async fn reset_retry_attempts(&self) {
        self.retry_manager.reset_attempts().await;
    }

    /// Increment the retry attempts counter and return the new value
    pub async fn increment_retry_attempts(&self) -> u32 {
        self.retry_manager.increment_attempts().await
    }

    /// Get the current retry attempts count
    pub async fn get_retry_attempts(&self) -> u32 {
        self.retry_manager.get_attempts().await
    }

    async fn handle_retry_logic(
        &self,
        messages: &mut Conversation,
        session_config: &SessionConfig,
        initial_messages: &[Message],
    ) -> Result<RetryResult> {
        let result = self
            .retry_manager
            .handle_retry_logic(messages, session_config, initial_messages)
            .await?;
        if matches!(result, RetryResult::Retried) {
            if let Some(tool) = self.final_output_tool.lock().await.as_mut() {
                tool.final_output = None;
            }
        }
        Ok(result)
    }
    async fn load_project_instructions(&self, session: &Session) -> Option<String> {
        let project_id = session.project_id.as_deref()?;
        let entry = crate::sources::read_project(project_id).ok()?;
        let mut parts = Vec::new();
        parts.push(format!("# Project: {}", entry.name));
        if !entry.description.is_empty() {
            parts.push(entry.description.clone());
        }
        if !entry.content.is_empty() {
            parts.push(entry.content.clone());
        }
        Some(parts.join("\n\n"))
    }

    async fn prepare_reply_context(
        &self,
        session_id: &str,
        unfixed_conversation: Conversation,
        working_dir: &std::path::Path,
    ) -> Result<ReplyContext> {
        let unfixed_messages = unfixed_conversation.messages().clone();
        let (conversation, issues) = fix_conversation(unfixed_conversation.clone());
        if !issues.is_empty() {
            debug!(
                "Conversation issue fixed: {}",
                debug_conversation_fix(
                    unfixed_messages.as_slice(),
                    conversation.messages(),
                    &issues
                )
            );
        }
        let (tools, toolshim_tools, system_prompt, model_config) = self
            .prepare_tools_and_prompt(session_id, working_dir)
            .await?;

        let goose_mode = *self.current_goose_mode.lock().await;

        let tool_call_cut_off = match Config::global().get_param::<usize>("GOOSE_TOOL_CALL_CUTOFF")
        {
            Ok(v) => v,
            Err(_) => {
                let context_limit = match self.provider().await {
                    Ok(provider) => provider
                        .get_context_limit(&model_config)
                        .await
                        .unwrap_or_else(|_| model_config.context_limit()),
                    Err(_) => goose_providers::model::DEFAULT_CONTEXT_LIMIT,
                };
                let compaction_threshold = Config::global()
                    .get_param::<f64>("GOOSE_AUTO_COMPACT_THRESHOLD")
                    .unwrap_or(crate::context_mgmt::DEFAULT_COMPACTION_THRESHOLD);
                crate::context_mgmt::compute_tool_call_cutoff(context_limit, compaction_threshold)
            }
        };

        Ok(ReplyContext {
            conversation,
            tools,
            toolshim_tools,
            system_prompt,
            goose_mode,
            tool_call_cut_off,
            model_config,
        })
    }

    async fn categorize_tools(
        &self,
        response: &Message,
        tools: &[rmcp::model::Tool],
        suppress_replayed_thinking: bool,
    ) -> ToolCategorizeResult {
        // Categorize tool requests
        let (frontend_requests, remaining_requests, filtered_response) = self
            .categorize_tool_requests(response, tools, suppress_replayed_thinking)
            .await;

        ToolCategorizeResult {
            frontend_requests,
            remaining_requests,
            filtered_response,
        }
    }

    async fn handle_approved_and_denied_tools(
        &self,
        permission_check_result: &PermissionCheckResult,
        request_to_response_map: &mut HashMap<String, Message>,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
        session: &Session,
    ) -> Result<Vec<(String, ToolStream)>> {
        let mut tool_futures: Vec<(String, ToolStream)> = Vec::new();

        // Handle pre-approved and read-only tools
        for request in &permission_check_result.approved {
            if let Ok(tool_call) = request.tool_call.clone() {
                let (req_id, tool_result) = self
                    .dispatch_tool_call(
                        tool_call,
                        request.id.clone(),
                        cancel_token.clone(),
                        session,
                    )
                    .await;

                tool_futures.push((
                    req_id,
                    match tool_result {
                        Ok(result) => tool_stream(
                            result
                                .notification_stream
                                .unwrap_or_else(|| Box::new(stream::empty())),
                            result
                                .action_required_stream
                                .unwrap_or_else(|| Box::new(stream::empty())),
                            result.result,
                        ),
                        Err(e) => tool_stream(
                            Box::new(stream::empty()),
                            Box::new(stream::empty()),
                            futures::future::ready(Err(e)),
                        ),
                    },
                ));
            }
        }

        Self::handle_denied_tools(permission_check_result, request_to_response_map);
        Ok(tool_futures)
    }

    fn handle_denied_tools(
        permission_check_result: &PermissionCheckResult,
        request_to_response_map: &mut HashMap<String, Message>,
    ) {
        for request in &permission_check_result.denied {
            if let Some(response) = request_to_response_map.get_mut(&request.id) {
                response.add_tool_response_with_metadata(
                    request.id.clone(),
                    Ok(CallToolResult::error(vec![
                        rmcp::model::ContentBlock::text(DECLINED_RESPONSE),
                    ])),
                    request.metadata.as_ref(),
                );
            }
        }
    }

    /// Get a reference count clone to the provider
    pub async fn provider(&self) -> Result<Arc<dyn Provider>, anyhow::Error> {
        match &*self.provider.lock().await {
            Some(provider) => Ok(Arc::clone(provider)),
            None => Err(anyhow!("Provider not set")),
        }
    }

    /// Resolve the active model config for a session.
    ///
    /// The session is the source of truth for the selected model and its
    /// settings. When the session has no stored config (e.g. before the
    /// provider has been persisted), fall back to the configured provider
    /// defaults.
    pub async fn model_config_for_session(
        &self,
        session_id: &str,
    ) -> Result<goose_providers::model::ModelConfig> {
        if let Ok(session) = self
            .config
            .session_manager
            .get_session(session_id, false)
            .await
        {
            if let Some(model_config) = session.model_config {
                return Ok(model_config);
            }
        }

        let config = Config::global();
        let provider_name = config
            .get_goose_provider()
            .map_err(|_| anyhow!("Could not resolve model config: missing provider"))?;
        let model_name = config
            .get_goose_model()
            .map_err(|_| anyhow!("Could not resolve model config: missing model"))?;
        crate::model_config::model_config_from_user_config(&provider_name, &model_name)
            .map_err(|e| anyhow!("Could not resolve model config: {e}"))
    }

    /// When set, all stdio extensions will be started via `docker exec` in the specified container.
    pub async fn set_container(&self, container: Option<Container>) {
        *self.container.lock().await = container.clone();
    }

    pub async fn container(&self) -> Option<Container> {
        self.container.lock().await.clone()
    }

    /// Check if a tool is a frontend tool
    pub async fn is_frontend_tool(&self, name: &str) -> bool {
        self.frontend_tools.lock().await.contains_key(name)
    }

    /// Get a reference to a frontend tool
    pub async fn get_frontend_tool(&self, name: &str) -> Option<FrontendTool> {
        self.frontend_tools.lock().await.get(name).cloned()
    }

    async fn frontend_extension_configs(&self) -> Vec<ExtensionConfig> {
        let mut configs = self
            .frontend_extensions
            .lock()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        configs.sort_by_key(|config| config.key());
        configs
    }

    async fn frontend_tools_for_extension(&self, extension_name: Option<&str>) -> Vec<Tool> {
        let requested_extension = extension_name.map(name_to_key);

        self.frontend_extension_configs()
            .await
            .into_iter()
            .filter_map(|config| {
                let include = requested_extension
                    .as_ref()
                    .is_none_or(|name| *name == config.key());

                match config {
                    ExtensionConfig::Frontend { tools, .. } if include => Some(tools),
                    _ => None,
                }
            })
            .flatten()
            .collect()
    }

    async fn rebuild_frontend_derived_state(&self, extensions: &HashMap<String, ExtensionConfig>) {
        let multiple = extensions.len() > 1;
        let mut tools = HashMap::new();
        let mut instructions = Vec::new();

        for config in extensions.values() {
            if let ExtensionConfig::Frontend {
                name,
                tools: ext_tools,
                instructions: ext_instructions,
                ..
            } = config
            {
                for tool in ext_tools {
                    let tool_name = tool.name.to_string();
                    tools.insert(
                        tool_name.clone(),
                        FrontendTool {
                            name: tool_name,
                            tool: tool.clone(),
                        },
                    );
                }

                let text = ext_instructions
                    .clone()
                    .unwrap_or_else(|| DEFAULT_FRONTEND_INSTRUCTIONS.to_string());
                instructions.push(if multiple {
                    format!("{name}: {text}")
                } else {
                    text
                });
            }
        }

        *self.frontend_tools.lock().await = tools;
        *self.frontend_instructions.lock().await = if instructions.is_empty() {
            None
        } else {
            Some(instructions.join("\n\n"))
        };
    }

    async fn insert_frontend_extension(&self, extension: ExtensionConfig) {
        let mut extensions = self.frontend_extensions.lock().await;
        extensions.insert(extension.key(), extension);
        self.rebuild_frontend_derived_state(&extensions).await;
    }

    async fn remove_frontend_extension(&self, name: &str) {
        let mut extensions = self.frontend_extensions.lock().await;
        extensions.remove(&name_to_key(name));
        self.rebuild_frontend_derived_state(&extensions).await;
    }

    async fn extension_configs_for_persistence(&self) -> Vec<ExtensionConfig> {
        let mut extension_configs = self.extension_manager.get_extension_configs().await;
        extension_configs.extend(self.frontend_extension_configs().await);
        extension_configs
    }

    pub async fn add_final_output_tool(&self, response: Response) {
        let mut final_output_tool = self.final_output_tool.lock().await;
        let created_final_output_tool = FinalOutputTool::new(response);
        let final_output_system_prompt = created_final_output_tool.system_prompt();
        *final_output_tool = Some(created_final_output_tool);
        self.extend_system_prompt("final_output".to_string(), final_output_system_prompt)
            .await;
    }

    pub async fn apply_recipe_components(
        &self,
        response: Option<Response>,
        include_final_output: bool,
    ) {
        if include_final_output {
            if let Some(response) = response {
                self.add_final_output_tool(response).await;
            }
        }
    }

    /// Dispatch a single tool call to the appropriate client
    #[instrument(
        skip(self, tool_call, request_id, cancellation_token, session),
        fields(
            input,
            output,
            session.id = %session.id,
            gen_ai.conversation.id = %session.id,
            gen_ai.operation.name = "execute_tool",
            gen_ai.tool.name = %tool_call.name,
            gen_ai.tool.call.id = %request_id,
            gen_ai.tool.call.arguments = tracing::field::Empty,
            gen_ai.tool.call.result = tracing::field::Empty,
        )
    )]
    pub async fn dispatch_tool_call(
        &self,
        tool_call: CallToolRequestParams,
        request_id: String,
        cancellation_token: Option<CancellationToken>,
        session: &Session,
    ) -> (String, Result<ToolCallResult, ErrorData>) {
        let input_summary = serde_json::json!({
            "tool": tool_call.name,
            "arguments": tool_call.arguments,
        });
        tracing::Span::current().record("input", tracing::field::display(&input_summary));
        if gen_ai_telemetry::capture_message_content() {
            let arguments = tool_call
                .arguments
                .as_ref()
                .map(|arguments| Value::Object(arguments.clone()))
                .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
            tracing::Span::current().record(
                "gen_ai.tool.call.arguments",
                tracing::field::display(arguments),
            );
        }

        self.prompt_manager
            .lock()
            .await
            .record_tool_arguments(&tool_call.arguments, &session.working_dir);

        if self
            .hook_manager
            .has_hooks(crate::hooks::HookEvent::PreToolUse)
        {
            let ctx =
                crate::hooks::HookContext::new(crate::hooks::HookEvent::PreToolUse, &session.id)
                    .with_tool(
                        tool_call.name.to_string(),
                        tool_call
                            .arguments
                            .as_ref()
                            .map(|a| serde_json::Value::Object(a.clone())),
                    )
                    .with_working_dir(session.working_dir.to_string_lossy().to_string());
            if let crate::hooks::HookDecision::Deny { reason, plugin } = self
                .hook_manager
                .emit_blocking(crate::hooks::HookEvent::PreToolUse, ctx)
                .await
            {
                return (
                    request_id,
                    Err(ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!(
                            "Tool call denied by policy hook `{plugin}`: {reason}. \
                             Do not retry; this is a policy denial, not a transient failure."
                        ),
                        None,
                    )),
                );
            }
        }

        let tool_input_for_extended = tool_call
            .arguments
            .as_ref()
            .map(|a| serde_json::Value::Object(a.clone()));
        self.emit_pre_tool_extended_hooks(
            &tool_call.name,
            tool_input_for_extended.as_ref(),
            session,
        )
        .await;

        if tool_call.name == FINAL_OUTPUT_TOOL_NAME {
            return if let Some(final_output_tool) = self.final_output_tool.lock().await.as_mut() {
                let result = final_output_tool.execute_tool_call(tool_call.clone()).await;
                (
                    request_id,
                    Ok(self.with_post_tool_hook(result, &tool_call, session)),
                )
            } else {
                (
                    request_id,
                    Err(ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        "Final output tool not defined".to_string(),
                        None,
                    )),
                )
            };
        }

        let ctx = super::tool_execution::ToolCallContext::new(
            session.id.clone(),
            Some(session.working_dir.clone()),
            Some(request_id.clone()),
        );

        debug!("WAITING_TOOL_START: {}", tool_call.name);
        let result: ToolCallResult = if self.is_frontend_tool(&tool_call.name).await {
            ToolCallResult::from(Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                "Frontend tool execution required".to_string(),
                None,
            )))
        } else {
            let result = self
                .extension_manager
                .dispatch_tool_call(
                    &ctx,
                    tool_call.clone(),
                    cancellation_token.unwrap_or_default(),
                )
                .await;
            result.unwrap_or_else(|error_data| {
                #[cfg(feature = "telemetry")]
                crate::posthog::emit_error(
                    "tool_execution_failed",
                    &format!("{}: {}", tool_call.name, error_data),
                );
                ToolCallResult::from(Err(error_data))
            })
        };

        debug!("WAITING_TOOL_END: {}", tool_call.name);

        (
            request_id,
            Ok(self.with_post_tool_hook(result, &tool_call, session)),
        )
    }

    /// Save current extension state to session metadata
    /// Should be called after any extension add/remove operation
    pub async fn save_extension_state(&self, session: &SessionConfig) -> Result<()> {
        let extensions_state =
            EnabledExtensionsState::new(self.extension_configs_for_persistence().await);

        let session_manager = self.config.session_manager.clone();
        let mut session_data = session_manager.get_session(&session.id, false).await?;

        if let Err(e) = extensions_state.to_extension_data(&mut session_data.extension_data) {
            warn!("Failed to serialize extension state: {}", e);
            return Err(anyhow!("Extension state serialization failed: {}", e));
        }

        session_manager
            .update(&session.id)
            .extension_data(session_data.extension_data)
            .apply()
            .await?;

        Ok(())
    }

    /// Save current extension state to session by session_id
    pub async fn persist_extension_state(&self, session_id: &str) -> Result<()> {
        let extensions_state =
            EnabledExtensionsState::new(self.extension_configs_for_persistence().await);

        let session_manager = self.config.session_manager.clone();
        let session = session_manager.get_session(session_id, false).await?;
        let mut extension_data = session.extension_data.clone();

        extensions_state
            .to_extension_data(&mut extension_data)
            .map_err(|e| anyhow!("Failed to serialize extension state: {}", e))?;

        session_manager
            .update(session_id)
            .extension_data(extension_data)
            .apply()
            .await?;

        Ok(())
    }

    /// Load extensions from session into the agent
    /// Skips extensions that are already loaded
    /// Uses the session's working_dir for extension initialization
    pub async fn load_extensions_from_session(
        self: &Arc<Self>,
        session: &Session,
    ) -> Vec<ExtensionLoadResult> {
        let session_extensions =
            EnabledExtensionsState::from_extension_data(&session.extension_data);
        let enabled_configs = match session_extensions {
            Some(state) => state.extensions,
            None => {
                tracing::warn!(
                    "No extensions found in session {}. This is unexpected.",
                    session.id
                );
                return vec![];
            }
        };

        let manages_own_context = self
            .provider()
            .await
            .map(|p| p.manages_own_context())
            .unwrap_or(false);
        let (skipped_configs, enabled_configs): (Vec<_>, Vec<_>) =
            enabled_configs.into_iter().partition(|config| {
                manages_own_context
                    && matches!(
                        config,
                        ExtensionConfig::Stdio { .. } | ExtensionConfig::StreamableHttp { .. }
                    )
            });

        let session_id = session.id.clone();

        let extension_futures = enabled_configs
            .into_iter()
            .map(|config| {
                let config_clone = config.clone();
                let agent_ref = self.clone();
                let session_id_clone = session_id.clone();

                async move {
                    let name = config_clone.name().to_string();

                    if agent_ref
                        .extension_manager
                        .is_extension_enabled(&name)
                        .await
                    {
                        tracing::debug!("Extension {} already loaded, skipping", name);
                        return ExtensionLoadResult {
                            name,
                            success: true,
                            error: None,
                        };
                    }

                    match agent_ref
                        .add_extension_inner(config_clone, &session_id_clone)
                        .await
                    {
                        Ok(_) => ExtensionLoadResult {
                            name,
                            success: true,
                            error: None,
                        },
                        Err(e) => {
                            let error_msg = e.to_string();
                            warn!("Failed to load extension {}: {}", name, error_msg);
                            ExtensionLoadResult {
                                name,
                                success: false,
                                error: Some(error_msg),
                            }
                        }
                    }
                }
            })
            .collect::<Vec<_>>();

        let results = futures::future::join_all(extension_futures).await;

        if results.iter().any(|r| r.success) && skipped_configs.is_empty() {
            if let Err(e) = self.persist_extension_state(&session_id).await {
                warn!("Failed to persist extension state after bulk load: {}", e);
            }
        }

        results
    }

    pub async fn add_extension(
        &self,
        extension: ExtensionConfig,
        session_id: &str,
    ) -> ExtensionResult<()> {
        self.add_extension_inner(extension, session_id).await?;

        // Persist extension state after successful add
        self.persist_extension_state(session_id)
            .await
            .map_err(|e| {
                error!("Failed to persist extension state: {}", e);
                crate::agents::extension::ExtensionError::SetupError(format!(
                    "Failed to persist extension state: {}",
                    e
                ))
            })?;

        Ok(())
    }

    /// Load multiple extensions in parallel, persisting state once at the end.
    ///
    /// Unlike `add_extension`, this avoids per-extension persistence and acquires
    /// the container lock once upfront to prevent serialisation of the parallel futures.
    pub async fn add_extensions_bulk(
        self: &Arc<Self>,
        extensions: Vec<ExtensionConfig>,
        session_id: &str,
    ) -> anyhow::Result<Vec<ExtensionLoadResult>> {
        let working_dir = match self
            .config
            .session_manager
            .get_session(session_id, false)
            .await
        {
            Ok(session) => Some(session.working_dir),
            Err(e) => {
                warn!("Failed to get session for bulk load: {}", e);
                None
            }
        };
        let container = self.container.lock().await.clone();

        let extension_futures = extensions
            .into_iter()
            .map(|config| {
                let ext_manager = Arc::clone(&self.extension_manager);
                let working_dir = working_dir.clone();
                let container = container.clone();
                let sid = session_id.to_string();

                async move {
                    let name = config.name().to_string();
                    match ext_manager
                        .add_extension(config, working_dir, container.as_ref(), Some(&sid))
                        .await
                    {
                        Ok(_) => ExtensionLoadResult {
                            name,
                            success: true,
                            error: None,
                        },
                        Err(e) => {
                            let error_msg = e.to_string();
                            warn!("Failed to load extension {}: {}", name, error_msg);
                            ExtensionLoadResult {
                                name,
                                success: false,
                                error: Some(error_msg),
                            }
                        }
                    }
                }
            })
            .collect::<Vec<_>>();

        let results = futures::future::join_all(extension_futures).await;

        if results.iter().any(|r| r.success) {
            self.persist_extension_state(session_id).await?;
        }

        Ok(results)
    }

    async fn add_extension_inner(
        &self,
        extension: ExtensionConfig,
        session_id: &str,
    ) -> ExtensionResult<()> {
        let session = self
            .config
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|e| {
                crate::agents::extension::ExtensionError::SetupError(format!(
                    "Failed to get session '{}': {}",
                    session_id, e
                ))
            })?;
        let working_dir = Some(session.working_dir);

        match &extension {
            ExtensionConfig::Frontend { .. } => {
                self.insert_frontend_extension(extension.clone()).await;
            }
            _ => {
                let container = self.container.lock().await;
                self.extension_manager
                    .add_extension(
                        extension.clone(),
                        working_dir,
                        container.as_ref(),
                        Some(session_id),
                    )
                    .await?;
            }
        }

        Ok(())
    }

    pub async fn list_tools(&self, session_id: &str, extension_name: Option<String>) -> Vec<Tool> {
        let mut prefixed_tools = self
            .extension_manager
            .get_prefixed_tools(session_id, extension_name.clone())
            .await
            .unwrap_or_default();

        prefixed_tools.extend(
            self.frontend_tools_for_extension(extension_name.as_deref())
                .await,
        );

        if extension_name.is_none() {
            if let Some(final_output_tool) = self.final_output_tool.lock().await.as_ref() {
                prefixed_tools.push(final_output_tool.tool());
            }
        }

        prefixed_tools
    }

    pub async fn remove_extension(&self, name: &str, session_id: &str) -> Result<()> {
        self.extension_manager.remove_extension(name).await?;
        self.remove_frontend_extension(name).await;

        // Persist extension state after successful removal
        self.persist_extension_state(session_id)
            .await
            .map_err(|e| {
                error!("Failed to persist extension state: {}", e);
                anyhow!("Failed to persist extension state: {}", e)
            })?;

        Ok(())
    }

    pub async fn list_extensions(&self) -> Vec<String> {
        let mut extensions = self
            .extension_manager
            .list_extensions()
            .await
            .expect("Failed to list extensions");
        extensions.extend(
            self.frontend_extension_configs()
                .await
                .into_iter()
                .map(|config| config.name()),
        );
        extensions
    }

    pub async fn get_extension_configs(&self) -> Vec<ExtensionConfig> {
        self.extension_configs_for_persistence().await
    }

    pub(in crate::agents) async fn extension_manager_for_session(
        &self,
        provider: Arc<dyn Provider>,
        extension_configs: Vec<ExtensionConfig>,
        session: &Session,
    ) -> Result<Arc<ExtensionManager>> {
        let provider = Arc::new(Mutex::new(Some(provider)));
        let extension_manager = new_extension_manager(&self.config, provider);
        let container = self.container.lock().await.clone();
        let loads = extension_configs.into_iter().map(|config| {
            let extension_manager = extension_manager.clone();
            let working_dir = session.working_dir.clone();
            let session_id = session.id.clone();
            let container = container.clone();
            async move {
                extension_manager
                    .add_extension(
                        config,
                        Some(working_dir),
                        container.as_ref(),
                        Some(&session_id),
                    )
                    .await
            }
        });
        futures::future::try_join_all(loads).await?;
        Ok(extension_manager)
    }

    /// Handle a confirmation response for a tool request
    pub async fn handle_confirmation(
        &self,
        request_id: String,
        confirmation: PermissionConfirmation,
    ) {
        let provider = self.provider.lock().await.clone();
        if let Some(provider) = provider.as_ref() {
            if provider.permission_routing() == PermissionRouting::ActionRequired
                && provider
                    .handle_permission_confirmation(&request_id, &confirmation)
                    .await
            {
                return;
            }
        }
        if !self
            .tool_confirmation_router
            .deliver(request_id, confirmation)
            .await
        {
            error!("Failed to deliver confirmation");
        }
    }

    pub async fn supports_action_required_permissions(&self) -> bool {
        if let Some(provider) = self.provider.lock().await.as_ref() {
            return provider.permission_routing() == PermissionRouting::ActionRequired;
        }
        false
    }

    pub(super) fn create_state_machine(
        &self,
        resources: StateMachineResources,
        max_turns: Option<u32>,
        cancel: CancellationToken,
        steer_queue: SteerQueue,
        report_operation: Option<Arc<dyn Operation<Session, GooseEffect>>>,
    ) -> StateMachine<'_, Session, GooseEffect> {
        let StateMachineResources {
            provider,
            model_config,
            extension_manager,
            context_limit,
        } = resources;
        let max_turns = max_turns.unwrap_or_else(|| {
            Config::global()
                .get_param::<u32>("GOOSE_MAX_TURNS")
                .unwrap_or(DEFAULT_MAX_TURNS)
        });
        let retry_timeout = Config::global()
            .get_param::<u64>("GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS")
            .unwrap_or(DEFAULT_RETRY_TIMEOUT_SECONDS);
        let on_failure_timeout = Config::global()
            .get_param::<u64>("GOOSE_RECIPE_ON_FAILURE_TIMEOUT_SECONDS")
            .unwrap_or(DEFAULT_ON_FAILURE_TIMEOUT_SECONDS);
        #[cfg(test)]
        let stop_hook_block_cap = self.stop_hook_block_cap_override.unwrap_or_else(|| {
            Config::global()
                .get_param::<u32>("GOOSE_STOP_HOOK_BLOCK_CAP")
                .unwrap_or(DEFAULT_STOP_HOOK_BLOCK_CAP)
        });
        #[cfg(not(test))]
        let stop_hook_block_cap = Config::global()
            .get_param::<u32>("GOOSE_STOP_HOOK_BLOCK_CAP")
            .unwrap_or(DEFAULT_STOP_HOOK_BLOCK_CAP);
        let compaction_threshold = Config::global()
            .get_param::<f64>("GOOSE_AUTO_COMPACT_THRESHOLD")
            .unwrap_or(DEFAULT_COMPACTION_THRESHOLD);
        let tool_call_cutoff = Config::global()
            .get_param::<usize>("GOOSE_TOOL_CALL_CUTOFF")
            .unwrap_or_else(|_| {
                crate::context_mgmt::compute_tool_call_cutoff(context_limit, compaction_threshold)
            });
        let manages_own_context = provider.manages_own_context();
        let tool_pair_compaction_enabled =
            crate::context_mgmt::tool_pair_summarization_enabled() && !manages_own_context;

        let mut operations: Vec<Arc<dyn Operation<Session, GooseEffect> + '_>> = vec![
            Arc::new(SteerOperation::new(steer_queue, self.hook_manager.clone())),
            Arc::new(MaxTurnsOperation::new(max_turns)),
            Arc::new(BangShellOperation::new()),
        ];
        if !manages_own_context {
            operations.push(Arc::new(CompactionOperation::new(
                provider.clone(),
                model_config.clone(),
                context_limit,
                compaction_threshold,
            )));
        }
        if let Some(report_operation) = report_operation {
            operations.push(report_operation);
        }
        let remaining_operations: Vec<Arc<dyn Operation<Session, GooseEffect> + '_>> = vec![
            Arc::new(ToolPairCompactionOperation::new(
                provider.clone(),
                model_config.clone(),
                tool_call_cutoff,
                tool_pair_compaction_enabled,
            )),
            Arc::new(ToolApprovalOperation::new(&self.tool_inspection_manager)),
            Arc::new(DoctorOperation),
            Arc::new(ProjectOperation),
            Arc::new(SkillOperation),
            Arc::new(RecipeOperation),
            Arc::new(ToolExecutionOperation::new(
                extension_manager.clone(),
                self.hook_manager.clone(),
            )),
            Arc::new(UnknownToolOperation),
            Arc::new(RetryOperation::new(
                &self.goal,
                &self.grind,
                std::time::Duration::from_secs(retry_timeout),
                std::time::Duration::from_secs(on_failure_timeout),
            )),
            Arc::new(StopHookOperation::new(
                self.hook_manager.clone(),
                stop_hook_block_cap,
            )),
            Arc::new(ExitOnErrorOperation),
        ];
        operations.extend(remaining_operations);
        let inference = Arc::new(InferenceRunner::new(
            provider,
            model_config,
            extension_manager,
            &self.prompt_manager,
            &self.tool_inspection_manager,
            &self.frontend_instructions,
        ));
        let mut command_handlers = operations.clone();
        command_handlers.push(inference.clone());
        let command_operation: Arc<dyn Operation<Session, GooseEffect> + '_> =
            Arc::new(SlashCommandOperation::new(command_handlers));
        let operations: Vec<_> =
            std::iter::once(Arc::new(EntryHookOperation::new(self.hook_manager.clone()))
                as Arc<dyn Operation<Session, GooseEffect> + '_>)
            .chain(std::iter::once(command_operation))
            .chain(operations)
            .collect();

        let steps = operations
            .into_iter()
            .map(Step::Operation)
            .chain(std::iter::once(Step::Inference(inference)))
            .collect();

        StateMachine::new(steps, cancel)
    }

    pub(crate) async fn reply_with_state_machine(
        &self,
        user_message: Message,
        session_config: SessionConfig,
        cancel_token: Option<CancellationToken>,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let session_manager = self.config.session_manager.clone();
        let cancel = cancel_token.unwrap_or_default();
        let session_id = session_config.id.clone();

        let entry_session = session_manager.get_session(&session_id, false).await?;
        if let Some(schedule_id) = session_config.schedule_id.clone() {
            session_manager
                .update(&session_id)
                .schedule_id(Some(schedule_id))
                .apply()
                .await?;
        }
        session_manager
            .add_message(&session_config.id, &user_message)
            .await?;

        let provider = self
            .provider
            .lock()
            .await
            .clone()
            .ok_or_else(|| anyhow!("Provider not set"))?;

        if !self.config.disable_session_naming {
            let manager = session_manager.clone();
            let tx = self.config.session_name_update_tx.clone();
            let id = session_id.clone();
            let provider = provider.clone();
            tokio::spawn(async move {
                match manager.maybe_update_name(&id, provider).await {
                    Ok(Some(update)) => {
                        if let Some(tx) = tx {
                            if tx.send(update).is_err() {
                                tracing::warn!("Failed to publish generated session name");
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => tracing::warn!("Failed to generate session description: {}", e),
                }
            });
        }

        let model_config = match entry_session.model_config {
            Some(model_config) => model_config,
            None => {
                let provider_name = Config::global()
                    .get_goose_provider()
                    .map_err(|_| anyhow!("Could not resolve model config: missing provider"))?;
                let model_name = Config::global()
                    .get_goose_model()
                    .map_err(|_| anyhow!("Could not resolve model config: missing model"))?;
                crate::model_config::model_config_from_user_config(&provider_name, &model_name)
                    .map_err(|error| anyhow!("Could not resolve model config: {error}"))?
            }
        };

        let context_limit = provider
            .get_context_limit(&model_config)
            .await
            .unwrap_or_else(|_| model_config.context_limit());
        let steer_queue = self.steer_queue(&session_id).await;
        let machine = self.create_state_machine(
            StateMachineResources {
                provider,
                model_config,
                extension_manager: self.extension_manager.clone(),
                context_limit,
            },
            session_config.max_turns,
            cancel.clone(),
            steer_queue,
            None,
        );
        let reply_span = tracing::Span::current();

        Ok(Box::pin(
            async_stream::try_stream! {
                let (tx, mut rx) = mpsc::channel::<AgentEvent>(32);
                let emit = Emitter::new(tx, cancel.clone());
                let result = {
                    let run = run_goose(&machine, session_manager.as_ref(), &session_id, &emit);
                    tokio::pin!(run);
                    loop {
                        tokio::select! {
                            biased;
                            Some(event) = rx.recv() => yield event,
                            result = &mut run => break result,
                        }
                    }
                };
                result?;
                // Without this the drain below never ends: `run` only borrows the emitter.
                drop(emit);
                while let Some(event) = rx.recv().await {
                    yield event;
                }
            }
            .instrument(reply_span),
        ))
    }

    #[instrument(
        skip(self, user_message, session_config, cancel_token),
        fields(
            user_message,
            trace_input,
            trace_output = tracing::field::Empty,
            session.id = %session_config.id,
            gen_ai.operation.name = "invoke_agent",
            gen_ai.input.messages = tracing::field::Empty,
            gen_ai.output.messages = tracing::field::Empty,
            gen_ai.usage.input_tokens = tracing::field::Empty,
            gen_ai.usage.output_tokens = tracing::field::Empty,
        )
    )]
    pub async fn reply(
        &self,
        user_message: Message,
        session_config: SessionConfig,
        cancel_token: Option<CancellationToken>,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let events = self
            .reply_impl(user_message, session_config, cancel_token)
            .await?;

        // This is the single live-event identity boundary. Callers that intentionally stream
        // multiple events for one logical message must assign their shared ID before this point.
        Ok(Box::pin(events.map_ok(ensure_message_event_id)))
    }

    async fn reply_impl(
        &self,
        user_message: Message,
        session_config: SessionConfig,
        cancel_token: Option<CancellationToken>,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let user_message = user_message.with_generated_id_if_missing();
        let session_manager = self.config.session_manager.clone();

        let message_text_for_trace = agent_visible_message_text(&user_message);
        tracing::Span::current().record("user_message", message_text_for_trace.as_str());
        tracing::Span::current().record("trace_input", message_text_for_trace.as_str());
        if gen_ai_telemetry::capture_message_content() {
            tracing::Span::current().record(
                "gen_ai.input.messages",
                gen_ai_telemetry::simple_input_json(&message_text_for_trace).as_str(),
            );
        }

        for content in &user_message.content {
            if let MessageContent::ActionRequired(action_required) = content {
                if let ActionRequiredData::ElicitationResponse {
                    id,
                    user_data,
                    action,
                } = &action_required.data
                {
                    // Surface stale/cancelled/timed-out elicitations as a hard
                    // error so callers (e.g. the HTTP handler) can propagate
                    // failure to the client instead of silently reporting
                    // success while the blocked tool call stays unblocked.
                    // The success path returns an empty stream after the MCP
                    // server receives the user's accept/decline/cancel action.
                    let response = match action {
                        ElicitationAction::Accept => ElicitationOutcome::Accept(user_data.clone()),
                        ElicitationAction::Decline => ElicitationOutcome::Decline,
                        ElicitationAction::Cancel => ElicitationOutcome::Cancel,
                        _ => ElicitationOutcome::Cancel,
                    };
                    crate::elicitation::complete_elicitation_with_message(
                        &session_manager,
                        &session_config.id,
                        id,
                        response,
                        &user_message,
                    )
                    .await
                    .map_err(|e| {
                        error!("Failed to submit elicitation response: {}", e);
                        anyhow!("Failed to submit elicitation response: {}", e)
                    })?;
                    return Ok(Box::pin(futures::stream::empty()));
                }
            }
        }

        if matches!(self.config.goose_platform, GoosePlatform::GooseCli) {
            if let Some(models) = super::state_machine::configured_supervised_models() {
                tracing::info!("dispatching reply via supervised state machines");
                return self
                    .reply_with_supervised_state_machines(
                        user_message,
                        session_config,
                        cancel_token,
                        models,
                    )
                    .await;
            }
        }

        if super::state_machine::enabled()
            || super::state_machine::bang_shell_command(&message_text_for_trace).is_some()
        {
            tracing::info!("dispatching reply via experimental state machine");
            return self
                .reply_with_state_machine(user_message, session_config, cancel_token)
                .await;
        }

        let message_text = message_text_for_trace;

        let session = session_manager
            .get_session(&session_config.id, true)
            .await?;
        let is_first_agent_turn = session
            .conversation
            .as_ref()
            .map(|conversation| {
                conversation.messages().iter().all(|message| {
                    !message.is_agent_visible()
                        || message.agent_visible_content().content.is_empty()
                })
            })
            .unwrap_or(true);

        if !user_message.is_agent_visible()
            || user_message.agent_visible_content().content.is_empty()
        {
            let user_visibility = user_message.is_user_visible();
            let user_message = user_message.with_visibility(user_visibility, false);
            session_manager
                .add_message(&session_config.id, &user_message)
                .await?;
            return Ok(Box::pin(futures::stream::empty()));
        }

        if is_first_agent_turn && !self.session_start_emitted.swap(true, Ordering::AcqRel) {
            self.emit_hook(crate::hooks::HookEvent::SessionStart, &session_config.id)
                .await;
        }

        if self
            .hook_manager
            .has_hooks(crate::hooks::HookEvent::UserPromptSubmit)
        {
            let ctx = crate::hooks::HookContext::new(
                crate::hooks::HookEvent::UserPromptSubmit,
                &session_config.id,
            )
            .with_message(message_text.clone());
            self.hook_manager
                .emit(crate::hooks::HookEvent::UserPromptSubmit, ctx)
                .await;
        }

        let command_result = self
            .execute_command(&message_text, &session_config.id)
            .await;

        let mut command_preamble: Vec<AgentEvent> = Vec::new();

        match command_result {
            Err(e) => {
                let error_message = Message::assistant()
                    .with_text(e.to_string())
                    .with_visibility(true, false);
                return Ok(Box::pin(stream::once(async move {
                    Ok(AgentEvent::Message(error_message))
                })));
            }
            Ok(Some(response))
                if response.role == rmcp::model::Role::Assistant
                    && crate::agents::execute_commands::command_starts_turn(&message_text) =>
            {
                let response = response.with_generated_id_if_missing();

                // Setting a goal/grind should immediately start a turn so the
                // agent begins pursuing it, rather than waiting for the next
                // user prompt. Record the command and its confirmation as
                // user-visible only, then inject an agent-visible kickoff and
                // fall through into the reply loop.
                session_manager
                    .add_message(
                        &session_config.id,
                        &user_message.clone().with_visibility(true, false),
                    )
                    .await?;
                session_manager
                    .add_message(
                        &session_config.id,
                        &response.clone().with_visibility(true, false),
                    )
                    .await?;
                let goal_text = crate::agents::execute_commands::parse_slash_command(&message_text)
                    .map(|parsed| parsed.params_str.to_string())
                    .unwrap_or_default();
                let kickoff = Message::user()
                    .with_text(format!(
                        "Start working toward this goal now:\n\n**Goal:** {goal_text}"
                    ))
                    .with_visibility(false, true);
                session_manager
                    .add_message(&session_config.id, &kickoff)
                    .await?;

                command_preamble = vec![
                    AgentEvent::Message(user_message.clone()),
                    AgentEvent::Message(response.clone()),
                ];
            }
            Ok(Some(response)) if response.role == rmcp::model::Role::Assistant => {
                let response = response.with_generated_id_if_missing();

                session_manager
                    .add_message(
                        &session_config.id,
                        &user_message.clone().with_visibility(true, false),
                    )
                    .await?;
                session_manager
                    .add_message(
                        &session_config.id,
                        &response.clone().with_visibility(true, false),
                    )
                    .await?;

                // Check if this was a command that modifies conversation history
                let modifies_history = crate::agents::execute_commands::COMPACT_TRIGGERS
                    .contains(&message_text.trim())
                    || message_text.trim() == "/clear";

                return Ok(Box::pin(async_stream::try_stream! {
                    yield AgentEvent::Message(user_message);
                    yield AgentEvent::Message(response);

                    // After commands that modify history, notify UI that history was replaced
                    if modifies_history {
                        let updated_session = session_manager.get_session(&session_config.id, true)
                            .await
                            .map_err(|e| anyhow!("Failed to fetch updated session: {}", e))?;
                        let updated_conversation = updated_session
                            .conversation
                            .ok_or_else(|| anyhow!("Session has no conversation after history modification"))?;
                        yield AgentEvent::HistoryReplaced(updated_conversation);
                    }
                }));
            }
            Ok(Some(resolved_message)) => {
                session_manager
                    .add_message(
                        &session_config.id,
                        &user_message.clone().with_visibility(true, false),
                    )
                    .await?;
                session_manager
                    .add_message(
                        &session_config.id,
                        &resolved_message.clone().with_visibility(false, true),
                    )
                    .await?;
            }
            Ok(None) => {
                session_manager
                    .add_message(&session_config.id, &user_message)
                    .await?;
            }
        }
        let session = session_manager
            .get_session(&session_config.id, true)
            .await?;
        let conversation = session
            .conversation
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Session {} has no conversation", session_config.id))?;

        let needs_auto_compact = check_if_compaction_needed(
            self.provider().await?.as_ref(),
            &conversation,
            None,
            &session,
        )
        .await?;

        let conversation_to_compact = conversation.clone();
        let reply_span = tracing::Span::current();

        Ok(Box::pin(async_stream::try_stream! {
            for event in command_preamble {
                yield event;
            }

            let final_conversation = if !needs_auto_compact {
                conversation
            } else {
                let config = Config::global();
                let threshold = config
                    .get_param::<f64>("GOOSE_AUTO_COMPACT_THRESHOLD")
                    .unwrap_or(DEFAULT_COMPACTION_THRESHOLD);
                let threshold_percentage = (threshold * 100.0) as u32;

                let inline_msg = format!(
                    "Exceeded auto-compact threshold of {}%. Performing auto-compaction...",
                    threshold_percentage
                );

                yield AgentEvent::Message(
                    Message::assistant().with_system_notification(
                        SystemNotificationType::InlineMessage,
                        inline_msg,
                    )
                );

                yield AgentEvent::Message(
                    Message::assistant().with_system_notification(
                        SystemNotificationType::ProgressMessage,
                        COMPACTION_PROGRESS_TEXT,
                    )
                );

                let compact_model_config = self.model_config_for_session(&session_config.id).await?;
                match compact_messages(
                    self.provider().await?.as_ref(),
                    &compact_model_config,
                    &session_config.id,
                    &conversation_to_compact,
                    false,
                )
                .await
                {
                    Ok(compaction) => {
                        let compacted_conversation = compaction.conversation;
                        session_manager.replace_conversation(&session_config.id, &compacted_conversation).await?;
                        self.update_session_metrics(&session_config.id, session_config.schedule_id.clone(), &compaction.usage, Some(compaction.retained_context_tokens)).await?;

                        yield AgentEvent::HistoryReplaced(compacted_conversation.clone());

                        yield AgentEvent::Message(
                            Message::assistant().with_system_notification(
                                SystemNotificationType::InlineMessage,
                                "Compaction complete",
                            )
                        );

                        compacted_conversation
                    }
                    Err(e) => {
                        yield AgentEvent::Message(
                            Message::assistant().with_text(
                                format!("Ran into this error trying to compact: {e}.\n\nPlease try again or create a new session")
                            )
                        );
                        return;
                    }
                }
            };

            let mut reply_stream = self.reply_internal(final_conversation, session_config, session, cancel_token, reply_span.clone()).await?;
            while let Some(event) = reply_stream.next().await {
                yield event?;
            }
        }))
    }

    async fn reply_internal(
        &self,
        conversation: Conversation,
        session_config: SessionConfig,
        session: Session,
        cancel_token: Option<CancellationToken>,
        reply_span: tracing::Span,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let context = self
            .prepare_reply_context(&session.id, conversation, session.working_dir.as_path())
            .await?;
        let ReplyContext {
            mut conversation,
            mut tools,
            mut toolshim_tools,
            mut system_prompt,
            tool_call_cut_off,
            goose_mode,
            model_config,
        } = context;

        if let Some(project_addendum) = self.load_project_instructions(&session).await {
            system_prompt = format!("{system_prompt}\n\n{project_addendum}");
        }

        self.reset_retry_attempts().await;

        let provider = self.provider().await?;
        let provider_name = provider.get_name().to_string();
        let saved_provider_session_id =
            super::latest_provider_session_id(conversation.messages(), &provider_name);
        if let Some(saved_provider_session_id) = saved_provider_session_id {
            if let Err(error) = provider.resume(saved_provider_session_id).await {
                warn!(
                    provider = provider_name,
                    %error,
                    "Could not resume provider session; continuing with a handoff"
                );
            }
        }

        let requested_model = model_config.model_name.clone();
        let resolved_model = provider
            .fetch_model_info(&requested_model)
            .await
            .ok()
            .and_then(|model_info| model_info.resolved_model);
        let provider_session_id = provider.provider_session_id();
        let inference = Some(InferenceMetadata {
            provider: provider_name.clone(),
            requested_model,
            resolved_model,
            provider_session_id,
        });
        let session_manager = self.config.session_manager.clone();
        let session_id = session_config.id.clone();
        if !self.config.disable_session_naming {
            let provider = provider.clone();
            let manager_for_spawn = session_manager.clone();
            let session_name_update_tx = self.config.session_name_update_tx.clone();
            tokio::spawn(async move {
                match manager_for_spawn
                    .maybe_update_name(&session_id, provider)
                    .await
                {
                    Ok(Some(update)) => {
                        if let Some(tx) = session_name_update_tx {
                            if tx.send(update).is_err() {
                                warn!("Failed to publish generated session name");
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => warn!("Failed to generate session description: {}", e),
                }
            });
        }

        // Count tool calls present before this reply — everything added during
        // the reply loop is part of the current turn and should not be summarized.
        let pre_turn_tool_count = conversation
            .messages()
            .iter()
            .flat_map(|m| m.content.iter())
            .filter(|c| matches!(c, MessageContent::ToolRequest(_)))
            .count();

        let working_dir = session.working_dir.clone();
        let reply_stream_span = tracing::info_span!(
            parent: &reply_span,
            "reply_stream",
            trace_output = tracing::field::Empty,
            session.id = %session_config.id,
            session.user = %crate::session_context::session_user(),
            session.host = %crate::session_context::session_host(),
            session.agent_type = "goose",
            gen_ai.operation.name = "invoke_agent",
            gen_ai.conversation.id = %session_config.id,
            gen_ai.request.model = %model_config.model_name,
            gen_ai.provider.name = %provider_name,
            gen_ai.input.messages = tracing::field::Empty,
            gen_ai.output.messages = tracing::field::Empty,
            gen_ai.usage.input_tokens = tracing::field::Empty,
            gen_ai.usage.output_tokens = tracing::field::Empty,
        );
        if gen_ai_telemetry::capture_message_content() {
            if let Some(last_user_msg) = conversation
                .messages()
                .iter()
                .rev()
                .find(|m| m.role == rmcp::model::Role::User)
            {
                reply_stream_span.record(
                    "gen_ai.input.messages",
                    gen_ai_telemetry::simple_input_json(&last_user_msg.as_concat_text()).as_str(),
                );
            }
        }
        let inner = Box::pin(async_stream::try_stream! {
            let mut turns_taken = 0u32;
            let max_turns = session_config.max_turns.unwrap_or_else(|| {
                Config::global()
                    .get_param::<u32>("GOOSE_MAX_TURNS")
                    .unwrap_or(DEFAULT_MAX_TURNS)
            });
            let mut compaction_attempts = 0;
            let mut empty_turn_retries = 0u32;
            let mut retrying_after_empty_turn = false;
            let mut last_assistant_text = String::new();
            let mut turn_total_usage = Usage::default();
            let mut goal_check_pending = false;
            let mut tool_pair_summarization_done = false;
            let mut stop_hook_handled_for_exit = false;
            let mut retrying_after_stop_hook_denial = false;
            let mut consecutive_stop_hook_blocks = 0u32;
            let stop_hook_block_cap = self.stop_hook_block_cap();
            let mut can_drain_pending_steers = false;
            let turn_start = chrono::Local::now();
            let turn_start_compaction_info =
                super::moim::compute_compaction_info(&session_config.id, &self.extension_manager)
                    .await;

            if let Some(turn_context) = super::moim::turn_context_message(
                &session_config.id,
                &self.extension_manager,
                turns_taken,
                max_turns,
                turn_start,
                turn_start_compaction_info,
            )
            .await
            {
                persist_and_push_message_with_id(
                    &session_manager,
                    &session_config.id,
                    &mut conversation,
                    turn_context,
                )
                .await?;
            }
            // Snapshot after the turn-context append so a retry keeps the sent prefix.
            let initial_messages = conversation.messages().clone();

            loop {
                if is_token_cancelled(&cancel_token) {
                    break;
                }

                if can_drain_pending_steers {
                    for message in self.drain_pending_steers(&session_config.id).await {
                        let message_text = agent_visible_message_text(&message);
                        if self
                            .hook_manager
                            .has_hooks(crate::hooks::HookEvent::UserPromptSubmit)
                        {
                            let ctx = crate::hooks::HookContext::new(
                                crate::hooks::HookEvent::UserPromptSubmit,
                                &session_config.id,
                            )
                            .with_message(message_text);
                            self.hook_manager
                                .emit(crate::hooks::HookEvent::UserPromptSubmit, ctx)
                                .await;
                        }
                        let message = persist_and_push_message_with_id(
                            &session_manager,
                            &session_config.id,
                            &mut conversation,
                            message,
                        )
                        .await?;
                        yield AgentEvent::Message(message);
                    }
                }

                let final_output = {
                    let mut guard = self.final_output_tool.lock().await;
                    guard.as_mut().and_then(|fot| fot.final_output.take())
                };
                if let Some(output) = final_output {
                    last_assistant_text = output.clone();
                    let message = Message::assistant()
                        .with_text(output)
                        .with_generated_id_if_missing();
                    yield AgentEvent::Message(message.clone());
                    session_manager.add_message(&session_config.id, &message).await?;
                    conversation.push(message);

                    match self
                        .emit_stop_hook_blocking(&session_config.id, &last_assistant_text, &session.working_dir.to_string_lossy())
                        .await
                    {
                        crate::hooks::HookDecision::Allow => {
                            stop_hook_handled_for_exit = true;
                            break;
                        }
                        crate::hooks::HookDecision::Deny { reason, plugin } => {
                            consecutive_stop_hook_blocks += 1;
                            if consecutive_stop_hook_blocks > stop_hook_block_cap {
                                let message = persist_message_with_id(
                                    &session_manager,
                                    &session_config.id,
                                    stop_hook_block_cap_warning(&plugin, stop_hook_block_cap),
                                )
                                .await?;
                                yield AgentEvent::Message(message);
                                stop_hook_handled_for_exit = true;
                                break;
                            }
                            persist_and_push_message_with_id(
                                &session_manager,
                                &session_config.id,
                                &mut conversation,
                                stop_hook_denial_context_message(&plugin, &reason),
                            )
                            .await?;
                            yield AgentEvent::Message(stop_hook_denial_notification(&plugin));
                            retrying_after_stop_hook_denial = true;
                            continue;
                        }
                    }
                }

                if retrying_after_stop_hook_denial {
                    retrying_after_stop_hook_denial = false;
                } else if retrying_after_empty_turn {
                    retrying_after_empty_turn = false;
                } else {
                    turns_taken += 1;
                }
                if turns_taken > max_turns {
                    last_assistant_text = MAX_TURNS_MESSAGE.to_string();
                    yield AgentEvent::Message(Message::assistant().with_text(last_assistant_text.clone()));
                    break;
                }

                let mut stream = crate::agents::reply_parts::stream_response_from_provider(
                    self.provider().await?,
                    model_config.clone(),
                    &session_config.id,
                    &system_prompt,
                    conversation.messages(),
                    &tools,
                    &toolshim_tools,
                ).await?;
                last_assistant_text.clear();

                let current_turn_tool_count = conversation.messages().iter()
                    .flat_map(|m| m.content.iter())
                    .filter(|c| matches!(c, MessageContent::ToolRequest(_)))
                    .count()
                    .saturating_sub(pre_turn_tool_count);

                let tool_pair_summarization_task = if tool_pair_summarization_done {
                    None
                } else {
                    crate::context_mgmt::maybe_summarize_tool_pairs(
                        self.provider().await?,
                        model_config.clone(),
                        session_config.id.clone(),
                        conversation.clone(),
                        tool_call_cut_off,
                        current_turn_tool_count,
                    )
                };

                let mut no_tools_called = true;
                let mut messages_to_add = Conversation::default();
                let mut tools_updated = false;
                let mut did_recovery_compact_this_iteration = false;
                let mut exit_chat = false;
                let mut provider_errored = false;
                let mut provider_produced_content = false;
                let mut provider_reached_output_token_limit = false;
                let mut pending_final_output: Option<String> = None;
                let mut pending_turn_usage: Option<ProviderUsage> = None;
                let mut preferred_turn_usage_message_id: Option<String> = None;

                // Track whether this provider turn has already emitted visible
                // thinking so a later tool-call chunk can suppress replayed
                // reasoning without hiding final-only non-streaming thoughts.
                let mut surfaced_thinking_in_turn = false;

                loop {
                    let next = if let Some(cancel_token) = &cancel_token {
                        tokio::select! {
                            biased;
                            _ = cancel_token.cancelled() => break,
                            next = stream.next() => next,
                        }
                    } else {
                        stream.next().await
                    };
                    let Some(next) = next else {
                        break;
                    };

                    if exit_chat {
                        break;
                    }

                    match next {
                        Ok((response, usage)) => {
                            compaction_attempts = 0;

                            if let Some(ref usage) = usage {
                                let enriched = self.update_session_metrics(&session_config.id, session_config.schedule_id.clone(), usage, None).await?;
                                yield AgentEvent::Usage(enriched.clone());
                                turn_total_usage += enriched.usage;
                                pending_turn_usage = Some(enriched);
                            }

                            if let Some(response) = response {
                                provider_reached_output_token_limit |=
                                    response.metadata.output_token_limit_reached;

                                if !response.content.is_empty()
                                    && response.content.iter().all(|content| {
                                        matches!(content, MessageContent::SystemNotification(_))
                                    })
                                {
                                    yield AgentEvent::Message(response);
                                    tokio::task::yield_now().await;
                                    continue;
                                }

                                provider_produced_content |= response.content.iter().any(|content| {
                                    match content {
                                        MessageContent::Text(text) => !text.text.is_empty(),
                                        MessageContent::Image(image) => !image.data.is_empty(),
                                        MessageContent::Thinking(thinking) => {
                                            !thinking.thinking.is_empty()
                                                || !thinking.signature.is_empty()
                                        }
                                        MessageContent::RedactedThinking(thinking) => {
                                            !thinking.data.is_empty()
                                        }
                                        MessageContent::SystemNotification(notification) => {
                                            !notification.msg.is_empty()
                                        }
                                        _ => true,
                                    }
                                });

                                let ToolCategorizeResult {
                                    frontend_requests,
                                    remaining_requests,
                                    filtered_response,
                                } = self
                                    .categorize_tools(
                                        &response,
                                        &tools,
                                        surfaced_thinking_in_turn,
                                    )
                                    .await;

                                let filtered_response = if let Some(inference) = inference.as_ref() {
                                    filtered_response.with_inference(inference.clone())
                                } else {
                                    filtered_response
                                };
                                let response = if let Some(inference) = inference.as_ref() {
                                    response.with_inference(inference.clone())
                                } else {
                                    response
                                };

                                surfaced_thinking_in_turn |= filtered_response.content.iter().any(
                                    |content| {
                                        matches!(
                                            content,
                                            MessageContent::Thinking(_)
                                                | MessageContent::RedactedThinking(_)
                                        )
                                    },
                                );

                                if !filtered_response.content.is_empty()
                                    || filtered_response.metadata.output_token_limit_reached
                                {
                                    yield AgentEvent::Message(filtered_response.clone());
                                    tokio::task::yield_now().await;
                                }

                                let num_tool_requests = frontend_requests.len() + remaining_requests.len();
                                if num_tool_requests == 0 {
                                    let text = filtered_response.as_concat_text();
                                    if !text.is_empty() {
                                        last_assistant_text.push_str(&text);
                                    }
                                    messages_to_add.push(response);
                                    continue;
                                }

                                let mut request_to_response_map = HashMap::new();
                                let mut request_metadata: HashMap<String, Option<ProviderMetadata>> = HashMap::new();
                                for request in frontend_requests.iter().chain(remaining_requests.iter()) {
                                    request_to_response_map.insert(request.id.clone(), Message::user().with_generated_id());
                                    request_metadata.insert(request.id.clone(), request.metadata.clone());
                                }

                                for request in frontend_requests.iter() {
                                    let response_msg = request_to_response_map.get_mut(&request.id)
                                        .ok_or_else(|| anyhow::anyhow!("missing response entry for request {}", request.id))?;
                                    let mut frontend_tool_stream = self.handle_frontend_tool_request(
                                        request,
                                        response_msg,
                                    );

                                    while let Some(msg) = frontend_tool_stream.try_next().await? {
                                        yield AgentEvent::Message(msg);
                                    }
                                }
                                if goose_mode == GooseMode::Chat {
                                    for request in remaining_requests.iter() {
                                        // An unparseable tool call should surface the parse error
                                        // (added in the Err branch below), not a successful skip —
                                        // otherwise the model sees a malformed call as "skipped OK"
                                        // and can't correct the arguments.
                                        if request.tool_call.is_err() {
                                            continue;
                                        }
                                        if let Some(response) = request_to_response_map.get_mut(&request.id) {
                                            response.add_tool_response_with_metadata(
                                                request.id.clone(),
                                                Ok(CallToolResult::success(vec![ContentBlock::text(CHAT_MODE_TOOL_SKIPPED_RESPONSE)])),
                                                request.metadata.as_ref(),
                                            );
                                        }
                                    }
                                } else {
                                    // Run all tool inspectors
                                    let inspection_results = self.tool_inspection_manager
                                        .inspect_tools(
                                            &session_config.id,
                                            &remaining_requests,
                                            conversation.messages(),
                                            goose_mode,
                                        )
                                        .await?;

                                    let permission_check_result = self.tool_inspection_manager
                                        .process_inspection_results_with_permission_inspector(
                                            &remaining_requests,
                                            &inspection_results,
                                        )
                                        .unwrap_or_else(|| {
                                            let mut result = PermissionCheckResult {
                                                approved: vec![],
                                                needs_approval: vec![],
                                                denied: vec![],
                                            };
                                            result.needs_approval.extend(remaining_requests.iter().cloned());
                                            result
                                        });

                                    // Track extension requests
                                    let mut enable_extension_request_ids = vec![];
                                    for request in &remaining_requests {
                                        if let Ok(tool_call) = &request.tool_call {
                                            if tool_call.name == MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE {
                                                enable_extension_request_ids.push(request.id.clone());
                                            }
                                        }
                                    }

                                    let mut tool_futures = self.handle_approved_and_denied_tools(
                                        &permission_check_result,
                                        &mut request_to_response_map,
                                        cancel_token.clone(),
                                        &session,
                                    ).await?;

                                    {
                                        let mut tool_approval_stream = self.handle_approval_tool_requests(
                                            &permission_check_result.needs_approval,
                                            &mut tool_futures,
                                            &mut request_to_response_map,
                                            cancel_token.clone(),
                                            &session,
                                            &inspection_results,
                                        );

                                        while let Some(msg) = tool_approval_stream.try_next().await? {
                                            yield AgentEvent::Message(msg);
                                        }
                                    }

                                    let with_id = tool_futures
                                        .into_iter()
                                        .map(|(request_id, stream)| {
                                            stream.map(move |item| (request_id.clone(), item))
                                        })
                                        .collect::<Vec<_>>();

                                    let mut combined = stream::select_all(with_id);
                                    let mut all_install_successful = true;

                                    loop {
                                        if is_token_cancelled(&cancel_token) {
                                            break;
                                        }

                                        tokio::select! {
                                            biased;

                                            tool_item = combined.next() => {
                                                match tool_item {
                                                    Some((request_id, item)) => {
                                                        match item {
                                                            ToolStreamItem::ActionRequired(msg) => {
                                                                let msg = msg.with_generated_id_if_missing();
                                                                if let Err(e) = session_manager.add_message(&session_config.id, &msg).await {
                                                                    warn!("Failed to save elicitation message to session: {}", e);
                                                                }
                                                                yield AgentEvent::Message(msg);
                                                            }
                                                            ToolStreamItem::Result(output) => {
                                                                if let Ok(ref call_result) = output {
                                                                    if let Some(ref meta) = call_result.meta {
                                                                        if let Some(notification_data) = meta.0.get("platform_notification") {
                                                                            if let Some(method) = notification_data.get("method").and_then(|v| v.as_str()) {
                                                                                let params = notification_data.get("params").cloned();
                                                                                let custom_notification = rmcp::model::CustomNotification::new(
                                                                                    method.to_string(),
                                                                                    params,
                                                                                );

                                                                                let server_notification = rmcp::model::ServerNotification::CustomNotification(custom_notification);
                                                                                yield AgentEvent::McpNotification((request_id.clone(), server_notification));
                                                                            }
                                                                        }
                                                                    }
                                                                }

                                                                if enable_extension_request_ids.contains(&request_id)
                                                                    && output.is_err()
                                                                {
                                                                    all_install_successful = false;
                                                                }
                                                                if let Some(response) = request_to_response_map.get_mut(&request_id) {
                                                                    let metadata = request_metadata.get(&request_id).and_then(|m| m.as_ref());
                                                                    response.add_tool_response_with_metadata(request_id, output, metadata);
                                                                }
                                                            }
                                                            ToolStreamItem::Message(msg) => {
                                                                yield AgentEvent::McpNotification((request_id, msg));
                                                            }
                                                        }
                                                    }
                                                    None => break,
                                                }
                                            }

                                            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
                                        }
                                    }

                                    if all_install_successful && !enable_extension_request_ids.is_empty() {
                                        if let Err(e) = self.save_extension_state(&session_config).await {
                                            warn!("Failed to save extension state after runtime changes: {}", e);
                                        }
                                        tools_updated = true;
                                    }
                                }

                                // Thinking/reasoning belongs on the tool-call messages, not also
                                // as a separate standalone message: Gemini and Kimi/DeepSeek
                                // require it echoed on each assistant tool-call message, and the
                                // provider formatters reconstruct per-provider shape from there.
                                // Storing it both standalone AND on the tool-call message
                                // duplicates it; once merge_consecutive_messages glues the adjacent
                                // standalone and tool-call messages together, the duplicate signed
                                // blocks make Anthropic reject the turn with a 400. So the thinking
                                // is carried onto the split request messages below and never kept
                                // as a redundant standalone message.
                                let direct_thinking: Vec<MessageContent> = response
                                    .content
                                    .iter()
                                    .filter(|c| {
                                        matches!(
                                            c,
                                            MessageContent::Thinking(_)
                                                | MessageContent::RedactedThinking(_)
                                        )
                                    })
                                    .cloned()
                                    .collect();
                                // When thinking arrived in earlier stream chunks it was stored as
                                // standalone thinking-only messages; reuse that thinking on the
                                // tool-call messages and drop the standalone messages so the
                                // thinking isn't duplicated.
                                // Always accumulate ALL prior thinking — even when
                                // direct_thinking is non-empty (reasoning arrived on the same
                                // chunk as tool_calls) — because otherwise only the last chunk's
                                // reasoning ends up on split tool-call messages.
                                // Also extract thinking from mixed (thinking+text) messages,
                                // not just pure-thinking-only ones.
                                let mut accumulated_prior: Vec<MessageContent> = Vec::new();
                                let mut indices_to_remove: Vec<usize> = Vec::new();
                                for (idx, m) in messages_to_add.messages_mut().iter_mut().enumerate()
                                {
                                    if m.role != response.role || m.content.is_empty() {
                                        continue;
                                    }
                                    let thinking_only = m.content.iter().all(|c| {
                                        matches!(
                                            c,
                                            MessageContent::Thinking(_)
                                                | MessageContent::RedactedThinking(_)
                                        )
                                    });
                                    let has_thinking = m.content.iter().any(|c| {
                                        matches!(
                                            c,
                                            MessageContent::Thinking(_)
                                                | MessageContent::RedactedThinking(_)
                                        )
                                    });
                                    if has_thinking {
                                        // Only accumulate thinking from messages that
                                        // have not already been split into tool-call
                                        // request_msg items — prior-split messages
                                        // already carry their own thinking copy.
                                        if !m.content.iter().any(|c| {
                                            matches!(c, MessageContent::ToolRequest(_))
                                        }) {
                                            for c in &m.content {
                                                if matches!(
                                                    c,
                                                    MessageContent::Thinking(_)
                                                        | MessageContent::RedactedThinking(_)
                                                ) {
                                                    accumulated_prior.push(c.clone());
                                                }
                                            }
                                        }
                                    }
                                    if thinking_only {
                                        indices_to_remove.push(idx);
                                    } else if has_thinking
                                        && !m.content.iter().any(|c| {
                                            matches!(c, MessageContent::ToolRequest(_))
                                        })
                                    {
                                        // Strip thinking blocks from mixed text+thinking
                                        // messages so the same signed/unsigned thinking is not
                                        // duplicated when carried onto the tool-call request
                                        // messages below. Messages that already contain tool
                                        // requests are prior-split request_msg items whose
                                        // thinking was already attached — stripping their
                                        // thinking would leave only the last split message
                                        // with reasoning, violating the signed-thinking
                                        // dedup expectation that the first split message
                                        // retains it.
                                        m.content.retain(|c| {
                                            !matches!(
                                                c,
                                                MessageContent::Thinking(_)
                                                    | MessageContent::RedactedThinking(_)
                                            )
                                        });
                                    }
                                }
                                // Remove in reverse order to preserve indices
                                for idx in indices_to_remove.into_iter().rev() {
                                    messages_to_add.remove(idx);
                                }
                                let response_thinking = if direct_thinking.is_empty() {
                                    accumulated_prior
                                } else if accumulated_prior.is_empty() {
                                    direct_thinking
                                } else {
                                    let mut merged = accumulated_prior;
                                    merged.extend(direct_thinking);
                                    merged
                                };

                                let response_message_id = response
                                    .id
                                    .as_deref()
                                    .expect("provider stream responses have IDs");
                                let has_existing_message_id_carrier = messages_to_add
                                    .iter()
                                    .any(|message| {
                                        message.id.as_deref() == Some(response_message_id)
                                    });
                                let carrier_tool_call_id = if has_existing_message_id_carrier {
                                    None
                                } else {
                                    remaining_requests
                                        .first()
                                        .or_else(|| frontend_requests.first())
                                        .map(|request| request.id.as_str())
                                };
                                preferred_turn_usage_message_id =
                                    Some(response_message_id.to_owned());

                                for request in frontend_requests.iter().chain(remaining_requests.iter()) {
                                    let mut request_msg =
                                        if carrier_tool_call_id == Some(request.id.as_str()) {
                                            Message::assistant().with_id(response_message_id)
                                        } else {
                                            Message::assistant().with_generated_id()
                                        };

                                    for thinking in &response_thinking {
                                        request_msg = request_msg.with_content(thinking.clone());
                                    }

                                    // For an unparseable tool call (Err), store a valid
                                    // placeholder Ok tool-call in history instead of the Err. This
                                    // keeps the conversation well-formed through EVERY provider
                                    // formatter's normal Ok path — so we don't have to special-case
                                    // each formatter's Err arm — and preserves provider metadata
                                    // (e.g. thought signatures), which is passed through below and
                                    // copied by the Ok path. The actual parse error rides on the
                                    // paired tool response.
                                    let history_tool_call = match &request.tool_call {
                                        Ok(_) => request.tool_call.clone(),
                                        Err(_) => Ok(CallToolRequestParams::new(
                                            "unparseable_tool_call",
                                        )
                                        .with_arguments(serde_json::Map::new())),
                                    };
                                    request_msg = request_msg
                                        .with_tool_request_with_metadata(
                                            request.id.clone(),
                                            history_tool_call,
                                            request.metadata.as_ref(),
                                            request.tool_meta.clone(),
                                        );

                                    let final_response = match &request.tool_call {
                                        Ok(_) => request_to_response_map
                                            .remove(&request.id)
                                            .unwrap_or_else(|| Message::user().with_generated_id()),
                                        Err(error) => {
                                            error!("Tool call could not be parsed: {error}");
                                            let mut response = request_to_response_map
                                                .remove(&request.id)
                                                .unwrap_or_else(|| Message::user().with_generated_id());
                                            // Only feed the parse error back if this id isn't
                                            // already answered. In Chat mode the skip branch above
                                            // already added a tool response for it; adding another
                                            // here would duplicate the tool_call_id (which strict
                                            // providers reject).
                                            let already_answered = response.content.iter().any(|c| {
                                                matches!(c, MessageContent::ToolResponse(r) if r.id == request.id)
                                            });
                                            if !already_answered {
                                                response.add_tool_response_with_metadata(
                                                    request.id.clone(),
                                                    Err(error.clone()),
                                                    request.metadata.as_ref(),
                                                );
                                            }
                                            response
                                        }
                                    };

                                    // Response placeholder is created before tools run, so clamp request to avoid inverted ordering.
                                    if request_msg.created > final_response.created {
                                        request_msg.created = final_response.created;
                                    }
                                    messages_to_add.push(request_msg);
                                    yield AgentEvent::Message(project_message_for_user_event(&final_response));
                                    messages_to_add.push(final_response);
                                }

                                no_tools_called = false;
                                // Agent is actively working — re-check goal when it next finishes
                                goal_check_pending = false;
                            }
                        }
                        #[allow(unused_variables)]
                        Err(ref provider_err @ ProviderError::ContextLengthExceeded(_)) => {
                            provider_errored = true;
                            #[cfg(feature = "telemetry")]
                            crate::posthog::emit_error(provider_err.telemetry_type(), &provider_err.to_string());
                            compaction_attempts += 1;

                            if compaction_attempts >= 2 {
                                error!("Context limit exceeded after compaction - prompt too large");
                                yield AgentEvent::Message(
                                    Message::assistant().with_system_notification(
                                        SystemNotificationType::InlineMessage,
                                        "Unable to continue: Context limit still exceeded after compaction. Try using a shorter message, a model with a larger context window, or start a new session."
                                    )
                                );
                                break;
                            }

                            yield AgentEvent::Message(
                                Message::assistant().with_system_notification(
                                    SystemNotificationType::InlineMessage,
                                    "Context limit reached. Compacting to continue conversation...",
                                )
                            );
                            yield AgentEvent::Message(
                                Message::assistant().with_system_notification(
                                    SystemNotificationType::ProgressMessage,
                                    COMPACTION_PROGRESS_TEXT,
                                )
                            );

                            match compact_messages(
                                self.provider().await?.as_ref(),
                                &model_config,
                                &session_config.id,
                                &conversation,
                                false,
                            )
                            .await
                            {
                                Ok(compaction) => {
                                    session_manager.replace_conversation(&session_config.id, &compaction.conversation).await?;
                                    self.update_session_metrics(&session_config.id, session_config.schedule_id.clone(), &compaction.usage, Some(compaction.retained_context_tokens)).await?;
                                    conversation = compaction.conversation;
                                    did_recovery_compact_this_iteration = true;
                                    yield AgentEvent::HistoryReplaced(conversation.clone());
                                    break;
                                }
                                Err(e) => {
                                    #[cfg(feature = "telemetry")]
                                    crate::posthog::emit_error("compaction_failed", &e.to_string());
                                    error!("Compaction failed: {}", e);
                                    yield AgentEvent::Message(
                                        Message::assistant().with_text(
                                            format!("Ran into this error trying to compact: {e}.\n\nPlease try again or create a new session")
                                        )
                                    );
                                    break;
                                }
                            }
                        }
                        Err(ref provider_err @ ProviderError::CreditsExhausted { details: _, ref top_up_url }) => {
                            provider_errored = true;
                            #[cfg(feature = "telemetry")]
                            crate::posthog::emit_error(provider_err.telemetry_type(), &provider_err.to_string());
                            error!("Error: {}", provider_err);

                            let user_msg = if top_up_url.is_some() {
                                "Please add credits to your account, then resend your message to continue.".to_string()
                            } else {
                                "Please check your account with your provider to add more credits, then resend your message to continue.".to_string()
                            };

                            let notification_data = serde_json::json!({
                                "top_up_url": top_up_url,
                            });

                            yield AgentEvent::Message(
                                Message::assistant().with_system_notification_with_data(
                                    SystemNotificationType::CreditsExhausted,
                                    user_msg,
                                    notification_data,
                                )
                            );
                            break;
                        }
                        Err(ref provider_err @ ProviderError::Refusal { ref details, ref category }) => {
                            provider_errored = true;
                            #[cfg(feature = "telemetry")]
                            crate::posthog::emit_error(provider_err.telemetry_type(), &provider_err.to_string());
                            error!("Error: {}", provider_err);

                            let category = category.as_deref().map(|c| format!("\n\nCategory: {c}")).unwrap_or_default();
                            yield AgentEvent::Message(Message::assistant().with_text(format!(
                                "The provider refused this request.\n\n{details}{category}\n\nPlease start a new session to continue — resending this conversation is likely to be refused again."
                            )));
                            // A refusal is terminal: skip goal/grind nudges and
                            // recipe retry_config, which would resend the same
                            // refused conversation.
                            exit_chat = true;
                            break;
                        }
                        Err(ref provider_err @ ProviderError::Authentication(_)) => {
                            provider_errored = true;
                            #[cfg(feature = "telemetry")]
                            crate::posthog::emit_error(provider_err.telemetry_type(), &provider_err.to_string());
                            error!("Error: {}", provider_err);
                            let message = persist_and_push_message_with_id(
                                &session_manager,
                                &session_config.id,
                                &mut conversation,
                                Message::from_provider_error(provider_err),
                            )
                            .await?;
                            yield AgentEvent::Message(message);
                            break;
                        }
                        Err(ref provider_err @ ProviderError::NetworkError(_)) => {
                            provider_errored = true;
                            #[cfg(feature = "telemetry")]
                            crate::posthog::emit_error(provider_err.telemetry_type(), &provider_err.to_string());
                            error!("Error: {}", provider_err);
                            yield AgentEvent::Message(
                                Message::assistant().with_text(
                                    format!("{provider_err}\n\nPlease resend your message to try again.")
                                )
                            );
                            break;
                        }
                        Err(ref provider_err) => {
                            provider_errored = true;
                            #[cfg(feature = "telemetry")]
                            crate::posthog::emit_error(provider_err.telemetry_type(), &provider_err.to_string());
                            error!("Error: {}", provider_err);
                            yield AgentEvent::Message(
                                Message::assistant().with_text(
                                    format!("Ran into this error: {provider_err}.\n\nPlease retry if you think this is a transient or recoverable error.")
                                )
                            );
                            break;
                        }
                    }
                }
                can_drain_pending_steers = true;

                if tools_updated {
                    (tools, toolshim_tools, system_prompt, _) =
                        self.prepare_tools_and_prompt(&session_config.id, &session.working_dir).await?;
                }

                {
                    let has_new_hints = self
                        .prompt_manager
                        .lock()
                        .await
                        .load_subdirectory_hints(&working_dir);
                    if has_new_hints && !tools_updated {
                        (tools, toolshim_tools, system_prompt, _) =
                            self.prepare_tools_and_prompt(&session_config.id, &session.working_dir).await?;
                    }
                }

                // An empty provider response — no tool calls, no text, and no error
                // or recovery compaction that legitimately produces no assistant
                // output — must never be persisted: strict providers reject a
                // conversation that contains an empty assistant turn. Drop it here
                // regardless of what the match below decides to do about the turn
                // (final-output nudge, steer, goal/grind, retry, or fallback).
                let empty_response = no_tools_called
                    && !exit_chat
                    && !provider_errored
                    && !did_recovery_compact_this_iteration
                    && !provider_reached_output_token_limit
                    && !provider_produced_content
                    && last_assistant_text.is_empty();

                if empty_response {
                    messages_to_add = Conversation::default();
                } else {
                    empty_turn_retries = 0;
                }

                if no_tools_called && !exit_chat {
                    // Lock, extract state, drop guard before branching — handle_retry_logic
                    // also locks final_output_tool and tokio::sync::Mutex is not reentrant.
                    let final_output = {
                        let mut guard = self.final_output_tool.lock().await;
                        guard.as_mut().map(|fot| fot.final_output.take())
                    };

                    match final_output {
                        Some(None) => {
                            warn!("Final output tool has not been called yet. Continuing agent loop.");
                            let message = push_message_with_id(
                                &mut messages_to_add,
                                Message::user().with_text(FINAL_OUTPUT_CONTINUATION_MESSAGE),
                            );
                            yield AgentEvent::Message(message);
                        }
                        Some(Some(output)) => {
                            pending_final_output = Some(output);
                            exit_chat = true;
                        }
                        None if did_recovery_compact_this_iteration => {
                            // continue from last user message after recovery compact
                        }
                        None if self.has_pending_steers(&session_config.id).await => {}
                        None if self.goal.lock().await.is_some() && !goal_check_pending => {
                            goal_check_pending = true;
                            let goal = self.goal.lock().await.clone().unwrap();
                            let nudge = format!(
                                "Before finishing, check whether the following goal has been fully met:\n\n\
                                 **Goal:** {goal}\n\n\
                                 If not, continue working toward it."
                            );
                            let message = Message::user().with_text(&nudge)
                                .with_visibility(false, true);
                            push_message_with_id(&mut messages_to_add, message);
                            yield AgentEvent::Message(
                                Message::assistant().with_system_notification(
                                    SystemNotificationType::InlineMessage,
                                    format!("Goal: {goal}"),
                                )
                            );
                        }

                        None if self.grind.lock().await.is_some() => {
                            let grind = self.grind.lock().await.clone().unwrap();
                            let nudge = format!(
                                "Keep working. The grind goal is not yet complete:\n\n\
                                 **Goal:** {grind}\n\n\
                                 Continue until it is fully done."
                            );
                            let message = Message::user().with_text(&nudge)
                                .with_visibility(false, true);
                            push_message_with_id(&mut messages_to_add, message);
                            yield AgentEvent::Message(
                                Message::assistant().with_system_notification(
                                    SystemNotificationType::InlineMessage,
                                    format!("Grind: {grind}"),
                                )
                            );
                        }

                        None => {
                            self.set_goal(None).await;
                            self.set_grind(None).await;
                            // Recipe retry logic owns the turn whenever a
                            // retry_config is present: it runs success checks,
                            // on_failure, and max_retries. Only when no recipe
                            // retry is configured (Skipped) does the empty-turn
                            // fallback apply.
                            match self.handle_retry_logic(&mut conversation, &session_config, &initial_messages).await {
                                Ok(RetryResult::Retried) => {
                                    info!("Retry logic triggered, restarting agent loop");
                                    messages_to_add = Conversation::default();
                                    session_manager.replace_conversation(&session_config.id, &conversation).await?;
                                    yield AgentEvent::HistoryReplaced(conversation.clone());
                                }
                                Ok(RetryResult::Skipped) if empty_response => {
                                    // No recipe retry configured, and this empty
                                    // turn would otherwise fall through to a
                                    // silent exit. Retry a bounded number of
                                    // times, then surface a visible message so
                                    // the user is never left with no response.
                                    if empty_turn_retries < MAX_EMPTY_TURN_RETRIES {
                                        empty_turn_retries += 1;
                                        retrying_after_empty_turn = true;
                                        warn!(
                                            "Provider returned an empty response; retrying ({}/{})",
                                            empty_turn_retries, MAX_EMPTY_TURN_RETRIES
                                        );
                                    } else {
                                        warn!("Provider returned an empty response after retries; ending turn");
                                        last_assistant_text = EMPTY_TURN_MESSAGE.to_string();
                                        let message = push_message_with_id(
                                            &mut messages_to_add,
                                            Message::assistant().with_text(EMPTY_TURN_MESSAGE),
                                        );
                                        yield AgentEvent::Message(message);
                                        exit_chat = true;
                                    }
                                }
                                Ok(RetryResult::MaxAttemptsReached(message)) => {
                                    // Surface and persist the failure message
                                    // through the normal path so recipes don't
                                    // exit silently when retries are exhausted.
                                    let message = push_message_with_id(&mut messages_to_add, message);
                                    last_assistant_text = message.as_concat_text();
                                    yield AgentEvent::Message(message);
                                    exit_chat = true;
                                }
                                Ok(_) => {
                                    exit_chat = true;
                                }
                                Err(e) => {
                                    error!("Retry logic failed: {}", e);
                                    yield AgentEvent::Message(
                                        Message::assistant().with_text(
                                            format!("Retry logic encountered an error: {}", e)
                                        )
                                    );
                                    exit_chat = true;
                                }
                            }
                        }
                    }
                }

                if is_token_cancelled(&cancel_token) {
                    if let Some(ref task) = tool_pair_summarization_task {
                        task.abort();
                    }
                }

                if let Some(task) = tool_pair_summarization_task {
                    tool_pair_summarization_done = true;
                    if let Ok(summaries) = task.await {
                        for (summary_msg, tool_id) in summaries {
                            let matching_ids: Vec<String> = conversation.messages()
                                .iter()
                                .filter(|msg| {
                                    msg.id.is_some() && msg.content.iter().any(|c| match c {
                                        MessageContent::ToolRequest(req) => req.id == tool_id,
                                        MessageContent::ToolResponse(resp) => resp.id == tool_id,
                                        _ => false,
                                    })
                                })
                                .filter_map(|msg| msg.id.clone())
                                .collect();

                            if matching_ids.len() == 2 {
                                for id in &matching_ids {
                                    session_manager.update_message_metadata(&session_config.id, id, |metadata| {
                                        metadata.with_agent_invisible()
                                    }).await?;
                                }
                                session_manager.add_message(&session_config.id, &summary_msg).await?;
                            } else {
                                warn!("Expected a tool request/reply pair, but found {} matching messages",
                                    matching_ids.len());
                            }
                        }
                    }
                }

                if let Some(output) = pending_final_output.take() {
                    preferred_turn_usage_message_id = None;
                    last_assistant_text = output.clone();
                    let message = push_message_with_id(
                        &mut messages_to_add,
                        Message::assistant().with_text(output),
                    );
                    yield AgentEvent::Message(message);
                }

                let mut messages_to_add = if let Some(ref inference) = inference {
                    Conversation::new_unvalidated(
                        messages_to_add
                            .into_iter()
                            .map(|message| message.with_inference_if_assistant(inference.clone())),
                    )
                } else {
                    messages_to_add
                };

                if let Some(usage) = pending_turn_usage.take() {
                    if let Some((message_id, usage)) = attach_turn_usage(
                        &mut messages_to_add,
                        &usage,
                        preferred_turn_usage_message_id.as_deref(),
                    ) {
                        yield AgentEvent::MessageUsage { message_id, usage };
                    }
                }

                for msg in &messages_to_add {
                    session_manager.add_message(&session_config.id, msg).await?;
                }
                conversation.extend(messages_to_add);

                if exit_chat && self.has_pending_steers(&session_config.id).await {
                    exit_chat = false;
                }

                if exit_chat {
                    match self
                        .emit_stop_hook_blocking(&session_config.id, &last_assistant_text, &session.working_dir.to_string_lossy())
                        .await
                    {
                        crate::hooks::HookDecision::Allow => {
                            stop_hook_handled_for_exit = true;
                            break;
                        }
                        crate::hooks::HookDecision::Deny { reason, plugin } => {
                            consecutive_stop_hook_blocks += 1;
                            if consecutive_stop_hook_blocks > stop_hook_block_cap {
                                let message = persist_message_with_id(
                                    &session_manager,
                                    &session_config.id,
                                    stop_hook_block_cap_warning(&plugin, stop_hook_block_cap),
                                )
                                .await?;
                                yield AgentEvent::Message(message);
                                stop_hook_handled_for_exit = true;
                                break;
                            }
                            persist_and_push_message_with_id(
                                &session_manager,
                                &session_config.id,
                                &mut conversation,
                                stop_hook_denial_context_message(&plugin, &reason),
                            )
                            .await?;
                            yield AgentEvent::Message(stop_hook_denial_notification(&plugin));
                            retrying_after_stop_hook_denial = true;
                        }
                    }
                }

                tokio::task::yield_now().await;
            }

            if !last_assistant_text.is_empty() {
                tracing::Span::current().record("trace_output", last_assistant_text.as_str());
                if gen_ai_telemetry::capture_message_content() {
                    let output_json =
                        gen_ai_telemetry::simple_output_json(&last_assistant_text);
                    tracing::Span::current().record(
                        "gen_ai.output.messages",
                        output_json.as_str(),
                    );
                    reply_span.record("gen_ai.output.messages", output_json.as_str());
                }
            }
            gen_ai_telemetry::record_usage(&tracing::Span::current(), &turn_total_usage);
            gen_ai_telemetry::record_usage(&reply_span, &turn_total_usage);

            if !stop_hook_handled_for_exit {
                self.emit_stop_hook(&session_config.id, &last_assistant_text, &session.working_dir.to_string_lossy()).await;
            }
        }.instrument(reply_stream_span));
        Ok(inner)
    }

    pub async fn extend_system_prompt(&self, key: String, instruction: String) {
        let mut prompt_manager = self.prompt_manager.lock().await;
        prompt_manager.add_system_prompt_extra(key, instruction);
    }

    pub async fn remove_system_prompt_extra(&self, key: &str) {
        let mut prompt_manager = self.prompt_manager.lock().await;
        prompt_manager.remove_system_prompt_extra(key);
    }

    pub async fn set_goal(&self, goal: Option<String>) {
        *self.goal.lock().await = goal;
    }

    pub async fn get_goal(&self) -> Option<String> {
        self.goal.lock().await.clone()
    }

    pub async fn set_grind(&self, goal: Option<String>) {
        *self.grind.lock().await = goal;
    }

    pub async fn get_grind(&self) -> Option<String> {
        self.grind.lock().await.clone()
    }

    pub async fn update_provider(
        &self,
        provider: Arc<dyn Provider>,
        model_config: goose_providers::model::ModelConfig,
        session_id: &str,
    ) -> Result<()> {
        let provider_name = provider.get_name().to_string();

        // Normalize against the provider entry so custom/declarative providers
        // backfill `context_limit` from their known models before the config is
        // persisted as the session source of truth; otherwise auto-compaction
        // would fall back to DEFAULT_CONTEXT_LIMIT.
        let model_config = match crate::providers::get_from_registry(&provider_name).await {
            Ok(entry) => entry
                .normalize_model_config(model_config.clone())
                .unwrap_or(model_config),
            Err(_) => model_config,
        };

        let mut current_provider = self.provider.lock().await;
        *current_provider = Some(provider);

        self.config
            .session_manager
            .clone()
            .update(session_id)
            .provider_name(&provider_name)
            .model_config(model_config)
            .apply()
            .await
            .context("Failed to persist provider config to session")
    }

    pub async fn update_goose_mode(&self, mode: GooseMode, session_id: &str) -> Result<()> {
        if let Some(provider) = self.provider.lock().await.as_ref() {
            provider
                .update_mode(session_id, mode)
                .await
                .map_err(|e| anyhow::anyhow!("Provider rejected mode update: {e}"))?;
        }
        *self.current_goose_mode.lock().await = mode;
        self.config
            .session_manager
            .clone()
            .update(session_id)
            .goose_mode(mode)
            .apply()
            .await
            .context("Failed to persist goose_mode to session")
    }

    pub async fn goose_mode(&self) -> GooseMode {
        *self.current_goose_mode.lock().await
    }

    pub async fn recreate_provider_for_session(
        &self,
        session_id: &str,
        provider_name: &str,
        model_config: goose_providers::model::ModelConfig,
    ) -> Result<()> {
        let session = self
            .config
            .session_manager
            .get_session(session_id, false)
            .await
            .context("Failed to get session")?;

        let extensions = EnabledExtensionsState::extensions_or_default(
            Some(&session.extension_data),
            Config::global(),
        );

        let provider = crate::providers::create_with_working_dir(
            provider_name,
            extensions,
            session.working_dir.clone(),
        )
        .await
        .map_err(|error| provider_creation_error(error, "Could not create provider"))?;

        self.update_provider(provider, model_config, session_id)
            .await?;

        let mode = self.goose_mode().await;
        self.update_goose_mode(mode, session_id).await
    }

    pub async fn update_thinking_effort(
        &self,
        session_id: &str,
        effort: ThinkingEffort,
    ) -> Result<()> {
        let current_provider = self.provider().await?;
        let provider_name = current_provider.get_name().to_string();
        let model_config = self
            .model_config_for_session(session_id)
            .await?
            .with_thinking_effort(effort);

        self.recreate_provider_for_session(session_id, &provider_name, model_config)
            .await
    }

    /// Restore the provider from session data or fall back to global config
    /// This is used when resuming a session to restore the provider state
    /// Returns true if the session's provider was replaced with a fallback.
    pub async fn restore_provider_from_session(&self, session: &Session) -> Result<bool> {
        let config = Config::global();

        let provider_name = session
            .provider_name
            .clone()
            .or_else(|| config.get_goose_provider().ok())
            .ok_or_else(|| anyhow!("Could not configure agent: missing provider"))?;

        let mut model_config = match session.model_config.clone() {
            Some(saved_config) => saved_config,
            None => {
                let model_name = config
                    .get_goose_model()
                    .ok()
                    .ok_or_else(|| anyhow!("Could not configure agent: missing model"))?;
                crate::model_config::model_config_from_user_config(&provider_name, &model_name)
                    .map_err(|e| anyhow!("Could not configure agent: invalid model {}", e))?
            }
        };

        // if the saved model is the ACP sentinel "current", only preserve this if the provider
        // uses this sentinel to indicate it's an ACP provider that manages its model
        if model_config.model_name == crate::acp::ACP_CURRENT_MODEL {
            if let Ok(entry) = crate::providers::get_from_registry(&provider_name).await {
                if entry.metadata().default_model != crate::acp::ACP_CURRENT_MODEL {
                    model_config = crate::model_config::model_config_from_user_config(
                        &provider_name,
                        &entry.metadata().default_model,
                    )
                    .map_err(|e| anyhow!("Could not resolve default model: {}", e))?;
                }
            }
        }

        let extensions =
            EnabledExtensionsState::extensions_or_default(Some(&session.extension_data), config);

        let (provider, active_model_config, provider_changed) =
            if crate::providers::get_from_registry(&provider_name)
                .await
                .is_ok()
            {
                let p = crate::providers::create_with_working_dir(
                    &provider_name,
                    extensions,
                    session.working_dir.clone(),
                )
                .await
                .map_err(|error| provider_creation_error(error, "Could not create provider"))?;
                (p, model_config, false)
            } else {
                let fallback_provider_name = config
                    .get_goose_provider()
                    .ok()
                    .filter(|name| name != &provider_name)
                    .ok_or_else(|| {
                        anyhow!(
                            "Could not create provider: provider '{}' not found",
                            provider_name
                        )
                    })?;

                tracing::warn!(
                    "Session provider '{}' unavailable, falling back to '{}'",
                    provider_name,
                    fallback_provider_name
                );

                let fallback_model_name = config.get_goose_model().ok().ok_or_else(|| {
                    anyhow!("Could not configure fallback provider: missing model")
                })?;
                let fallback_model_config = crate::model_config::model_config_from_user_config(
                    &fallback_provider_name,
                    &fallback_model_name,
                )
                .map_err(|e| {
                    anyhow!("Could not configure fallback provider: invalid model {}", e)
                })?;

                let fallback_provider = crate::providers::create_with_working_dir(
                    &fallback_provider_name,
                    extensions,
                    session.working_dir.clone(),
                )
                .await
                .map_err(|error| {
                    provider_creation_error(
                        error,
                        format!(
                            "Could not create provider '{provider_name}' or fallback '{fallback_provider_name}'"
                        ),
                    )
                })?;

                if let Err(e) = self
                    .config
                    .session_manager
                    .update(&session.id)
                    .provider_name(&fallback_provider_name)
                    .model_config(fallback_model_config.clone())
                    .apply()
                    .await
                {
                    tracing::warn!("Failed to update session provider: {}", e);
                }

                (fallback_provider, fallback_model_config, true)
            };

        self.update_provider(provider, active_model_config, &session.id)
            .await?;
        // Propagate session mode to the new provider
        if let Some(provider) = self.provider.lock().await.as_ref() {
            provider
                .update_mode(&session.id, session.goose_mode)
                .await
                .map_err(|e| anyhow!("Failed to propagate mode to provider: {}", e))?;
        }
        *self.current_goose_mode.lock().await = session.goose_mode;
        Ok(provider_changed)
    }

    /// Override the system prompt with a custom template
    pub async fn override_system_prompt(&self, template: String) {
        let mut prompt_manager = self.prompt_manager.lock().await;
        prompt_manager.set_system_prompt_override(template);
    }

    pub async fn clear_system_prompt_override(&self) {
        let mut prompt_manager = self.prompt_manager.lock().await;
        prompt_manager.clear_system_prompt_override();
    }

    pub async fn list_extension_prompts(&self, session_id: &str) -> HashMap<String, Vec<Prompt>> {
        self.extension_manager
            .list_prompts(session_id, CancellationToken::default())
            .await
            .expect("Failed to list prompts")
    }

    pub async fn get_prompt(
        &self,
        session_id: &str,
        name: &str,
        arguments: Value,
    ) -> Result<GetPromptResult> {
        // First find which extension has this prompt
        let prompts = self
            .extension_manager
            .list_prompts(session_id, CancellationToken::default())
            .await
            .map_err(|e| anyhow!("Failed to list prompts: {}", e))?;

        if let Some(extension) = prompts
            .iter()
            .find(|(_, prompt_list)| prompt_list.iter().any(|p| p.name == name))
            .map(|(extension, _)| extension)
        {
            return self
                .extension_manager
                .get_prompt(
                    session_id,
                    extension,
                    name,
                    arguments,
                    CancellationToken::default(),
                )
                .await
                .map_err(|e| anyhow!("Failed to get prompt: {}", e));
        }

        Err(anyhow!("Prompt '{}' not found", name))
    }

    pub async fn get_plan_prompt(&self, session_id: &str) -> Result<String> {
        let tools = self
            .extension_manager
            .get_prefixed_tools(session_id, None)
            .await?;
        let tools_info = tools
            .into_iter()
            .map(|tool| {
                ToolInfo::new(
                    &tool.name,
                    tool.description
                        .as_ref()
                        .map(|d| d.as_ref())
                        .unwrap_or_default(),
                    get_parameter_names(&tool),
                    None,
                )
            })
            .collect();

        let plan_prompt = self.extension_manager.get_planning_prompt(tools_info).await;

        Ok(plan_prompt)
    }

    pub async fn handle_tool_result(&self, id: String, result: ToolResult<CallToolResult>) {
        if let Err(e) = self.tool_result_tx.send((id, result)).await {
            error!("Failed to send tool result: {}", e);
        }
    }

    pub async fn create_recipe(
        &self,
        session_id: &str,
        mut messages: Conversation,
    ) -> Result<Recipe> {
        tracing::info!("Starting recipe creation with {} messages", messages.len());

        let session = self
            .config
            .session_manager
            .get_session(session_id, false)
            .await?;
        let extensions_info = self
            .extension_manager
            .get_extensions_info(&session.working_dir)
            .await;
        tracing::debug!("Retrieved {} extensions info", extensions_info.len());

        let model_config = self.model_config_for_session(session_id).await?;
        let model_name = &model_config.model_name;
        tracing::debug!("Using model: {}", model_name);

        let goose_mode = *self.current_goose_mode.lock().await;
        let prompt_manager = self.prompt_manager.lock().await;
        let system_prompt = prompt_manager
            .builder()
            .with_extensions(extensions_info.into_iter())
            .with_frontend_instructions(self.frontend_instructions.lock().await.clone())
            .with_goose_mode(goose_mode)
            .build();

        let recipe_prompt = prompt_manager.get_recipe_prompt().await;
        let tools: Vec<_> = self
            .extension_manager
            .get_prefixed_tools(session_id, None)
            .await
            .map_err(|e| {
                tracing::error!("Failed to get tools for recipe creation: {}", e);
                e
            })?
            .into_iter()
            .filter(super::reply_parts::is_tool_visible_to_model)
            .collect();

        messages = Conversation::new_unvalidated(recipe_conversation_history(&messages));
        messages.push(Message::user().with_text(recipe_prompt));

        let (messages, issues) = fix_conversation(messages);
        if !issues.is_empty() {
            issues
                .iter()
                .for_each(|issue| tracing::warn!(recipe.conversation.issue = issue));
        }
        let messages = Conversation::new_unvalidated(merge_consecutive_messages_for_request(
            messages.messages().clone(),
        ));

        tracing::debug!(
            "Added recipe prompt to messages, total messages: {}",
            messages.len()
        );

        tracing::info!("Calling provider to generate recipe content");
        let provider = self.provider.lock().await;
        let provider = provider.as_ref().ok_or_else(|| {
            let error = anyhow!("Provider not available during recipe creation");
            tracing::error!("{}", error);
            error
        })?;
        let (result, _usage) = crate::session_context::with_session_id(
            Some(session_id.to_string()),
            provider.complete(&model_config, &system_prompt, messages.messages(), &tools),
        )
        .await
        .map_err(|e| {
            tracing::error!("Provider completion failed during recipe creation: {}", e);
            e
        })?;

        let content = result.as_concat_text();
        tracing::debug!(
            "Provider returned content with {} characters",
            content.len()
        );

        // the response may be contained in ```json ```, strip that before parsing json
        let re = Regex::new(r"(?s)```[^\n]*\n(.*?)\n```").unwrap();
        let clean_content = re
            .captures(&content)
            .and_then(|caps| caps.get(1).map(|m| m.as_str()))
            .unwrap_or(&content)
            .trim()
            .to_string();

        let (instructions, activities) =
            if let Ok(json_content) = serde_json::from_str::<Value>(&clean_content) {
                let instructions = json_content
                    .get("instructions")
                    .ok_or_else(|| anyhow!("Missing 'instructions' in json response"))?
                    .as_str()
                    .ok_or_else(|| anyhow!("instructions' is not a string"))?
                    .to_string();

                let activities = json_content
                    .get("activities")
                    .ok_or_else(|| anyhow!("Missing 'activities' in json response"))?
                    .as_array()
                    .ok_or_else(|| anyhow!("'activities' is not an array'"))?
                    .iter()
                    .map(|act| {
                        act.as_str()
                            .map(|s| s.to_string())
                            .ok_or(anyhow!("'activities' array element is not a string"))
                    })
                    .collect::<Result<_, _>>()?;

                (instructions, activities)
            } else {
                tracing::warn!("Failed to parse JSON, falling back to string parsing");
                // If we can't get valid JSON, try string parsing
                // Use split_once to get the content after "Instructions:".
                let after_instructions = content
                    .split_once("instructions:")
                    .map(|(_, rest)| rest)
                    .unwrap_or(&content);

                // Split once more to separate instructions from activities.
                let (instructions_part, activities_text) = after_instructions
                    .split_once("activities:")
                    .unwrap_or((after_instructions, ""));

                let instructions = instructions_part
                    .trim_end_matches(|c: char| c.is_whitespace() || c == '#')
                    .trim()
                    .to_string();
                let activities_text = activities_text.trim();

                // Regex to remove bullet markers or numbers with an optional dot.
                let bullet_re = Regex::new(r"^[•\-*\d]+\.?\s*").expect("Invalid regex");

                // Process each line in the activities section.
                let activities: Vec<String> = activities_text
                    .lines()
                    .map(|line| bullet_re.replace(line, "").to_string())
                    .map(|s| s.trim().to_string())
                    .filter(|line| !line.is_empty())
                    .collect();

                (instructions, activities)
            };

        let extension_configs = get_enabled_extensions();

        let author = Author {
            contact: std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .ok(),
            metadata: None,
        };

        // Ideally we'd get the name of the provider we are using from the provider itself,
        // but it doesn't know and the plumbing looks complicated.
        let config = Config::global();
        let provider_name: String = config
            .get_goose_provider()
            .expect("No provider configured. Run 'goose configure' first");

        let settings = Settings {
            goose_provider: Some(provider_name.clone()),
            goose_model: Some(model_name.clone()),
            temperature: Some(model_config.temperature.unwrap_or(0.0)),
            max_turns: None,
        };

        tracing::debug!(
            "Building recipe with {} activities and {} extensions",
            activities.len(),
            extension_configs.len()
        );

        let (title, description) =
            if let Ok(json_content) = serde_json::from_str::<Value>(&clean_content) {
                let title = json_content
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("Custom recipe from chat")
                    .to_string();

                let description = json_content
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("a custom recipe instance from this chat session")
                    .to_string();

                (title, description)
            } else {
                (
                    "Custom recipe from chat".to_string(),
                    "a custom recipe instance from this chat session".to_string(),
                )
            };

        let recipe = Recipe::builder()
            .title(title)
            .description(description)
            .instructions(instructions)
            .activities(activities)
            .extensions(extension_configs)
            .settings(settings)
            .author(author)
            .build()
            .map_err(|e| {
                tracing::error!("Failed to build recipe: {}", e);
                anyhow!("Recipe build failed: {}", e)
            })?;

        tracing::info!("Recipe creation completed successfully");
        Ok(recipe)
    }
}

fn recipe_conversation_history(messages: &Conversation) -> Vec<Message> {
    // The recipe prompt has no turn-context instructions; drop the blocks.
    messages
        .agent_visible_messages()
        .into_iter()
        .filter(|message| !message.is_turn_context())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::gen_ai_telemetry::{self, test_support::SpanFieldCapture};
    use crate::permission::permission_confirmation::PrincipalType;
    use crate::plugins::discovery::{DiscoveredPlugin, PluginScope};
    use crate::providers::base::{stream_from_single_message, MessageStream, PermissionRouting};
    use crate::recipe::Response;
    use crate::session::session_manager::SessionType;
    use goose_providers::conversation::token_usage::{ProviderUsage, Usage};
    use rmcp::model::Tool;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    #[test]
    fn provider_creation_context_preserves_acp_error_code() {
        let source = anyhow::Error::new(agent_client_protocol::Error::auth_required())
            .context("ACP session/new failed: Authentication required");

        let error = provider_creation_error(source, "Could not create provider");

        assert_eq!(
            error.to_string(),
            "Could not create provider: ACP session/new failed: Authentication required"
        );
        assert!(error.chain().any(|source| {
            source
                .downcast_ref::<agent_client_protocol::Error>()
                .is_some_and(|error| {
                    error.code == agent_client_protocol::schema::v1::ErrorCode::AuthRequired
                })
        }));
    }

    #[test]
    fn provider_session_id_comes_from_latest_inference() {
        let messages = vec![
            Message::assistant().with_inference(InferenceMetadata {
                provider: "codex-acp".to_string(),
                requested_model: "current".to_string(),
                resolved_model: None,
                provider_session_id: Some("codex-session".to_string()),
            }),
            Message::assistant().with_inference(InferenceMetadata {
                provider: "claude-acp".to_string(),
                requested_model: "current".to_string(),
                resolved_model: None,
                provider_session_id: Some("claude-session".to_string()),
            }),
        ];

        assert_eq!(
            super::super::latest_provider_session_id(&messages, "claude-acp"),
            Some("claude-session")
        );
        assert_eq!(
            super::super::latest_provider_session_id(&messages, "codex-acp"),
            None
        );
    }

    #[test]
    fn recipe_history_excludes_turn_context_events() {
        use crate::conversation::message::MessageMetadata;

        let history = Conversation::new_unvalidated([
            Message::user().with_text("build me a recipe"),
            Message::user()
                .with_text("<turn-context>cwd /repo</turn-context>")
                .with_metadata(MessageMetadata::agent_only().with_turn_context()),
            Message::assistant().with_text("on it"),
        ]);

        let texts: Vec<String> = recipe_conversation_history(&history)
            .iter()
            .map(|message| message.as_concat_text())
            .collect();
        assert_eq!(texts, ["build me a recipe", "on it"]);
    }

    async fn tracing_test_agent_and_session() -> (Agent, Session, TempDir) {
        let data_dir = TempDir::new().unwrap();
        let data_path = data_dir.path().to_path_buf();
        let session_manager = Arc::new(SessionManager::new(data_path.clone()));
        let agent = Agent::with_config(AgentConfig::new(
            Arc::clone(&session_manager),
            Arc::new(PermissionManager::new(data_path)),
            None,
            GooseMode::default(),
            false,
            GoosePlatform::GooseCli,
        ));
        let session = session_manager
            .create_session(
                std::env::current_dir().unwrap(),
                "otel-tool-span".to_string(),
                SessionType::Hidden,
                GooseMode::default(),
            )
            .await
            .unwrap();
        (agent, session, data_dir)
    }

    #[tokio::test]
    async fn tool_dispatch_records_gen_ai_span_attributes() {
        use goose_test_support::otel::clear_otel_env;
        use rmcp::object;

        let _env = clear_otel_env(&[(gen_ai_telemetry::CAPTURE_MESSAGE_CONTENT_ENV, "true")]);
        let capture = SpanFieldCapture::new("dispatch_tool_call");
        let _subscriber = capture.clone().set_default();
        let (agent, session, _data_dir) = tracing_test_agent_and_session().await;
        let tool_name =
            crate::agents::platform_extensions::scheduler::MANAGE_SCHEDULE_TOOL_NAME_COMPLETE;
        let tool_call =
            CallToolRequestParams::new(tool_name).with_arguments(object!({ "action": "list" }));

        let (request_id, result) = agent
            .dispatch_tool_call(
                tool_call,
                "call-42".to_string(),
                Some(CancellationToken::new()),
                &session,
            )
            .await;
        assert_eq!(request_id, "call-42");
        let result = result.unwrap();
        assert!(result.result.await.is_err());

        let fields = capture.fields();
        assert_eq!(fields["gen_ai.operation.name"], "execute_tool");
        assert_eq!(fields["gen_ai.tool.name"], tool_name);
        assert_eq!(fields["gen_ai.tool.call.id"], "call-42");
        assert_eq!(fields["gen_ai.conversation.id"], session.id);
        let arguments: Value =
            serde_json::from_str(fields["gen_ai.tool.call.arguments"].as_str().unwrap()).unwrap();
        assert_eq!(arguments["action"], "list");
        let output: Value = serde_json::from_str(fields["output"].as_str().unwrap()).unwrap();
        assert_eq!(output["status"], "error");
        assert!(!fields.contains_key("gen_ai.tool.call.result"));
    }

    #[tokio::test]
    async fn successful_tool_result_is_recorded_after_execution() {
        use goose_test_support::otel::clear_otel_env;
        use rmcp::model::ContentBlock;

        let _env = clear_otel_env(&[(gen_ai_telemetry::CAPTURE_MESSAGE_CONTENT_ENV, "true")]);
        let capture = SpanFieldCapture::new("successful_tool");
        let _subscriber = capture.clone().set_default();
        let (agent, session, _data_dir) = tracing_test_agent_and_session().await;
        let tool_call = CallToolRequestParams::new("test_tool");
        let span = tracing::info_span!(
            "successful_tool",
            output = tracing::field::Empty,
            gen_ai.tool.call.result = tracing::field::Empty,
        );
        let entered = span.enter();
        let result = agent.with_post_tool_hook(
            ToolCallResult::from(Ok(CallToolResult::success(vec![ContentBlock::text(
                "done",
            )]))),
            &tool_call,
            &session,
        );
        drop(entered);
        drop(span);

        assert!(result.result.await.is_ok());
        let fields = capture.fields();
        let result: Value =
            serde_json::from_str(fields["gen_ai.tool.call.result"].as_str().unwrap()).unwrap();
        assert_eq!(result["content"][0]["text"], "done");
        let output: Value = serde_json::from_str(fields["output"].as_str().unwrap()).unwrap();
        assert_eq!(output["status"], "success");
    }

    #[test]
    fn ensure_message_event_id_assigns_missing_ids_and_preserves_existing_ids() {
        let generated =
            ensure_message_event_id(AgentEvent::Message(Message::assistant().with_text("hello")));
        let AgentEvent::Message(generated_message) = generated else {
            panic!("expected message event");
        };
        let generated_id = generated_message
            .id
            .as_deref()
            .expect("generated message id");
        assert!(generated_id.starts_with("msg_"));

        let preserved = ensure_message_event_id(AgentEvent::Message(
            Message::assistant()
                .with_id("provider-message-id")
                .with_text("hello"),
        ));
        let AgentEvent::Message(preserved_message) = preserved else {
            panic!("expected message event");
        };
        assert_eq!(preserved_message.id.as_deref(), Some("provider-message-id"));

        let non_message =
            ensure_message_event_id(AgentEvent::HistoryReplaced(Conversation::empty()));
        assert!(matches!(non_message, AgentEvent::HistoryReplaced(_)));
    }

    #[test]
    fn resolve_use_login_shell_path_defaults_by_platform() {
        assert!(resolve_use_login_shell_path(
            None,
            &GoosePlatform::GooseDesktop
        ));
        assert!(!resolve_use_login_shell_path(
            None,
            &GoosePlatform::GooseCli
        ));
    }

    #[test]
    fn resolve_use_login_shell_path_explicit_overrides_platform() {
        assert!(resolve_use_login_shell_path(
            Some(true),
            &GoosePlatform::GooseCli
        ));
        assert!(!resolve_use_login_shell_path(
            Some(false),
            &GoosePlatform::GooseDesktop
        ));
    }

    #[test]
    fn user_event_projection_preserves_hidden_tool_response_wrapper() {
        use rmcp::model::{Annotations, ContentBlock, Role, TextContent};

        let hidden_only = Message::user().with_tool_response(
            "tool-1",
            Ok(CallToolResult::success(vec![ContentBlock::Text(
                TextContent::new("provider-only")
                    .with_annotations(Annotations::default().with_audience(vec![Role::Assistant])),
            )])),
        );

        let projected = project_message_for_user_event(&hidden_only);
        let result = projected.content[0]
            .as_tool_response()
            .expect("hidden tool response wrapper")
            .tool_result
            .as_ref()
            .expect("successful hidden tool result");
        assert!(result.content.is_empty());
    }

    #[test]
    fn agent_visible_message_text_excludes_user_only_blocks() {
        use rmcp::model::{Annotations, Role, TextContent};

        let user_only = TextContent::new("SECRET_USER_ONLY")
            .with_annotations(Annotations::default().with_audience(vec![Role::User]));
        let message = Message::user()
            .with_text("/goal visible objective")
            .with_content(MessageContent::Text(user_only));

        assert_eq!(
            agent_visible_message_text(&message),
            "/goal visible objective"
        );
    }

    struct ActionRequiredProvider {
        handled: tokio::sync::Mutex<Vec<(String, PermissionConfirmation)>>,
    }

    impl ActionRequiredProvider {
        fn new() -> Self {
            Self {
                handled: tokio::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl std::fmt::Debug for ActionRequiredProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("ActionRequiredProvider").finish()
        }
    }

    #[async_trait::async_trait]
    impl crate::providers::base::Provider for ActionRequiredProvider {
        fn get_name(&self) -> &str {
            "test-action-required"
        }
        async fn stream(
            &self,
            _: &goose_providers::model::ModelConfig,
            _: &str,
            _: &[crate::conversation::message::Message],
            _: &[rmcp::model::Tool],
        ) -> Result<crate::providers::base::MessageStream, ProviderError> {
            unimplemented!()
        }
        fn permission_routing(&self) -> PermissionRouting {
            PermissionRouting::ActionRequired
        }
        async fn handle_permission_confirmation(
            &self,
            request_id: &str,
            confirmation: &PermissionConfirmation,
        ) -> bool {
            self.handled
                .lock()
                .await
                .push((request_id.to_string(), confirmation.clone()));
            request_id == "known"
        }
    }

    #[tokio::test]
    async fn test_handle_confirmation_routes_to_provider() {
        let agent = Agent::new();
        let provider = Arc::new(ActionRequiredProvider::new());
        *agent.provider.lock().await =
            Some(provider.clone() as Arc<dyn crate::providers::base::Provider>);

        // Known request_id → provider handles it, confirmation_router NOT called
        agent
            .handle_confirmation(
                "known".to_string(),
                PermissionConfirmation {
                    principal_type: PrincipalType::Tool,
                    permission: crate::permission::Permission::AllowOnce,
                },
            )
            .await;
        assert_eq!(provider.handled.lock().await.len(), 1);

        // Unknown request_id → provider returns false, falls through to confirmation_router
        // Register first so deliver() has somewhere to send
        let rx = agent
            .tool_confirmation_router
            .register("unknown".to_string())
            .await;
        agent
            .handle_confirmation(
                "unknown".to_string(),
                PermissionConfirmation {
                    principal_type: PrincipalType::Tool,
                    permission: crate::permission::Permission::DenyOnce,
                },
            )
            .await;
        assert_eq!(provider.handled.lock().await.len(), 2);
        // Verify the fallthrough went to confirmation_router
        let conf = rx.await.unwrap();
        assert_eq!(conf.permission, crate::permission::Permission::DenyOnce);
    }

    #[tokio::test]
    async fn test_handle_confirmation_noop_provider() {
        let agent = Agent::new();
        // No provider set → Noop routing, goes straight to confirmation_router
        // Register first so deliver() has somewhere to send
        let rx = agent
            .tool_confirmation_router
            .register("any".to_string())
            .await;
        agent
            .handle_confirmation(
                "any".to_string(),
                PermissionConfirmation {
                    principal_type: PrincipalType::Tool,
                    permission: crate::permission::Permission::AllowOnce,
                },
            )
            .await;

        let conf = rx.await.unwrap();
        assert_eq!(conf.permission, crate::permission::Permission::AllowOnce);
    }

    const ALWAYS_BLOCK_SCRIPT: &str = r#"#!/bin/sh
echo blocked >> "$PLUGIN_ROOT/hook.log"
echo "always block" >&2
exit 2
"#;

    const ALTERNATE_BLOCK_ALLOW_SCRIPT: &str = r#"#!/bin/sh
count_file="$PLUGIN_ROOT/count"
count=0
if [ -f "$count_file" ]; then
  count=$(cat "$count_file")
fi
count=$((count + 1))
echo "$count" > "$count_file"
echo "$count" >> "$PLUGIN_ROOT/hook.log"
if [ $((count % 2)) -eq 1 ]; then
  echo "block $count" >&2
  exit 2
fi
exit 0
"#;

    const RECORD_PAYLOAD_SCRIPT: &str = r#"#!/bin/sh
cat > "$PLUGIN_ROOT/payload.json"
exit 0
"#;

    struct StopHookTestEnv {
        temp_dir: TempDir,
        hook_log: PathBuf,
        payload_path: PathBuf,
    }

    impl StopHookTestEnv {
        fn new(script: &str) -> Result<Self> {
            let temp_dir = tempfile::tempdir()?;
            let plugin_dir = temp_dir.path().join("stop-blocker");
            std::fs::create_dir_all(plugin_dir.join("hooks"))?;
            std::fs::write(
                plugin_dir.join("hooks/hooks.json"),
                r#"{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          { "type": "command", "command": "sh ${PLUGIN_ROOT}/block.sh" }
        ]
      }
    ]
  }
}
"#,
            )?;
            std::fs::write(plugin_dir.join("block.sh"), script)?;

            Ok(Self {
                temp_dir,
                hook_log: plugin_dir.join("hook.log"),
                payload_path: plugin_dir.join("payload.json"),
            })
        }

        fn hook_manager(&self) -> crate::hooks::HookManager {
            crate::hooks::HookManager::from_plugins_for_test(vec![DiscoveredPlugin {
                name: "stop-blocker".into(),
                root: self.temp_dir.path().join("stop-blocker"),
                scope: PluginScope::Project,
            }])
        }

        fn data_dir(&self) -> PathBuf {
            self.temp_dir.path().join("data")
        }

        fn hook_invocations(&self) -> usize {
            std::fs::read_to_string(&self.hook_log)
                .unwrap_or_default()
                .lines()
                .count()
        }

        fn stop_payload(&self) -> Result<Value> {
            let payload = std::fs::read_to_string(&self.payload_path)?;
            Ok(serde_json::from_str(&payload)?)
        }
    }

    struct SessionStartHookTestEnv {
        temp_dir: TempDir,
        hook_log: PathBuf,
    }

    impl SessionStartHookTestEnv {
        fn new() -> Result<Self> {
            let temp_dir = tempfile::tempdir()?;
            let plugin_dir = temp_dir.path().join("session-start");
            std::fs::create_dir_all(plugin_dir.join("hooks"))?;
            std::fs::write(
                plugin_dir.join("hooks/hooks.json"),
                r#"{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          { "type": "command", "command": "sh ${PLUGIN_ROOT}/start.sh" }
        ]
      }
    ]
  }
}
"#,
            )?;
            std::fs::write(
                plugin_dir.join("start.sh"),
                r#"#!/bin/sh
echo start >> "$PLUGIN_ROOT/hook.log"
"#,
            )?;

            Ok(Self {
                temp_dir,
                hook_log: plugin_dir.join("hook.log"),
            })
        }

        fn hook_manager(&self) -> crate::hooks::HookManager {
            crate::hooks::HookManager::from_plugins_for_test(vec![DiscoveredPlugin {
                name: "session-start".into(),
                root: self.temp_dir.path().join("session-start"),
                scope: PluginScope::Project,
            }])
        }

        fn data_dir(&self) -> PathBuf {
            self.temp_dir.path().join("data")
        }

        fn hook_invocations(&self) -> usize {
            std::fs::read_to_string(&self.hook_log)
                .unwrap_or_default()
                .lines()
                .count()
        }
    }

    struct CountingTextProvider {
        call_count: AtomicUsize,
    }

    impl CountingTextProvider {
        fn new() -> Self {
            Self {
                call_count: AtomicUsize::new(0),
            }
        }

        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl crate::providers::base::Provider for CountingTextProvider {
        async fn stream(
            &self,
            _model_config: &goose_providers::model::ModelConfig,
            _system_prompt: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<MessageStream, ProviderError> {
            let call = self.call_count.fetch_add(1, Ordering::SeqCst);
            let message = Message::assistant().with_text(format!("provider response {call}"));
            let usage = ProviderUsage::new("mock-model".to_string(), Usage::default());
            Ok(stream_from_single_message(message, usage))
        }

        fn get_name(&self) -> &str {
            "counting-text"
        }
    }

    struct ChunkedTextProvider;

    #[async_trait::async_trait]
    impl crate::providers::base::Provider for ChunkedTextProvider {
        async fn stream(
            &self,
            _model_config: &goose_providers::model::ModelConfig,
            _system_prompt: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<MessageStream, ProviderError> {
            let usage = ProviderUsage::new("mock-model".to_string(), Usage::default());
            Ok(Box::pin(futures::stream::iter(vec![
                Ok((Some(Message::assistant().with_text("streamed ")), None)),
                Ok((
                    Some(Message::assistant().with_text("assistant reply")),
                    Some(usage),
                )),
            ])))
        }

        fn get_name(&self) -> &str {
            "chunked-text"
        }
    }

    struct OutputLimitMarkerProvider {
        include_content: bool,
        call_count: AtomicUsize,
    }

    impl OutputLimitMarkerProvider {
        fn new(include_content: bool) -> Self {
            Self {
                include_content,
                call_count: AtomicUsize::new(0),
            }
        }

        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl crate::providers::base::Provider for OutputLimitMarkerProvider {
        async fn stream(
            &self,
            _model_config: &goose_providers::model::ModelConfig,
            _system_prompt: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<MessageStream, ProviderError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let message_id = "provider-output-limit";
            let content = Message::assistant()
                .with_text("Partial answer")
                .with_id(message_id);
            let mut marker = Message::assistant().with_id(message_id);
            marker.metadata.output_token_limit_reached = true;
            let usage = ProviderUsage::new("mock-model".to_string(), Usage::default());

            let mut events = Vec::new();
            if self.include_content {
                events.push(Ok((Some(content), None)));
            }
            events.push(Ok((Some(marker), Some(usage))));
            Ok(Box::pin(futures::stream::iter(events)))
        }

        fn get_name(&self) -> &str {
            "output-limit-marker"
        }
    }

    struct RefusingProvider {
        call_count: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl crate::providers::base::Provider for RefusingProvider {
        async fn stream(
            &self,
            _model_config: &goose_providers::model::ModelConfig,
            _system_prompt: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<MessageStream, ProviderError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(Box::pin(futures::stream::once(async {
                Err(ProviderError::Refusal {
                    details: "This request was declined.".to_string(),
                    category: Some("cyber".to_string()),
                })
            })))
        }

        fn get_name(&self) -> &str {
            "refusing"
        }
    }

    #[tokio::test]
    async fn refusal_exits_turn_without_recipe_retry() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let provider = Arc::new(RefusingProvider {
            call_count: AtomicUsize::new(0),
        });
        let hook_manager = crate::hooks::HookManager::from_plugins_for_test(vec![]);
        let (agent, session_id) =
            create_test_agent(temp_dir.path().join("data"), hook_manager, provider.clone()).await?;

        let session_config = SessionConfig {
            id: session_id,
            schedule_id: None,
            max_turns: Some(10),
            retry_config: Some(crate::agents::types::RetryConfig {
                max_retries: 3,
                checks: vec![crate::agents::types::SuccessCheck::Shell {
                    command: "false".to_string(),
                }],
                on_failure: None,
                timeout_seconds: None,
                on_failure_timeout_seconds: None,
            }),
        };

        let reply_stream = agent
            .reply(Message::user().with_text("hi"), session_config, None)
            .await?;
        tokio::pin!(reply_stream);
        let mut emitted_refusal_id = None;
        while let Some(event) = reply_stream.next().await {
            if let AgentEvent::Message(message) = event? {
                if message.as_concat_text().contains("provider refused") {
                    emitted_refusal_id = message.id;
                }
            }
        }

        assert_eq!(
            provider.call_count.load(Ordering::SeqCst),
            1,
            "a refused request must not be resent"
        );
        let emitted_refusal_id =
            emitted_refusal_id.expect("refusal message should be emitted with an ID");
        assert!(emitted_refusal_id.starts_with("msg_"));
        Ok(())
    }

    async fn create_test_agent(
        data_dir: PathBuf,
        hook_manager: crate::hooks::HookManager,
        provider: Arc<dyn crate::providers::base::Provider>,
    ) -> Result<(Agent, String)> {
        let session_manager = Arc::new(SessionManager::new(data_dir.clone()));
        let permission_manager = Arc::new(PermissionManager::new(data_dir));
        let config = AgentConfig::new(
            session_manager.clone(),
            permission_manager,
            None,
            GooseMode::Auto,
            true,
            GoosePlatform::GooseCli,
        );
        let mut agent = Agent::with_config(config);
        agent.set_hook_manager_for_test(hook_manager);
        let session = session_manager
            .create_session(
                PathBuf::default(),
                "test".to_string(),
                SessionType::Hidden,
                GooseMode::Auto,
            )
            .await?;
        agent
            .update_provider(
                provider,
                goose_providers::model::ModelConfig::new("mock-model"),
                &session.id,
            )
            .await?;
        Ok((agent, session.id))
    }

    async fn create_stop_hook_test_agent(
        env: &StopHookTestEnv,
        stop_hook_block_cap: u32,
    ) -> Result<(Agent, String, Arc<CountingTextProvider>)> {
        let provider = Arc::new(CountingTextProvider::new());
        let (mut agent, session_id) =
            create_test_agent(env.data_dir(), env.hook_manager(), provider.clone()).await?;
        agent.set_stop_hook_block_cap_for_test(stop_hook_block_cap);
        Ok((agent, session_id, provider))
    }

    async fn run_stop_hook_test_turn(
        agent: &Agent,
        session_id: &str,
        text: &str,
    ) -> Result<Vec<Message>> {
        let session_config = SessionConfig {
            id: session_id.to_string(),
            schedule_id: None,
            max_turns: Some(10),
            retry_config: None,
        };
        let reply_stream = agent
            .reply(Message::user().with_text(text), session_config, None)
            .await?;
        tokio::pin!(reply_stream);

        let mut messages = Vec::new();
        while let Some(event) = reply_stream.next().await {
            match event? {
                AgentEvent::Message(message) => messages.push(message),
                AgentEvent::McpNotification(_)
                | AgentEvent::HistoryReplaced(_)
                | AgentEvent::Usage(_)
                | AgentEvent::MessageUsage { .. } => {}
            }
        }
        Ok(messages)
    }

    fn visible_texts(messages: &[Message]) -> Vec<String> {
        messages
            .iter()
            .map(Message::as_concat_text)
            .filter(|text| !text.is_empty())
            .collect()
    }

    #[tokio::test]
    async fn output_limit_marker_is_emitted_and_persisted() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let hook_manager = crate::hooks::HookManager::from_plugins_for_test(vec![]);
        let provider = Arc::new(OutputLimitMarkerProvider::new(true));
        let (agent, session_id) =
            create_test_agent(temp_dir.path().join("data"), hook_manager, provider).await?;

        let messages = run_stop_hook_test_turn(&agent, &session_id, "hello").await?;
        let marker = messages
            .iter()
            .find(|message| message.metadata.output_token_limit_reached)
            .expect("output-limit marker should be emitted");
        assert!(marker.content.is_empty());
        assert_eq!(marker.id.as_deref(), Some("provider-output-limit"));

        let session = agent
            .config
            .session_manager
            .get_session(&session_id, true)
            .await?;
        let conversation = session
            .conversation
            .expect("session should have a conversation");
        let persisted = conversation
            .messages()
            .iter()
            .find(|message| message.id.as_deref() == Some("provider-output-limit"))
            .expect("provider response should be persisted");
        assert_eq!(persisted.as_concat_text(), "Partial answer");
        assert!(persisted.metadata.output_token_limit_reached);
        assert!(persisted.metadata.usage.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn zero_content_output_limit_is_persisted_without_empty_response_retry() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let hook_manager = crate::hooks::HookManager::from_plugins_for_test(vec![]);
        let provider = Arc::new(OutputLimitMarkerProvider::new(false));
        let (agent, session_id) =
            create_test_agent(temp_dir.path().join("data"), hook_manager, provider.clone()).await?;

        run_stop_hook_test_turn(&agent, &session_id, "hello").await?;

        assert_eq!(provider.call_count(), 1);
        let session = agent
            .config
            .session_manager
            .get_session(&session_id, true)
            .await?;
        let conversation = session
            .conversation
            .expect("session should have a conversation");
        let persisted = conversation
            .messages()
            .iter()
            .find(|message| message.id.as_deref() == Some("provider-output-limit"))
            .expect("zero-content output-limit marker should be persisted");
        assert!(persisted.content.is_empty());
        assert!(persisted.metadata.user_visible);
        assert!(!persisted.metadata.agent_visible);
        assert!(persisted.metadata.output_token_limit_reached);
        assert!(conversation
            .agent_visible_messages()
            .iter()
            .all(|message| message.id.as_deref() != Some("provider-output-limit")));

        Ok(())
    }

    #[tokio::test]
    async fn session_start_hook_emits_once_for_first_reply_turn() -> Result<()> {
        let env = SessionStartHookTestEnv::new()?;
        let provider = Arc::new(CountingTextProvider::new());
        let (agent, session_id) =
            create_test_agent(env.data_dir(), env.hook_manager(), provider.clone()).await?;

        run_stop_hook_test_turn(&agent, &session_id, "first").await?;
        run_stop_hook_test_turn(&agent, &session_id, "second").await?;

        assert_eq!(env.hook_invocations(), 1);
        assert_eq!(provider.call_count(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn skipped_user_message_does_not_enter_empty_response_retry_loop() -> Result<()> {
        use rmcp::model::{Annotations, Role, TextContent};

        let env = SessionStartHookTestEnv::new()?;
        let provider = Arc::new(CountingTextProvider::new());
        let hook_manager = env.hook_manager();
        let (agent, session_id) =
            create_test_agent(env.data_dir(), hook_manager, provider.clone()).await?;
        let session_config = SessionConfig {
            id: session_id.clone(),
            schedule_id: None,
            max_turns: Some(10),
            retry_config: None,
        };
        let user_only_content = MessageContent::Text(
            TextContent::new("user-only")
                .with_annotations(Annotations::default().with_audience(vec![Role::User])),
        );

        let mut stream = agent
            .reply(
                Message::user().with_content(user_only_content),
                session_config,
                None,
            )
            .await?;

        assert!(stream.next().await.is_none());
        assert_eq!(provider.call_count.load(Ordering::SeqCst), 0);
        assert_eq!(env.hook_invocations(), 0);
        let session = agent
            .config
            .session_manager
            .get_session(&session_id, true)
            .await?;
        let conversation = session.conversation.unwrap();
        assert_eq!(conversation.messages().len(), 1);
        assert!(!conversation.messages()[0].is_agent_visible());

        let visible_session_config = SessionConfig {
            id: session_id.clone(),
            schedule_id: None,
            max_turns: Some(10),
            retry_config: None,
        };
        let mut visible_stream = agent
            .reply(
                Message::user().with_text("agent-visible"),
                visible_session_config,
                None,
            )
            .await?;
        while let Some(event) = visible_stream.next().await {
            event?;
        }
        assert_eq!(provider.call_count.load(Ordering::SeqCst), 1);
        assert_eq!(env.hook_invocations(), 1);

        let final_session_config = SessionConfig {
            id: session_id,
            schedule_id: None,
            max_turns: Some(10),
            retry_config: None,
        };
        let mut final_stream = agent
            .reply(
                Message::user().with_text("second-agent-visible"),
                final_session_config,
                None,
            )
            .await?;
        while let Some(event) = final_stream.next().await {
            event?;
        }
        assert_eq!(provider.call_count.load(Ordering::SeqCst), 2);
        assert_eq!(env.hook_invocations(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn stop_hook_block_cap_allows_configured_consecutive_blocks_then_overrides() -> Result<()>
    {
        let env = StopHookTestEnv::new(ALWAYS_BLOCK_SCRIPT)?;
        let (agent, session_id, provider) = create_stop_hook_test_agent(&env, 2).await?;

        let messages = run_stop_hook_test_turn(&agent, &session_id, "hello").await?;
        let texts = visible_texts(&messages);

        assert_eq!(
            provider.call_count(),
            3,
            "cap=2 should allow two blocked retries, then override on the third block"
        );
        assert_eq!(
            env.hook_invocations(),
            3,
            "Stop hook should run for the initial response plus the two honored retries"
        );
        assert!(texts.iter().any(|text| text == "provider response 0"));
        assert!(texts.iter().any(|text| text == "provider response 1"));
        assert!(texts.iter().any(|text| text == "provider response 2"));
        assert!(messages.iter().any(|message| {
            message.content.iter().any(|content| {
                matches!(
                    content,
                    MessageContent::SystemNotification(notification)
                        if notification.msg.contains("more than 2 consecutive times")
                            && notification.msg.contains("GOOSE_STOP_HOOK_BLOCK_CAP")
                )
            })
        }));

        let stored_session = agent
            .config
            .session_manager
            .get_session(&session_id, true)
            .await?;
        let stored_messages = stored_session
            .conversation
            .expect("session should have stored conversation");
        let stop_hook_context_messages = stored_messages
            .messages()
            .iter()
            .filter(|message| {
                message.role == rmcp::model::Role::User
                    && !message.is_user_visible()
                    && message.is_agent_visible()
                    && message
                        .as_concat_text()
                        .contains("Address this policy hook denial")
            })
            .collect::<Vec<_>>();
        assert_eq!(stop_hook_context_messages.len(), 2);
        assert!(stop_hook_context_messages.iter().all(|message| {
            message
                .id
                .as_deref()
                .is_some_and(|id| id.starts_with("msg_"))
        }));

        Ok(())
    }

    #[tokio::test]
    async fn stop_hook_block_cap_counts_only_consecutive_blocks() -> Result<()> {
        let env = StopHookTestEnv::new(ALTERNATE_BLOCK_ALLOW_SCRIPT)?;
        let (agent, session_id, provider) = create_stop_hook_test_agent(&env, 1).await?;

        let first_turn = run_stop_hook_test_turn(&agent, &session_id, "first").await?;
        let second_turn = run_stop_hook_test_turn(&agent, &session_id, "second").await?;
        let mut texts = visible_texts(&first_turn);
        texts.extend(visible_texts(&second_turn));

        assert_eq!(
            provider.call_count(),
            4,
            "each turn should honor one block, retry, then stop when the next Stop hook allows"
        );
        assert_eq!(env.hook_invocations(), 4);
        assert!(texts.iter().any(|text| text == "provider response 0"));
        assert!(texts.iter().any(|text| text == "provider response 1"));
        assert!(texts.iter().any(|text| text == "provider response 2"));
        assert!(texts.iter().any(|text| text == "provider response 3"));
        assert!(
            !texts
                .iter()
                .any(|text| text.contains("overriding and ending turn")),
            "non-consecutive Stop hook blocks should not trip the cap warning"
        );

        Ok(())
    }

    #[tokio::test]
    async fn stop_hook_payload_includes_streamed_assistant_reply_text() -> Result<()> {
        let env = StopHookTestEnv::new(RECORD_PAYLOAD_SCRIPT)?;
        let provider = Arc::new(ChunkedTextProvider);
        let (agent, session_id) =
            create_test_agent(env.data_dir(), env.hook_manager(), provider).await?;

        let messages = run_stop_hook_test_turn(&agent, &session_id, "hello").await?;
        let texts = visible_texts(&messages);
        assert_eq!(texts.join(""), "streamed assistant reply");

        let payload = env.stop_payload()?;
        assert_eq!(payload.get("event").and_then(Value::as_str), Some("Stop"));
        assert_eq!(
            payload.get("session_id").and_then(Value::as_str),
            Some(session_id.as_str())
        );
        assert_eq!(
            payload
                .get("last_assistant_message")
                .and_then(Value::as_str),
            Some("streamed assistant reply")
        );
        assert!(payload.get("message").is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_final_output_tool() -> Result<()> {
        let agent = Agent::new();

        let response = Response {
            json_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "result": {"type": "string"}
                }
            })),
        };

        agent.add_final_output_tool(response).await;

        let tools = agent.list_tools("test-session-id", None).await;
        let final_output_tool = tools
            .iter()
            .find(|tool| tool.name == FINAL_OUTPUT_TOOL_NAME);

        assert!(
            final_output_tool.is_some(),
            "Final output tool should be present after adding"
        );

        let prompt_manager = agent.prompt_manager.lock().await;
        let system_prompt = prompt_manager
            .builder()
            .with_goose_mode(GooseMode::default())
            .build();

        let final_output_tool_ref = agent.final_output_tool.lock().await;
        let final_output_tool_system_prompt =
            final_output_tool_ref.as_ref().unwrap().system_prompt();
        assert!(system_prompt.contains(&final_output_tool_system_prompt));
        Ok(())
    }

    #[tokio::test]
    async fn test_tool_inspection_manager_has_all_inspectors() -> Result<()> {
        let agent = Agent::new();

        // Verify that the tool inspection manager has all expected inspectors
        let inspector_names = agent.tool_inspection_manager.inspector_names();

        assert!(
            inspector_names.contains(&"repetition"),
            "Tool inspection manager should contain repetition inspector"
        );
        assert!(
            inspector_names.contains(&"permission"),
            "Tool inspection manager should contain permission inspector"
        );
        assert!(
            inspector_names.contains(&"security"),
            "Tool inspection manager should contain security inspector"
        );
        assert!(
            inspector_names.contains(&"adversary"),
            "Tool inspection manager should contain adversary inspector"
        );

        Ok(())
    }

    #[tokio::test]
    async fn discard_pending_steers_clears_queued_messages() {
        let agent = Agent::new();
        let session_id = "session-discard";

        agent
            .steer(session_id, Message::user().with_text("queued steer"))
            .await;
        assert!(agent.has_pending_steers(session_id).await);

        agent.discard_pending_steers(session_id).await;

        assert!(
            !agent.has_pending_steers(session_id).await,
            "discarding must drop steers orphaned by a cancelled run so they cannot leak into a later prompt"
        );
        assert!(agent.drain_pending_steers(session_id).await.is_empty());
    }

    #[test]
    fn categorize_tool_recognizes_conventional_names() {
        assert_eq!(categorize_tool("developer__shell"), ToolCategory::Shell);
        assert_eq!(categorize_tool("filesystem__write"), ToolCategory::Write);
        assert_eq!(categorize_tool("filesystem__edit"), ToolCategory::Write);
        assert_eq!(categorize_tool("filesystem__read"), ToolCategory::Read);
        assert_eq!(categorize_tool("filesystem__view"), ToolCategory::Read);
        assert_eq!(categorize_tool("filesystem__cat"), ToolCategory::Read);
        assert_eq!(categorize_tool("scheduler__list"), ToolCategory::Other);
        assert_eq!(categorize_tool("shell"), ToolCategory::Shell);
    }

    #[test]
    fn extract_string_arg_picks_first_present_key() {
        let input = serde_json::json!({ "file_path": "/tmp/a.txt", "path": "/tmp/b.txt" });
        assert_eq!(
            extract_string_arg(&input, &["path", "file", "file_path"]).as_deref(),
            Some("/tmp/b.txt")
        );
        let input = serde_json::json!({ "file_path": "/tmp/a.txt" });
        assert_eq!(
            extract_string_arg(&input, &["path", "file", "file_path"]).as_deref(),
            Some("/tmp/a.txt")
        );
        let input = serde_json::json!({ "other": 1 });
        assert!(extract_string_arg(&input, &["path"]).is_none());
        let input = serde_json::json!({ "path": "" });
        assert!(extract_string_arg(&input, &["path"]).is_none());
    }

    #[test]
    fn attach_turn_usage_targets_last_assistant_message() {
        let usage = ProviderUsage::new(
            "test-model".to_string(),
            Usage::new(Some(1200), Some(340), None),
        );
        let mut conversation = Conversation::new_unvalidated([
            Message::user().with_text("hi"),
            Message::assistant().with_id("a1").with_text("first"),
            Message::user().with_text("again"),
            Message::assistant().with_id("a2").with_text("second"),
        ]);

        let (message_id, attached) =
            attach_turn_usage(&mut conversation, &usage, None).expect("usage should attach");

        assert_eq!(message_id.as_deref(), Some("a2"));
        assert_eq!(attached.input_tokens, Some(1200));
        assert_eq!(attached.output_tokens, Some(340));
        assert!(!attached.is_compaction, "turn usage is not a compaction");

        let messages = conversation.messages();
        let stored = messages[3]
            .metadata
            .usage
            .as_deref()
            .expect("usage must be stored on the last assistant message");
        assert_eq!(*stored, attached);
        assert!(
            messages[1].metadata.usage.is_none(),
            "earlier assistant message must not receive the usage"
        );
    }

    #[test]
    fn attach_turn_usage_returns_none_without_assistant_message() {
        let usage = ProviderUsage::new("test-model".to_string(), Usage::default());
        let mut conversation = Conversation::new_unvalidated([Message::user().with_text("hi")]);

        assert!(attach_turn_usage(&mut conversation, &usage, None).is_none());
        assert!(
            conversation.messages()[0].metadata.usage.is_none(),
            "user message must stay untouched"
        );
    }

    #[test]
    fn attach_turn_usage_suppresses_notification_for_assistant_only_message() {
        use rmcp::model::{Annotations, Role, TextContent};

        let usage = ProviderUsage::new(
            "test-model".to_string(),
            Usage::new(Some(1200), Some(340), None),
        );
        let assistant_only = TextContent::new("provider-only state")
            .with_annotations(Annotations::default().with_audience(vec![Role::Assistant]));
        let mut conversation = Conversation::new_unvalidated([
            Message::user().with_text("hi"),
            Message::assistant()
                .with_id("hidden")
                .with_content(MessageContent::Text(assistant_only)),
        ]);

        assert!(attach_turn_usage(&mut conversation, &usage, None).is_none());

        let stored = conversation.messages()[1]
            .metadata
            .usage
            .as_deref()
            .expect("usage must remain stored on the hidden assistant message");
        assert_eq!(stored.input_tokens, Some(1200));
        assert_eq!(stored.output_tokens, Some(340));
    }
}
