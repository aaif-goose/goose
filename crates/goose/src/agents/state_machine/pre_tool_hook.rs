use std::path::Path;

use rmcp::model::{CallToolResult, ContentBlock};

use crate::agents::state_machine::ops_toolcalling::{
    categorize_tool, emit_with_matcher, string_argument, ToolCategory,
};
use crate::conversation::message::ToolRequest;
use crate::hooks::{HookChainOutcome, HookContext, HookEvent, HookManager};

pub(super) async fn emit_pre_tool_hooks(
    hook_manager: &HookManager,
    session_id: &str,
    working_dir: &Path,
    request: &ToolRequest,
) -> Option<CallToolResult> {
    let tool_call = request.tool_call.as_ref().ok()?;
    let tool_input = tool_call
        .arguments
        .as_ref()
        .map(|arguments| serde_json::Value::Object(arguments.clone()));
    let outcome = if hook_manager.has_hooks(HookEvent::PreToolUse) {
        let context = HookContext::new(HookEvent::PreToolUse, session_id)
            .with_tool(tool_call.name.to_string(), tool_input.clone())
            .with_tool_call_id(&request.id)
            .with_working_dir(working_dir.to_string_lossy().to_string());
        hook_manager
            .emit_blocking_with_outcome(HookEvent::PreToolUse, context)
            .await
    } else {
        HookChainOutcome::allow(false)
    };

    // Emitted before the denial returns, so an observer sees the denial
    // before the model receives the refusal. Best effort, like every other
    // hook emission: a subscriber that fails or is absent changes nothing.
    emit_pre_tool_use_result(
        hook_manager,
        session_id,
        working_dir,
        &request.id,
        &tool_call.name,
        tool_input.as_ref(),
        &outcome,
    )
    .await;

    if let Some(denial) = outcome.denial() {
        tracing::Span::current().record("error.type", denial.error_type);
        return Some(CallToolResult::error(vec![ContentBlock::text(
            denial.message,
        )]));
    }
    emit_extended_pre_hooks(
        hook_manager,
        &tool_call.name,
        tool_input.as_ref(),
        session_id,
        working_dir,
    )
    .await;
    None
}

/// Observation-only record of what the `PreToolUse` chain decided. Carries
/// no veto: the decision has already been made by the time this runs.
async fn emit_pre_tool_use_result(
    hook_manager: &HookManager,
    session_id: &str,
    working_dir: &Path,
    tool_call_id: &str,
    tool_name: &str,
    tool_input: Option<&serde_json::Value>,
    outcome: &HookChainOutcome,
) {
    if !hook_manager.has_hooks(HookEvent::PreToolUseResult) {
        return;
    }
    let context = HookContext::new(HookEvent::PreToolUseResult, session_id)
        .with_tool(tool_name.to_string(), tool_input.cloned())
        .with_tool_call_id(tool_call_id)
        .with_working_dir(working_dir.to_string_lossy().to_string())
        .with_pre_tool_use_outcome(outcome);
    hook_manager.emit_pre_tool_use_result(context).await;
}

async fn emit_extended_pre_hooks(
    hook_manager: &HookManager,
    tool_name: &str,
    tool_input: Option<&serde_json::Value>,
    session_id: &str,
    working_dir: &Path,
) {
    let (event, matcher) = match categorize_tool(tool_name) {
        ToolCategory::Shell => (
            HookEvent::BeforeShellExecution,
            tool_input.and_then(|input| string_argument(input, &["command"])),
        ),
        ToolCategory::Read => (
            HookEvent::BeforeReadFile,
            tool_input.and_then(|input| string_argument(input, &["path", "file", "file_path"])),
        ),
        ToolCategory::Write | ToolCategory::Other => return,
    };
    if let Some(matcher) = matcher {
        emit_with_matcher(
            hook_manager,
            event,
            session_id,
            working_dir,
            matcher,
            tool_name,
            tool_input.cloned(),
        )
        .await;
    }
}
