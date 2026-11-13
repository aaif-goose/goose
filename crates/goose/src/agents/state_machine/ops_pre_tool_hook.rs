use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ContentBlock};

use crate::agents::extension_manager::ExtensionLease;
use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::ops_toolcalling::{
    approval_expired, call_will_not_run, leased_working_dir, pending_tool_requests, tool_span,
    ToolDisposition,
};
use crate::agents::state_machine::{
    applied, messages_since_kickoff, not_applicable, ConversationEffect, Emitter, Operation,
    OperationResult,
};
use crate::conversation::message::{Message, ToolRequest};
use crate::conversation::Conversation;
use crate::hooks::{HookChainOutcome, HookContext, HookDenial, HookEvent, HookManager};
use crate::session::Session;

const PRE_HOOK_ALLOWED_KEY: &str = "goose.pre_hook_allowed";

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

pub struct PreToolCallHookOperation {
    hook_manager: HookManager,
    extension_lease: Arc<Mutex<Option<Arc<ExtensionLease>>>>,
}

impl PreToolCallHookOperation {
    pub fn new(
        hook_manager: HookManager,
        extension_lease: Arc<Mutex<Option<Arc<ExtensionLease>>>>,
    ) -> Self {
        Self {
            hook_manager,
            extension_lease,
        }
    }

    async fn denial(
        &self,
        session: &Session,
        conversation: &Conversation,
        request: &ToolRequest,
    ) -> Result<Option<HookDenial>> {
        if call_will_not_run(session, conversation, request)?
            || approval_expired(
                &self.extension_lease,
                messages_since_kickoff(conversation)?,
                request,
            )
        {
            return Ok(None);
        }
        let Ok(tool_call) = &request.tool_call else {
            return Ok(None);
        };
        let tool_input = tool_call
            .arguments
            .as_ref()
            .map(|arguments| serde_json::Value::Object(arguments.clone()));
        let denial = pre_tool_use_denial(
            &self.hook_manager,
            &session.id,
            &leased_working_dir(&self.extension_lease, session),
            &request.id,
            &tool_call.name,
            tool_input.as_ref(),
        )
        .await;
        if let Some(denial) = &denial {
            tool_span(&tool_call.name, &request.id, &session.id)
                .record("error.type", denial.error_type);
        }
        Ok(denial)
    }
}

fn pre_hook_allowed(request: &ToolRequest) -> bool {
    request
        .tool_meta
        .as_ref()
        .and_then(|meta| meta.get(PRE_HOOK_ALLOWED_KEY))
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

#[async_trait]
impl Operation<Session, GooseEffect> for PreToolCallHookOperation {
    fn name(&self) -> &'static str {
        "pre_tool_call_hook"
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let Some(request) = pending_tool_requests(messages_since_kickoff(conversation)?)
            .into_iter()
            .find(|(request, disposition)| {
                matches!(disposition, ToolDisposition::Execute) && !pre_hook_allowed(request)
            })
            .map(|(request, _)| request)
        else {
            return not_applicable();
        };
        let Some(denial) = self.denial(session, conversation, &request).await? else {
            return applied([ConversationEffect::PatchToolRequestMeta {
                tool_call_id: request.id.clone(),
                patch: serde_json::json!({ PRE_HOOK_ALLOWED_KEY: true }),
            }
            .into()]);
        };
        let mut tool_responses = Message::user();
        tool_responses.add_tool_response_with_metadata(
            request.id.clone(),
            Ok(CallToolResult::error(vec![ContentBlock::text(
                denial.message,
            )])),
            request.metadata.as_ref(),
        );
        applied([emit.message(tool_responses).into()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::state_machine::OperationResult;
    use goose_agent::tool_call_state::EXECUTABLE_KEY;
    use rmcp::model::CallToolRequestParams;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    fn call(message: Message, id: &str, allowed: bool) -> Message {
        let meta = if allowed {
            serde_json::json!({ EXECUTABLE_KEY: true, PRE_HOOK_ALLOWED_KEY: true })
        } else {
            serde_json::json!({ EXECUTABLE_KEY: true })
        };
        message.with_tool_request_with_metadata(
            id,
            Ok(CallToolRequestParams::new("shell")),
            None,
            Some(meta),
        )
    }

    async fn allowed_ids(calls: &[(&str, bool)]) -> Result<Option<Vec<String>>> {
        let reply = calls
            .iter()
            .fold(Message::assistant(), |message, (id, allowed)| {
                call(message, id, *allowed)
            });
        let conversation = Conversation::new_unvalidated([Message::user().with_text("go"), reply]);
        let (tx, _rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, CancellationToken::new());
        let result = PreToolCallHookOperation::new(HookManager::default(), Arc::default())
            .run(&Session::default(), &conversation, &emit)
            .await?;
        Ok(match result {
            OperationResult::NotApplicable => None,
            OperationResult::Applied(step) => Some(
                step.effects
                    .into_iter()
                    .map(|effect| match effect {
                        GooseEffect::Conversation(ConversationEffect::PatchToolRequestMeta {
                            tool_call_id,
                            patch,
                        }) => {
                            assert_eq!(patch, serde_json::json!({ PRE_HOOK_ALLOWED_KEY: true }));
                            tool_call_id
                        }
                        _ => panic!("expected a pre-hook allowed patch"),
                    })
                    .collect(),
            ),
        })
    }

    #[tokio::test]
    async fn allows_one_call_per_pass_and_skips_calls_already_allowed() -> Result<()> {
        assert_eq!(
            allowed_ids(&[("first", false), ("second", false)]).await?,
            Some(vec!["first".to_string()])
        );
        assert_eq!(
            allowed_ids(&[("first", true), ("second", false)]).await?,
            Some(vec!["second".to_string()])
        );
        assert_eq!(
            allowed_ids(&[("first", true), ("second", true)]).await?,
            None
        );
        Ok(())
    }
}
