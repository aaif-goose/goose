//! Answers tool requests that no other operation handled.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ContentBlock};
use tracing_futures::Instrument;

use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::ops_toolcalling::{
    emit_extended_pre_hooks, pending_tool_requests, request_was_advertised, tool_span,
    ToolDisposition,
};
use crate::agents::state_machine::post_tool_hook::GoosePostToolHook;
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
    post_tool_call_hook: Arc<GoosePostToolHook>,
}

impl RemainingToolCallsOperation {
    pub fn new(hook_manager: HookManager, post_tool_call_hook: Arc<GoosePostToolHook>) -> Self {
        Self {
            hook_manager,
            post_tool_call_hook,
        }
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
                            let tool_input = tool_call
                                .arguments
                                .as_ref()
                                .map(|arguments| serde_json::Value::Object(arguments.clone()));
                            emit_extended_pre_hooks(
                                &self.hook_manager,
                                &tool_call.name,
                                tool_input.as_ref(),
                                session,
                            )
                            .instrument(span.clone())
                            .await;
                            span.record("error.type", "tool_not_available");
                            let output = Ok(CallToolResult::error(vec![ContentBlock::text(
                                format!("Tool '{}' is not available.", tool_call.name),
                            )]));
                            let tool_response = ToolResponse {
                                id: request.id.clone(),
                                tool_result: output.clone(),
                                metadata: request.metadata.clone(),
                            };
                            self.post_tool_call_hook
                                .after_call(session, &request, &tool_response)
                                .instrument(span.clone())
                            .await;
                            (output, true)
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
    use crate::plugins::discovery::{DiscoveredPlugin, PluginScope};
    use goose_agent::tool_call_state::EXECUTABLE_KEY;
    use rmcp::model::CallToolRequestParams;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    struct PostFailureLog {
        _temp_dir: tempfile::TempDir,
        plugin_dir: std::path::PathBuf,
    }

    impl PostFailureLog {
        fn new() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            let plugin_dir = temp_dir.path().join("test-plugin");
            std::fs::create_dir_all(plugin_dir.join("hooks")).unwrap();
            std::fs::write(
                plugin_dir.join("hooks/hooks.json"),
                r#"{"hooks": {"PostToolUseFailure": [{"hooks": [{"type": "command", "command": "sh ${PLUGIN_ROOT}/hook.sh"}]}]}}"#,
            )
            .unwrap();
            std::fs::write(
                plugin_dir.join("hook.sh"),
                "#!/bin/sh\ncat >> \"$PLUGIN_ROOT/hook.log\"\nprintf '\\n' >> \"$PLUGIN_ROOT/hook.log\"\nexit 0\n",
            )
            .unwrap();
            Self {
                _temp_dir: temp_dir,
                plugin_dir,
            }
        }

        fn hook_manager(&self) -> HookManager {
            HookManager::from_plugins_for_test(vec![DiscoveredPlugin {
                name: "test-plugin".into(),
                root: self.plugin_dir.clone(),
                scope: PluginScope::Project,
            }])
        }

        fn tool_call_ids(&self) -> Vec<String> {
            std::fs::read_to_string(self.plugin_dir.join("hook.log"))
                .unwrap_or_default()
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| {
                    let payload: serde_json::Value = serde_json::from_str(line).unwrap();
                    payload["tool_call_id"].as_str().unwrap().to_string()
                })
                .collect()
        }
    }

    async fn run_with_hooks(
        session: &Session,
        messages: Vec<Message>,
        hook_manager: HookManager,
    ) -> Result<OperationResult<GooseEffect>> {
        let (tx, _rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, CancellationToken::new());
        let post_tool_call_hook =
            Arc::new(GoosePostToolHook::new(hook_manager.clone(), Arc::default()));
        RemainingToolCallsOperation::new(hook_manager, post_tool_call_hook)
            .run(session, &Conversation::new_unvalidated(messages), &emit)
            .await
    }

    async fn run(
        session: &Session,
        messages: Vec<Message>,
    ) -> Result<OperationResult<GooseEffect>> {
        run_with_hooks(session, messages, HookManager::default()).await
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

        let log = PostFailureLog::new();
        run_with_hooks(&Session::default(), messages(), log.hook_manager()).await?;
        assert_eq!(log.tool_call_ids(), ["missing", "final"]);

        let chat = Session {
            goose_mode: GooseMode::Chat,
            ..Session::default()
        };
        let log = PostFailureLog::new();
        run_with_hooks(&chat, messages(), log.hook_manager()).await?;
        assert!(log.tool_call_ids().is_empty());
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
            run(&Session::default(), messages).await?,
            OperationResult::NotApplicable
        ));
        Ok(())
    }
}
