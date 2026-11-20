//! Runs an ordered, re-entrant pipeline over persisted conversation state.
//!
//! Callers persist incoming messages, construct `Step`s from their own operations,
//! and choose whether to call `StateMachine::step`, `StateMachine::apply`, or
//! `StateMachine::run`. Goose's concrete operations remain internal because their
//! configuration is part of `Agent::reply`, not the state-machine protocol.

mod effects;
mod inference_preparation;
mod ops_auto_effort;
mod ops_context_relevance;
mod ops_doctor;
mod ops_final_output;
mod ops_llm;
mod ops_mcp_prompts;
mod ops_platform_extensions;
mod ops_project;
mod ops_recipe;
mod ops_retry;
mod ops_skills;
mod ops_status;
mod ops_stop_hook;
mod ops_subagent;
mod ops_subdirectory_hints;
mod ops_thinking_recovery;
mod ops_tool_pair_compaction;
mod ops_toolcalling;
mod session;
pub(crate) use session::run as run_goose;
mod tool_approval;
mod tool_confirmation;
mod tool_hooks;
mod usage;

#[cfg(test)]
mod tests;

pub use effects::GooseEffect;
pub use oi_kit_agent::command::Command;
pub(super) use oi_kit_agent::final_output::FinalOutputOperation;
pub use oi_kit_agent::machine::{
    EffectHandler, EffectUsage, MachineSession, SessionLoader, StateMachine, Step,
};
pub use oi_kit_agent::operation::{
    applied, assistant_turn_count, ends_turn, last_effective_role, messages_since_kickoff,
    not_applicable, trailing_error, yielded, yielded_with, ConversationEffect, Emitter, Inference,
    InferenceInput, MachineEffect, Operation, OperationResult, RunStatus, StepResult, CLIENT_LOG,
};
pub(super) use oi_kit_agent::slash_command::SlashCommandOperation;
pub(super) use oi_kit_agent::subagent::SubagentOperation;
pub(super) use oi_kit_agent::tool_approval::ToolApprovalOperation;
pub(super) use oi_kit_agent::unhandled_tool_calls::UnhandledToolCallsOperation;
pub(crate) use tool_confirmation::{
    has_unapplied_tool_confirmation_response, pending_tool_confirmations,
    persist_tool_confirmation_decision,
};

pub(super) use inference_preparation::GooseInferenceRequestPreparer;
pub(super) use oi_kit_agent::compaction::{CompactionMode, CompactionOperation};
pub(super) use oi_kit_agent::skills::SkillOperation;
pub(super) use ops_auto_effort::AutoEffortOperation;
pub(super) use ops_context_relevance::{ContextRelevanceOperation, UnavailableDecisions};
pub(super) use ops_doctor::DoctorOperation;
pub(super) use ops_final_output::{recipe_unsupported_message, GooseFinalOutputSchemaSource};
pub(super) use ops_llm::{GooseInferenceProvider, InferenceRunner};
pub(super) use ops_mcp_prompts::McpPromptOperation;
pub(super) use ops_platform_extensions::PlatformExtensionOperation;
pub(super) use ops_project::ProjectOperation;
pub(super) use ops_recipe::RecipeOperation;
pub(super) use ops_retry::RetryOperation;
pub(super) use ops_skills::{GooseSkillSource, NO_SKILLS_HELP};
pub(super) use ops_status::StatusOperation;
pub(super) use ops_stop_hook::{StopHookOperation, DEFAULT_BLOCK_CAP};
pub(super) use ops_subagent::DELEGATED_META_KEY;
pub(super) use ops_subdirectory_hints::SubdirectoryHintOperation;
pub(super) use ops_thinking_recovery::ThinkingRecoveryOperation;
pub(super) use ops_tool_pair_compaction::ToolPairCompactionOperation;
pub(super) use ops_toolcalling::{
    ExpiredToolsOperation, GooseExtensions, ToolDeferral, ToolDeferralOperation,
};
pub(super) use tool_approval::GooseToolApproval;
