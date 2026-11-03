use std::collections::HashSet;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result;
use rmcp::model::ElicitationAction;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::calculator_extension::CalculatorExtension;
use super::dummy_api::{DummyApi, ProviderFeatures};
use crate::action_required_manager::ElicitationOutcome;
use crate::agents::extension::ExtensionConfig;
use crate::agents::extension_manager::ExtensionLease;
use crate::agents::mcp_client::McpClientTrait;
use crate::agents::state_machine::{
    messages_since_kickoff, Emitter, GooseEffect, Operation, RunStatus, StateMachine,
};
use crate::agents::{AgentEvent, StateMachineServices, StateMachineServicesConfig, TurnSettings};
use crate::config::permission::{PermissionLevel, PermissionManager};
use crate::config::GooseMode;
use crate::conversation::message::{ActionRequiredData, Message, MessageContent, MessageErrorKind};
use crate::conversation::Conversation;
use crate::hooks::HookManager;
use crate::permission::Permission;
use crate::providers::base::Provider;
use crate::session::extension_data::EnabledExtensionsState;
use crate::session::{GoalState, Session, SessionManager, SessionType};
use goose_providers::model::ModelConfig;
use goose_providers::thinking::ThinkingEffort;

struct FeatureProvider {
    inner: Arc<dyn Provider>,
    features: ProviderFeatures,
}

struct RecordingProvider {
    inner: Arc<dyn Provider>,
    model_configs: Arc<StdMutex<Vec<ModelConfig>>>,
}

#[async_trait::async_trait]
impl Provider for RecordingProvider {
    fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    fn provider_session_id(&self) -> Option<String> {
        self.inner.provider_session_id()
    }

    async fn resume(&self, session_id: &str) -> Result<(), goose_providers::errors::ProviderError> {
        self.inner.resume(session_id).await
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[rmcp::model::Tool],
    ) -> Result<crate::providers::base::MessageStream, goose_providers::errors::ProviderError> {
        self.model_configs
            .lock()
            .unwrap()
            .push(model_config.clone());
        self.inner
            .stream(model_config, system, messages, tools)
            .await
    }

    async fn get_context_limit(&self, model: &str, override_limit: Option<usize>) -> usize {
        self.inner.get_context_limit(model, override_limit).await
    }

    fn manages_own_context(&self) -> bool {
        self.inner.manages_own_context()
    }

    async fn fetch_model_info(
        &self,
        model_name: &str,
    ) -> Result<goose_providers::base::ModelInfo, goose_providers::errors::ProviderError> {
        self.inner.fetch_model_info(model_name).await
    }
}

#[async_trait::async_trait]
impl Provider for FeatureProvider {
    fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[rmcp::model::Tool],
    ) -> Result<crate::providers::base::MessageStream, goose_providers::errors::ProviderError> {
        self.inner
            .stream(model_config, system, messages, tools)
            .await
    }

    async fn get_context_limit(&self, model: &str, override_limit: Option<usize>) -> usize {
        self.inner.get_context_limit(model, override_limit).await
    }

    fn manages_own_context(&self) -> bool {
        self.features.manages_own_context
    }

    async fn fetch_model_info(
        &self,
        model_name: &str,
    ) -> Result<goose_providers::base::ModelInfo, goose_providers::errors::ProviderError> {
        let mut model_info = self.inner.fetch_model_info(model_name).await?;
        if let Some(resolved_model) = self.features.resolved_model {
            model_info.resolved_model = Some(resolved_model.to_string());
        }
        Ok(model_info)
    }
}

pub(super) const MAX_TURNS: u32 = 25;
pub(super) const COMPACTION_THRESHOLD: f64 = 0.8;

pub(super) struct TestPipeline {
    pub(super) session_manager: Arc<SessionManager>,
    api: Arc<DummyApi>,
    provider_features: ProviderFeatures,
    provider: Arc<dyn Provider>,
    model_config: ModelConfig,
    permission_manager: Arc<PermissionManager>,
    hook_manager: HookManager,
    turn_settings: TurnSettings,
    max_turns: u32,
    calculator: Arc<CalculatorExtension>,
    pub(super) session_id: String,
    session: Session,
    working_dir: std::path::PathBuf,
    scheduler: Option<Arc<dyn crate::scheduler_trait::SchedulerTrait>>,
    recorded_model_configs: Option<Arc<StdMutex<Vec<ModelConfig>>>>,
    /// Built on first use, so the `with_*` settings above are what it runs with.
    services: tokio::sync::OnceCell<Arc<StateMachineServices>>,
    _temp_dir: Arc<tempfile::TempDir>,
}

impl TestPipeline {
    pub(super) async fn services(&self) -> &Arc<StateMachineServices> {
        self.services
            .get_or_init(|| async {
                let mut config = StateMachineServicesConfig::new(
                    self.session_manager.clone(),
                    self.permission_manager.clone(),
                    self.scheduler.clone(),
                    true,
                    crate::agents::GoosePlatform::GooseCli,
                );
                config.turn_settings = Some(self.turn_settings.clone());
                config.hooks = Some(self.hook_manager.clone());
                let services = StateMachineServices::with_config(config);
                services
                    .providers
                    .set_provider(&self.session_id, self.provider.clone())
                    .await;
                enable_session_extensions(&services, &self.session, &self.calculator)
                    .await
                    .expect("test session extensions start");
                services
            })
            .await
    }

    fn extension_lease(
        &self,
        services: &StateMachineServices,
    ) -> Arc<StdMutex<Option<Arc<ExtensionLease>>>> {
        services
            .runtime
            .confirmations
            .session(&self.session_id)
            .extension_lease()
    }

    async fn start_new_turn(&self) {
        let services = self.services().await;
        *self.extension_lease(services).lock().unwrap() = None;
    }

    pub(super) async fn machine(
        &self,
        cancel: CancellationToken,
    ) -> StateMachine<'_, Session, GooseEffect> {
        let services = self.services().await;
        services
            .create_state_machine(
                self.provider.clone(),
                self.model_config.clone(),
                self.model_config.context_limit(),
                crate::agents::SessionConfig {
                    id: self.session_id.clone(),
                    schedule_id: None,
                    max_turns: Some(self.max_turns),
                },
                cancel,
                services.runtime.steer_queue(&self.session_id),
            )
            .await
    }

    pub(super) async fn with_goose_mode(self, mode: GooseMode) -> Self {
        self.session_manager
            .update(&self.session_id)
            .goose_mode(mode)
            .apply()
            .await
            .unwrap();
        self
    }

    pub(super) fn context_limit(&self) -> usize {
        self.model_config.context_limit()
    }

    pub(super) async fn with_model(mut self, model: &str) -> Self {
        self.model_config = ModelConfig::new(model).with_canonical_limits("openai");
        self.session_manager
            .update(&self.session_id)
            .model_config(self.model_config.clone())
            .apply()
            .await
            .unwrap();
        self
    }

    pub(super) async fn with_model_config(mut self, model_config: ModelConfig) -> Self {
        self.model_config = model_config;
        self.session_manager
            .update(&self.session_id)
            .model_config(self.model_config.clone())
            .apply()
            .await
            .unwrap();
        self
    }

    pub(super) async fn with_provider_name(self, provider_name: &str) -> Result<Self> {
        self.session_manager
            .update(&self.session_id)
            .provider_name(provider_name)
            .apply()
            .await?;
        self.reconstruct().await
    }

    pub(super) fn provider(&self) -> &dyn Provider {
        self.provider.as_ref()
    }

    pub(super) fn with_context_relevance(
        mut self,
        decisions: Arc<dyn goose_providers::decision::DecisionProvider>,
    ) -> Self {
        assert!(
            !self.services.initialized(),
            "configure the pipeline before using it"
        );
        self.turn_settings.context_relevance = Some(decisions);
        self
    }

    pub(super) fn with_max_turns(mut self, max_turns: u32) -> Self {
        self.max_turns = max_turns;
        self
    }

    pub(super) fn with_auto_effort(
        mut self,
        operation: Arc<dyn Operation<Session, GooseEffect>>,
    ) -> Self {
        assert!(
            !self.services.initialized(),
            "configure the pipeline before using it"
        );
        self.turn_settings.auto_effort = Some(operation);
        self
    }

    pub(super) fn record_model_configs(mut self) -> Self {
        assert!(
            !self.services.initialized(),
            "configure the pipeline before using it"
        );
        let model_configs = Arc::new(StdMutex::new(Vec::new()));
        self.provider = Arc::new(RecordingProvider {
            inner: self.provider,
            model_configs: model_configs.clone(),
        });
        self.recorded_model_configs = Some(model_configs);
        self
    }

    pub(super) fn recorded_efforts(&self) -> Vec<Option<ThinkingEffort>> {
        self.recorded_model_configs
            .as_ref()
            .expect("model config recording was not enabled")
            .lock()
            .unwrap()
            .iter()
            .map(ModelConfig::thinking_effort)
            .collect()
    }

    pub(super) fn working_dir(&self) -> &std::path::Path {
        &self.working_dir
    }

    pub(super) fn with_hook_manager(mut self, hook_manager: HookManager) -> Self {
        assert!(
            !self.services.initialized(),
            "configure the pipeline before using it"
        );
        self.hook_manager = hook_manager;
        self
    }

    pub(super) fn with_stop_hook_block_cap(mut self, cap: u32) -> Self {
        assert!(
            !self.services.initialized(),
            "configure the pipeline before using it"
        );
        self.turn_settings.stop_hook_block_cap = cap;
        self
    }

    pub(super) async fn set_total_tokens(&self, tokens: i32) {
        use goose_providers::conversation::token_usage::Usage;
        self.session_manager
            .update(&self.session_id)
            .usage(Usage::new(None, None, Some(tokens)))
            .apply()
            .await
            .unwrap();
    }

    pub(super) async fn set_recipe(&self, recipe: crate::recipe::Recipe) -> Result<()> {
        self.session_manager
            .update(&self.session_id)
            .recipe(Some(recipe.clone()))
            .apply()
            .await?;
        for extension in recipe.extensions.clone().unwrap_or_default() {
            self.services()
                .await
                .extension_manager
                .enable(&self.session_id, extension)
                .await?;
        }
        Ok(())
    }

    #[cfg(feature = "scheduler")]
    pub(super) async fn set_schedule_id(&self, schedule_id: String) -> Result<()> {
        self.session_manager
            .update(&self.session_id)
            .schedule_id(Some(schedule_id))
            .apply()
            .await
    }

    pub(super) async fn get_goal(&self) -> Option<String> {
        GoalState::of(&self.session().await.unwrap()).goal
    }

    pub(super) async fn set_goal(&self, goal: Option<String>) {
        let mut state = GoalState::of(&self.session().await.unwrap());
        state.goal = goal;
        self.session_manager
            .set_extension_state(&self.session_id, &state)
            .await
            .unwrap();
    }

    pub(super) async fn set_grind(&self, grind: Option<String>) {
        let mut state = GoalState::of(&self.session().await.unwrap());
        state.grind = grind;
        self.session_manager
            .set_extension_state(&self.session_id, &state)
            .await
            .unwrap();
    }

    pub(super) async fn session(&self) -> Result<Session> {
        self.session_manager
            .get_session(&self.session_id, true)
            .await
    }

    pub(super) async fn steer(&self, message: Message) {
        self.services().await.steer(&self.session_id, message).await;
    }

    pub(super) async fn has_pending_steers(&self) -> bool {
        let services = self.services().await;
        !services
            .runtime
            .steer_queue(&self.session_id)
            .lock()
            .await
            .is_empty()
    }

    pub(super) fn calculator_total(&self) -> i64 {
        self.calculator.total()
    }

    pub(super) async fn wait_for_calculator_result(&self) {
        self.calculator.wait_for_result().await;
    }

    #[cfg(feature = "tree-sitter")]
    pub(super) fn tool_contexts(&self) -> Vec<crate::agents::tool_execution::ToolCallContext> {
        self.calculator.contexts()
    }

    pub(super) async fn reconstruct(&self) -> Result<Self> {
        let session = self.session().await?;
        let mut pipeline = build_test_pipeline(
            self.session_manager.clone(),
            self.api.clone(),
            self.provider_features,
            self.scheduler.clone(),
            session,
            self._temp_dir.clone(),
        )
        .await?;
        pipeline.hook_manager = self.hook_manager.clone();
        pipeline.turn_settings = self.turn_settings.clone();
        pipeline.max_turns = self.max_turns;
        let lease = self
            .extension_lease(self.services().await)
            .lock()
            .unwrap()
            .clone();
        *pipeline
            .extension_lease(pipeline.services().await)
            .lock()
            .unwrap() = lease;
        Ok(pipeline)
    }

    #[cfg(feature = "tree-sitter")]
    pub(super) async fn new_session(&self, working_dir: std::path::PathBuf) -> Result<Self> {
        let source_session = self.session().await?;
        let session = self
            .session_manager
            .create_session(
                working_dir,
                "pipeline-test".to_string(),
                SessionType::Hidden,
                GooseMode::Auto,
            )
            .await?;
        self.session_manager
            .update(&session.id)
            .provider_name(
                source_session
                    .provider_name
                    .expect("source test session has a provider"),
            )
            .model_config(self.model_config.clone())
            .apply()
            .await?;
        build_test_pipeline(
            self.session_manager.clone(),
            self.api.clone(),
            self.provider_features,
            self.scheduler.clone(),
            self.session_manager.get_session(&session.id, true).await?,
            self._temp_dir.clone(),
        )
        .await
    }

    pub(super) fn synchronize_calculator(&self, calls: usize) {
        self.calculator.synchronize(calls);
    }

    pub(super) async fn run<const N: usize>(&self, user_messages: [&str; N]) -> Result<TestRun> {
        let mut events = Vec::new();

        for text in user_messages {
            self.start_new_turn().await;
            self.session_manager
                .add_message(&self.session_id, &Message::user().with_text(text))
                .await?;

            events.extend(run_machine(self).await?);
        }

        Ok(TestRun::new(self.session().await?, events))
    }

    pub(super) async fn run_message(&self, message: Message) -> Result<TestRun> {
        self.start_new_turn().await;
        self.session_manager
            .add_message(&self.session_id, &message)
            .await?;
        let events = run_machine(self).await?;
        Ok(TestRun::new(self.session().await?, events))
    }

    pub(super) async fn run_reconstructing_each_step(
        mut self,
        message: &str,
    ) -> Result<(Self, TestRun, usize)> {
        self.start_new_turn().await;
        self.session_manager
            .add_message(&self.session_id, &Message::user().with_text(message))
            .await?;

        let cancel = CancellationToken::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, cancel.clone());
        let mut events = Vec::new();
        let mut applied_steps = 0;
        self.start_turn().await?;

        loop {
            let session = self.session().await?;
            let machine = self.machine(cancel.clone()).await;
            let mut result = machine.step(&session, &emit).await?;
            assert_only_tool_operations_act_on_open_calls(&session, result.applied_step);
            machine
                .apply(
                    self.session_manager.as_ref(),
                    &session,
                    &mut result.effects,
                    &emit,
                )
                .await?;
            if result.applied_step.is_some() {
                applied_steps += 1;
            }
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }

            drop(machine);
            self = self.reconstruct().await?;
            if result.status != RunStatus::Continuing {
                break;
            }
        }

        drop(emit);
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        let result = TestRun::new(self.session().await?, events);
        Ok((self, result, applied_steps))
    }

    pub(super) async fn seed<const N: usize>(&self, messages: [Message; N]) -> Result<()> {
        for message in messages {
            self.session_manager
                .add_message(&self.session_id, &message)
                .await?;
        }
        Ok(())
    }

    pub(super) async fn start_turn(&self) -> Result<()> {
        crate::agents::state_machine::session::run_turn_start_hooks(
            &self.services().await.hook_manager,
            &self.session().await?,
        )
        .await
    }

    pub(super) async fn resume(&self) -> Result<TestRun> {
        let events = run_machine(self).await?;
        Ok(TestRun::new(self.session().await?, events))
    }

    pub(super) async fn confirm(&self, id: &str, permission: Permission) -> Result<()> {
        self.session_manager
            .add_message(
                &self.session_id,
                &Message::user()
                    .with_content(MessageContent::action_required_tool_confirmation_response(
                        id, permission,
                    ))
                    .with_visibility(false, false),
            )
            .await
    }

    pub(super) fn set_permission(&self, tool: &str, level: PermissionLevel) {
        self.permission_manager.update_user_permission(tool, level);
    }

    pub(super) async fn leased_extensions(&self) -> Result<Vec<String>> {
        Ok(self
            .services()
            .await
            .extension_manager
            .current_lease(&self.session_id)
            .await?
            .configs()
            .iter()
            .map(ExtensionConfig::key)
            .collect())
    }

    pub(super) async fn remove_extension(&self, name: &str) -> Result<()> {
        self.services()
            .await
            .extension_manager
            .disable(&self.session_id, name)
            .await?;
        Ok(())
    }

    pub(super) async fn add_extension(&self, name: &str) -> Result<()> {
        self.services()
            .await
            .extension_manager
            .enable(
                &self.session_id,
                ExtensionConfig::Platform {
                    name: name.to_string(),
                    description: name.to_string(),
                    display_name: None,
                    bundled: None,
                    available_tools: vec![],
                },
            )
            .await
            .map_err(anyhow::Error::from)
    }

    pub(super) async fn resume_cancelled(&self) -> Result<TestRun> {
        let cancel = CancellationToken::new();
        cancel.cancel();
        self.run_turn(cancel).await
    }

    pub(super) async fn run_with_cancel(
        &self,
        message: &str,
        cancel: CancellationToken,
    ) -> Result<TestRun> {
        self.start_new_turn().await;
        self.session_manager
            .add_message(&self.session_id, &Message::user().with_text(message))
            .await?;
        self.run_turn(cancel).await
    }

    async fn run_turn(&self, cancel: CancellationToken) -> Result<TestRun> {
        let machine = self.machine(cancel.clone()).await;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, cancel);
        let session = crate::agents::state_machine::session::run(
            &machine,
            self.session_manager.as_ref(),
            &self.services().await.hook_manager,
            &self.session_id,
            &emit,
        )
        .await?;
        drop(emit);
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        Ok(TestRun::new(session, events))
    }

    pub(super) async fn run_with_elicitation(
        &self,
        message: &str,
        action: ElicitationAction,
        user_data: serde_json::Value,
    ) -> Result<TestRun> {
        self.start_new_turn().await;
        self.session_manager
            .add_message(&self.session_id, &Message::user().with_text(message))
            .await?;
        let cancel = CancellationToken::new();
        let machine = self.machine(cancel.clone()).await;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, cancel);
        let mut events = Vec::new();
        let mut answered = false;
        self.start_turn().await?;

        loop {
            let session = self.session().await?;
            let step = machine.step(&session, &emit);
            tokio::pin!(step);
            let mut result = loop {
                tokio::select! {
                    result = &mut step => break result?,
                    Some(event) = rx.recv() => {
                        if let AgentEvent::Message(message) = &event {
                            let elicitation_id = message.content.iter().find_map(|content| {
                                match content {
                                    MessageContent::ActionRequired(action) => match &action.data {
                                        ActionRequiredData::Elicitation { id, .. } => Some(id.clone()),
                                        _ => None,
                                    },
                                    _ => None,
                                }
                            });
                            if let Some(id) = elicitation_id {
                                let outcome = match &action {
                                    ElicitationAction::Accept => {
                                        ElicitationOutcome::Accept(user_data.clone())
                                    }
                                    ElicitationAction::Decline => ElicitationOutcome::Decline,
                                    ElicitationAction::Cancel => ElicitationOutcome::Cancel,
                                    _ => ElicitationOutcome::Cancel,
                                };
                                let response = Message::user()
                                    .with_generated_id()
                                    .with_content(
                                        MessageContent::action_required_elicitation_response(
                                            id.clone(),
                                            user_data.clone(),
                                            action.clone(),
                                        ),
                                    )
                                    .agent_only();
                                crate::elicitation::complete_elicitation_with_message(
                                    &self.session_manager,
                                    &self.session_id,
                                    &id,
                                    outcome,
                                    &response,
                                )
                                .await?;
                                answered = true;
                            }
                        }
                        events.push(event);
                    }
                }
            };
            assert_only_tool_operations_act_on_open_calls(&session, result.applied_step);
            machine
                .apply(
                    self.session_manager.as_ref(),
                    &session,
                    &mut result.effects,
                    &emit,
                )
                .await?;
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
            if result.status != RunStatus::Continuing {
                break;
            }
        }
        assert!(answered, "tool did not request elicitation");
        drop(emit);
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        Ok(TestRun::new(self.session().await?, events))
    }

    pub(super) async fn set_system_prompt_override(&self, prompt: Option<&str>) {
        self.session_manager
            .update(&self.session_id)
            .system_prompt_override(prompt.map(str::to_string))
            .apply()
            .await
            .unwrap();
    }
}

pub(super) async fn test_pipeline() -> Result<(TestPipeline, Arc<DummyApi>)> {
    test_pipeline_with(ProviderFeatures::default()).await
}

#[cfg(feature = "scheduler")]
pub(super) async fn test_pipeline_with_scheduler() -> Result<(
    TestPipeline,
    Arc<DummyApi>,
    Arc<crate::scheduler::Scheduler>,
)> {
    let temp_dir = Arc::new(tempfile::tempdir()?);
    let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
    let scheduler = crate::scheduler::Scheduler::new(
        temp_dir.path().join("schedule.json"),
        session_manager.clone(),
    )
    .await?;
    let (pipeline, api) = test_pipeline_with_components(
        ProviderFeatures::default(),
        temp_dir,
        session_manager,
        Some(scheduler.clone()),
    )
    .await?;
    Ok((pipeline, api, scheduler))
}

pub(super) async fn test_pipeline_with(
    features: ProviderFeatures,
) -> Result<(TestPipeline, Arc<DummyApi>)> {
    let temp_dir = Arc::new(tempfile::tempdir()?);
    let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
    test_pipeline_with_components(features, temp_dir, session_manager, None).await
}

async fn test_pipeline_with_components(
    features: ProviderFeatures,
    temp_dir: Arc<tempfile::TempDir>,
    session_manager: Arc<SessionManager>,
    scheduler: Option<Arc<dyn crate::scheduler_trait::SchedulerTrait>>,
) -> Result<(TestPipeline, Arc<DummyApi>)> {
    let api = Arc::new(DummyApi::start(features).await);
    let session = session_manager
        .create_session(
            temp_dir.path().to_path_buf(),
            "pipeline-test".to_string(),
            if scheduler.is_some() {
                SessionType::Scheduled
            } else {
                SessionType::Hidden
            },
            GooseMode::Auto,
        )
        .await?;
    let model_config = ModelConfig::new(goose_providers::openai::OPEN_AI_DEFAULT_MODEL)
        .with_canonical_limits("openai");
    session_manager
        .update(&session.id)
        .provider_name("openai")
        .model_config(model_config)
        .apply()
        .await?;
    let session = session_manager.get_session(&session.id, true).await?;
    let pipeline = build_test_pipeline(
        session_manager,
        api.clone(),
        features,
        scheduler,
        session,
        temp_dir,
    )
    .await?;

    Ok((pipeline, api))
}

async fn build_test_pipeline(
    session_manager: Arc<SessionManager>,
    api: Arc<DummyApi>,
    provider_features: ProviderFeatures,
    scheduler: Option<Arc<dyn crate::scheduler_trait::SchedulerTrait>>,
    session: Session,
    temp_dir: Arc<tempfile::TempDir>,
) -> Result<TestPipeline> {
    let provider_name = session
        .provider_name
        .as_deref()
        .expect("test session has a provider");
    let api_client = goose_providers::api_client::ApiClient::new_with_tls(
        api.uri(),
        goose_providers::api_client::AuthMethod::NoAuth,
        None,
    )?;
    let provider: Arc<dyn Provider> = Arc::new(
        goose_providers::openai::OpenAiProviderBuilder::new(api_client)
            .base_path("chat/completions")
            .name(provider_name)
            .preserve_thinking_context(provider_features.preserves_thinking)
            .build(),
    );
    let provider: Arc<dyn Provider> =
        if provider_features.resolved_model.is_some() || provider_features.manages_own_context {
            Arc::new(FeatureProvider {
                inner: provider,
                features: provider_features,
            })
        } else {
            provider
        };
    let model_config = session
        .model_config
        .clone()
        .expect("test session has a model config");
    Ok(TestPipeline {
        permission_manager: Arc::new(PermissionManager::new(temp_dir.path().join("permissions"))),
        calculator: Arc::new(CalculatorExtension::new(session_manager.action_required())),
        session_manager,
        api,
        provider_features,
        provider,
        model_config,
        hook_manager: HookManager::default(),
        turn_settings: TurnSettings {
            max_turns: MAX_TURNS,
            retry_timeout: std::time::Duration::from_secs(1),
            on_failure_timeout: std::time::Duration::from_secs(1),
            stop_hook_block_cap: 3,
            tool_pair_summarization: true,
            compaction_threshold: COMPACTION_THRESHOLD,
            compaction_token_limit: usize::MAX,
            tool_call_cutoff: None,
            auto_effort: None,
            context_relevance: None,
        },
        max_turns: MAX_TURNS,
        session_id: session.id.clone(),
        working_dir: session.working_dir.clone(),
        session,
        scheduler,
        recorded_model_configs: None,
        services: tokio::sync::OnceCell::new(),
        _temp_dir: temp_dir,
    })
}

async fn enable_session_extensions(
    services: &StateMachineServices,
    session: &Session,
    calculator: &Arc<CalculatorExtension>,
) -> Result<()> {
    let mut extensions = EnabledExtensionsState::from_extension_data(&session.extension_data)
        .map(|state| state.extensions)
        .unwrap_or_else(default_extensions);
    for recipe_extension in session
        .recipe
        .as_ref()
        .and_then(|recipe| recipe.extensions.as_ref())
        .into_iter()
        .flatten()
    {
        if !extensions
            .iter()
            .any(|extension| extension.name() == recipe_extension.name())
        {
            extensions.push(recipe_extension.clone());
        }
    }
    if !extensions
        .iter()
        .any(|extension| extension.name() == "calculator")
    {
        extensions.push(calculator_extension());
    }
    for extension in extensions {
        if extension.name() == "calculator" {
            services
                .extension_manager
                .add_test_client(
                    &session.id,
                    extension,
                    calculator.clone(),
                    calculator.get_info().cloned(),
                )
                .await;
        } else {
            services
                .extension_manager
                .enable(&session.id, extension)
                .await?;
        }
    }

    Ok(())
}

fn default_extensions() -> Vec<ExtensionConfig> {
    [
        ("extensionmanager", "Extension Manager"),
        ("todo", "Todo"),
        #[cfg(feature = "scheduler")]
        (
            crate::agents::platform_extensions::scheduler::EXTENSION_NAME,
            "Scheduler",
        ),
    ]
    .into_iter()
    .map(|(name, description)| platform_extension(name, description))
    .chain([calculator_extension()])
    .collect()
}

/// Builtin rather than platform: a session's selection keeps only platform
/// extensions goose ships.
pub(super) fn calculator_extension() -> ExtensionConfig {
    ExtensionConfig::Builtin {
        name: "calculator".to_string(),
        description: "Stateful test calculator".to_string(),
        display_name: None,
        timeout: None,
        bundled: None,
        available_tools: vec![],
    }
}

fn platform_extension(name: &str, description: &str) -> ExtensionConfig {
    ExtensionConfig::Platform {
        name: name.to_string(),
        description: description.to_string(),
        display_name: None,
        bundled: None,
        available_tools: vec![],
    }
}

pub(super) struct TestRun {
    pub session: Session,
    pub events: Vec<AgentEvent>,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum MessageKind {
    Agent,
    Confirmation,
    Error,
    Thinking,
    ToolCall,
    ToolResponse,
    User,
}

impl TestRun {
    fn new(session: Session, events: Vec<AgentEvent>) -> Self {
        let conversation = session
            .conversation
            .as_ref()
            .expect("session has a conversation");
        for message in conversation.messages() {
            assert!(
                message.id.is_some(),
                "persisted message has no id: {message:#?}"
            );
        }
        for event in &events {
            if let AgentEvent::Message(message) = event {
                assert!(
                    message.id.is_some(),
                    "emitted message has no id: {message:#?}"
                );
            }
        }
        Self { session, events }
    }

    /// Ids of the last history a client was told to swap in, and of what was stored.
    pub(super) fn replaced_and_stored_ids(&self) -> (Vec<String>, Vec<String>) {
        let ids = |conversation: &Conversation| {
            conversation
                .messages()
                .iter()
                .map(|message| message.id.clone().unwrap())
                .collect()
        };
        let replaced = self
            .events
            .iter()
            .rev()
            .find_map(|event| match event {
                AgentEvent::HistoryReplaced(conversation) => Some(conversation),
                _ => None,
            })
            .expect("history replaced");
        (ids(replaced), ids(self.conversation()))
    }

    pub(super) fn conversation(&self) -> &Conversation {
        self.session
            .conversation
            .as_ref()
            .expect("session has a conversation")
    }

    pub(super) fn history_replacements(&self) -> usize {
        self.events
            .iter()
            .filter(|event| matches!(event, AgentEvent::HistoryReplaced(_)))
            .count()
    }

    pub(super) fn emitted_error_kinds(&self) -> Vec<MessageErrorKind> {
        self.events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::Message(message) => message.error_kind(),
                _ => None,
            })
            .collect()
    }

    pub(super) fn assert_emitted(&self, contains: &str) {
        let emitted = self
            .events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::Message(message) => Some(
                    message
                        .content
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" "),
                ),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            emitted.iter().any(|message| message.contains(contains)),
            "no emitted message contains {contains:?}:\n{}",
            emitted.join("\n")
        );
    }

    pub(super) fn assert_emitted_message_matches_persisted(&self, contains: &str) {
        let emitted = self
            .events
            .iter()
            .find_map(|event| match event {
                AgentEvent::Message(message) if message.as_concat_text().contains(contains) => {
                    Some(message)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no emitted message contains {contains:?}"));
        let persisted = self
            .conversation()
            .messages()
            .iter()
            .find(|message| message.as_concat_text().contains(contains))
            .unwrap_or_else(|| panic!("no persisted message contains {contains:?}"));
        assert_eq!(emitted.id, persisted.id);
    }

    pub(super) fn assert_message(&self, index: isize, kind: MessageKind, contains: &str) {
        let messages = self
            .conversation()
            .messages()
            .iter()
            .filter(|message| !message.is_turn_context())
            .flat_map(|message| {
                message
                    .content
                    .iter()
                    .map(move |content| (message, content))
            })
            .collect::<Vec<_>>();
        let resolved = if index < 0 {
            messages.len() as isize + index
        } else {
            index
        };
        assert!(
            resolved >= 0 && resolved < messages.len() as isize,
            "message index {index} is out of bounds for {} content blocks",
            messages.len()
        );
        let (message, content) = messages[resolved as usize];
        let content = match (kind, content) {
            (MessageKind::Agent, MessageContent::Text(text))
                if message.role == rmcp::model::Role::Assistant =>
            {
                text.text.clone()
            }
            (MessageKind::User, MessageContent::Text(text))
                if message.role == rmcp::model::Role::User =>
            {
                text.text.clone()
            }
            (MessageKind::Thinking, MessageContent::Thinking(thinking)) => {
                thinking.thinking.clone()
            }
            (MessageKind::ToolCall, MessageContent::ToolRequest(request)) => {
                match &request.tool_call {
                    Ok(call) => {
                        let arguments = call
                            .arguments
                            .clone()
                            .map(serde_json::Value::Object)
                            .unwrap_or_default();
                        format!("{}({arguments})", call.name)
                    }
                    Err(error) => error.message.to_string(),
                }
            }
            (MessageKind::ToolResponse, MessageContent::ToolResponse(response)) => {
                match response.tool_result.as_ref() {
                    Ok(result) => result
                        .content
                        .iter()
                        .filter_map(|content| content.as_text().map(|text| text.text.clone()))
                        .collect(),
                    Err(error) => error.message.to_string(),
                }
            }
            (MessageKind::Confirmation, MessageContent::ActionRequired(action)) => {
                match &action.data {
                    ActionRequiredData::ToolConfirmation {
                        id,
                        tool_name,
                        prompt,
                        ..
                    } => format!("{id} {tool_name} {}", prompt.as_deref().unwrap_or_default()),
                    _ => panic!("message {index} is not a tool confirmation: {content:?}"),
                }
            }
            (MessageKind::Error, MessageContent::Error(error)) => error.message.clone(),
            _ => panic!("message {index} is not {kind:?}: {content:?}"),
        };
        assert!(
            content.contains(contains),
            "message {index} does not contain {contains:?}: {content:?}"
        );
    }
}

const TOOL_OPERATIONS: [&str; 5] = [
    "tool_approval",
    "skills",
    "recipe",
    "tool_execution",
    "remaining_tool_calls",
];

fn assert_only_tool_operations_act_on_open_calls(session: &Session, applied_step: Option<&str>) {
    let Some(step) = applied_step else {
        return;
    };
    let conversation = session
        .conversation
        .as_ref()
        .expect("session has a conversation");
    let turn = messages_since_kickoff(conversation).expect("conversation has a turn");
    assert!(
        TOOL_OPERATIONS.contains(&step) || !has_open_tool_call(turn),
        "{step} acted while a tool call was open"
    );
}

fn has_open_tool_call(turn: &[Message]) -> bool {
    let mut open = HashSet::new();
    for content in turn.iter().flat_map(|message| &message.content) {
        match content {
            MessageContent::ToolRequest(request) if !request.was_executed_externally() => {
                open.insert(request.id.as_str());
            }
            MessageContent::ToolResponse(response) => {
                open.remove(response.id.as_str());
            }
            _ => {}
        }
    }
    !open.is_empty()
}

pub(super) async fn run_machine(pipeline: &TestPipeline) -> Result<Vec<AgentEvent>> {
    let cancel = CancellationToken::new();
    let machine = pipeline.machine(cancel.clone()).await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, cancel);
    let mut events = Vec::new();
    pipeline.start_turn().await?;
    loop {
        let session = pipeline.session().await?;
        let mut result = machine.step(&session, &emit).await?;
        assert_only_tool_operations_act_on_open_calls(&session, result.applied_step);
        machine
            .apply(
                pipeline.session_manager.as_ref(),
                &session,
                &mut result.effects,
                &emit,
            )
            .await?;
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        if result.status != RunStatus::Continuing {
            break;
        }
    }
    drop(emit);
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    Ok(events)
}
