use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::post_tool_call_hook::PostToolCallHook;
use goose_agent::procedure::{kickoff_id, ProcedureRuntime};
use rmcp::model::{CallToolResult, ErrorData};

use crate::agents::extension_manager::ExtensionLease;
use crate::agents::state_machine::ops_toolcalling::{
    categorize_tool, emit_with_matcher, leased_working_dir, string_argument, ToolCategory,
};
use crate::conversation::message::{ToolRequest, ToolResponse};
use crate::conversation::Conversation;
use crate::hooks::{HookContext, HookEvent, HookManager};
use crate::session::Session;

pub struct GoosePostToolHook {
    hook_manager: HookManager,
    extension_lease: Arc<Mutex<Option<Arc<ExtensionLease>>>>,
    /// Procedure control runtime, shared with the pre-tool and stop
    /// hooks. After a call finishes, the runtime advances the procedure
    /// state when the call matches a defined transition. `None` preserves
    /// the previous behavior exactly.
    procedure: Option<Arc<Mutex<ProcedureRuntime>>>,
}

impl GoosePostToolHook {
    pub fn new(
        hook_manager: HookManager,
        extension_lease: Arc<Mutex<Option<Arc<ExtensionLease>>>>,
    ) -> Self {
        Self {
            hook_manager,
            extension_lease,
            procedure: None,
        }
    }

    pub fn with_procedure(mut self, runtime: Arc<Mutex<ProcedureRuntime>>) -> Self {
        self.procedure = Some(runtime);
        self
    }
}

#[async_trait]
impl PostToolCallHook<Session> for GoosePostToolHook {
    async fn after_call(
        &self,
        session: &Session,
        conversation: &Conversation,
        request: &ToolRequest,
        response: &ToolResponse,
    ) -> Result<()> {
        let Ok(tool_call) = &request.tool_call else {
            return Ok(());
        };
        if let Some(runtime) = &self.procedure {
            let kickoff = kickoff_id(conversation);
            if let Some(new_state) = runtime.lock().unwrap().advance(
                kickoff.as_deref(),
                &tool_call.name,
                &response.tool_result,
            ) {
                tracing::debug!(
                    tool_call_id = %request.id,
                    new_state = %new_state,
                    "procedure control advanced state"
                );
            }
        }
        let tool_input = tool_call
            .arguments
            .as_ref()
            .map(|arguments| serde_json::Value::Object(arguments.clone()));
        let working_dir = leased_working_dir(&self.extension_lease, session);
        let event = emit_post_tool_use(
            &self.hook_manager,
            &session.id,
            &working_dir.to_string_lossy(),
            &tool_call.name,
            &request.id,
            tool_input.as_ref(),
            &response.tool_result,
        )
        .await;
        if event == HookEvent::PostToolUse {
            emit_extended_post_hooks(
                &self.hook_manager,
                &tool_call.name,
                tool_input.as_ref(),
                &session.id,
                &working_dir,
            )
            .await;
        }
        Ok(())
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

pub(super) async fn run_post_tool_call_hook(
    hook: &dyn PostToolCallHook<Session>,
    session: &Session,
    conversation: &Conversation,
    request: &ToolRequest,
    response: &ToolResponse,
) {
    if let Err(error) = hook
        .after_call(session, conversation, request, response)
        .await
    {
        tracing::warn!(%error, tool_call_id = %request.id, "post tool call hook failed");
    }
}
