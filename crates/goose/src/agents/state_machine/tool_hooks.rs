use std::path::Path;

use goose_agent::write_integrity::WriteIntegrityChecker;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData};

use crate::conversation::message::{ToolRequest, ToolResponse};
use crate::conversation::Conversation;
use crate::hooks::{HookChainOutcome, HookContext, HookEvent, HookManager};

pub(super) fn check_write_integrity(
    checker: Option<&WriteIntegrityChecker>,
    conversation: &Conversation,
    request: &ToolRequest,
) -> Option<CallToolResult> {
    let checker = checker?;
    let tool_call = request.tool_call.as_ref().ok()?;
    checker
        .check(conversation, &tool_call.name, tool_call.arguments.as_ref())
        .err()
        .map(|reason| CallToolResult::error(vec![ContentBlock::text(reason)]))
}

pub(super) async fn check_pre_tool_hooks(
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

pub(super) async fn emit_post_tool_hooks(
    hook_manager: &HookManager,
    session_id: &str,
    working_dir: &Path,
    request: &ToolRequest,
    response: &ToolResponse,
) {
    let Ok(tool_call) = &request.tool_call else {
        return;
    };
    let tool_input = tool_call
        .arguments
        .as_ref()
        .map(|arguments| serde_json::Value::Object(arguments.clone()));
    let event = emit_post_tool_use(
        hook_manager,
        session_id,
        &working_dir.to_string_lossy(),
        &tool_call.name,
        &request.id,
        tool_input.as_ref(),
        &response.tool_result,
    )
    .await;
    if event == HookEvent::PostToolUse {
        emit_extended_post_hooks(
            hook_manager,
            &tool_call.name,
            tool_input.as_ref(),
            session_id,
            working_dir,
        )
        .await;
    }
}

async fn emit_extended_post_hooks(
    hook_manager: &HookManager,
    tool_name: &str,
    tool_input: Option<&serde_json::Value>,
    session_id: &str,
    working_dir: &Path,
) {
    let (event, matcher) = match categorize_tool(tool_name) {
        ToolCategory::Shell => (
            HookEvent::AfterShellExecution,
            tool_input.and_then(|input| string_argument(input, &["command"])),
        ),
        ToolCategory::Write => (
            HookEvent::AfterFileEdit,
            tool_input.and_then(|input| string_argument(input, &["path", "file", "file_path"])),
        ),
        ToolCategory::Read | ToolCategory::Other => return,
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

/// Emits the post-tool event for a finished call and reports which one fired.
///
/// Which event fires is policy: a result flagged `is_error` counts as a failure,
/// as does a transport error.
async fn emit_post_tool_use(
    hook_manager: &HookManager,
    session_id: &str,
    working_dir: &str,
    tool_name: &str,
    tool_call_id: &str,
    tool_input: Option<&serde_json::Value>,
    result: &std::result::Result<CallToolResult, ErrorData>,
) -> HookEvent {
    let event = match result {
        Ok(result) if result.is_error != Some(true) => HookEvent::PostToolUse,
        _ => HookEvent::PostToolUseFailure,
    };
    if hook_manager.has_hooks(event) {
        let context = HookContext::new(event, session_id)
            .with_tool(tool_name.to_string(), tool_input.cloned())
            .with_tool_call_id(tool_call_id)
            .with_working_dir(working_dir.to_string());
        hook_manager.emit(event, context).await;
    }
    event
}

#[derive(Clone, Copy)]
enum ToolCategory {
    Shell,
    Read,
    Write,
    Other,
}

fn categorize_tool(tool_name: &str) -> ToolCategory {
    match tool_name.rsplit("__").next().unwrap_or(tool_name) {
        "shell" | "bash" | "exec" | "run" => ToolCategory::Shell,
        "read" | "view" | "cat" | "read_file" => ToolCategory::Read,
        "write" | "edit" | "patch" | "write_file" | "edit_file" => ToolCategory::Write,
        _ => ToolCategory::Other,
    }
}

fn string_argument(input: &serde_json::Value, keys: &[&str]) -> Option<String> {
    let arguments = input.as_object()?;
    keys.iter().find_map(|key| {
        arguments
            .get(*key)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

async fn emit_with_matcher(
    hook_manager: &HookManager,
    event: HookEvent,
    session_id: &str,
    working_dir: &Path,
    matcher: String,
    tool_name: &str,
    tool_input: Option<serde_json::Value>,
) {
    if !hook_manager.has_hooks(event) {
        return;
    }
    let mut context = HookContext::new(event, session_id)
        .with_tool(tool_name.to_string(), tool_input)
        .with_working_dir(working_dir.to_string_lossy().to_string());
    context.matcher_context = Some(matcher);
    hook_manager.emit(event, context).await;
}
