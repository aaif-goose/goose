use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use futures::stream::BoxStream;
use futures::{stream, FutureExt, Stream, StreamExt, TryStreamExt};
use tracing_futures::Instrument;
use uuid::Uuid;

use super::container::Container;
use super::final_output_tool::FinalOutputTool;
use super::platform_tools;
use super::tool_confirmation_router::ToolConfirmationRouter;
use super::tool_execution::{ToolCallResult, CHAT_MODE_TOOL_SKIPPED_RESPONSE, DECLINED_RESPONSE};
use crate::action_required_manager::ActionRequiredManager;
use crate::agents::extension::{ExtensionConfig, ExtensionResult, ToolInfo};
use crate::agents::extension_manager::{
    get_parameter_names, ExtensionManager, ExtensionManagerCapabilities,
};
use crate::agents::final_output_tool::{FINAL_OUTPUT_CONTINUATION_MESSAGE, FINAL_OUTPUT_TOOL_NAME};
use crate::agents::platform_extensions::MANAGE_EXTENSIONS_TOOL_NAME_COMPLETE;
use crate::agents::platform_tools::PLATFORM_MANAGE_SCHEDULE_TOOL_NAME;
use crate::agents::prompt_manager::PromptManager;
use crate::agents::retry::{RetryManager, RetryResult};
use crate::agents::types::{FrontendTool, SessionConfig, SharedProvider, ToolResultReceiver};
use crate::config::permission::PermissionManager;
use crate::config::{get_enabled_extensions, Config, GooseMode};
use crate::context_mgmt::{
    check_if_compaction_needed, compact_messages, DEFAULT_COMPACTION_THRESHOLD,
};
use crate::conversation::message::{
    ActionRequiredData, Message, MessageContent, ProviderMetadata, SystemNotificationType,
    ToolRequest,
};
use crate::conversation::{debug_conversation_fix, fix_conversation, Conversation};
use crate::mcp_utils::ToolResult;
use crate::permission::permission_inspector::PermissionInspector;
use crate::permission::permission_judge::PermissionCheckResult;
use crate::permission::PermissionConfirmation;
use crate::providers::base::{PermissionRouting, Provider};
use crate::providers::errors::ProviderError;
use crate::recipe::{Author, Recipe, Response, Settings};
use crate::scheduler_trait::SchedulerTrait;
use crate::security::adversary_inspector::AdversaryInspector;
use crate::security::egress_inspector::EgressInspector;
use crate::security::security_inspector::SecurityInspector;
use crate::session::extension_data::{EnabledExtensionsState, ExtensionState};
use crate::session::{Session, SessionManager};
use crate::tool_inspection::ToolInspectionManager;
use crate::tool_monitor::RepetitionInspector;
use crate::utils::is_token_cancelled;
use regex::Regex;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, ErrorCode, ErrorData, GetPromptResult, Prompt,
    ServerNotification, Tool,
};
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

const DEFAULT_MAX_TURNS: u32 = 1000;
const COMPACTION_THINKING_TEXT: &str = "goose is compacting the conversation...";

/// Context needed for the reply function
pub struct ReplyContext {
    pub conversation: Conversation,
    pub tools: Vec<Tool>,
    pub toolshim_tools: Vec<Tool>,
    pub system_prompt: String,
    pub goose_mode: GooseMode,
    pub tool_call_cut_off: usize,
    pub initial_messages: Vec<Message>,
}

pub struct ToolCategorizeResult {
    pub frontend_requests: Vec<ToolRequest>,
    pub remaining_requests: Vec<ToolRequest>,
    pub filtered_response: Message,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
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
        }
    }
}

/// The main goose Agent
pub struct Agent {
    pub(super) provider: SharedProvider,
    pub config: AgentConfig,
    pub(super) current_goose_mode: Mutex<GooseMode>,

    pub extension_manager: Arc<ExtensionManager>,
    pub(super) final_output_tool: Arc<Mutex<Option<FinalOutputTool>>>,
    pub(super) frontend_tools: Mutex<HashMap<String, FrontendTool>>,
    pub(super) frontend_instructions: Mutex<Option<String>>,
    pub(super) prompt_manager: Mutex<PromptManager>,
    pub tool_confirmation_router: ToolConfirmationRouter,
    pub(super) tool_result_tx: mpsc::Sender<(String, ToolResult<CallToolResult>)>,
    pub(super) tool_result_rx: ToolResultReceiver,

    pub(super) retry_manager: RetryManager,
    pub(super) tool_inspection_manager: ToolInspectionManager,
    container: Mutex<Option<Container>>,
}

#[derive(Clone, Debug)]
pub enum AgentEvent {
    Message(Message),
    McpNotification((String, ServerNotification)),
    HistoryReplaced(Conversation),
}

impl Default for Agent {
    fn default() -> Self {
        Self::new()
    }
}

pub enum ToolStreamItem<T> {
    Message(ServerNotification),
    Result(T),
}

pub type ToolStream =
    Pin<Box<dyn Stream<Item = ToolStreamItem<ToolResult<CallToolResult>>> + Send>>;

// tool_stream combines a stream of ServerNotifications with a future representing the
// final result of the tool call. MCP notifications are not request-scoped, but
// this lets us capture all notifications emitted during the tool call for
// simpler consumption
pub fn tool_stream<S, F>(rx: S, done: F) -> ToolStream
where
    S: Stream<Item = ServerNotification> + Send + Unpin + 'static,
    F: Future<Output = ToolResult<CallToolResult>> + Send + 'static,
{
    Box::pin(async_stream::stream! {
        tokio::pin!(done);
        let mut rx = rx;

        loop {
            tokio::select! {
                Some(msg) = rx.next() => {
                    yield ToolStreamItem::Message(msg);
                }
                r = &mut done => {
                    yield ToolStreamItem::Result(r);
                    break;
                }
            }
        }
    })
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

        let goose_platform = config.goose_platform.clone();
        let initial_mode = config.goose_mode;
        let capabilities = match config.goose_platform {
            GoosePlatform::GooseDesktop => ExtensionManagerCapabilities { mcpui: true },
            GoosePlatform::GooseCli => ExtensionManagerCapabilities { mcpui: false },
        };
        let session_manager = Arc::clone(&config.session_manager);
        let permission_manager = Arc::clone(&config.permission_manager);
        Self {
            provider: provider.clone(),
            config,
            current_goose_mode: Mutex::new(initial_mode),
            extension_manager: Arc::new(ExtensionManager::new(
                provider.clone(),
                session_manager,
                goose_platform.to_string(),
                capabilities,
            )),
            final_output_tool: Arc::new(Mutex::new(None)),
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
            ),
            container: Mutex::new(None),
        }
    }

    /// Create a tool inspection manager with default inspectors
    fn create_tool_inspection_manager(
        permission_manager: Arc<PermissionManager>,
        provider: SharedProvider,
    ) -> ToolInspectionManager {
        let mut tool_inspection_manager = ToolInspectionManager::new();

        // Add security inspector (highest priority - runs first)
        tool_inspection_manager.add_inspector(Box::new(SecurityInspector::new()));
        tool_inspection_manager.add_inspector(Box::new(EgressInspector::new()));

        // Add adversary inspector (LLM-based review, enabled by ~/.config/goose/adversary.md)
        tool_inspection_manager.add_inspector(Box::new(AdversaryInspector::new(provider.clone())));

        // Add permission inspector (medium-high priority)
        tool_inspection_manager.add_inspector(Box::new(PermissionInspector::new(
            permission_manager,
            provider,
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
    ) -> Result<bool> {
        let result = self
            .retry_manager
            .handle_retry_logic(
                messages,
                session_config,
                initial_messages,
                &self.final_output_tool,
            )
            .await?;

        match result {
            RetryResult::Retried => Ok(true),
            RetryResult::Skipped
            | RetryResult::MaxAttemptsReached
            | RetryResult::SuccessChecksPassed => Ok(false),
        }
    }
    async fn drain_elicitation_messages(&self, session_id: &str) -> Vec<Message> {
        let mut messages = Vec::new();
        let manager = self.config.session_manager.clone();
        let mut elicitation_rx = ActionRequiredManager::global().request_rx.lock().await;
        while let Ok(mut elicitation_message) = elicitation_rx.try_recv() {
            if elicitation_message.id.is_none() {
                elicitation_message = elicitation_message.with_generated_id();
            }
            if let Err(e) = manager.add_message(session_id, &elicitation_message).await {
                warn!("Failed to save elicitation message to session: {}", e);
            }
            messages.push(elicitation_message);
        }
        messages
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
        let initial_messages = conversation.messages().clone();

        let (tools, toolshim_tools, system_prompt) = self
            .prepare_tools_and_prompt(session_id, working_dir)
            .await?;

        let goose_mode = *self.current_goose_mode.lock().await;

        if goose_mode == GooseMode::SmartApprove {
            self.tool_inspection_manager.apply_tool_annotations(&tools);
        }

        let tool_call_cut_off = match Config::global().get_param::<usize>("GOOSE_TOOL_CALL_CUTOFF")
        {
            Ok(v) => v,
            Err(_) => {
                let context_limit = self
                    .provider()
                    .await
                    .map(|p| p.get_model_config().context_limit())
                    .unwrap_or(crate::model::DEFAULT_CONTEXT_LIMIT);
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
            initial_messages,
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
                            result.result,
                        ),
                        Err(e) => {
                            tool_stream(Box::new(stream::empty()), futures::future::ready(Err(e)))
                        }
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
                    Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                        DECLINED_RESPONSE,
                    )])),
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
    #[instrument(skip(self, tool_call, request_id, cancellation_token, session), fields(input, output, session.id = %session.id))]
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

        self.prompt_manager
            .lock()
            .await
            .record_tool_arguments(&tool_call.arguments, &session.working_dir);

        if tool_call.name == PLATFORM_MANAGE_SCHEDULE_TOOL_NAME {
            let arguments = tool_call
                .arguments
                .map(Value::Object)
                .unwrap_or(Value::Object(serde_json::Map::new()));
            let result = self
                .handle_schedule_management(arguments, request_id.clone())
                .await;
            let wrapped_result = result.map(CallToolResult::success);
            return (request_id, Ok(ToolCallResult::from(wrapped_result)));
        }

        if tool_call.name == FINAL_OUTPUT_TOOL_NAME {
            return if let Some(final_output_tool) = self.final_output_tool.lock().await.as_mut() {
                let result = final_output_tool.execute_tool_call(tool_call.clone()).await;
                (request_id, Ok(result))
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
            result.unwrap_or_else(|e| {
                #[cfg(feature = "telemetry")]
                crate::posthog::emit_error(
                    "tool_execution_failed",
                    &format!("{}: {}", tool_call.name, e),
                );
                let error_data = e.downcast::<ErrorData>().unwrap_or_else(|e| {
                    ErrorData::new(ErrorCode::INTERNAL_ERROR, e.to_string(), None)
                });
                ToolCallResult::from(Err(error_data))
            })
        };

        debug!("WAITING_TOOL_END: {}", tool_call.name);

        (
            request_id,
            Ok(ToolCallResult {
                notification_stream: result.notification_stream,
                result: Box::new(
                    result
                        .result
                        .map(super::large_response_handler::process_tool_response),
                ),
            }),
        )
    }

    /// Save current extension state to session metadata
    /// Should be called after any extension add/remove operation
    pub async fn save_extension_state(&self, session: &SessionConfig) -> Result<()> {
        let extension_configs = self.extension_manager.get_extension_configs().await;

        let extensions_state = EnabledExtensionsState::new(extension_configs);

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
        let extension_configs = self.extension_manager.get_extension_configs().await;
        let extensions_state = EnabledExtensionsState::new(extension_configs);

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

        // Persist once after all extensions are loaded
        if results.iter().any(|r| r.success) {
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
            ExtensionConfig::Frontend {
                tools,
                instructions,
                ..
            } => {
                // For frontend tools, just store them in the frontend_tools map
                let mut frontend_tools = self.frontend_tools.lock().await;
                for tool in tools {
                    let frontend_tool = FrontendTool {
                        name: tool.name.to_string(),
                        tool: tool.clone(),
                    };
                    frontend_tools.insert(tool.name.to_string(), frontend_tool);
                }
                // Store instructions if provided, using "frontend" as the key
                let mut frontend_instructions = self.frontend_instructions.lock().await;
                if let Some(instructions) = instructions {
                    *frontend_instructions = Some(instructions.clone());
                } else {
                    // Default frontend instructions if none provided
                    *frontend_instructions = Some(
                        "The following tools are provided directly by the frontend and will be executed by the frontend when called.".to_string(),
                    );
                }
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

        if (extension_name.is_none() || extension_name.as_deref() == Some("platform"))
            && self.config.scheduler_service.is_some()
        {
            prefixed_tools.push(platform_tools::manage_schedule_tool());
        }

        if extension_name.is_none() {
            if let Some(final_output_tool) = self.final_output_tool.lock().await.as_ref() {
                prefixed_tools.push(final_output_tool.tool());
            }
        }

        prefixed_tools
    }

    pub async fn remove_extension(&self, name: &str, session_id: &str) -> Result<()> {
        self.extension_manager.remove_extension(name).await?;

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
        self.extension_manager
            .list_extensions()
            .await
            .expect("Failed to list extensions")
    }

    pub async fn get_extension_configs(&self) -> Vec<ExtensionConfig> {
        self.extension_manager.get_extension_configs().await
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

    #[instrument(
        skip(self, user_message, session_config, cancel_token),
        fields(user_message, trace_input, session.id = %session_config.id)
    )]
    pub async fn reply(
        &self,
        user_message: Message,
        session_config: SessionConfig,
        cancel_token: Option<CancellationToken>,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let session_manager = self.config.session_manager.clone();

        let message_text_for_trace = user_message.as_concat_text();
        tracing::Span::current().record("user_message", message_text_for_trace.as_str());
        tracing::Span::current().record("trace_input", message_text_for_trace.as_str());

        for content in &user_message.content {
            if let MessageContent::ActionRequired(action_required) = content {
                if let ActionRequiredData::ElicitationResponse { id, user_data } =
                    &action_required.data
                {
                    if let Err(e) = ActionRequiredManager::global()
                        .submit_response(id.clone(), user_data.clone())
                        .await
                    {
                        let error_text = format!("Failed to submit elicitation response: {}", e);
                        error!(error_text);
                        return Ok(Box::pin(stream::once(async {
                            Ok(AgentEvent::Message(
                                Message::assistant().with_text(error_text),
                            ))
                        })));
                    }
                    session_manager
                        .add_message(&session_config.id, &user_message)
                        .await?;
                    return Ok(Box::pin(futures::stream::empty()));
                }
            }
        }

        let message_text = user_message.as_concat_text();

        // Track custom slash command usage (don't track command name for privacy)
        if message_text.trim().starts_with('/') {
            let command = message_text.split_whitespace().next();
            if let Some(cmd) = command {
                if crate::slash_commands::get_recipe_for_command(cmd).is_some() {
                    #[cfg(feature = "telemetry")]
                    crate::posthog::emit_custom_slash_command_used();
                }
            }
        }

        let command_result = self
            .execute_command(&message_text, &session_config.id)
            .await;

        match command_result {
            Err(e) => {
                let error_message = Message::assistant()
                    .with_text(e.to_string())
                    .with_visibility(true, false);
                return Ok(Box::pin(stream::once(async move {
                    Ok(AgentEvent::Message(error_message))
                })));
            }
            Ok(Some(response)) if response.role == rmcp::model::Role::Assistant => {
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

        Ok(Box::pin(async_stream::try_stream! {
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
                        SystemNotificationType::ThinkingMessage,
                        COMPACTION_THINKING_TEXT,
                    )
                );

                match compact_messages(
                    self.provider().await?.as_ref(),
                    &session_config.id,
                    &conversation_to_compact,
                    false,
                )
                .await
                {
                    Ok((compacted_conversation, summarization_usage)) => {
                        session_manager.replace_conversation(&session_config.id, &compacted_conversation).await?;
                        self.update_session_metrics(&session_config.id, session_config.schedule_id.clone(), &summarization_usage, true).await?;

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

            let mut reply_stream = self.reply_internal(final_conversation, session_config, session, cancel_token).await?;
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
            initial_messages,
        } = context;
        self.reset_retry_attempts().await;

        let provider = self.provider().await?;
        let session_manager = self.config.session_manager.clone();
        let session_id = session_config.id.clone();
        if !self.config.disable_session_naming {
            let manager_for_spawn = session_manager.clone();
            tokio::spawn(async move {
                if let Err(e) = manager_for_spawn
                    .maybe_update_name(&session_id, provider)
                    .await
                {
                    warn!("Failed to generate session description: {}", e);
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
        let reply_stream_span = tracing::info_span!(target: "goose::agents::agent", "reply_stream", session.id = %session_config.id);
        let inner = Box::pin(async_stream::try_stream! {
            let mut turns_taken = 0u32;
            let max_turns = session_config.max_turns.unwrap_or_else(|| {
                Config::global()
                    .get_param::<u32>("GOOSE_MAX_TURNS")
                    .unwrap_or(DEFAULT_MAX_TURNS)
            });
            let mut compaction_attempts = 0;
            let mut last_assistant_text = String::new();

            loop {
                if is_token_cancelled(&cancel_token) {
                    break;
                }

                {
                    let guard = self.final_output_tool.lock().await;
                    if let Some(ref output) = guard.as_ref().and_then(|fot| fot.final_output.clone()) {
                        yield AgentEvent::Message(Message::assistant().with_text(output));
                        break;
                    }
                }

                turns_taken += 1;
                if turns_taken > max_turns {
                    yield AgentEvent::Message(
                        Message::assistant().with_text(
                            "I've reached the maximum number of actions I can do without user input. Would you like me to continue?"
                        )
                    );
                    break;
                }

                let conversation_with_moim = super::moim::inject_moim(
                    &session_config.id,
                    conversation.clone(),
                    &self.extension_manager,
                    &working_dir,
                ).await;

                let mut stream = Self::stream_response_from_provider(
                    self.provider().await?,
                    &session_config.id,
                    &system_prompt,
                    conversation_with_moim.messages(),
                    &tools,
                    &toolshim_tools,
                ).await?;

                let current_turn_tool_count = conversation.messages().iter()
                    .flat_map(|m| m.content.iter())
                    .filter(|c| matches!(c, MessageContent::ToolRequest(_)))
                    .count()
                    .saturating_sub(pre_turn_tool_count);

                let tool_pair_summarization_task = crate::context_mgmt::maybe_summarize_tool_pairs(
                    self.provider().await?,
                    session_config.id.clone(),
                    conversation.clone(),
                    tool_call_cut_off,
                    current_turn_tool_count,
                );

                let mut no_tools_called = true;
                let mut messages_to_add = Conversation::default();
                let mut tools_updated = false;
                let mut did_recovery_compact_this_iteration = false;
                let mut exit_chat = false;

                // Track whether this provider turn has already emitted visible
                // thinking so a later tool-call chunk can suppress replayed
                // reasoning without hiding final-only non-streaming thoughts.
                let mut surfaced_thinking_in_turn = false;

                while let Some(next) = stream.next().await {
                    if is_token_cancelled(&cancel_token) || exit_chat {
                        break;
                    }

                    match next {
                        Ok((response, usage)) => {
                            compaction_attempts = 0;

                            if let Some(ref usage) = usage {
                                self.update_session_metrics(&session_config.id, session_config.schedule_id.clone(), usage, false).await?;
                            }

                            if let Some(response) = response {
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

                                surfaced_thinking_in_turn |= filtered_response.content.iter().any(
                                    |content| {
                                        matches!(
                                            content,
                                            MessageContent::Thinking(_)
                                                | MessageContent::RedactedThinking(_)
                                        )
                                    },
                                );

                                yield AgentEvent::Message(filtered_response.clone());
                                tokio::task::yield_now().await;

                                let num_tool_requests = frontend_requests.len() + remaining_requests.len();
                                if num_tool_requests == 0 {
                                    let text = filtered_response.as_concat_text();
                                    if !text.is_empty() {
                                        last_assistant_text = text;
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
                                        if let Some(response) = request_to_response_map.get_mut(&request.id) {
                                            response.add_tool_response_with_metadata(
                                                request.id.clone(),
                                                Ok(CallToolResult::success(vec![Content::text(CHAT_MODE_TOOL_SKIPPED_RESPONSE)])),
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

                                        for msg in self.drain_elicitation_messages(&session_config.id).await {
                                            yield AgentEvent::Message(msg);
                                        }

                                        tokio::select! {
                                            biased;

                                            tool_item = combined.next() => {
                                                match tool_item {
                                                    Some((request_id, item)) => {
                                                        match item {
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

                                            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                                                // Continue loop to drain elicitation messages
                                            }
                                        }
                                    }

                                    // check for remaining elicitation messages after all tools complete
                                    for msg in self.drain_elicitation_messages(&session_config.id).await {
                                        yield AgentEvent::Message(msg);
                                    }

                                    if all_install_successful && !enable_extension_request_ids.is_empty() {
                                        if let Err(e) = self.save_extension_state(&session_config).await {
                                            warn!("Failed to save extension state after runtime changes: {}", e);
                                        }
                                        tools_updated = true;
                                    }
                                }

                                // Preserve thinking/reasoning content from the original response
                                // Gemini (and other thinking models) require thinking to be echoed back
                                // Kimi/DeepSeek require reasoning_content on assistant tool call messages
                                let thinking_content: Vec<MessageContent> = response.content.iter()
                                    .filter(|c| matches!(c, MessageContent::Thinking(_)))
                                    .cloned()
                                    .collect();
                                if !thinking_content.is_empty() {
                                    let thinking_msg = Message::new(
                                        response.role.clone(),
                                        response.created,
                                        thinking_content,
                                    ).with_id(format!("msg_{}", Uuid::new_v4()));
                                    messages_to_add.push(thinking_msg);
                                }

                                // Collect reasoning content to attach to tool request messages
                                let reasoning_content: Vec<MessageContent> = response.content.iter()
                                    .filter(|c| matches!(c, MessageContent::Thinking(_)))
                                    .cloned()
                                    .collect();

                                for request in frontend_requests.iter().chain(remaining_requests.iter()) {
                                    if request.tool_call.is_ok() {
                                        let mut request_msg = Message::assistant()
                                            .with_id(format!("msg_{}", Uuid::new_v4()));

                                        // Providers like Kimi require reasoning_content on all assistant
                                        // messages with tool_calls when thinking mode is enabled.
                                        for rc in &reasoning_content {
                                            request_msg = request_msg.with_content(rc.clone());
                                        }

                                        request_msg = request_msg
                                            .with_tool_request_with_metadata(
                                                request.id.clone(),
                                                request.tool_call.clone(),
                                                request.metadata.as_ref(),
                                                request.tool_meta.clone(),
                                            );
                                        messages_to_add.push(request_msg);
                                        let final_response = request_to_response_map
                                            .remove(&request.id)
                                            .unwrap_or_else(|| Message::user().with_generated_id());
                                        yield AgentEvent::Message(final_response.clone());
                                        messages_to_add.push(final_response);
                                    } else {
                                        error!(
                                            "Tool call could not be parsed: {}",
                                            request.tool_call.as_ref().unwrap_err(),
                                        );
                                        yield AgentEvent::Message(
                                            Message::assistant().with_text(
                                                "A tool call could not be parsed — the response may have been truncated. Try breaking the task into smaller steps or resending your message."
                                            )
                                        );
                                        exit_chat = true;
                                        break;
                                    }
                                }

                                no_tools_called = false;
                            }
                        }
                        #[allow(unused_variables)]
                        Err(ref provider_err @ ProviderError::ContextLengthExceeded(_)) => {
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
                                    SystemNotificationType::ThinkingMessage,
                                    COMPACTION_THINKING_TEXT,
                                )
                            );

                            match compact_messages(
                                self.provider().await?.as_ref(),
                                &session_config.id,
                                &conversation,
                                false,
                            )
                            .await
                            {
                                Ok((compacted_conversation, usage)) => {
                                    session_manager.replace_conversation(&session_config.id, &compacted_conversation).await?;
                                    self.update_session_metrics(&session_config.id, session_config.schedule_id.clone(), &usage, true).await?;
                                    conversation = compacted_conversation;
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
                        Err(ref provider_err @ ProviderError::NetworkError(_)) => {
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
                if tools_updated {
                    (tools, toolshim_tools, system_prompt) =
                        self.prepare_tools_and_prompt(&session_config.id, &session.working_dir).await?;
                }

                {
                    let has_new_hints = self
                        .prompt_manager
                        .lock()
                        .await
                        .load_subdirectory_hints(&working_dir);
                    if has_new_hints && !tools_updated {
                        (tools, toolshim_tools, system_prompt) =
                            self.prepare_tools_and_prompt(&session_config.id, &session.working_dir).await?;
                    }
                }

                if no_tools_called {
                    // Lock, extract state, drop guard before branching — handle_retry_logic
                    // also locks final_output_tool and tokio::sync::Mutex is not reentrant.
                    let final_output = {
                        let guard = self.final_output_tool.lock().await;
                        guard.as_ref().map(|fot| fot.final_output.clone())
                    };

                    match final_output {
                        Some(None) => {
                            warn!("Final output tool has not been called yet. Continuing agent loop.");
                            let message = Message::user().with_text(FINAL_OUTPUT_CONTINUATION_MESSAGE);
                            messages_to_add.push(message.clone());
                            yield AgentEvent::Message(message);
                        }
                        Some(Some(output)) => {
                            let message = Message::assistant().with_text(output);
                            messages_to_add.push(message.clone());
                            yield AgentEvent::Message(message);
                            exit_chat = true;
                        }
                        None if did_recovery_compact_this_iteration => {
                            // continue from last user message after recovery compact
                        }
                        None => {
                            match self.handle_retry_logic(&mut conversation, &session_config, &initial_messages).await {
                                Ok(should_retry) => {
                                    if should_retry {
                                        info!("Retry logic triggered, restarting agent loop");
                                        messages_to_add = Conversation::default();
                                        session_manager.replace_conversation(&session_config.id, &conversation).await?;
                                        yield AgentEvent::HistoryReplaced(conversation.clone());
                                    } else {
                                        exit_chat = true;
                                    }
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
                    tool_pair_summarization_task.abort();
                }

                if let Ok(summaries) = tool_pair_summarization_task.await {
                    let mut updated_messages = conversation.messages().clone();

                    for (summary_msg, tool_id) in summaries {
                        let matching: Vec<&mut Message> = updated_messages
                            .iter_mut()
                            .filter(|msg| {
                                msg.id.is_some() && msg.content.iter().any(|c| match c {
                                    MessageContent::ToolRequest(req) => req.id == tool_id,
                                    MessageContent::ToolResponse(resp) => resp.id == tool_id,
                                    _ => false,
                                })
                            })
                            .collect();

                        if matching.len() == 2 {
                            for msg in matching {
                                let id = msg.id.as_ref().unwrap();
                                msg.metadata = msg.metadata.with_agent_invisible();
                                SessionManager::update_message_metadata(&session_config.id, id, |metadata| {
                                    metadata.with_agent_invisible()
                                }).await?;
                            }
                            messages_to_add.push(summary_msg);
                        } else {
                            warn!("Expected a tool request/reply pair, but found {} matching messages",
                                matching.len());
                        }
                    }
                    conversation = Conversation::new_unvalidated(updated_messages);
                }

                for msg in &messages_to_add {
                    session_manager.add_message(&session_config.id, msg).await?;
                }
                conversation.extend(messages_to_add);
                if exit_chat {
                    break;
                }

                tokio::task::yield_now().await;
            }

            if !last_assistant_text.is_empty() {
                tracing::info!(target: "goose::agents::agent", trace_output = last_assistant_text.as_str());
            }
        }.instrument(reply_stream_span));
        Ok(inner)
    }

    pub async fn extend_system_prompt(&self, key: String, instruction: String) {
        let mut prompt_manager = self.prompt_manager.lock().await;
        prompt_manager.add_system_prompt_extra(key, instruction);
    }

    pub async fn update_provider(
        &self,
        provider: Arc<dyn Provider>,
        session_id: &str,
    ) -> Result<()> {
        let provider_name = provider.get_name().to_string();
        let model_config = provider.get_model_config();

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

        let model_config = match session.model_config.clone() {
            Some(saved_config) => saved_config,
            None => {
                let model_name = config
                    .get_goose_model()
                    .ok()
                    .ok_or_else(|| anyhow!("Could not configure agent: missing model"))?;
                crate::model::ModelConfig::new(&model_name)
                    .map_err(|e| anyhow!("Could not configure agent: invalid model {}", e))?
                    .with_canonical_limits(&provider_name)
            }
        };

        let extensions =
            EnabledExtensionsState::extensions_or_default(Some(&session.extension_data), config);

        let (provider, provider_changed) = if crate::providers::get_from_registry(&provider_name)
            .await
            .is_ok()
        {
            let p = crate::providers::create(&provider_name, model_config, extensions)
                .await
                .map_err(|e| anyhow!("Could not create provider: {}", e))?;
            (p, false)
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

            let fallback_model_name = config
                .get_goose_model()
                .ok()
                .ok_or_else(|| anyhow!("Could not configure fallback provider: missing model"))?;
            let fallback_model_config = crate::model::ModelConfig::new(&fallback_model_name)
                .map_err(|e| anyhow!("Could not configure fallback provider: invalid model {}", e))?
                .with_canonical_limits(&fallback_provider_name);

            let fallback_provider = crate::providers::create(
                &fallback_provider_name,
                fallback_model_config.clone(),
                extensions,
            )
            .await
            .map_err(|e| {
                anyhow!(
                    "Could not create provider '{}' or fallback '{}': {}",
                    provider_name,
                    fallback_provider_name,
                    e
                )
            })?;

            if let Err(e) = self
                .config
                .session_manager
                .update(&session.id)
                .provider_name(&fallback_provider_name)
                .model_config(fallback_model_config)
                .apply()
                .await
            {
                tracing::warn!("Failed to update session provider: {}", e);
            }

            (fallback_provider, true)
        };

        self.update_provider(provider, &session.id).await?;
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
        let (extension_count, tool_count) = self
            .extension_manager
            .get_extension_and_tool_counts(session_id)
            .await;

        // Get model name from provider
        let provider = self.provider().await.map_err(|e| {
            tracing::error!("Failed to get provider for recipe creation: {}", e);
            e
        })?;
        let model_config = provider.get_model_config();
        let model_name = &model_config.model_name;
        tracing::debug!("Using model: {}", model_name);

        let goose_mode = *self.current_goose_mode.lock().await;
        let prompt_manager = self.prompt_manager.lock().await;
        let system_prompt = prompt_manager
            .builder()
            .with_extensions(extensions_info.into_iter())
            .with_frontend_instructions(self.frontend_instructions.lock().await.clone())
            .with_extension_and_tool_counts(extension_count, tool_count)
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

        messages.push(Message::user().with_text(recipe_prompt));

        let (messages, issues) = fix_conversation(messages);
        if !issues.is_empty() {
            issues
                .iter()
                .for_each(|issue| tracing::warn!(recipe.conversation.issue = issue));
        }

        tracing::debug!(
            "Added recipe prompt to messages, total messages: {}",
            messages.len()
        );

        tracing::info!("Calling provider to generate recipe content");
        let model_config = {
            let provider_guard = self.provider.lock().await;
            let provider = provider_guard.as_ref().ok_or_else(|| {
                let error = anyhow!("Provider not available during recipe creation");
                tracing::error!("{}", error);
                error
            })?;
            provider.get_model_config()
        };
        let (result, _usage) = self
            .provider
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| {
                let error = anyhow!("Provider not available during recipe creation");
                tracing::error!("{}", error);
                error
            })?
            .complete(
                &model_config,
                session_id,
                &system_prompt,
                messages.messages(),
                &tools,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::permission_confirmation::PrincipalType;
    use crate::providers::base::PermissionRouting;
    use crate::recipe::Response;

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
        fn get_model_config(&self) -> crate::model::ModelConfig {
            crate::model::ModelConfig::new("test").unwrap()
        }
        async fn stream(
            &self,
            _: &crate::model::ModelConfig,
            _: &str,
            _: &str,
            _: &[crate::conversation::message::Message],
            _: &[rmcp::model::Tool],
        ) -> Result<crate::providers::base::MessageStream, crate::providers::errors::ProviderError>
        {
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
}
