use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::pre_tool_call_hook::{PreToolCallDecision, PreToolCallHook};

use crate::agents::extension_manager::ExtensionLease;
use crate::agents::state_machine::ops_toolcalling::{
    call_will_not_run, leased_working_dir, tool_span,
};
use crate::conversation::message::ToolRequest;
use crate::conversation::Conversation;
use crate::hooks::{HookChainOutcome, HookContext, HookDenial, HookEvent, HookManager};
use crate::session::Session;

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

async fn pre_tool_use_denial(
    hook_manager: &HookManager,
    session_id: &str,
    working_dir: &Path,
    tool_call_id: &str,
    tool_name: &str,
    tool_input: Option<&serde_json::Value>,
) -> Option<HookDenial> {
    let outcome = if hook_manager.has_hooks(HookEvent::PreToolUse) {
        let context = HookContext::new(HookEvent::PreToolUse, session_id)
            .with_tool(tool_name.to_string(), tool_input.cloned())
            .with_tool_call_id(tool_call_id)
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
        tool_call_id,
        tool_name,
        tool_input,
        &outcome,
    )
    .await;

    outcome.denial()
}

pub struct GoosePreToolHook {
    hook_manager: HookManager,
    extension_lease: Arc<Mutex<Option<Arc<ExtensionLease>>>>,
}

impl GoosePreToolHook {
    pub fn new(
        hook_manager: HookManager,
        extension_lease: Arc<Mutex<Option<Arc<ExtensionLease>>>>,
    ) -> Self {
        Self {
            hook_manager,
            extension_lease,
        }
    }
}

#[async_trait]
impl PreToolCallHook<Session> for GoosePreToolHook {
    async fn before_call(
        &self,
        session: &Session,
        conversation: &Conversation,
        request: &ToolRequest,
    ) -> Result<PreToolCallDecision> {
        if call_will_not_run(session, conversation, request)? {
            return Ok(PreToolCallDecision::Allow);
        }
        let Ok(tool_call) = &request.tool_call else {
            return Ok(PreToolCallDecision::Allow);
        };
        let tool_input = tool_call
            .arguments
            .as_ref()
            .map(|arguments| serde_json::Value::Object(arguments.clone()));
        let Some(denial) = pre_tool_use_denial(
            &self.hook_manager,
            &session.id,
            &leased_working_dir(&self.extension_lease, session),
            &request.id,
            &tool_call.name,
            tool_input.as_ref(),
        )
        .await
        else {
            return Ok(PreToolCallDecision::Allow);
        };
        tool_span(&tool_call.name, &request.id, &session.id)
            .record("error.type", denial.error_type);
        Ok(PreToolCallDecision::Deny {
            reason: denial.message,
        })
    }
}
