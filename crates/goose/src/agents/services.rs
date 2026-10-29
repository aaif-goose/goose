use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures::stream::BoxStream;
use futures::{StreamExt, TryStreamExt};
use tracing_futures::Instrument;

use super::gen_ai_telemetry;
use super::tool_confirmation_coordinator::{ActiveTurnGuard, ConfirmationAnswer};
use crate::action_required_manager::ElicitationOutcome;
use crate::agents::extension_manager::{ClientContext, ExtensionManager};
use crate::agents::provider_manager::ProviderManager;
use crate::agents::session_runtime::{Attachment, SessionRuntime};
use crate::agents::state_machine::ops_recipe;
use crate::agents::state_machine::{
    has_unapplied_tool_confirmation_response, pending_tool_confirmations,
    persist_tool_confirmation_decision, run_goose, AutoEffortOperation, Emitter, GooseEffect,
    Operation, StateMachine, SteerQueue,
};
use crate::agents::state_machine_builder::{build_state_machine, StateMachineConfig};
use crate::agents::subagent_handler::ForegroundSubagentRunner;
use crate::agents::types::{
    SessionConfig, DEFAULT_ON_FAILURE_TIMEOUT_SECONDS, DEFAULT_RETRY_TIMEOUT_SECONDS,
};
use crate::agents::AgentEvent;
use crate::config::permission::PermissionManager;
use crate::config::{Config, GooseMode};
use crate::conversation::message::{ActionRequiredData, Message, MessageContent};
use crate::permission::permission_inspector::PermissionInspector;
use crate::permission::{Permission, PermissionConfirmation};
use crate::providers::base::{PermissionRouting, Provider};
use crate::scheduler_trait::SchedulerTrait;
use crate::security::adversary_inspector::AdversaryInspector;
use crate::security::egress_inspector::EgressInspector;
use crate::security::security_inspector::SecurityInspector;
use crate::session::{Session, SessionManager};
use crate::tool_inspection::ToolInspectionManager;
use crate::tool_monitor::RepetitionInspector;
use goose_agent::stop_hook::DEFAULT_DENY_LIMIT;
use goose_providers::api_client::{ApiClient, AuthMethod};
use goose_providers::errors::ProviderError;
use goose_providers::thinking::{ThinkingEffort, ThinkingEffortSupport};
use goose_providers::typesafe::{TypeSafeProvider, TYPESAFE_DEFAULT_HOST, TYPESAFE_DEFAULT_MODEL};
use rmcp::model::{ElicitationAction, Tool};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, instrument, warn};

const AUTO_EFFORTS: [ThinkingEffort; 5] = [
    ThinkingEffort::Off,
    ThinkingEffort::Low,
    ThinkingEffort::Medium,
    ThinkingEffort::High,
    ThinkingEffort::Max,
];

pub(super) fn available_auto_efforts(
    provider_name: &str,
    model_config: &goose_providers::model::ModelConfig,
    support: ThinkingEffortSupport,
) -> Vec<ThinkingEffort> {
    let applied_as_selected = |effort: &ThinkingEffort, applied: Option<String>| match effort {
        ThinkingEffort::Off => applied.as_deref() == Some("none"),
        ThinkingEffort::Low => applied.as_deref() == Some("low"),
        ThinkingEffort::Medium => applied.as_deref() == Some("medium"),
        ThinkingEffort::High => applied.as_deref() == Some("high"),
        ThinkingEffort::Max => matches!(applied.as_deref(), Some("xhigh" | "max")),
    };

    match support {
        ThinkingEffortSupport::Unspecified
            if provider_name == goose_providers::openai::OPEN_AI_PROVIDER_NAME
                && model_config.is_openai_reasoning_model() =>
        {
            AUTO_EFFORTS
                .into_iter()
                .filter(|effort| {
                    applied_as_selected(
                        effort,
                        goose_providers::formats::openai::openai_reasoning_effort_for_thinking(
                            &model_config.model_name,
                            *effort,
                        ),
                    )
                })
                .collect()
        }
        ThinkingEffortSupport::Unspecified
            if provider_name == goose_providers::formats::anthropic::ANTHROPIC_PROVIDER_NAME
                && model_config.is_reasoning_model() =>
        {
            let always_on = goose_providers::canonical::maybe_get_canonical_model(
                provider_name,
                &model_config.model_name,
            )
            .and_then(|model| model.thinking_mode)
                == Some(goose_providers::canonical::ThinkingMode::AlwaysOnAdaptive);
            AUTO_EFFORTS
                .into_iter()
                .filter(|effort| !always_on || *effort != ThinkingEffort::Off)
                .collect()
        }
        ThinkingEffortSupport::Unspecified
        | ThinkingEffortSupport::Unsupported
        | ThinkingEffortSupport::Options(_) => Vec::new(),
    }
}

/// The knobs a turn runs with.
#[derive(Clone)]
pub struct TurnSettings {
    pub max_turns: u32,
    pub retry_timeout: std::time::Duration,
    pub on_failure_timeout: std::time::Duration,
    pub stop_hook_block_cap: u32,
    pub tool_pair_summarization: bool,
    pub compaction_threshold: f64,
    pub compaction_token_limit: usize,
    /// `None` derives it from the context limit.
    pub tool_call_cutoff: Option<usize>,
    pub auto_effort: Option<Arc<dyn Operation<Session, GooseEffect>>>,
}

impl TurnSettings {
    pub fn from_config(
        provider: &dyn Provider,
        model_config: &goose_providers::model::ModelConfig,
    ) -> Self {
        let config = Config::global();
        Self {
            max_turns: config.default_max_turns(),
            retry_timeout: std::time::Duration::from_secs(
                config
                    .get_param::<u64>("GOOSE_RECIPE_RETRY_TIMEOUT_SECONDS")
                    .unwrap_or(DEFAULT_RETRY_TIMEOUT_SECONDS),
            ),
            on_failure_timeout: std::time::Duration::from_secs(
                config
                    .get_param::<u64>("GOOSE_RECIPE_ON_FAILURE_TIMEOUT_SECONDS")
                    .unwrap_or(DEFAULT_ON_FAILURE_TIMEOUT_SECONDS),
            ),
            stop_hook_block_cap: config
                .get_param::<u32>("GOOSE_STOP_HOOK_BLOCK_CAP")
                .unwrap_or(DEFAULT_DENY_LIMIT),
            tool_pair_summarization: crate::context_mgmt::tool_pair_summarization_enabled(),
            compaction_threshold: config
                .get_param::<f64>("GOOSE_AUTO_COMPACT_THRESHOLD")
                .unwrap_or(crate::context_mgmt::DEFAULT_COMPACTION_THRESHOLD),
            compaction_token_limit: config
                .get_param::<usize>("GOOSE_AUTO_COMPACT_TOKEN_LIMIT")
                .unwrap_or(crate::context_mgmt::DEFAULT_AUTO_COMPACT_TOKEN_LIMIT),
            tool_call_cutoff: config.get_param::<usize>("GOOSE_TOOL_CALL_CUTOFF").ok(),
            auto_effort: auto_effort_from_config(provider, model_config),
        }
    }
}

fn auto_effort_from_config(
    provider: &dyn Provider,
    model_config: &goose_providers::model::ModelConfig,
) -> Option<Arc<dyn Operation<Session, GooseEffect>>> {
    let config = Config::global();
    if !config
        .get_param::<bool>("GOOSE_AUTO_EFFORT_ENABLED")
        .unwrap_or(false)
    {
        return None;
    }

    let efforts = available_auto_efforts(
        provider.get_name(),
        model_config,
        provider.thinking_effort_support(),
    );
    if efforts.is_empty() {
        return None;
    }

    let api_key = config
        .get_secret::<String>("TYPESAFE_API_KEY")
        .ok()?
        .trim()
        .to_string();
    if api_key.is_empty() {
        return None;
    }

    let tls_config = crate::config::tls::provider_tls_config_from_config(config).ok()?;
    let api_client = ApiClient::with_timeout_and_tls(
        TYPESAFE_DEFAULT_HOST.to_string(),
        AuthMethod::BearerToken(api_key),
        Duration::from_secs(2),
        tls_config,
    )
    .ok()?;
    Some(Arc::new(AutoEffortOperation::new(
        Arc::new(TypeSafeProvider::new(api_client)),
        TYPESAFE_DEFAULT_MODEL.to_string(),
        efforts,
    )))
}

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
pub struct StateMachineServicesConfig {
    pub session_manager: Arc<SessionManager>,
    pub permission_manager: Arc<PermissionManager>,
    pub scheduler_service: Option<Arc<dyn SchedulerTrait>>,
    pub disable_session_naming: bool,
    pub goose_platform: GoosePlatform,
    pub elicitation_handler: Option<crate::agents::mcp_client::ElicitationHandler>,
    pub mcp_protocol_version: Option<rmcp::model::ProtocolVersion>,
    pub providers: Arc<ProviderManager>,
    /// Sessions kept loaded before the least recently used idle one is released.
    pub max_sessions: usize,
    /// `None` reads them from config at the start of every turn.
    pub turn_settings: Option<TurnSettings>,
    /// `None` loads the enabled plugins' hooks.
    pub hooks: Option<crate::hooks::HookManager>,
}

impl StateMachineServicesConfig {
    /// What a CLI process runs on: the process's session store and permissions.
    pub fn cli() -> Self {
        Self::new(
            Arc::new(SessionManager::instance()),
            PermissionManager::instance(),
            None,
            Config::global()
                .get_goose_disable_session_naming()
                .unwrap_or(false),
            GoosePlatform::GooseCli,
        )
    }

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
            elicitation_handler: None,
            mcp_protocol_version: None,
            providers: Arc::default(),
            max_sessions: Config::global()
                .get_goose_max_active_agents()
                .unwrap_or(DEFAULT_MAX_SESSIONS),
            turn_settings: None,
            hooks: None,
        }
    }
}

const DEFAULT_MAX_SESSIONS: usize = 100;

pub struct StateMachineServices {
    pub session_manager: Arc<SessionManager>,
    pub permission_manager: Arc<PermissionManager>,
    pub providers: Arc<ProviderManager>,
    pub scheduler: Option<Arc<dyn SchedulerTrait>>,
    pub goose_platform: GoosePlatform,
    pub disable_session_naming: bool,
    pub runtime: SessionRuntime,

    pub extension_manager: Arc<ExtensionManager>,

    pub(super) tool_inspection_manager: ToolInspectionManager,
    pub(super) hook_manager: crate::hooks::HookManager,
    turn_settings: Option<TurnSettings>,
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

impl StateMachineServices {
    pub fn with_config(config: StateMachineServicesConfig) -> Arc<Self> {
        let mut default_client = ClientContext::new(&config.goose_platform, None);
        default_client.capabilities.elicitation_handler = config.elicitation_handler;
        default_client.capabilities.protocol_version = config.mcp_protocol_version;
        let services = Arc::new(Self {
            extension_manager: Arc::new(ExtensionManager::new(
                config.providers.clone(),
                config.session_manager.clone(),
                config.scheduler_service.clone(),
                default_client,
            )),
            tool_inspection_manager: Self::create_tool_inspection_manager(
                config.permission_manager.clone(),
                config.providers.clone(),
                config.session_manager.clone(),
            ),
            session_manager: config.session_manager,
            permission_manager: config.permission_manager,
            providers: config.providers,
            scheduler: config.scheduler_service,
            goose_platform: config.goose_platform,
            disable_session_naming: config.disable_session_naming,
            runtime: SessionRuntime::new(config.max_sessions),
            hook_manager: config.hooks.unwrap_or_else(|| {
                crate::hooks::HookManager::load(std::env::current_dir().ok().as_deref())
            }),
            turn_settings: config.turn_settings,
        });
        services
            .runtime
            .runs
            .set_services(Arc::downgrade(&services));
        if let Some(scheduler) = &services.scheduler {
            scheduler.run_jobs_on(&services);
        }
        services
    }

    /// Emit a lifecycle hook event with no extra context. Useful for events
    /// that have no matcher (e.g. `SessionStart`, `SessionEnd`).
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
        self.session_manager
            .add_message(
                session_id,
                &crate::agents::state_machine::session_start_message(&banners),
            )
            .await?;
        Ok(banners)
    }

    pub async fn steer(&self, session_id: &str, message: Message) {
        self.runtime.steer(session_id, message).await;
    }

    pub fn discard_pending_steers(&self, session_id: &str) {
        self.runtime.discard_pending_steers(session_id);
    }

    #[cfg(test)]
    pub(crate) fn hold_turn_for_test(&self, session_id: &str) -> impl Drop {
        self.runtime
            .confirmations
            .session(session_id)
            .try_start_turn()
            .unwrap()
    }

    /// Opens the session for a connection, which from now on is who extensions start for
    /// and who gets its generated names.
    pub async fn attach_session(&self, session_id: &str, connection: u64, attachment: Attachment) {
        let evicted = {
            let _lifetime = self.runtime.lock_lifetime(session_id).await;
            self.extension_manager
                .attach_client(session_id, attachment.client.clone());
            self.runtime.attach(session_id, connection, attachment);
            self.runtime.touch(session_id)
        };
        self.evict(evicted).await;
    }

    /// Closes the session for one connection; the session is released when that was the
    /// last one. Returns whether it was.
    pub async fn detach_session(&self, session_id: &str, connection: u64) -> bool {
        let _lifetime = self.runtime.lock_lifetime(session_id).await;
        match self.runtime.detach(session_id, connection) {
            Some(latest) => {
                self.extension_manager
                    .attach_client(session_id, latest.client);
                false
            }
            None => {
                self.release(session_id).await;
                true
            }
        }
    }

    /// For a connection that went away without closing: its sessions keep running.
    pub async fn detach_connection(&self, connection: u64) {
        for session_id in self.runtime.sessions_for_connection(connection) {
            let _lifetime = self.runtime.lock_lifetime(&session_id).await;
            if let Some(latest) = self.runtime.detach(&session_id, connection) {
                self.extension_manager
                    .attach_client(&session_id, latest.client);
            }
        }
    }

    pub(crate) async fn start_prompt_run(
        &self,
        session_id: &str,
        connection: u64,
        attachment: Attachment,
        run_id: String,
        cancel_token: CancellationToken,
    ) -> std::result::Result<(), super::active_run::StartRunError> {
        let evicted = {
            let _lifetime = self.runtime.lock_lifetime(session_id).await;
            self.runtime
                .runs
                .start_prompt_run(session_id, run_id, cancel_token)?;
            self.extension_manager
                .attach_client(session_id, attachment.client.clone());
            self.runtime.attach(session_id, connection, attachment);
            self.runtime.touch(session_id)
        };
        self.evict(evicted).await;
        Ok(())
    }

    pub async fn touch_session(&self, session_id: &str) {
        let evicted = {
            let _lifetime = self.runtime.lock_lifetime(session_id).await;
            self.runtime.touch(session_id)
        };
        self.evict(evicted).await;
    }

    /// Releases what went over the limit while every session was busy.
    pub(super) async fn trim_sessions(&self) {
        self.evict(self.runtime.trim()).await;
    }

    /// Sessions are chosen for eviction before their lifetime lock is held, so each is
    /// checked again under it: one touched or busy since then is kept.
    async fn evict(&self, candidates: Vec<String>) {
        for session_id in candidates {
            let _lifetime = self.runtime.lock_lifetime(&session_id).await;
            if self.runtime.keep_resident(&session_id) {
                continue;
            }
            self.release(&session_id).await;
        }
    }

    #[cfg(test)]
    pub(crate) fn tracks_confirmations_for(&self, session_id: &str) -> bool {
        self.runtime.confirmations.tracks(session_id)
    }

    /// Stops the session's extensions, drops its provider and everything the runtime
    /// keeps for it, and cancels its run. Connections that had it open attach again on
    /// their next request.
    pub async fn release_session(&self, session_id: &str) {
        let _lifetime = self.runtime.lock_lifetime(session_id).await;
        self.release(session_id).await;
    }

    /// Releases the session unless a connection has it open.
    pub async fn release_session_if_closed(&self, session_id: &str) {
        let _lifetime = self.runtime.lock_lifetime(session_id).await;
        if !self.runtime.is_open(session_id) {
            self.release(session_id).await;
        }
    }

    async fn release(&self, session_id: &str) {
        self.runtime.forget(session_id);
        self.session_manager
            .action_required()
            .forget_forwarded_from(session_id);
        self.extension_manager.release(session_id).await;
        self.providers.release(session_id);
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
        let session = self.session_manager.get_session(session_id, false).await?;
        self.providers.provider_for(&session).await
    }

    pub async fn model_config_for_session(
        &self,
        session_id: &str,
    ) -> Result<goose_providers::model::ModelConfig> {
        let session = self.session_manager.get_session(session_id, false).await?;
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

    pub async fn list_tools(
        &self,
        session_id: &str,
        extension_name: Option<String>,
    ) -> Result<Vec<Tool>> {
        let include_final_output = extension_name.is_none();
        let lease = self.extension_manager.current_lease(session_id).await?;
        let mut prefixed_tools = match extension_name {
            Some(name) => lease.tools_for(&name).await,
            None => lease.tools().await,
        };

        if include_final_output {
            let final_output_tool = self
                .session_manager
                .get_session(session_id, false)
                .await
                .and_then(|session| ops_recipe::final_output_tool(&session));
            if let Ok(Some(final_output_tool)) = final_output_tool {
                prefixed_tools.push(final_output_tool.tool());
            }
        }

        Ok(prefixed_tools)
    }

    pub async fn submit_tool_confirmation(
        &self,
        session_id: &str,
        request_id: &str,
        permission: Permission,
    ) -> Result<()> {
        self.session_manager.get_session(session_id, false).await?;

        let (session_id, request_id) = self
            .session_manager
            .action_required()
            .answered_request(session_id, request_id)?;
        let (session_id, request_id) = (session_id.as_str(), request_id.as_str());
        let state = self.runtime.confirmations.session(session_id);
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
                self.session_manager.as_ref(),
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
        self: &Arc<Self>,
        provider: Arc<dyn Provider>,
        model_config: goose_providers::model::ModelConfig,
        context_limit: usize,
        session_config: SessionConfig,
        cancel: CancellationToken,
        steer_queue: SteerQueue,
    ) -> StateMachine<'_, Session, GooseEffect> {
        let settings = match &self.turn_settings {
            Some(settings) => settings.clone(),
            None => TurnSettings::from_config(provider.as_ref(), &model_config),
        };
        let max_turns = session_config.max_turns.unwrap_or(settings.max_turns);
        let extension_lease = self
            .runtime
            .confirmations
            .session(&session_config.id)
            .extension_lease();
        let compaction_threshold = crate::context_mgmt::capped_compaction_threshold(
            settings.compaction_threshold,
            settings.compaction_token_limit,
            context_limit,
        );
        let tool_call_cutoff = settings.tool_call_cutoff.unwrap_or_else(|| {
            crate::context_mgmt::compute_tool_call_cutoff(context_limit, compaction_threshold)
        });

        build_state_machine(StateMachineConfig {
            provider,
            model_config,
            context_limit,
            compaction_threshold,
            tool_call_cutoff,
            tool_pair_summarization: settings.tool_pair_summarization,
            session_manager: self.session_manager.clone(),
            extension_manager: Arc::clone(&self.extension_manager),
            extension_lease,
            tool_inspection_manager: &self.tool_inspection_manager,
            hook_manager: self.hook_manager.clone(),
            steer_queue,
            subagent_runner: ForegroundSubagentRunner::new(Arc::clone(self)),
            retry_timeout: settings.retry_timeout,
            on_failure_timeout: settings.on_failure_timeout,
            stop_hook_block_cap: settings.stop_hook_block_cap,
            max_turns,
            extra_operations: settings.auto_effort.into_iter().collect(),
            cancel,
        })
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
        self: &Arc<Self>,
        user_message: Message,
        session_config: SessionConfig,
        cancel_token: Option<CancellationToken>,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let reply_span = tracing::Span::current();
        let session_id = session_config.id.clone();
        let events = crate::session_context::with_session_id(
            Some(session_id),
            self.start_reply(
                user_message,
                session_config,
                cancel_token.unwrap_or_default(),
            ),
        )
        .await?;
        // This is the single live-event boundary. Callers that intentionally stream
        // multiple events for one logical message must assign their shared ID before this point.
        Ok(Box::pin(
            events
                .try_filter_map(|event| async move { Ok(user_event(event)) })
                .instrument(reply_span),
        ))
    }

    async fn start_reply(
        self: &Arc<Self>,
        user_message: Message,
        session_config: SessionConfig,
        cancel: CancellationToken,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let user_message = user_message.with_generated_id_if_missing();
        let session_manager = self.session_manager.clone();
        let session_id = session_config.id.clone();

        let message_text_for_trace = agent_visible_message_text(&user_message);
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

        let (turn_guard, evicted) = {
            let _lifetime = self.runtime.lock_lifetime(&session_id).await;
            let turn_guard = self
                .runtime
                .confirmations
                .session(&session_id)
                .try_start_turn()?;
            let evicted = self.runtime.touch(&session_id);
            (turn_guard, evicted)
        };
        self.evict(evicted).await;
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

        if !self.disable_session_naming {
            let provider = self.provider(&session_config.id).await?;
            let manager = session_manager.clone();
            let tx = self.runtime.session_names(&session_id);
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

        Ok(self.turn(session_config, cancel, turn_guard, true))
    }

    /// Runs a subagent on the conversation saved for it.
    pub(super) fn run_subagent_turn(
        self: &Arc<Self>,
        session_config: SessionConfig,
        cancel: CancellationToken,
    ) -> Result<BoxStream<'static, Result<AgentEvent>>> {
        let turn_guard = self
            .runtime
            .confirmations
            .session(&session_config.id)
            .try_start_turn()?;
        turn_guard.state().start_new_turn();
        Ok(self.turn(session_config, cancel, turn_guard, true))
    }

    /// Continues a turn that stopped for tool confirmations, or `None` when there is none.
    pub(crate) async fn resume_turn(
        self: &Arc<Self>,
        session_config: SessionConfig,
        cancel: CancellationToken,
    ) -> Result<Option<BoxStream<'static, Result<AgentEvent>>>> {
        let session = self
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
            .runtime
            .confirmations
            .session(&session_config.id)
            .try_start_turn()?;
        for request in pending_confirmations {
            turn_guard.state().register_request(request.id);
        }
        Ok(Some(self.turn(
            session_config,
            cancel,
            turn_guard,
            resume_from_persisted_response,
        )))
    }

    fn tool_confirmation_request_ids(&self, event: &AgentEvent) -> Vec<String> {
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
                let forwarded = self.session_manager.action_required().is_forwarded(id);
                (!forwarded).then(|| id.clone())
            })
            .collect()
    }

    /// Runs the state machine, and again each time the confirmations it stopped for are
    /// all answered. `run_first` is false when the turn starts by waiting for answers.
    fn turn(
        self: &Arc<Self>,
        session_config: SessionConfig,
        cancel: CancellationToken,
        turn_guard: ActiveTurnGuard,
        run_first: bool,
    ) -> BoxStream<'static, Result<AgentEvent>> {
        let session_id = session_config.id.clone();
        let services = Arc::clone(self);
        let turn_end = TurnEnd {
            services: Arc::clone(self),
            turn_guard: Some(turn_guard),
        };
        let events = async_stream::try_stream! {
            let mut turn_end = turn_end;
            let turn_guard = turn_end.turn_guard.as_ref().expect("held until the turn ends");
            let mut run = run_first;
            loop {
                if run {
                    let mut stream = services
                        .run_machine(session_config.clone(), cancel.clone())
                        .await?;
                    let mut has_confirmations = false;
                    while let Some(event) = stream.next().await {
                        let event = event?;
                        for request_id in services.tool_confirmation_request_ids(&event) {
                            turn_guard.state().register_request(request_id);
                            has_confirmations = true;
                        }
                        yield event;
                    }
                    if !has_confirmations {
                        break;
                    }
                }
                if !turn_guard
                    .state()
                    .wait_for_all_confirmation_answers(&cancel)
                    .await
                {
                    break;
                }
                run = true;
            }
            turn_guard.state().clear_confirmations();
            turn_end.finish().await;
        };
        crate::session_context::with_session_id_stream(Some(session_id), Box::pin(events))
    }

    async fn run_machine(
        self: &Arc<Self>,
        session_config: SessionConfig,
        cancel: CancellationToken,
    ) -> Result<BoxStream<'_, Result<AgentEvent>>> {
        let session_manager = self.session_manager.clone();
        let session_id = session_config.id.clone();
        let provider = self.provider(&session_id).await?;
        let model_config = self.effective_model_config_for_session(&session_id).await?;

        let session = session_manager.get_session(&session_id, true).await?;
        let saved_provider_session_id = session
            .conversation
            .as_ref()
            .and_then(|conversation| {
                super::latest_provider_session_id(conversation.messages(), provider.get_name())
            })
            .map(str::to_string);
        if let Some(saved_provider_session_id) = saved_provider_session_id {
            if let Err(error) = provider.resume(&saved_provider_session_id).await {
                warn!(
                    provider = provider.get_name(),
                    %error,
                    "Could not resume provider session; continuing with a handoff"
                );
            }
        }
        if let Err(error) = provider.apply_model_selection(&model_config).await {
            warn!(
                provider = provider.get_name(),
                %error,
                "Could not apply model selection before building state machine"
            );
        }

        let context_limit =
            crate::context_limit::get_context_limit(provider.as_ref(), &model_config.model_name)
                .await?;
        let steer_queue = self.runtime.steer_queue(&session_id);
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
                let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();
                let emit = Emitter::new(tx, cancel.clone());
                let result = {
                    let run = crate::session_context::with_session_id(
                        Some(session_id.clone()),
                        run_goose(
                            &machine,
                            session_manager.as_ref(),
                            &self.hook_manager,
                            &session_id,
                            &emit,
                        ),
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

    /// Pins `provider` to the session instead of letting the provider manager
    /// build one.
    pub async fn update_provider(
        &self,
        provider: Arc<dyn Provider>,
        model_config: goose_providers::model::ModelConfig,
        session_id: &str,
    ) -> Result<()> {
        let provider_name = provider.get_name().to_string();
        self.providers.set_provider(session_id, provider).await;
        self.switch_provider(session_id, &provider_name, model_config)
            .await
    }

    pub async fn switch_provider(
        &self,
        session_id: &str,
        provider_name: &str,
        model_config: goose_providers::model::ModelConfig,
    ) -> Result<()> {
        let mut session = self.session_manager.get_session(session_id, false).await?;
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

        self.session_manager
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
        self.session_manager
            .clone()
            .update(session_id)
            .goose_mode(mode)
            .apply()
            .await
            .context("Failed to persist goose_mode to session")
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
}

/// Ends a turn however its stream stops (finished, failed, cancelled or dropped): frees
/// the session and trims what went over the limit while every session was busy.
struct TurnEnd {
    services: Arc<StateMachineServices>,
    turn_guard: Option<ActiveTurnGuard>,
}

impl TurnEnd {
    async fn finish(&mut self) {
        drop(self.turn_guard.take());
        self.services.trim_sessions().await;
    }
}

impl Drop for TurnEnd {
    fn drop(&mut self) {
        if self.turn_guard.take().is_none() {
            return;
        }
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let services = Arc::clone(&self.services);
            handle.spawn(async move { services.trim_sessions().await });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod session_lru {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use tempfile::TempDir;

        use goose_test_support::mcp::McpFixtureServer;
        use rmcp::ServiceExt;
        use tokio::sync::Barrier;

        use crate::agents::extension::ExtensionConfig;
        use crate::agents::{GoosePlatform, StateMachineServices, StateMachineServicesConfig};
        use crate::config::permission::PermissionManager;
        use crate::config::GooseMode;
        use crate::session::{EnabledExtensionsState, ExtensionState, SessionManager, SessionType};

        fn test_services(temp_dir: &TempDir, max_sessions: usize) -> Arc<StateMachineServices> {
            let mut config = StateMachineServicesConfig::new(
                Arc::new(SessionManager::new(temp_dir.path().to_path_buf())),
                PermissionManager::instance(),
                None,
                true,
                GoosePlatform::GooseDesktop,
            );
            config.max_sessions = max_sessions;
            StateMachineServices::with_config(config)
        }

        fn serve_fixture(read: tokio::io::DuplexStream, write: tokio::io::DuplexStream) {
            tokio::spawn(async move {
                let running = McpFixtureServer::new().serve((read, write)).await.unwrap();
                let _ = running.waiting().await;
            });
        }

        static CONCURRENT_STARTS: AtomicUsize = AtomicUsize::new(0);

        fn serve_concurrent_fixture(read: tokio::io::DuplexStream, write: tokio::io::DuplexStream) {
            CONCURRENT_STARTS.fetch_add(1, Ordering::SeqCst);
            serve_fixture(read, write);
        }

        static EVICTION_STARTS: AtomicUsize = AtomicUsize::new(0);

        fn serve_eviction_fixture(read: tokio::io::DuplexStream, write: tokio::io::DuplexStream) {
            EVICTION_STARTS.fetch_add(1, Ordering::SeqCst);
            serve_fixture(read, write);
        }

        async fn session_with_fixture(
            services: &StateMachineServices,
            temp_dir: &TempDir,
            fixture: &'static str,
            serve: crate::builtin_extension::SpawnServerFn,
        ) -> String {
            crate::builtin_extension::register_builtin_extension(fixture, serve);
            let session_manager = &services.session_manager;
            let session = session_manager
                .create_session(
                    temp_dir.path().to_path_buf(),
                    "fixture".to_string(),
                    SessionType::User,
                    GooseMode::default(),
                )
                .await
                .unwrap();
            let extension = ExtensionConfig::Builtin {
                name: fixture.to_string(),
                display_name: None,
                description: String::new(),
                timeout: None,
                bundled: None,
                available_tools: vec![],
            };
            let mut extension_data = session.extension_data.clone();
            EnabledExtensionsState::new(vec![extension])
                .to_extension_data(&mut extension_data)
                .unwrap();
            session_manager
                .update(&session.id)
                .extension_data(extension_data)
                .apply()
                .await
                .unwrap();
            session.id
        }

        async fn use_session(services: &StateMachineServices, session_id: &str) {
            services.touch_session(session_id).await;
            services
                .extension_manager
                .current_lease(session_id)
                .await
                .unwrap()
                .start()
                .await;
        }

        #[tokio::test]
        async fn concurrent_first_requests_start_extensions_once() {
            let temp_dir = TempDir::new().unwrap();
            let services = test_services(&temp_dir, 100);
            let session_id = session_with_fixture(
                &services,
                &temp_dir,
                "concurrent_fixture",
                serve_concurrent_fixture,
            )
            .await;

            let callers = 20;
            let barrier = Arc::new(Barrier::new(callers));
            let handles = (0..callers).map(|_| {
                let services = Arc::clone(&services);
                let session_id = session_id.clone();
                let barrier = Arc::clone(&barrier);
                tokio::spawn(async move {
                    barrier.wait().await;
                    use_session(&services, &session_id).await;
                })
            });
            for handle in futures::future::join_all(handles).await {
                handle.unwrap();
            }

            assert_eq!(CONCURRENT_STARTS.load(Ordering::SeqCst), 1);
        }

        fn attachment(name: &str) -> crate::agents::Attachment {
            let mut client = crate::agents::extension_manager::ClientContext::new(
                &GoosePlatform::GooseDesktop,
                None,
            );
            client.name = name.to_string();
            crate::agents::Attachment {
                client,
                session_names: None,
            }
        }

        #[tokio::test]
        async fn closing_on_one_connection_leaves_the_others_attached() {
            let temp_dir = TempDir::new().unwrap();
            let services = test_services(&temp_dir, 100);
            services.attach_session("s", 1, attachment("first")).await;
            services.attach_session("s", 2, attachment("second")).await;
            services.attach_session("s", 1, attachment("first")).await;

            assert!(!services.detach_session("s", 1).await);
            assert!(services.runtime.is_attached("s", 2));
            assert_eq!(services.extension_manager.client("s").name, "second");

            assert!(services.detach_session("s", 2).await);
            assert!(!services.runtime.is_open("s"));
        }

        #[tokio::test]
        async fn releasing_a_session_makes_every_connection_attach_again() {
            let temp_dir = TempDir::new().unwrap();
            let services = test_services(&temp_dir, 100);
            services.attach_session("s", 1, attachment("first")).await;
            services.attach_session("s", 2, attachment("second")).await;

            services.release_session("s").await;

            assert!(!services.runtime.is_attached("s", 1));
            assert!(!services.runtime.is_attached("s", 2));
        }

        #[tokio::test]
        async fn touching_a_session_never_evicts_it() {
            let temp_dir = TempDir::new().unwrap();
            let services = test_services(&temp_dir, 1);
            services
                .attach_session("busy", 1, attachment("client"))
                .await;
            let _turn = services.hold_turn_for_test("busy");

            services
                .attach_session("new", 1, attachment("client"))
                .await;

            assert!(services.runtime.is_attached("new", 1));
            assert!(services.runtime.is_attached("busy", 1));
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn a_release_never_clears_a_session_attached_while_it_ran() {
            let temp_dir = TempDir::new().unwrap();
            let services = test_services(&temp_dir, 100);
            for _ in 0..500 {
                services.attach_session("s", 1, attachment("first")).await;
                let closing = tokio::spawn({
                    let services = Arc::clone(&services);
                    async move { services.detach_session("s", 1).await }
                });
                let opening = tokio::spawn({
                    let services = Arc::clone(&services);
                    async move { services.attach_session("s", 2, attachment("second")).await }
                });
                closing.await.unwrap();
                opening.await.unwrap();

                if services.runtime.is_attached("s", 2) {
                    assert_eq!(services.extension_manager.client("s").name, "second");
                }
                services.release_session("s").await;
            }
        }

        #[tokio::test]
        async fn a_live_conversation_keeps_its_session() {
            let temp_dir = TempDir::new().unwrap();
            let services = test_services(&temp_dir, 1);
            services
                .attach_session("live", 1, attachment("client"))
                .await;
            assert!(services.runtime.runs.start_live("live"));

            services
                .attach_session("new", 1, attachment("client"))
                .await;

            assert!(services.runtime.is_attached("live", 1));
        }

        #[tokio::test]
        async fn eviction_releases_the_least_recently_used_idle_session() {
            let temp_dir = TempDir::new().unwrap();
            let services = test_services(&temp_dir, 2);
            let mut sessions = Vec::new();
            for _ in 0..3 {
                sessions.push(
                    session_with_fixture(
                        &services,
                        &temp_dir,
                        "eviction_fixture",
                        serve_eviction_fixture,
                    )
                    .await,
                );
            }
            let [busy, idle, recent] = <[String; 3]>::try_from(sessions).unwrap();
            for session_id in [&busy, &idle] {
                use_session(&services, session_id).await;
            }
            let _turn = services.hold_turn_for_test(&busy);
            let starts_before = EVICTION_STARTS.load(Ordering::SeqCst);

            use_session(&services, &recent).await;
            use_session(&services, &busy).await;
            assert_eq!(
                EVICTION_STARTS.load(Ordering::SeqCst) - starts_before,
                1,
                "only the new session starts; the busy one keeps its extensions"
            );

            use_session(&services, &idle).await;
            assert_eq!(
                EVICTION_STARTS.load(Ordering::SeqCst) - starts_before,
                2,
                "the evicted idle session starts its extensions again"
            );
        }
    }

    use crate::conversation::message::InferenceMetadata;
    use crate::plugins::discovery::{DiscoveredPlugin, PluginScope};
    use crate::providers::base::{stream_from_single_message, MessageStream, ModelInfo};
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

    async fn session_context_agent() -> (
        Arc<StateMachineServices>,
        Arc<SessionContextProvider>,
        SessionConfig,
        TempDir,
    ) {
        let temp_dir = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let services = StateMachineServices::with_config(StateMachineServicesConfig::new(
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
        services
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

        (services, provider, session_config, temp_dir)
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
        let (services, provider, session_config, _temp_dir) = session_context_agent().await;
        let session_id = session_config.id.clone();
        let mut events = services
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
    async fn an_agent_only_reply_scopes_state_machine_setup_and_stream_to_session() {
        let (services, provider, mut session_config, _temp_dir) = session_context_agent().await;
        session_config.max_turns = Some(2);
        let session_id = session_config.id.clone();
        let mut events = services
            .reply(
                Message::user().with_text("continue").agent_only(),
                session_config,
                Some(CancellationToken::new()),
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

    async fn tracing_test_agent_and_session() -> (Arc<StateMachineServices>, Session, TempDir) {
        let data_dir = TempDir::new().unwrap();
        let data_path = data_dir.path().to_path_buf();
        let session_manager = Arc::new(SessionManager::new(data_path.clone()));
        let services = StateMachineServices::with_config(StateMachineServicesConfig::new(
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
        (services, session, data_dir)
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
        let (services, session, _data_dir) = tracing_test_agent_and_session().await;
        let provider = Arc::new(ActionRequiredProvider::new());
        services
            .update_provider(
                provider.clone(),
                goose_providers::model::ModelConfig::new("test-model"),
                &session.id,
            )
            .await
            .unwrap();

        services
            .submit_tool_confirmation(
                &session.id,
                "known",
                crate::permission::Permission::AllowOnce,
            )
            .await
            .unwrap();
        assert_eq!(provider.handled.lock().await.len(), 1);

        let error = services
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
    ) -> (
        Arc<StateMachineServices>,
        String,
        Arc<EffortProvider>,
        TempDir,
    ) {
        let (services, session, data_dir) = tracing_test_agent_and_session().await;
        let provider = Arc::new(EffortProvider::new(outcome));
        services
            .update_provider(
                provider.clone(),
                goose_providers::model::ModelConfig::new("mock-model"),
                &session.id,
            )
            .await
            .unwrap();
        (services, session.id, provider, data_dir)
    }

    async fn persisted_thinking_effort(
        services: &Arc<StateMachineServices>,
        session_id: &str,
    ) -> Option<String> {
        services
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
        let (services, session, _data_dir) = tracing_test_agent_and_session().await;
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
        services
            .update_provider(
                provider,
                goose_providers::model::ModelConfig::new("test-model"),
                &session.id,
            )
            .await
            .unwrap();

        assert!(
            !services
                .model_config_for_session(&session.id)
                .await
                .unwrap()
                .toolshim
        );
        assert!(
            services
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
            !services
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
        let (services, session, _data_dir) = tracing_test_agent_and_session().await;
        let provider = Arc::new(EffortProvider::new(EffortOutcome::Unhandled));
        let model_config =
            goose_providers::model::ModelConfig::new("mock-model").with_merged_request_params(
                HashMap::from([("thinking_effort".to_string(), serde_json::json!("default"))]),
            );

        services
            .update_provider(provider, model_config, &session.id)
            .await
            .unwrap();

        assert_eq!(
            persisted_thinking_effort(&services, &session.id)
                .await
                .as_deref(),
            Some("high")
        );
    }

    #[tokio::test]
    async fn update_provider_preserves_harness_only_effort_for_managed_provider() {
        let _guard = env_lock::lock_env([("GOOSE_THINKING_EFFORT", Some("high"))]);
        let (services, session, _data_dir) = tracing_test_agent_and_session().await;
        let provider = Arc::new(EffortProvider::new(EffortOutcome::Applied));
        let model_config =
            goose_providers::model::ModelConfig::new("mock-model").with_merged_request_params(
                HashMap::from([("thinking_effort".to_string(), serde_json::json!("default"))]),
            );

        services
            .update_provider(provider, model_config, &session.id)
            .await
            .unwrap();

        assert_eq!(
            persisted_thinking_effort(&services, &session.id)
                .await
                .as_deref(),
            Some("default")
        );
    }

    #[tokio::test]
    async fn update_thinking_effort_persists_the_raw_value_when_the_provider_applies_it() {
        let (services, session_id, provider, _data_dir) =
            effort_test_agent(EffortOutcome::Applied).await;

        // "xhigh" is a harness value, not a ThinkingEffort member spelling.
        services
            .update_thinking_effort(&session_id, "xhigh")
            .await
            .unwrap();

        assert_eq!(provider.effort_calls(), ["xhigh"]);
        assert_eq!(
            persisted_thinking_effort(&services, &session_id)
                .await
                .as_deref(),
            Some("xhigh")
        );
        // The unregistered test provider was not respawned.
        assert_eq!(
            services.provider(&session_id).await.unwrap().get_name(),
            "test-effort"
        );
    }

    #[tokio::test]
    async fn update_thinking_effort_rejects_an_unparseable_value_on_the_legacy_path() {
        let (services, session_id, provider, _data_dir) =
            effort_test_agent(EffortOutcome::Unhandled).await;

        let err = services
            .update_thinking_effort(&session_id, "bogus")
            .await
            .unwrap_err();

        assert!(matches!(
            err.downcast_ref::<ProviderError>(),
            Some(ProviderError::InvalidValue(_))
        ));
        assert!(err.to_string().contains("Invalid thinking effort"));
        assert_eq!(provider.effort_calls(), ["bogus"]);
        assert!(persisted_thinking_effort(&services, &session_id)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn update_thinking_effort_surfaces_a_provider_rejection() {
        let (services, session_id, _provider, _data_dir) =
            effort_test_agent(EffortOutcome::Rejected).await;

        let err = services
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
        assert!(persisted_thinking_effort(&services, &session_id)
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
        let (services, session_id) = create_test_agent(
            temp_dir.path().join("data"),
            hook_manager,
            provider.clone(),
            None,
        )
        .await?;

        let session_config = SessionConfig {
            id: session_id,
            schedule_id: None,
            max_turns: Some(10),
        };

        let reply_stream = services
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
        turn_settings: Option<TurnSettings>,
    ) -> Result<(Arc<StateMachineServices>, String)> {
        let session_manager = Arc::new(SessionManager::new(data_dir.clone()));
        let permission_manager = Arc::new(PermissionManager::new(data_dir));
        let mut config = StateMachineServicesConfig::new(
            session_manager.clone(),
            permission_manager,
            None,
            true,
            GoosePlatform::GooseCli,
        );
        config.hooks = Some(hook_manager);
        config.turn_settings = turn_settings;
        let services = StateMachineServices::with_config(config);
        let session = session_manager
            .create_session(
                PathBuf::default(),
                "test".to_string(),
                SessionType::Hidden,
                GooseMode::Auto,
            )
            .await?;
        services
            .update_provider(
                provider,
                goose_providers::model::ModelConfig::new("mock-model"),
                &session.id,
            )
            .await?;
        Ok((services, session.id))
    }

    async fn create_stop_hook_test_agent(
        env: &StopHookTestEnv,
        stop_hook_block_cap: u32,
    ) -> Result<(Arc<StateMachineServices>, String, Arc<CountingTextProvider>)> {
        let provider = Arc::new(CountingTextProvider::new());
        let (services, session_id) = create_test_agent(
            env.data_dir(),
            env.hook_manager(),
            provider.clone(),
            Some(TurnSettings {
                stop_hook_block_cap,
                ..TurnSettings::from_config(
                    provider.as_ref(),
                    &goose_providers::model::ModelConfig::new("stop-hook-test"),
                )
            }),
        )
        .await?;
        Ok((services, session_id, provider))
    }

    async fn run_stop_hook_test_turn(
        services: &Arc<StateMachineServices>,
        session_id: &str,
        text: &str,
    ) -> Result<Vec<Message>> {
        let session_config = SessionConfig {
            id: session_id.to_string(),
            schedule_id: None,
            max_turns: Some(10),
        };
        let reply_stream = services
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
        let (services, session_id) =
            create_test_agent(temp_dir.path().join("data"), hook_manager, provider, None).await?;

        let messages = run_stop_hook_test_turn(&services, &session_id, "hello").await?;
        let marker = messages
            .iter()
            .find(|message| message.metadata.output_token_limit_reached)
            .expect("output-limit marker should be emitted");
        assert!(marker.content.is_empty());
        assert_eq!(marker.id.as_deref(), Some("provider-output-limit"));

        let session = services
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
        let (services, session_id) = create_test_agent(
            temp_dir.path().join("data"),
            hook_manager,
            provider.clone(),
            None,
        )
        .await?;

        run_stop_hook_test_turn(&services, &session_id, "hello").await?;

        assert_eq!(provider.call_count(), 1);
        let session = services
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
        let (services, session_id) =
            create_test_agent(env.data_dir(), env.hook_manager(), provider.clone(), None).await?;

        run_stop_hook_test_turn(&services, &session_id, "first").await?;
        run_stop_hook_test_turn(&services, &session_id, "second").await?;

        assert_eq!(env.hook_invocations(), 1);
        assert_eq!(provider.call_count(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn stop_hook_block_cap_allows_configured_consecutive_blocks_then_overrides() -> Result<()>
    {
        let env = StopHookTestEnv::new(ALWAYS_BLOCK_SCRIPT)?;
        let (services, session_id, provider) = create_stop_hook_test_agent(&env, 2).await?;

        let messages = run_stop_hook_test_turn(&services, &session_id, "hello").await?;
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

        let stored_session = services
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
        let (services, session_id, provider) = create_stop_hook_test_agent(&env, 1).await?;

        let first_turn = run_stop_hook_test_turn(&services, &session_id, "first").await?;
        let second_turn = run_stop_hook_test_turn(&services, &session_id, "second").await?;
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
        let (services, session_id) =
            create_test_agent(env.data_dir(), env.hook_manager(), provider, None).await?;

        let messages = run_stop_hook_test_turn(&services, &session_id, "hello").await?;
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
        let services = StateMachineServices::with_config(StateMachineServicesConfig::cli());

        // Verify that the tool inspection manager has all expected inspectors
        let inspector_names = services.tool_inspection_manager.inspector_names();

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
