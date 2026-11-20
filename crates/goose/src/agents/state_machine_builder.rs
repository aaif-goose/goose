use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use oi_kit_agent::mcp::ExtensionOperation;
use oi_kit_agent::{
    bang_command::BangCommandOperation,
    empty_response::EmptyResponseOperation,
    exit_on_error::ExitOnErrorOperation,
    max_turns::MaxTurnsOperation,
    steer::{SteerOperation, SteerQueue},
};
use oi_kit_providers::api_client::{ApiClient, AuthMethod};
use oi_kit_providers::decision::DecisionProvider;
use oi_kit_providers::model::ModelConfig;
use oi_kit_providers::typesafe::{TypeSafeProvider, TYPESAFE_DEFAULT_HOST};
use tokio_util::sync::CancellationToken;

use crate::agents::state_machine::{
    recipe_unsupported_message, CompactionMode, CompactionOperation, ContextRelevanceOperation,
    DoctorOperation, ExpiredToolsOperation, FinalOutputOperation, GooseEffect, GooseExtensions,
    GooseFinalOutputSchemaSource, GooseInferenceProvider, GooseInferenceRequestPreparer,
    GooseSkillSource, GooseToolApproval, InferenceRunner, McpPromptOperation, Operation,
    PlatformExtensionOperation, ProjectOperation, RecipeOperation, RetryOperation, SkillOperation,
    SlashCommandOperation, StateMachine, StatusOperation, Step, StopHookOperation,
    SubagentOperation, SubdirectoryHintOperation, ThinkingRecoveryOperation, ToolApprovalOperation,
    ToolDeferral, ToolDeferralOperation, ToolPairCompactionOperation, UnavailableDecisions,
    UnhandledToolCallsOperation, NO_SKILLS_HELP,
};
use crate::agents::subagent_handler::ForegroundSubagentRunner;
use crate::agents::tool_execution::DECLINED_RESPONSE;
use crate::agents::tool_snapshot::ToolSnapshot;
use crate::agents::StateMachineServices;
use crate::config::Config;
use crate::context_mgmt::{compaction_templates, GooseCompactionModel, GooseTokenEstimator};
use crate::hooks::HookManager;
use crate::providers::base::Provider;
use crate::session::{Session, SessionManager};
use crate::tool_inspection::ToolInspectionManager;

pub(crate) struct StateMachineConfig<'a> {
    pub(crate) provider: Arc<dyn Provider>,
    pub(crate) model_config: ModelConfig,
    pub(crate) context_limit: usize,
    pub(crate) compaction_threshold: f64,
    pub(crate) standalone_compaction: bool,
    pub(crate) tool_call_cutoff: usize,
    pub(crate) tool_pair_summarization: bool,
    pub(crate) defer_tools: bool,
    pub(crate) always_load_tools: Vec<String>,
    pub(crate) session_manager: Arc<SessionManager>,
    pub(crate) services: Arc<StateMachineServices>,
    pub(crate) tool_snapshot: Arc<StdMutex<Option<Arc<ToolSnapshot>>>>,
    pub(crate) tool_inspection_manager: &'a ToolInspectionManager,
    pub(crate) hook_manager: HookManager,
    pub(crate) steer_queue: SteerQueue,
    pub(crate) subagent_runner: ForegroundSubagentRunner,
    pub(crate) retry_timeout: Duration,
    pub(crate) on_failure_timeout: Duration,
    pub(crate) stop_hook_block_cap: u32,
    pub(crate) max_turns: u32,
    pub(crate) extra_operations: Vec<Arc<dyn Operation<Session, GooseEffect>>>,
    pub(crate) cancel: CancellationToken,
    pub(crate) context_relevance: Option<Arc<dyn DecisionProvider>>,
}

/// Providers that manage their own context never run the operation, so they
/// must not trigger a secret lookup either. After an explicit opt-in, setup
/// failures are logged and recorded with each observation instead of turning
/// the experiment off silently.
pub(crate) fn context_relevance_provider(
    config: &Config,
    provider: &dyn Provider,
) -> Option<Arc<dyn DecisionProvider>> {
    if provider.manages_own_context()
        || !config
            .get_param::<bool>("GOOSE_CONTEXT_RELEVANCE_SHADOW")
            .unwrap_or(false)
    {
        return None;
    }
    Some(typesafe_provider(config).unwrap_or_else(|error| {
        tracing::error!("context relevance shadow is enabled but TypeSafe setup failed: {error}");
        Arc::new(UnavailableDecisions(error.to_string()))
    }))
}

fn typesafe_provider(config: &Config) -> anyhow::Result<Arc<dyn DecisionProvider>> {
    let key = config
        .get_secret::<String>("TYPESAFE_API_KEY")?
        .trim()
        .to_string();
    anyhow::ensure!(!key.is_empty(), "TYPESAFE_API_KEY is empty");
    let tls = crate::config::tls::provider_tls_config_from_config(config)?;
    let client = ApiClient::with_timeout_and_tls(
        TYPESAFE_DEFAULT_HOST.to_string(),
        AuthMethod::BearerToken(key),
        ContextRelevanceOperation::timeout(),
        tls,
    )?;
    Ok(Arc::new(TypeSafeProvider::new(client)))
}

pub(crate) fn build_state_machine(
    config: StateMachineConfig<'_>,
) -> anyhow::Result<StateMachine<'_, Session, GooseEffect>> {
    let StateMachineConfig {
        provider,
        model_config,
        context_limit,
        compaction_threshold,
        standalone_compaction,
        tool_call_cutoff,
        tool_pair_summarization,
        defer_tools,
        always_load_tools,
        session_manager,
        services,
        tool_snapshot,
        tool_inspection_manager,
        hook_manager,
        steer_queue,
        subagent_runner,
        retry_timeout,
        on_failure_timeout,
        stop_hook_block_cap,
        max_turns,
        extra_operations,
        cancel,
        context_relevance,
    } = config;
    let manages_own_context = provider.manages_own_context();
    let deferral = Arc::new(ToolDeferral::new(defer_tools, always_load_tools));

    let skills: Arc<dyn Operation<Session, GooseEffect> + '_> = Arc::new(
        SkillOperation::new(Arc::new(GooseSkillSource)).with_no_skills_help(NO_SKILLS_HELP),
    );
    // Compaction precedes everything that reacts to an assistant reply, so it
    // harvests the in-band summary before tool execution, steering, or turn-end
    // logic sees it. Bang-shell turns never reach the model, so they go first.
    let mut operations: Vec<Arc<dyn Operation<Session, GooseEffect> + '_>> =
        vec![Arc::new(BangCommandOperation::new("shell", "command"))];
    if !manages_own_context {
        if let Some(decisions) = context_relevance {
            operations.push(Arc::new(ContextRelevanceOperation::new(
                decisions,
                context_limit,
                compaction_threshold,
            )));
        }
        let mode = if standalone_compaction {
            CompactionMode::Standalone
        } else {
            CompactionMode::InBand
        };
        operations.push(Arc::new(
            CompactionOperation::new(
                Arc::new(GooseCompactionModel::new(
                    provider.clone(),
                    model_config.clone(),
                )),
                Arc::new(GooseTokenEstimator),
                context_limit,
                compaction_threshold,
            )
            .with_templates(compaction_templates()?)
            .with_mode(mode)
            .with_compact_command(),
        ));
    }
    operations.extend([
        Arc::new(
            FinalOutputOperation::new(
                Arc::new(GooseFinalOutputSchemaSource),
                provider.supports_builtin_tools(),
            )
            .with_unsupported_message(recipe_unsupported_message(provider.get_name())),
        ) as Arc<dyn Operation<_, _>>,
        Arc::new(
            ToolApprovalOperation::new(Arc::new(GooseToolApproval::new(tool_inspection_manager)))
                .with_declined_message(DECLINED_RESPONSE),
        ),
        Arc::clone(&skills),
        Arc::new(RecipeOperation::new(Arc::clone(&services))),
        Arc::new(PlatformExtensionOperation::new(
            Arc::clone(&services),
            Arc::clone(&tool_snapshot),
        )),
        Arc::new(SubdirectoryHintOperation),
        Arc::new(McpPromptOperation::new(
            Arc::clone(&services),
            Arc::clone(&tool_snapshot),
        )),
        Arc::new(ExpiredToolsOperation::new(Arc::clone(&tool_snapshot))),
        Arc::new(ExtensionOperation::new(Arc::new(GooseExtensions::new(
            Arc::clone(&services),
            Arc::clone(&tool_snapshot),
            Arc::clone(&deferral),
        )))),
        Arc::new(ToolDeferralOperation::new(
            Arc::clone(&services),
            Arc::clone(&tool_snapshot),
            deferral,
        )),
        Arc::new(UnhandledToolCallsOperation::new()),
        Arc::new(
            SubagentOperation::new(Arc::new(subagent_runner)).with_max_concurrency(
                Config::global()
                    .get_param::<NonZeroUsize>("GOOSE_MAX_CONCURRENT_SUBAGENTS")
                    .unwrap_or(NonZeroUsize::new(5).unwrap()),
            ),
        ),
        Arc::new(SteerOperation::new(steer_queue)),
        Arc::new(ToolPairCompactionOperation::new(
            provider.clone(),
            model_config.clone(),
            tool_call_cutoff,
            tool_pair_summarization && !manages_own_context,
        )),
        Arc::new(DoctorOperation::new(session_manager)),
        Arc::new(ProjectOperation),
        Arc::new(RetryOperation::new(retry_timeout, on_failure_timeout)),
        Arc::new(EmptyResponseOperation),
        Arc::new(ThinkingRecoveryOperation),
        Arc::new(StopHookOperation::new(hook_manager, stop_hook_block_cap)),
        Arc::new(ExitOnErrorOperation),
        Arc::new(MaxTurnsOperation::new(max_turns)),
        Arc::new(StatusOperation::new(provider.clone(), model_config.clone())),
    ]);
    operations.extend(extra_operations);
    let request_preparer = GooseInferenceRequestPreparer {
        services,
        tool_snapshot,
        tool_inspection_manager,
        context_limit,
    };
    let inference_provider = Arc::new(GooseInferenceProvider::new(provider));
    let inference = Arc::new(
        InferenceRunner::new(inference_provider, model_config)
            .with_request_preparer(Arc::new(request_preparer))
            .with_prices(crate::providers::canonical_cost::prices()),
    );
    let mut steps = Vec::new();
    if crate::agents::execute_commands::slash_commands_enabled() {
        let mut command_operations: Vec<_> = operations
            .iter()
            .filter(|operation| !Arc::ptr_eq(operation, &skills))
            .cloned()
            .collect();
        command_operations.push(Arc::clone(&skills));
        steps.push(Step::Operation(Arc::new(SlashCommandOperation::new(
            command_operations,
        ))));
    }
    steps.extend(operations.into_iter().map(Step::Operation));
    steps.push(Step::Inference(inference));

    Ok(StateMachine::new(steps, cancel))
}
