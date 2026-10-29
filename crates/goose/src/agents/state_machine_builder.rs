use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use goose_providers::model::ModelConfig;
use tokio_util::sync::CancellationToken;

use crate::agents::extension_manager::ExtensionManager;
use crate::agents::state_machine::{
    BangShellOperation, CompactionOperation, DoctorOperation, EmptyResponseOperation,
    ExitOnErrorOperation, GooseEffect, GooseInferenceProvider, GooseInferenceRequestPreparer,
    InferenceRunner, MaxTurnsOperation, Operation, ProjectOperation, RecipeOperation,
    RemainingToolCallsOperation, RetryOperation, SkillOperation, SlashCommandOperation,
    StateMachine, StatusOperation, SteerOperation, SteerQueue, Step, StopHookOperation,
    SubagentOperation, ToolApprovalOperation, ToolExecutionOperation, ToolPairCompactionOperation,
};
use crate::agents::subagent_handler::ForegroundSubagentRunner;
use crate::config::Config;
use crate::hooks::HookManager;
use crate::providers::base::Provider;
use crate::session::{Session, SessionManager};
use crate::tool_inspection::ToolInspectionManager;
use goose_agent::extension_manager::ExtensionLease;

pub(crate) struct StateMachineConfig<'a> {
    pub(crate) provider: Arc<dyn Provider>,
    pub(crate) model_config: ModelConfig,
    pub(crate) context_limit: usize,
    pub(crate) compaction_threshold: f64,
    pub(crate) tool_call_cutoff: usize,
    pub(crate) tool_pair_summarization: bool,
    pub(crate) session_manager: Arc<SessionManager>,
    pub(crate) extension_manager: Arc<ExtensionManager>,
    pub(crate) extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
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
}

pub(crate) fn build_state_machine(
    config: StateMachineConfig<'_>,
) -> StateMachine<'_, Session, GooseEffect> {
    let StateMachineConfig {
        provider,
        model_config,
        context_limit,
        compaction_threshold,
        tool_call_cutoff,
        tool_pair_summarization,
        session_manager,
        extension_manager,
        extension_lease,
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
    } = config;
    let manages_own_context = provider.manages_own_context();

    let mut operations: Vec<Arc<dyn Operation<Session, GooseEffect> + '_>> = vec![
        Arc::new(ToolApprovalOperation::new(tool_inspection_manager)),
        Arc::new(SkillOperation::new(
            hook_manager.clone(),
            Arc::clone(&extension_lease),
        )),
        Arc::new(RecipeOperation::new(
            provider.clone(),
            Arc::clone(&extension_manager),
            hook_manager.clone(),
        )),
        Arc::new(ToolExecutionOperation::new(
            Arc::clone(&extension_manager),
            hook_manager.clone(),
            Arc::clone(&extension_lease),
        )),
        Arc::new(RemainingToolCallsOperation::new(hook_manager.clone())),
        Arc::new(
            SubagentOperation::new(Arc::new(subagent_runner)).with_max_concurrency(
                Config::global()
                    .get_param::<NonZeroUsize>("GOOSE_MAX_CONCURRENT_SUBAGENTS")
                    .unwrap_or(NonZeroUsize::new(5).unwrap()),
            ),
        ),
        Arc::new(SteerOperation::new(steer_queue, hook_manager.clone())),
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
            tool_pair_summarization && !manages_own_context,
        )),
        Arc::new(DoctorOperation::new(session_manager)),
        Arc::new(ProjectOperation),
        Arc::new(RetryOperation::new(retry_timeout, on_failure_timeout)),
        Arc::new(EmptyResponseOperation),
        Arc::new(StopHookOperation::new(hook_manager, stop_hook_block_cap)),
        Arc::new(ExitOnErrorOperation),
        Arc::new(MaxTurnsOperation::new(max_turns)),
    ];
    operations.extend(remaining_operations);
    operations.extend(extra_operations);
    let request_preparer = GooseInferenceRequestPreparer {
        extension_manager,
        extension_lease,
        tool_inspection_manager,
        context_limit,
    };
    let status_operation = Arc::new(StatusOperation::new(provider.clone(), model_config.clone()));
    let inference_provider = Arc::new(GooseInferenceProvider::new(provider));
    let inference = Arc::new(
        InferenceRunner::new(inference_provider, model_config)
            .with_request_preparer(Arc::new(request_preparer)),
    );
    let mut command_handlers = operations.clone();
    command_handlers.push(status_operation);
    let command_operation: Arc<dyn Operation<Session, GooseEffect> + '_> =
        Arc::new(SlashCommandOperation::new(command_handlers));
    let steps = std::iter::once(command_operation)
        .chain(operations)
        .map(Step::Operation)
        .chain(std::iter::once(Step::Inference(inference)))
        .collect();

    StateMachine::new(steps, cancel)
}
