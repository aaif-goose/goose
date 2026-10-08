use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use futures::stream::BoxStream;
use futures::{StreamExt, TryStreamExt};
use tracing_futures::Instrument;

use super::gen_ai_telemetry;
use super::mcp_client::GooseMcpHostInfo;
use super::tool_confirmation_coordinator::{
    ActiveTurnGuard, ConfirmationAnswer, ToolConfirmationCoordinator,
};
use crate::action_required_manager::ElicitationOutcome;
use crate::agents::extension::{ExtensionConfig, ExtensionResult};
use crate::agents::extension_manager::{ExtensionManager, ExtensionManagerCapabilities};
use crate::agents::provider_manager::ProviderManager;
use crate::agents::state_machine::ops_recipe;
use crate::agents::state_machine::{
    has_unapplied_tool_confirmation_response, pending_tool_confirmations,
    persist_tool_confirmation_decision, run_goose, subagent_cancelled_message, BangShellOperation,
    CompactionOperation, DoctorOperation, Emitter, EmptyResponseOperation, EntryHookOperation,
    ExitOnErrorOperation, ForegroundSubagentOperation, GooseEffect, GooseInferenceProvider,
    GooseInferenceRequestPreparer, InferenceRunner, MaxTurnsOperation, Operation, ProjectOperation,
    RecipeOperation, RetryOperation, SkillOperation, SlashCommandOperation, StateMachine,
    StatusOperation, SteerOperation, SteerQueue, Step, StopHookOperation, ToolApprovalOperation,
    ToolExecutionOperation, ToolPairCompactionOperation, UnknownToolOperation,
};
use crate::agents::subagent_handler::ForegroundSubagentRunner;
use crate::agents::types::{
    SessionConfig, DEFAULT_ON_FAILURE_TIMEOUT_SECONDS, DEFAULT_RETRY_TIMEOUT_SECONDS,
};
use crate::agents::AgentEvent;
use crate::config::extensions::name_to_key;
use crate::config::permission::PermissionManager;
use crate::config::{Config, GooseMode};
use crate::context_mgmt::DEFAULT_COMPACTION_THRESHOLD;
use crate::conversation::message::{ActionRequiredData, Message, MessageContent};
use crate::permission::permission_inspector::PermissionInspector;
use crate::permission::{Permission, PermissionConfirmation};
use crate::providers::base::{PermissionRouting, Provider};
use crate::scheduler_trait::SchedulerTrait;
use crate::security::adversary_inspector::AdversaryInspector;
use crate::security::egress_inspector::EgressInspector;
use crate::security::security_inspector::SecurityInspector;
use crate::session::extension_data::{EnabledExtensionsState, ExtensionState};
use crate::session::{Session, SessionManager, SessionNameUpdate};
use crate::tool_inspection::ToolInspectionManager;
use crate::tool_monitor::RepetitionInspector;
use goose_providers::errors::ProviderError;
use goose_providers::thinking::{ThinkingEffort, ThinkingEffortSupport};
use rmcp::model::{ElicitationAction, GetPromptResult, Prompt, Tool};
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{error, instrument, warn};

const DEFAULT_MAX_TURNS: u32 = 1000;
const DEFAULT_STOP_HOOK_BLOCK_CAP: u32 = 8;

fn provider_creation_error(error: anyhow::Error, context: impl fmt::Display) -> anyhow::Error {
    let message = format!("{context}: {error}");
    error.context(message)
}

fn normalize_legacy_provider_thinking_effort(
    mut model_config: goose_providers::model::ModelConfig,
    effort_support: &ThinkingEffortSupport,
) -> goose_providers::model::ModelConfig {
    let has_raw_effort = model_config
        .request_params
        .as_ref()
        .is_some_and(|params| params.contains_key("thinking_effort"));
    if !matches!(effort_support, ThinkingEffortSupport::Unspecified)
        || !has_raw_effort
        || model_config.thinking_effort().is_some()
    {
        return model_config;
    }

    if let Some(params) = model_config.request_params.as_mut() {
        params.remove("thinking_effort");
    }
    model_config.with_default_thinking_effort(Config::global().get_goose_thinking_effort())
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
    pub disable_session_naming: bool,
    pub goose_platform: GoosePlatform,
    pub mcp_host_info: Option<GooseMcpHostInfo>,
    pub elicitation_handler: Option<crate::agents::mcp_client::ElicitationHandler>,
    pub mcp_protocol_version: Option<rmcp::model::ProtocolVersion>,
    pub session_name_update_tx: Option<mpsc::UnboundedSender<SessionNameUpdate>>,
    pub use_login_shell_path: Option<bool>,
    pub is_subagent: bool,
    pub providers: Arc<ProviderManager>,
}

impl AgentConfig {
    pub fn new(
        session_manager: Arc<SessionManager>,
        permission_manager: Arc<PermissionManager>,
        scheduler_service: Option<Arc<dyn SchedulerTrait>>,
        disable_session_naming: bool,
        goose_platform: GoosePlatform,
    ) -> Self {
        Self {
            session_manager,
            permission_manager,
            scheduler_service,
            disable_session_naming,
            goose_platform,
            mcp_host_info: None,
            elicitation_handler: None,
            mcp_protocol_version: None,
            session_name_update_tx: None,
            use_login_shell_path: None,
            is_subagent: false,
            providers: Arc::default(),
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

/// The main goose Agent
pub struct Agent {
    pub config: AgentConfig,

    pub extension_manager: Arc<ExtensionManager>,
    tool_confirmation_coordinator: ToolConfirmationCoordinator,

    pub(super) tool_inspection_manager: ToolInspectionManager,
    pub(super) hook_manager: crate::hooks::HookManager,
    #[cfg(test)]
    pub(super) stop_hook_block_cap_override: Option<u32>,
    steer_queues: Mutex<HashMap<String, SteerQueue>>,
}

fn user_event(event: AgentEvent) -> Option<AgentEvent> {
    match event {
        AgentEvent::Message(message) => {
            let had_content = !message.content.is_empty();
            let projected = message
                .with_generated_id_if_missing()
                .user_visible_content();
            (!had_content || !projected.content.is_empty())
                .then_some(AgentEvent::Message(projected))
        }
        other => Some(other),
    }
}

fn agent_visible_message_text(message: &Message) -> String {
    message.agent_visible_content().as_concat_text()
}

impl Default for Agent {
    fn default() -> Self {
        Self::new()
    }
}

fn has_unique_persisted_extension(configs: &[ExtensionConfig], key: &str) -> Result<bool> {
    match configs
        .iter()
        .filter(|config| config.key() == key)
        .take(2)
        .count()
    {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(anyhow!("Duplicate session extension key '{key}'")),
    }
}

impl Agent {
    pub fn new() -> Self {
        let config = Config::global();
        Self::with_config(AgentConfig::new(
            Arc::new(SessionManager::instance()),
            PermissionManager::instance(),
            None,
            config.get_goose_disable_session_naming().unwrap_or(false),
            GoosePlatform::GooseCli,
        ))
    }

    pub fn with_config(config: AgentConfig) -> Self {
        let providers = config.providers.clone();

        let goose_platform = config.goose_platform.clone();
        let explicit_mcp_host_info = config.mcp_host_info.clone();
        let mcpui = explicit_mcp_host_info
            .as_ref()
            .filter(|host_info| host_info.explicit_extensions)
            .map(GooseMcpHostInfo::mcpui_enabled)
            .unwrap_or_else(|| match config.goose_platform {
                GoosePlatform::GooseDesktop => true,
                GoosePlatform::GooseCli => false,
            });
        let capabilities = ExtensionManagerCapabilities {
            mcpui,
            host_info: explicit_mcp_host_info.clone(),
            elicitation_handler: config.elicitation_handler.clone(),
            protocol_version: config.mcp_protocol_version.clone(),
        };
        let client_name = explicit_mcp_host_info
            .as_ref()
            .and_then(|host_info| host_info.client_name.clone())
            .unwrap_or_else(|| goose_platform.to_string());
        let session_manager = Arc::clone(&config.session_manager);
        let scheduler = config.scheduler_service.clone();
        let inspection_session_manager = Arc::clone(&config.session_manager);
        let permission_manager = Arc::clone(&config.permission_manager);
        let use_login_shell_path = config.resolve_use_login_shell_path();
        let is_subagent = config.is_subagent;
        Self {
            config,
            extension_manager: Arc::new(ExtensionManager::new(
                providers.clone(),
                session_manager,
                scheduler,
                client_name,
                capabilities,
                use_login_shell_path,
            )),
            tool_confirmation_coordinator: ToolConfirmationCoordinator::new(),
            tool_inspection_manager: Self::create_tool_inspection_manager(
                permission_manager,
                providers,
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
            #[cfg(test)]
            stop_hook_block_cap_override: None,
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
    pub(crate) fn clear_extension_lease_for_test(&self, session_id: &str) {
        self.tool_confirmation_coordinator
            .session(session_id)
            .clear_extension_lease();
    }

    #[cfg(test)]
    pub(crate) fn set_stop_hook_block_cap_for_test(&mut self, cap: u32) {
        self.stop_hook_block_cap_override = Some(cap);
    }

    pub async fn emit_hook(&self, event: crate::hooks::HookEvent, session_id: &str) {
        if !self.hook_manager.has_hooks(event) {
            return;
        }
        self.hook_manager
            .emit(event, crate::hooks::HookContext::new(event, session_id))
            .await;
    }

    /// Fires `SessionStart` for a client opening the session and returns the hooks'
    /// banners, which the client shows before the first prompt.
    pub async fn emit_session_start(&self, session_id: &str) -> Result<Vec<String>> {
        let event = crate::hooks::HookEvent::SessionStart;
        if !self.hook_manager.has_hooks(event) {
            return Ok(Vec::new());
        }
        let banners = self
            .hook_manager
            .emit_collecting_banners(event, crate::hooks::HookContext::new(event, session_id))
            .await;
        self.config
            .session_manager
            .add_message(
                session_id,
                &crate::agents::state_machine::session_start_message(&banners),
            )
            .await?;
        Ok(banners)
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

    async fn steer_queue(&self, session_id: &str) -> SteerQueue {
        self.steer_queues
            .lock()
            .await
            .entry(session_id.to_string())
            .or_default()
            .clone()
    }

    /// Create a tool inspection manager with default inspectors
    fn create_tool_inspection_manager(
        permission_manager: Arc<PermissionManager>,
        providers: Arc<ProviderManager>,
        session_manager: Arc<SessionManager>,
    ) -> ToolInspectionManager {
        let mut tool_inspection_manager = ToolInspectionManager::new();

        // Add security inspector (highest priority - runs first)
        tool_inspection_manager.add_inspector(Box::new(SecurityInspector::new()));
        tool_inspection_manager.add_inspector(Box::new(EgressInspector::new()));

        // Add adversary inspector (LLM-based review, enabled by ~/.config/goose/adversary.md)
        tool_inspection_manager.add_inspector(Box::new(AdversaryInspector::new(
            providers.clone(),
            session_manager.clone(),
        )));

        // Add permission inspector (medium-high priority)
        tool_inspection_manager.add_inspector(Box::new(PermissionInspector::new(
            permission_manager,
            providers,
            session_manager,
        )));

        // Add repetition inspector (lower priority - basic repetition checking)
        tool_inspection_manager.add_inspector(Box::new(RepetitionInspector::new(None)));

        tool_inspection_manager
    }

    pub async fn provider(&self, session_id: &str) -> Result<Arc<dyn Provider>> {
        let session = self
            .config
            .session_manager
            .get_session(session_id, false)
            .await?;
        self.config.providers.provider_for(&session).await
    }

    pub async fn model_config_for_session(
        &self,
        session_id: &str,
    ) -> Result<goose_providers::model::ModelConfig> {
        let session = self
            .config
            .session_manager
            .get_session(session_id, false)
            .await?;
        crate::agents::provider_manager::model_config_for(&session)
    }

    pub(super) async fn effective_model_config_for_session(
        &self,
        session_id: &str,
    ) -> Result<goose_providers::model::ModelConfig> {
        let model_config = self.model_config_for_session(session_id).await?;
        let provider_name = self.provider(session_id).await?.get_name().to_string();
        match crate::providers::get_from_registry(&provider_name).await {
            Ok(entry) => Ok(entry
                .normalize_model_config(model_config.clone())
                .unwrap_or(model_config)),
            Err(_) => Ok(model_config),
        }
    }

    /// Save current extension state to session by session_id
    pub async fn persist_extension_state(&self, session_id: &str) -> Result<()> {
        self.persist_extension_configs(
            session_id,
            self.extension_manager
                .get_extension_configs(session_id)
                .await,
        )
        .await
    }

    /// Save the provided extension configuration to session metadata.
    pub async fn persist_extension_configs(
        &self,
        session_id: &str,
        extensions: Vec<ExtensionConfig>,
    ) -> Result<()> {
        let extensions_state = EnabledExtensionsState::new(extensions);

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
            .config
            .providers
            .provider_for(session)
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
                        .is_extension_enabled(&session_id_clone, &name)
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
    ///
    /// State is persisted once every extension has settled, even when all of them
    /// fail: the session's enabled list records what actually loaded, so failed
    /// extensions are dropped instead of staying marked as enabled and being
    /// retried on every subsequent resume.
    pub async fn add_extensions_bulk(
        self: &Arc<Self>,
        extensions: Vec<ExtensionConfig>,
        session_id: &str,
    ) -> anyhow::Result<Vec<ExtensionLoadResult>> {
        let (working_dir, container) = match self
            .config
            .session_manager
            .get_session(session_id, false)
            .await
        {
            Ok(session) => (Some(session.working_dir), session.container),
            Err(e) => {
                warn!("Failed to get session for bulk load: {}", e);
                (None, None)
            }
        };

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
                        .add_extension(config, working_dir, container.as_ref(), &sid)
                        .await
                    {
                        Ok(_) => ExtensionLoadResult {
                            name,
                            success: true,
                            error: None,
                        },
                        Err(e) => {
                            let error = e.to_string();
                            warn!("Failed to load extension {}: {}", name, error);
                            ExtensionLoadResult {
                                name,
                                success: false,
                                error: Some(error),
                            }
                        }
                    }
                }
            })
            .collect::<Vec<_>>();

        let results = futures::future::join_all(extension_futures).await;

        self.persist_extension_state(session_id).await?;

        Ok(results)
    }

    pub(super) async fn add_extension_inner(
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
        self.extension_manager
            .add_extension(
                extension,
                Some(session.working_dir),
                session.container.as_ref(),
                session_id,
            )
            .await?;

        Ok(())
    }

    pub(crate) async fn update_extension_working_dir(
        &self,
        session_id: &str,
        working_dir: &std::path::Path,
    ) -> ExtensionResult<()> {
        let session = self
            .config
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|e| crate::agents::extension::ExtensionError::SetupError(e.to_string()))?;
        self.extension_manager
            .update_working_dir(working_dir, session.container.as_ref(), session_id)
            .await
    }

    pub async fn list_tools(&self, session_id: &str, extension_name: Option<String>) -> Vec<Tool> {
        let include_final_output = extension_name.is_none();
        let lease = self.extension_manager.current_lease(session_id, None).await;
        let mut prefixed_tools = match extension_name {
            Some(name) => lease.tools_for(&name).await,
            None => lease.tools().await,
        };

        if include_final_output {
            let final_output_tool = self
                .config
                .session_manager
                .get_session(session_id, false)
                .await
                .and_then(|session| ops_recipe::final_output_tool(&session));
            if let Ok(Some(final_output_tool)) = final_output_tool {
                prefixed_tools.push(final_output_tool.tool());
            }
        }

        prefixed_tools
    }

    pub async fn remove_extension(&self, name: &str, session_id: &str) -> Result<()> {
        self.remove_extension_by_key(&name_to_key(name), session_id)
            .await?;
        Ok(())
    }

    pub async fn remove_extension_by_key(&self, key: &str, session_id: &str) -> Result<bool> {
        let session = self
            .config
            .session_manager
            .get_session(session_id, false)
            .await?;
        let persisted_extensions = EnabledExtensionsState::extensions_or_default(
            Some(&session.extension_data),
            Config::global(),
        );
        if !has_unique_persisted_extension(&persisted_extensions, key)? {
            return Ok(false);
        }

        self.extension_manager
            .remove_extension_by_key(session_id, key)
            .await?;

        // Persist extension state after successful removal
        self.persist_extension_state(session_id)
            .await
            .map_err(|e| {
                error!("Failed to persist extension state: {}", e);
                anyhow!("Failed to persist extension state: {}", e)
            })?;

        Ok(true)
    }

    pub async fn list_extensions(&self, session_id: &str) -> Vec<String> {
        self.extension_manager.list_extensions(session_id).await
    }

    pub async fn get_extension_configs(&self, session_id: &str) -> Vec<ExtensionConfig> {
        self.extension_manager
            .get_extension_configs(session_id)
            .await
    }

    pub async fn submit_tool_confirmation(
        &self,
        session_id: &str,
        request_id: &str,
        permission: Permission,
    ) -> Result<()> {
        self.config
            .session_manager
            .get_session(session_id, false)
            .await?;

        let state = self.tool_confirmation_coordinator.session(session_id);
        let _confirmation_submission_guard = state.confirmation_submission_lock.lock().await;
        let state_machine_permission = if permission == Permission::Cancel {
            Permission::DenyOnce
        } else {
            permission.clone()
        };

        if let Some(answer) = state.answer(request_id) {
            return match answer {
                ConfirmationAnswer::LiveHandled => {
                    Err(anyhow!("tool confirmation request was already answered"))
                }
                ConfirmationAnswer::StateMachine(previous)
                    if previous == state_machine_permission =>
                {
                    Ok(())
                }
                ConfirmationAnswer::StateMachine(_) => Err(anyhow!(
                    "tool confirmation request already has a different decision"
                )),
            };
        }

        let confirmation = PermissionConfirmation {
            principal_type: crate::permission::permission_confirmation::PrincipalType::Tool,
            permission: permission.clone(),
        };
        if self
            .try_route_tool_confirmation_to_provider(session_id, request_id, &confirmation)
            .await
        {
            if state.contains_request(request_id) {
                state.record_answer(request_id, ConfirmationAnswer::LiveHandled)?;
            }
            return Ok(());
        }

        if state.contains_request(request_id) {
            persist_tool_confirmation_decision(
                self.config.session_manager.as_ref(),
                session_id,
                request_id,
                &state_machine_permission,
            )
            .await?;
            state.record_answer(
                request_id,
                ConfirmationAnswer::StateMachine(state_machine_permission),
            )?;
            return Ok(());
        }

        Err(anyhow!(
            "unknown or stale tool confirmation request {request_id} for session {session_id}"
        ))
    }

    async fn try_route_tool_confirmation_to_provider(
        &self,
        session_id: &str,
        request_id: &str,
        confirmation: &PermissionConfirmation,
    ) -> bool {
        if let Ok(provider) = self.provider(session_id).await {
            if provider.permission_routing() == PermissionRouting::ActionRequired
                && provider
                    .handle_permission_confirmation(request_id, confirmation)
                    .await
            {
                return true;
            }
        }
        false
    }

    pub(super) async fn create_state_machine(
        &self,
        provider: Arc<dyn Provider>,
        model_config: goose_providers::model::ModelConfig,
        context_limit: usize,
        session_config: SessionConfig,
        cancel: CancellationToken,
        steer_queue: SteerQueue,
    ) -> StateMachine<'_, Session, GooseEffect> {
        let max_turns = session_config.max_turns.unwrap_or_else(|| {
            Config::global()
                .get_param::<u32>("GOOSE_MAX_TURNS")
                .unwrap_or(DEFAULT_MAX_TURNS)
        });
        let extension_lease = self
            .tool_confirmation_coordinator
            .session(&session_config.id)
            .extension_lease();
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
        let remaining_operations: Vec<Arc<dyn Operation<Session, GooseEffect> + '_>> = vec![
            Arc::new(ToolPairCompactionOperation::new(
                provider.clone(),
                model_config.clone(),
                tool_call_cutoff,
                tool_pair_compaction_enabled,
            )),
            Arc::new(ToolApprovalOperation::new(&self.tool_inspection_manager)),
            Arc::new(DoctorOperation::new(self.config.session_manager.clone())),
            Arc::new(ProjectOperation),
            Arc::new(SkillOperation::new(
                self.hook_manager.clone(),
                Arc::clone(&extension_lease),
            )),
            // Before RecipeOperation: a `delegate` response only means the subagent
            // started, so a final output from the same batch must not be shown until
            // the subagents have run.
            Arc::new(ForegroundSubagentOperation::new(
                ForegroundSubagentRunner::new(
                    self.config.session_manager.clone(),
                    self.config.resolve_use_login_shell_path(),
                ),
                cancel.clone(),
            )),
            Arc::new(RecipeOperation::new(
                provider.clone(),
                self.hook_manager.clone(),
            )),
            Arc::new(ToolExecutionOperation::new(
                self.extension_manager.clone(),
                self.hook_manager.clone(),
                Arc::clone(&extension_lease),
            )),
            Arc::new(UnknownToolOperation::new(self.hook_manager.clone())),
            Arc::new(RetryOperation::new(
                std::time::Duration::from_secs(retry_timeout),
                std::time::Duration::from_secs(on_failure_timeout),
            )),
            Arc::new(EmptyResponseOperation),
            Arc::new(StopHookOperation::new(
                self.hook_manager.clone(),
                stop_hook_block_cap,
            )),
            Arc::new(ExitOnErrorOperation),
            Arc::new(MaxTurnsOperation::new(max_turns)),
        ];
        operations.extend(remaining_operations);
        let request_preparer = GooseInferenceRequestPreparer {
            extension_manager: Arc::clone(&self.extension_manager),
            extension_lease,
            tool_inspection_manager: &self.tool_inspection_manager,
            context_limit,
        };
        let status_operation =
            Arc::new(StatusOperation::new(provider.clone(), model_config.clone()));
        let inference_provider = Arc::new(GooseInferenceProvider::new(provider));
        let inference = Arc::new(
            InferenceRunner::new(inference_provider, model_config)
                .with_request_preparer(Arc::new(request_preparer)),
        );
        let mut command_handlers = operations.clone();
        command_handlers.push(status_operation);
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
        let session_id = session_config.id.clone();
        let events = crate::session_context::with_session_id(
            Some(session_id.clone()),
            self.reply_with_state_machine_inner(user_message, session_config, cancel_token),
        )
        .await?;
        Ok(crate::session_context::with_session_id_stream(
            Some(session_id),
            events,
        ))
    }

    async fn reply_with_state_machine_inner(
        &self,
        user_message: Message,
        session_config: SessionConfig,
        cancel_token: Option<CancellationToken>,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let session_manager = self.config.session_manager.clone();
        let session_id = session_config.id.clone();
        let turn_guard = self
            .tool_confirmation_coordinator
            .session(&session_id)
            .try_start_turn()?;
        turn_guard.state().start_new_turn();

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

        if !self.config.disable_session_naming {
            let provider = self.provider(&session_config.id).await?;
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

        let cancel = cancel_token.unwrap_or_default();
        let initial_stream = self
            .stream_state_machine_session(session_config.clone(), cancel.clone())
            .await?;
        Ok(
            self.stream_state_machine_turn(
                session_config,
                cancel,
                turn_guard,
                Some(initial_stream),
            ),
        )
    }

    pub(crate) async fn resume_state_machine_turn(
        self: &Arc<Self>,
        session_config: SessionConfig,
        cancel: CancellationToken,
    ) -> Result<Option<BoxStream<'static, Result<AgentEvent>>>> {
        let session_id = session_config.id.clone();
        let stream = crate::session_context::with_session_id(
            Some(session_id.clone()),
            self.resume_state_machine_turn_inner(session_config, cancel),
        )
        .await?;
        Ok(stream
            .map(|stream| crate::session_context::with_session_id_stream(Some(session_id), stream)))
    }

    pub async fn cancel_foreground_subagents(&self, session_id: &str) {
        if let Err(error) = self.record_cancelled_subagents(session_id).await {
            error!(
                session_id,
                ?error,
                "Failed to record cancelled foreground subagents"
            );
        }
    }

    async fn record_cancelled_subagents(&self, session_id: &str) -> Result<()> {
        let session_manager = &self.config.session_manager;
        let session = session_manager.get_session(session_id, true).await?;
        let Some(message) = session
            .conversation
            .as_ref()
            .and_then(|conversation| subagent_cancelled_message(conversation.messages()))
        else {
            return Ok(());
        };
        session_manager.add_message(session_id, &message).await
    }

    async fn resume_state_machine_turn_inner(
        self: &Arc<Self>,
        session_config: SessionConfig,
        cancel: CancellationToken,
    ) -> Result<Option<BoxStream<'static, Result<AgentEvent>>>> {
        let session = self
            .config
            .session_manager
            .get_session(&session_config.id, true)
            .await?;
        let conversation = session
            .conversation
            .as_ref()
            .ok_or_else(|| anyhow!("Session {} has no conversation", session_config.id))?;
        let pending_confirmations = pending_tool_confirmations(conversation);
        let resume_from_persisted_response = pending_confirmations.is_empty()
            && has_unapplied_tool_confirmation_response(conversation);
        if pending_confirmations.is_empty() && !resume_from_persisted_response {
            return Ok(None);
        }

        let turn_guard = self
            .tool_confirmation_coordinator
            .session(&session_config.id)
            .try_start_turn()?;
        for request in pending_confirmations {
            turn_guard.state().register_request(request.id);
        }

        let agent = Arc::clone(self);
        let stream = Box::pin(async_stream::try_stream! {
            let initial_stream = if resume_from_persisted_response {
                Some(
                    agent
                        .stream_state_machine_session(
                            session_config.clone(),
                            cancel.clone(),
                        )
                        .await?,
                )
            } else {
                None
            };
            let mut stream = agent.stream_state_machine_turn(
                session_config,
                cancel,
                turn_guard,
                initial_stream,
            );
            while let Some(event) = stream.next().await {
                yield event?;
            }
        });
        Ok(Some(stream))
    }

    fn tool_confirmation_request_ids(event: &AgentEvent) -> Vec<String> {
        let AgentEvent::Message(message) = event else {
            return Vec::new();
        };

        message
            .content
            .iter()
            .filter_map(|content| {
                let MessageContent::ActionRequired(action) = content else {
                    return None;
                };
                let ActionRequiredData::ToolConfirmation { id, .. } = &action.data else {
                    return None;
                };
                Some(id.clone())
            })
            .collect()
    }

    fn stream_state_machine_turn<'a>(
        &'a self,
        session_config: SessionConfig,
        cancel: CancellationToken,
        turn_guard: ActiveTurnGuard,
        initial_stream: Option<BoxStream<'a, Result<AgentEvent>>>,
    ) -> BoxStream<'a, Result<AgentEvent>> {
        Box::pin(async_stream::try_stream! {
            let mut stream = initial_stream;
            loop {
                if let Some(active_stream) = stream.as_mut() {
                    let mut has_confirmations = false;
                    while let Some(event) = active_stream.next().await {
                        let event = event?;
                        for request_id in Self::tool_confirmation_request_ids(&event) {
                            turn_guard.state().register_request(request_id);
                            has_confirmations = true;
                        }
                        yield event;
                    }

                    if !has_confirmations {
                        turn_guard.state().clear_confirmations();
                        return;
                    }
                }

                let has_state_machine_answer = turn_guard
                    .state()
                    .wait_for_all_confirmation_answers(&cancel)
                    .await?;
                if !has_state_machine_answer {
                    turn_guard.state().clear_confirmations();
                    return;
                }
                stream = Some(
                    self.stream_state_machine_session(
                        session_config.clone(),
                        cancel.clone(),
                    )
                    .await?,
                );
            }
        })
    }

    pub(super) async fn stream_state_machine_session(
        &self,
        session_config: SessionConfig,
        cancel: CancellationToken,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let session_manager = self.config.session_manager.clone();
        let session_id = session_config.id.clone();
        let provider = self.provider(&session_id).await?;
        let model_config = self.effective_model_config_for_session(&session_id).await?;

        let context_limit =
            crate::context_limit::get_context_limit(provider.as_ref(), &model_config.model_name)
                .await?;
        let steer_queue = self.steer_queue(&session_id).await;
        let machine = self
            .create_state_machine(
                provider,
                model_config,
                context_limit,
                session_config,
                cancel.clone(),
                steer_queue,
            )
            .await;
        let reply_span = tracing::Span::current();

        Ok(Box::pin(
            async_stream::try_stream! {
                let (tx, mut rx) = mpsc::channel::<AgentEvent>(32);
                let emit = Emitter::new(tx, cancel.clone());
                let result = {
                    let run = crate::session_context::with_session_id(
                        Some(session_id.clone()),
                        run_goose(&machine, session_manager.as_ref(), &session_id, &emit),
                    );
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

    pub(crate) async fn reply_live_delegation(
        &self,
        user_message: Message,
        session_config: SessionConfig,
        cancel_token: CancellationToken,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let user_message = user_message.agent_only();
        let events = self
            .reply_with_state_machine(user_message, session_config, Some(cancel_token))
            .await?;
        Ok(Box::pin(events.try_filter_map(|event| async move {
            Ok(user_event(event))
        })))
    }

    #[instrument(
        skip(self, user_message, session_config, cancel_token),
        fields(
            user_message,
            trace_input,
            trace_output = tracing::field::Empty,
            session.id = %session_config.id,
            gen_ai.operation.name = "invoke_agent",
            gen_ai.agent.name = tracing::field::Empty,
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
        let reply_span = tracing::Span::current();
        let session_id = session_config.id.clone();
        let events = crate::session_context::with_session_id(
            Some(session_id.clone()),
            self.reply_impl(user_message, session_config, cancel_token),
        )
        .await?;
        let events = crate::session_context::with_session_id_stream(Some(session_id), events);

        // This is the single live-event boundary. Callers that intentionally stream
        // multiple events for one logical message must assign their shared ID before this point.
        Ok(Box::pin(
            events
                .try_filter_map(|event| async move { Ok(user_event(event)) })
                .instrument(reply_span),
        ))
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

        // Superfast Decision Gate (shadow mode, off by default). When enabled it
        // classifies the pending user turn on a detached task and only logs the
        // recommendation and latency. It never changes routing and never blocks
        // this turn. When disabled (the default) this call returns immediately.
        crate::agents::superfast::shadow_classify(message_text_for_trace.clone());

        if gen_ai_telemetry::capture_message_content() {
            tracing::Span::current().record("user_message", message_text_for_trace.as_str());
            tracing::Span::current().record("trace_input", message_text_for_trace.as_str());
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

        self.reply_with_state_machine(user_message, session_config, cancel_token)
            .await
    }

    /// Pins `provider` to the session instead of letting the provider manager
    /// build one.
    pub async fn update_provider(
        &self,
        provider: Arc<dyn Provider>,
        model_config: goose_providers::model::ModelConfig,
        session_id: &str,
    ) -> Result<()> {
        let provider_name = provider.get_name().to_string();
        self.config
            .providers
            .set_provider(session_id, provider)
            .await;
        self.switch_provider(session_id, &provider_name, model_config)
            .await
    }

    pub async fn switch_provider(
        &self,
        session_id: &str,
        provider_name: &str,
        model_config: goose_providers::model::ModelConfig,
    ) -> Result<()> {
        let mut session = self
            .config
            .session_manager
            .get_session(session_id, false)
            .await?;
        let registry_entry = crate::providers::get_from_registry(provider_name)
            .await
            .ok();
        let model_config = if registry_entry.is_some() {
            crate::model_config::materialize_model_config(provider_name, model_config.clone())
                .unwrap_or(model_config)
        } else {
            model_config
        };

        session.provider_name = Some(provider_name.to_string());
        session.model_config = Some(model_config.clone());
        let provider = self
            .config
            .providers
            .provider_for(&session)
            .await
            .map_err(|error| provider_creation_error(error, "Could not create provider"))?;

        let model_config = normalize_legacy_provider_thinking_effort(
            model_config,
            &provider.thinking_effort_support(),
        );
        let effective_model_config = match registry_entry {
            Some(entry) => entry
                .normalize_model_config(model_config.clone())
                .unwrap_or_else(|_| model_config.clone()),
            None => model_config.clone(),
        };

        // A provider that manages its own model has to be told about the
        // selection before the next config snapshot is built. Failures are not
        // fatal here: the selection is re-applied at stream time.
        if let Err(e) = provider
            .apply_model_selection(&effective_model_config)
            .await
        {
            warn!("Failed to apply model selection to provider: {e}");
        }

        self.config
            .session_manager
            .clone()
            .update(session_id)
            .provider_name(provider_name)
            .model_config(model_config)
            .apply()
            .await
            .context("Failed to persist provider config to session")
    }

    pub async fn update_goose_mode(&self, mode: GooseMode, session_id: &str) -> Result<()> {
        if let Ok(provider) = self.provider(session_id).await {
            provider
                .update_mode(session_id, mode)
                .await
                .map_err(|e| anyhow::anyhow!("Provider rejected mode update: {e}"))?;
        }
        self.config
            .session_manager
            .clone()
            .update(session_id)
            .goose_mode(mode)
            .apply()
            .await
            .context("Failed to persist goose_mode to session")
    }

    pub async fn goose_mode(&self, session_id: &str) -> Result<GooseMode> {
        Ok(self
            .config
            .session_manager
            .get_session(session_id, false)
            .await?
            .goose_mode)
    }

    /// Apply a thinking-effort selection. `effort` is the raw option value: a
    /// provider that manages effort through a harness has its own vocabulary,
    /// which is not always a `ThinkingEffort` member.
    pub async fn update_thinking_effort(&self, session_id: &str, effort: &str) -> Result<()> {
        let current_provider = self.provider(session_id).await?;
        // Context rather than a formatted string: the caller distinguishes a
        // value rejection from an operational failure by downcasting to
        // `ProviderError`, which stringifying would destroy.
        let provider_handled = current_provider
            .set_thinking_effort(session_id, effort)
            .await
            .context("Provider rejected thinking effort update")?;

        let model_config = self.model_config_for_session(session_id).await?;

        if provider_handled {
            let model_config = model_config.with_merged_request_params(HashMap::from([(
                "thinking_effort".to_string(),
                Value::String(effort.to_string()),
            )]));
            return self
                .config
                .session_manager
                .clone()
                .update(session_id)
                .model_config(model_config)
                .apply()
                .await
                .context("Failed to persist thinking effort to session");
        }

        let effort = effort.parse::<ThinkingEffort>().map_err(|_| {
            anyhow::Error::new(ProviderError::InvalidValue(format!(
                "Invalid thinking effort: {effort}"
            )))
        })?;
        let provider_name = current_provider.get_name().to_string();
        self.switch_provider(
            session_id,
            &provider_name,
            model_config.with_thinking_effort(effort),
        )
        .await
    }

    /// Points the session at its stored provider, falling back to the global
    /// provider when the stored one no longer exists. Returns true if it fell
    /// back.
    pub async fn restore_provider_from_session(&self, session: &Session) -> Result<bool> {
        let config = Config::global();
        let provider_name = crate::agents::provider_manager::provider_name_for(session)?;

        let mut model_config = match session.model_config.clone() {
            Some(saved_config) => crate::model_config::with_rederived_cache_ttl(saved_config)
                .map_err(|e| anyhow!("Could not configure agent: {}", e))?,
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

        if crate::providers::get_from_registry(&provider_name)
            .await
            .is_ok()
        {
            self.switch_provider(&session.id, &provider_name, model_config)
                .await?;
            return Ok(false);
        }

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
        let fallback_model_config = crate::model_config::model_config_from_user_config(
            &fallback_provider_name,
            &fallback_model_name,
        )
        .map_err(|e| anyhow!("Could not configure fallback provider: invalid model {}", e))?;

        self.switch_provider(&session.id, &fallback_provider_name, fallback_model_config)
            .await
            .map_err(|error| {
                provider_creation_error(
                    error,
                    format!(
                        "Could not create provider '{provider_name}' or fallback '{fallback_provider_name}'"
                    ),
                )
            })?;
        Ok(true)
    }

    pub async fn list_extension_prompts(&self, session_id: &str) -> HashMap<String, Vec<Prompt>> {
        self.extension_manager
            .current_lease(session_id, None)
            .await
            .list_prompts(CancellationToken::default())
            .await
    }

    pub async fn get_prompt(
        &self,
        session_id: &str,
        name: &str,
        arguments: Value,
    ) -> Result<GetPromptResult> {
        let lease = self.extension_manager.current_lease(session_id, None).await;
        let prompts = lease.list_prompts(CancellationToken::default()).await;

        if let Some(extension) = prompts
            .iter()
            .find(|(_, prompt_list)| prompt_list.iter().any(|p| p.name == name))
            .map(|(extension, _)| extension)
        {
            return lease
                .get_prompt(extension, name, arguments, CancellationToken::default())
                .await;
        }

        Err(anyhow!("Prompt '{}' not found", name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::conversation::message::InferenceMetadata;
    use crate::plugins::discovery::{DiscoveredPlugin, PluginScope};
    use crate::providers::base::{stream_from_single_message, MessageStream, ModelInfo};
    use crate::session::session_manager::SessionType;
    use goose_providers::conversation::token_usage::{ProviderUsage, Usage};
    use rmcp::model::Tool;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::TempDir;

    fn persisted_builtin(name: &str) -> ExtensionConfig {
        ExtensionConfig::Builtin {
            name: name.to_string(),
            description: String::new(),
            display_name: None,
            timeout: None,
            bundled: None,
            available_tools: Vec::new(),
        }
    }

    #[test]
    fn persisted_extension_identity_must_be_unique_before_removal() {
        let session_extension = persisted_builtin("session-only");
        assert!(has_unique_persisted_extension(&[session_extension], "session-only").unwrap());
        assert!(!has_unique_persisted_extension(&[], "missing").unwrap());

        let duplicate_result = has_unique_persisted_extension(
            &[persisted_builtin("a.b"), persisted_builtin("a/b")],
            "a_b",
        );
        assert_eq!(
            duplicate_result.unwrap_err().to_string(),
            "Duplicate session extension key 'a_b'"
        );
    }

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

    #[derive(Debug, Default)]
    struct SessionContextProvider {
        calls: std::sync::Mutex<Vec<(&'static str, Option<String>)>>,
    }

    impl SessionContextProvider {
        fn record(&self, operation: &'static str) {
            self.calls
                .lock()
                .unwrap()
                .push((operation, crate::session_context::current_session_id()));
        }

        fn calls(&self) -> Vec<(&'static str, Option<String>)> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl crate::providers::base::Provider for SessionContextProvider {
        fn get_name(&self) -> &str {
            "session-context"
        }

        async fn resume(&self, _session_id: &str) -> Result<(), ProviderError> {
            self.record("resume");
            Ok(())
        }

        async fn fetch_model_info(&self, model_name: &str) -> Result<ModelInfo, ProviderError> {
            self.record("fetch_model_info");
            Ok(ModelInfo::new(model_name).with_context_limit(32_000))
        }

        async fn get_context_limit(&self, _model: &str, _override_limit: Option<usize>) -> usize {
            self.record("get_context_limit");
            32_000
        }

        async fn stream(
            &self,
            _model_config: &goose_providers::model::ModelConfig,
            _system_prompt: &str,
            _messages: &[Message],
            _tools: &[Tool],
        ) -> Result<MessageStream, ProviderError> {
            self.record("stream");
            Ok(stream_from_single_message(
                Message::assistant().with_text("done"),
                ProviderUsage::new("mock-model".to_string(), Usage::default()),
            ))
        }
    }

    async fn session_context_agent() -> (Agent, Arc<SessionContextProvider>, SessionConfig, TempDir)
    {
        let temp_dir = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let agent = Agent::with_config(AgentConfig::new(
            Arc::clone(&session_manager),
            Arc::new(PermissionManager::new(temp_dir.path().join("permissions"))),
            None,
            true,
            GoosePlatform::GooseCli,
        ));
        let session = session_manager
            .create_session(
                temp_dir.path().to_path_buf(),
                "session-context".to_string(),
                SessionType::Hidden,
                GooseMode::default(),
            )
            .await
            .unwrap();
        let provider = Arc::new(SessionContextProvider::default());
        agent
            .update_provider(
                provider.clone(),
                goose_providers::model::ModelConfig::new("mock-model"),
                &session.id,
            )
            .await
            .unwrap();
        session_manager
            .add_message(&session.id, &Message::user().with_text("initial request"))
            .await
            .unwrap();
        session_manager
            .add_message(
                &session.id,
                &Message::assistant()
                    .with_text("previous response")
                    .with_inference(InferenceMetadata {
                        provider: provider.get_name().to_string(),
                        requested_model: "mock-model".to_string(),
                        resolved_model: None,
                        provider_session_id: Some("saved-provider-session".to_string()),
                    }),
            )
            .await
            .unwrap();

        let session_config = SessionConfig {
            id: session.id.clone(),
            schedule_id: None,
            max_turns: Some(1),
        };

        (agent, provider, session_config, temp_dir)
    }

    fn assert_session_context_calls(
        provider: &SessionContextProvider,
        session_id: &str,
        operations: &[&'static str],
    ) {
        let calls = provider.calls();
        for operation in operations {
            assert!(
                calls
                    .iter()
                    .any(|call| call == &(*operation, Some(session_id.to_string()))),
                "{operation} was not scoped to session {session_id}: {calls:?}"
            );
        }
        assert_eq!(crate::session_context::current_session_id(), None);
    }

    #[tokio::test]
    async fn reply_scopes_resume_model_info_and_stream_to_session() {
        let (agent, provider, session_config, _temp_dir) = session_context_agent().await;
        let session_id = session_config.id.clone();
        let mut events = agent
            .reply(Message::user().with_text("continue"), session_config, None)
            .await
            .unwrap();
        while let Some(event) = events.next().await {
            event.unwrap();
        }

        assert_session_context_calls(
            provider.as_ref(),
            &session_id,
            &["resume", "fetch_model_info", "stream"],
        );
    }

    #[tokio::test]
    async fn live_delegation_scopes_state_machine_setup_and_stream_to_session() {
        let (agent, provider, mut session_config, _temp_dir) = session_context_agent().await;
        session_config.max_turns = Some(2);
        let session_id = session_config.id.clone();
        let mut events = agent
            .reply_live_delegation(
                Message::user().with_text("continue"),
                session_config,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        while let Some(event) = events.next().await {
            event.unwrap();
        }

        assert_session_context_calls(
            provider.as_ref(),
            &session_id,
            &["get_context_limit", "stream"],
        );
    }

    async fn tracing_test_agent_and_session() -> (Agent, Session, TempDir) {
        let data_dir = TempDir::new().unwrap();
        let data_path = data_dir.path().to_path_buf();
        let session_manager = Arc::new(SessionManager::new(data_path.clone()));
        let agent = Agent::with_config(AgentConfig::new(
            Arc::clone(&session_manager),
            Arc::new(PermissionManager::new(data_path)),
            None,
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
    async fn test_submit_tool_confirmation_routes_to_provider() {
        let (agent, session, _data_dir) = tracing_test_agent_and_session().await;
        let provider = Arc::new(ActionRequiredProvider::new());
        agent
            .update_provider(
                provider.clone(),
                goose_providers::model::ModelConfig::new("test-model"),
                &session.id,
            )
            .await
            .unwrap();

        agent
            .submit_tool_confirmation(
                &session.id,
                "known",
                crate::permission::Permission::AllowOnce,
            )
            .await
            .unwrap();
        assert_eq!(provider.handled.lock().await.len(), 1);

        let error = agent
            .submit_tool_confirmation(
                &session.id,
                "unknown",
                crate::permission::Permission::DenyOnce,
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("unknown or stale"));
        assert_eq!(provider.handled.lock().await.len(), 2);
    }

    enum EffortOutcome {
        Applied,
        Unhandled,
        Rejected,
    }

    #[derive(Debug)]
    struct EffortProvider {
        applies_effort: bool,
        rejects_effort: bool,
        effort_calls: std::sync::Mutex<Vec<String>>,
        model_selections: std::sync::Mutex<Vec<String>>,
    }

    impl EffortProvider {
        fn new(outcome: EffortOutcome) -> Self {
            Self {
                applies_effort: matches!(outcome, EffortOutcome::Applied),
                rejects_effort: matches!(outcome, EffortOutcome::Rejected),
                effort_calls: std::sync::Mutex::new(Vec::new()),
                model_selections: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn effort_calls(&self) -> Vec<String> {
            self.effort_calls.lock().unwrap().clone()
        }

        fn model_selections(&self) -> Vec<String> {
            self.model_selections.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl crate::providers::base::Provider for EffortProvider {
        fn get_name(&self) -> &str {
            "test-effort"
        }
        fn thinking_effort_support(&self) -> ThinkingEffortSupport {
            if self.applies_effort {
                ThinkingEffortSupport::Options(
                    goose_providers::thinking::ThinkingEffortCapability {
                        option_id: "effort".to_string(),
                        values: vec![goose_providers::thinking::ThinkingEffortOption {
                            value: "default".to_string(),
                            label: "Default".to_string(),
                        }],
                        current: Some("default".to_string()),
                    },
                )
            } else {
                ThinkingEffortSupport::Unspecified
            }
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
        async fn set_thinking_effort(
            &self,
            _session_id: &str,
            value: &str,
        ) -> Result<bool, ProviderError> {
            self.effort_calls.lock().unwrap().push(value.to_string());
            if self.rejects_effort {
                return Err(ProviderError::RequestFailed("no such effort".to_string()));
            }
            Ok(self.applies_effort)
        }
        async fn apply_model_selection(
            &self,
            model_config: &goose_providers::model::ModelConfig,
        ) -> Result<(), ProviderError> {
            self.model_selections
                .lock()
                .unwrap()
                .push(model_config.model_name.clone());
            Ok(())
        }
    }

    async fn effort_test_agent(
        outcome: EffortOutcome,
    ) -> (Agent, String, Arc<EffortProvider>, TempDir) {
        let (agent, session, data_dir) = tracing_test_agent_and_session().await;
        let provider = Arc::new(EffortProvider::new(outcome));
        agent
            .update_provider(
                provider.clone(),
                goose_providers::model::ModelConfig::new("mock-model"),
                &session.id,
            )
            .await
            .unwrap();
        (agent, session.id, provider, data_dir)
    }

    async fn persisted_thinking_effort(agent: &Agent, session_id: &str) -> Option<String> {
        agent
            .model_config_for_session(session_id)
            .await
            .unwrap()
            .request_param::<String>("thinking_effort")
    }

    #[tokio::test]
    async fn update_provider_applies_the_model_selection() {
        let (_agent, _session_id, provider, _data_dir) =
            effort_test_agent(EffortOutcome::Applied).await;

        assert_eq!(provider.model_selections(), ["mock-model"]);
    }

    #[tokio::test]
    async fn provider_toolshim_is_effective_without_being_persisted() {
        let (agent, session, _data_dir) = tracing_test_agent_and_session().await;
        let provider_root = TempDir::new().unwrap();
        let provider_root_path = provider_root.path().display().to_string();
        let _guard = env_lock::lock_env([
            ("GOOSE_PATH_ROOT", Some(provider_root_path.as_str())),
            ("GOOSE_TOOLSHIM", None),
        ]);

        let config = crate::config::declarative_providers::create_custom_provider(
            crate::config::declarative_providers::CreateCustomProviderParams {
                engine: "openai".to_string(),
                display_name: "Sticky Toolshim".to_string(),
                api_url: "https://example.invalid/v1".to_string(),
                api_key: None,
                models: vec![crate::providers::base::ModelInfo::new("test-model")],
                supports_streaming: Some(true),
                headers: None,
                requires_auth: false,
                catalog_provider_id: None,
                base_path: None,
                toolshim: true,
                preserves_thinking: None,
                auth: None,
            },
        )
        .unwrap();
        crate::providers::refresh_custom_providers().await.unwrap();

        let provider = crate::providers::create(&config.name, Vec::new())
            .await
            .unwrap();
        agent
            .update_provider(
                provider,
                goose_providers::model::ModelConfig::new("test-model"),
                &session.id,
            )
            .await
            .unwrap();

        assert!(
            !agent
                .model_config_for_session(&session.id)
                .await
                .unwrap()
                .toolshim
        );
        assert!(
            agent
                .effective_model_config_for_session(&session.id)
                .await
                .unwrap()
                .toolshim
        );

        crate::config::declarative_providers::update_custom_provider(
            crate::config::declarative_providers::UpdateCustomProviderParams {
                id: config.name.clone(),
                engine: "openai".to_string(),
                display_name: config.display_name,
                api_url: config.base_url,
                api_key: None,
                models: config.models,
                supports_streaming: config.supports_streaming,
                headers: config.headers,
                requires_auth: false,
                catalog_provider_id: None,
                base_path: None,
                toolshim: false,
                preserves_thinking: None,
                auth: None,
            },
        )
        .unwrap();
        crate::providers::refresh_custom_providers().await.unwrap();

        assert!(
            !agent
                .effective_model_config_for_session(&session.id)
                .await
                .unwrap()
                .toolshim
        );

        crate::config::declarative_providers::remove_custom_provider(&config.name).unwrap();
        crate::providers::refresh_custom_providers().await.unwrap();
    }

    #[tokio::test]
    async fn update_provider_replaces_harness_only_effort_for_legacy_provider() {
        let _guard = env_lock::lock_env([("GOOSE_THINKING_EFFORT", Some("high"))]);
        let (agent, session, _data_dir) = tracing_test_agent_and_session().await;
        let provider = Arc::new(EffortProvider::new(EffortOutcome::Unhandled));
        let model_config =
            goose_providers::model::ModelConfig::new("mock-model").with_merged_request_params(
                HashMap::from([("thinking_effort".to_string(), serde_json::json!("default"))]),
            );

        agent
            .update_provider(provider, model_config, &session.id)
            .await
            .unwrap();

        assert_eq!(
            persisted_thinking_effort(&agent, &session.id)
                .await
                .as_deref(),
            Some("high")
        );
    }

    #[tokio::test]
    async fn update_provider_preserves_harness_only_effort_for_managed_provider() {
        let _guard = env_lock::lock_env([("GOOSE_THINKING_EFFORT", Some("high"))]);
        let (agent, session, _data_dir) = tracing_test_agent_and_session().await;
        let provider = Arc::new(EffortProvider::new(EffortOutcome::Applied));
        let model_config =
            goose_providers::model::ModelConfig::new("mock-model").with_merged_request_params(
                HashMap::from([("thinking_effort".to_string(), serde_json::json!("default"))]),
            );

        agent
            .update_provider(provider, model_config, &session.id)
            .await
            .unwrap();

        assert_eq!(
            persisted_thinking_effort(&agent, &session.id)
                .await
                .as_deref(),
            Some("default")
        );
    }

    #[tokio::test]
    async fn update_thinking_effort_persists_the_raw_value_when_the_provider_applies_it() {
        let (agent, session_id, provider, _data_dir) =
            effort_test_agent(EffortOutcome::Applied).await;

        // "xhigh" is a harness value, not a ThinkingEffort member spelling.
        agent
            .update_thinking_effort(&session_id, "xhigh")
            .await
            .unwrap();

        assert_eq!(provider.effort_calls(), ["xhigh"]);
        assert_eq!(
            persisted_thinking_effort(&agent, &session_id)
                .await
                .as_deref(),
            Some("xhigh")
        );
        // The unregistered test provider was not respawned.
        assert_eq!(
            agent.provider(&session_id).await.unwrap().get_name(),
            "test-effort"
        );
    }

    #[tokio::test]
    async fn update_thinking_effort_rejects_an_unparseable_value_on_the_legacy_path() {
        let (agent, session_id, provider, _data_dir) =
            effort_test_agent(EffortOutcome::Unhandled).await;

        let err = agent
            .update_thinking_effort(&session_id, "bogus")
            .await
            .unwrap_err();

        assert!(matches!(
            err.downcast_ref::<ProviderError>(),
            Some(ProviderError::InvalidValue(_))
        ));
        assert!(err.to_string().contains("Invalid thinking effort"));
        assert_eq!(provider.effort_calls(), ["bogus"]);
        assert!(persisted_thinking_effort(&agent, &session_id)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn update_thinking_effort_surfaces_a_provider_rejection() {
        let (agent, session_id, _provider, _data_dir) =
            effort_test_agent(EffortOutcome::Rejected).await;

        let err = agent
            .update_thinking_effort(&session_id, "high")
            .await
            .unwrap_err();

        assert!(err.to_string().contains("Provider rejected"));
        // The caller classifies the failure by variant, so the provider's typed
        // error has to survive the trip up.
        assert!(matches!(
            err.downcast_ref::<ProviderError>(),
            Some(ProviderError::RequestFailed(_))
        ));
        assert!(persisted_thinking_effort(&agent, &session_id)
            .await
            .is_none());
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
        };

        let reply_stream = agent
            .reply(Message::user().with_text("hi"), session_config, None)
            .await?;
        tokio::pin!(reply_stream);
        let mut emitted_refusal_id = None;
        while let Some(event) = reply_stream.next().await {
            if let AgentEvent::Message(message) = event? {
                if message.content.iter().any(|content| {
                    content.as_error().is_some_and(|error| {
                        error
                            .message
                            .contains("Please start a new session to continue")
                    })
                }) {
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
