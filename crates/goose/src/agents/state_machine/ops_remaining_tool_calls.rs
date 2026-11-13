//! Answers tool requests that no other operation handled.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::write_integrity::WriteIntegrityChecker;
use rmcp::model::{CallToolResult, ContentBlock};
use tracing_futures::Instrument;

use crate::agents::extension_manager::ExtensionLease;
use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::ops_toolcalling::{
    leased_working_dir, pending_tool_requests, request_was_advertised, tool_span, ToolDisposition,
};
use crate::agents::state_machine::tool_hooks::{
    check_pre_tool_hooks, check_write_integrity, emit_post_tool_hooks,
};
use crate::agents::state_machine::{
    applied, messages_since_kickoff, not_applicable, Emitter, Operation, OperationResult,
};
use crate::agents::tool_execution::CHAT_MODE_TOOL_SKIPPED_RESPONSE;
use crate::config::GooseMode;
use crate::conversation::message::{Message, ToolResponse};
use crate::conversation::Conversation;
use crate::hooks::HookManager;
use crate::session::Session;

pub(crate) const UNCLAIMED_TOOL_ERROR: &str = "goose.unclaimed_tool";

pub struct RemainingToolCallsOperation {
    hook_manager: HookManager,
    extension_lease: Arc<Mutex<Option<Arc<ExtensionLease>>>>,
    write_integrity: Option<Arc<WriteIntegrityChecker>>,
}

impl RemainingToolCallsOperation {
    pub fn new(
        hook_manager: HookManager,
        extension_lease: Arc<Mutex<Option<Arc<ExtensionLease>>>>,
    ) -> Self {
        Self {
            hook_manager,
            extension_lease,
            write_integrity: None,
        }
    }

    pub fn with_write_integrity(mut self, checker: Arc<WriteIntegrityChecker>) -> Self {
        self.write_integrity = Some(checker);
        self
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for RemainingToolCallsOperation {
    fn name(&self) -> &'static str {
        "remaining_tool_calls"
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        let pending = pending_tool_requests(messages);
        if pending.is_empty() {
            return not_applicable();
        }

        let mut response = Message::user();
        for (request, disposition) in pending {
            let tool_name = request
                .tool_call
                .as_ref()
                .map(|tool_call| tool_call.name.as_ref())
                .unwrap_or("unknown");
            let span = tool_span(tool_name, &request.id, &session.id);
            let (result, unclaimed) = match disposition {
                ToolDisposition::ParseError(error) => (
                    Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                        "The tool call could not be parsed: {error}. Correct the arguments and try again."
                    ))])),
                    false,
                ),
                ToolDisposition::Execute if !request_was_advertised(messages, &request) => {
                    span.record("error.type", "tool_not_available");
                    (
                        Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                            "Tool '{tool_name}' is not available."
                        ))])),
                        true,
                    )
                }
                ToolDisposition::Execute if session.goose_mode == GooseMode::Chat => (
                    Ok(CallToolResult::success(vec![ContentBlock::text(
                        CHAT_MODE_TOOL_SKIPPED_RESPONSE,
                    )])),
                    false,
                ),
                ToolDisposition::Execute => {
                    match request.tool_call.as_ref() {
                        Ok(tool_call) => {
                            let working_dir = leased_working_dir(&self.extension_lease, session);
                            if let Some(denied) = check_write_integrity(
                                self.write_integrity.as_deref(),
                                conversation,
                                &request,
                            ) {
                                (Ok(denied), false)
                            } else if let Some(denied) = check_pre_tool_hooks(
                                &self.hook_manager,
                                &session.id,
                                &working_dir,
                                &request,
                            )
                            .instrument(span.clone())
                            .await
                            {
                                (Ok(denied), false)
                            } else {
                                span.record("error.type", "tool_not_available");
                                let output = Ok(CallToolResult::error(vec![ContentBlock::text(
                                    format!("Tool '{}' is not available.", tool_call.name),
                                )]));
                                let tool_response = ToolResponse {
                                    id: request.id.clone(),
                                    tool_result: output.clone(),
                                    metadata: request.metadata.clone(),
                                };
                                emit_post_tool_hooks(
                                    &self.hook_manager,
                                    &session.id,
                                    &working_dir,
                                    &request,
                                    &tool_response,
                                )
                                .instrument(span.clone())
                                .await;
                                (output, true)
                            }
                        }
                        Err(error) => (
                            Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                                "The tool call could not be parsed: {error}."
                            ))])),
                            false,
                        ),
                    }
                }
            };
            let mut metadata = request.metadata.clone();
            if unclaimed {
                metadata
                    .get_or_insert_default()
                    .insert(UNCLAIMED_TOOL_ERROR.to_string(), true.into());
            }
            response.add_tool_response_with_metadata(request.id, result, metadata.as_ref());
        }

        let response = emit.message(response);
        applied([response.into()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::final_output_tool::FINAL_OUTPUT_TOOL_NAME;
    use crate::agents::state_machine::tests::hooks_lifecycle::{
        HookTestEnv, LOG_AND_BLOCK_SCRIPT, RECORD_AND_ALLOW_SCRIPT,
    };
    use crate::agents::state_machine::ConversationEffect;
    use crate::conversation::message::MessageContent;
    use goose_agent::tool_call_state::EXECUTABLE_KEY;
    use rmcp::model::CallToolRequestParams;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    async fn run(
        session: &Session,
        messages: Vec<Message>,
        hook_manager: HookManager,
    ) -> Result<OperationResult<GooseEffect>> {
        let (tx, _rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, CancellationToken::new());
        RemainingToolCallsOperation::new(hook_manager, Arc::default())
            .run(session, &Conversation::new_unvalidated(messages), &emit)
            .await
    }

    fn executable_calls(calls: &[(&str, &str)]) -> Message {
        calls
            .iter()
            .fold(Message::assistant(), |message, (id, tool_name)| {
                message.with_tool_request_with_metadata(
                    *id,
                    Ok(CallToolRequestParams::new(tool_name.to_string())),
                    None,
                    Some(serde_json::json!({ EXECUTABLE_KEY: true })),
                )
            })
    }

    #[tokio::test]
    async fn offered_calls_no_runner_ran_get_a_failed_post_hook() -> Result<()> {
        let messages = || {
            vec![
                Message::user().with_text("go"),
                executable_calls(&[
                    ("missing", "missing_tool"),
                    ("final", FINAL_OUTPUT_TOOL_NAME),
                ]),
            ]
        };

        let env = HookTestEnv::new("PostToolUseFailure", RECORD_AND_ALLOW_SCRIPT);
        run(&Session::default(), messages(), env.hook_manager()).await?;
        let tool_call_ids: Vec<_> = env
            .payloads()
            .iter()
            .map(|payload| payload["tool_call_id"].clone())
            .collect();
        assert_eq!(tool_call_ids, ["missing", "final"]);

        let chat = Session {
            goose_mode: GooseMode::Chat,
            ..Session::default()
        };
        let env = HookTestEnv::new("PostToolUseFailure", RECORD_AND_ALLOW_SCRIPT);
        run(&chat, messages(), env.hook_manager()).await?;
        assert!(env.payloads().is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn a_pre_tool_hook_denial_answers_the_call_with_the_hook_reason() -> Result<()> {
        let messages = vec![
            Message::user().with_text("go"),
            executable_calls(&[("missing", "missing_tool")]),
        ];
        let env = HookTestEnv::new("PreToolUse", LOG_AND_BLOCK_SCRIPT);

        let OperationResult::Applied(step) =
            run(&Session::default(), messages, env.hook_manager()).await?
        else {
            panic!("expected the call to be answered");
        };
        let [GooseEffect::Conversation(ConversationEffect::AppendMessage(message))] =
            step.effects.as_slice()
        else {
            panic!("expected one response message");
        };
        let MessageContent::ToolResponse(response) = &message.content[0] else {
            panic!("expected a tool response");
        };
        let result = response.tool_result.as_ref().unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("not done yet"));
        assert!(response
            .metadata
            .as_ref()
            .is_none_or(|metadata| !metadata.contains_key(UNCLAIMED_TOOL_ERROR)));
        Ok(())
    }

    #[tokio::test]
    async fn a_call_awaiting_approval_is_left_to_the_unhandled_tool_calls_step() -> Result<()> {
        let messages = vec![
            Message::user().with_text("run it"),
            Message::assistant().with_tool_request_with_metadata(
                "call",
                Ok(CallToolRequestParams::new("shell")),
                None,
                Some(serde_json::json!({ EXECUTABLE_KEY: false })),
            ),
            Message::assistant()
                .with_action_required("call", "shell".to_string(), Default::default(), None)
                .user_only(),
        ];

        assert!(matches!(
            run(&Session::default(), messages, HookManager::default()).await?,
            OperationResult::NotApplicable
        ));
        Ok(())
    }
}
