mod active_run;
pub mod execute_commands;
pub mod extension;
pub mod extension_manager;
pub mod final_output_tool;
pub(crate) mod gen_ai_telemetry;
mod large_response_handler;
pub mod moim;
pub mod platform_extensions;
#[cfg(feature = "scheduler")]
pub mod platform_tools;
pub mod prompt_manager;
pub mod provider_manager;
pub mod reply_parts;
pub mod retry;
#[cfg(feature = "scheduler")]
mod schedule_tool;
mod services;
mod session_runtime;
pub mod state_machine;
mod state_machine_builder;
pub mod subagent_execution_tool;
pub(crate) mod subagent_handler;
pub(crate) mod subagent_task_config;
mod tool_confirmation_coordinator;
pub mod tool_execution;
pub mod types;
pub mod validate_extensions;

pub use active_run::ActiveRunRegistry;
pub(crate) use active_run::StartRunError;
pub use execute_commands::{context_management_unsupported_message, COMPACT_TRIGGERS};
pub use extension_manager::ExtensionManager;
pub use goose_agent::events::AgentEvent;
pub(crate) use large_response_handler::max_tool_response_size;
pub use prompt_manager::PromptManager;
#[cfg(feature = "scheduler")]
pub use schedule_tool::ScheduleTool;
pub use services::{GoosePlatform, StateMachineServices, StateMachineServicesConfig, TurnSettings};
pub use session_runtime::{Attachment, SessionRuntime};
pub use subagent_handler::SUBAGENT_TOOL_REQUEST_TYPE;
pub use subagent_task_config::TaskConfig;
pub use types::{RetryConfig, SessionConfig, SuccessCheck};

pub(crate) fn latest_provider_session_id<'a>(
    messages: &'a [crate::conversation::message::Message],
    provider: &str,
) -> Option<&'a str> {
    let inference = messages
        .iter()
        .rev()
        .find_map(|message| message.metadata.inference.as_ref())?;
    (inference.provider == provider)
        .then_some(inference.provider_session_id.as_deref())
        .flatten()
}
