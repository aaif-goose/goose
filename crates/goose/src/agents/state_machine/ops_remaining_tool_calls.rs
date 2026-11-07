//! Answers tool requests that no other operation handled.

use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::post_tool_call_hook::PostToolCallHook;
use rmcp::model::{CallToolResult, ContentBlock, ErrorCode, ErrorData, Tool};
use tracing_futures::Instrument;

use crate::agents::extension_manager::ExtensionLease;
use crate::agents::final_output_tool::FINAL_OUTPUT_TOOL_NAME;
use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::ops_toolcalling::{
    emit_extended_pre_hooks, pending_tool_requests, request_was_advertised, requesting_message,
    tool_span, ToolDisposition,
};
use crate::agents::state_machine::post_tool_hook::run_post_tool_call_hook;
use crate::agents::state_machine::{
    applied, messages_since_kickoff, not_applicable, Emitter, Operation, OperationResult,
};
use crate::agents::tool_discovery::{
    deferred_tool_named, deferred_tools, not_loaded_message, NotSent, LOAD_TOOLS_TOOL_NAME,
};
use crate::agents::tool_execution::CHAT_MODE_TOOL_SKIPPED_RESPONSE;
use crate::config::GooseMode;
use crate::conversation::message::{Message, ToolRequest, ToolResponse};
use crate::conversation::Conversation;
use crate::hooks::HookManager;
use crate::session::{Session, ToolDiscoveryState};

pub(crate) const UNCLAIMED_TOOL_ERROR: &str = "goose.unclaimed_tool";

pub struct RemainingToolCallsOperation {
    hook_manager: HookManager,
    post_tool_call_hook: Arc<dyn PostToolCallHook<Session>>,
    extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
}

impl RemainingToolCallsOperation {
    pub fn new(
        hook_manager: HookManager,
        post_tool_call_hook: Arc<dyn PostToolCallHook<Session>>,
        extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
    ) -> Self {
        Self {
            hook_manager,
            post_tool_call_hook,
            extension_lease,
        }
    }

    async fn deferred_tools(&self, session: &Session) -> Vec<Tool> {
        let lease = self
            .extension_lease
            .lock()
            .expect("extension lease unavailable")
            .clone()
            .filter(|lease| lease.scope_id() == session.id.as_str());
        match lease {
            Some(lease) => deferred_tools(session, &lease).await,
            None => Vec::new(),
        }
    }
}

/// Whether the response that made `request` also called `load_tools`.
fn loads_in_same_response(messages: &[Message], request: &ToolRequest) -> bool {
    requesting_message(messages, request).is_some_and(|message| {
        message.content.iter().any(|content| {
            content.as_tool_request().is_some_and(|other| {
                other
                    .tool_call
                    .as_ref()
                    .is_ok_and(|call| call.name == LOAD_TOOLS_TOOL_NAME)
            })
        })
    })
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
        let active_final_output = session
            .recipe
            .as_ref()
            .is_some_and(|recipe| recipe.response.is_some());
        let messages = messages_since_kickoff(conversation)?;
        let pending: Vec<_> = pending_tool_requests(messages)
            .into_iter()
            .filter(|(request, _)| {
                // Reserve for RecipeOperation the final-output calls it will execute.
                !(active_final_output
                    && request_was_advertised(messages, request)
                    && request
                        .tool_call
                        .as_ref()
                        .is_ok_and(|tool_call| tool_call.name == FINAL_OUTPUT_TOOL_NAME))
            })
            .collect();
        if pending.is_empty() {
            return not_applicable();
        }

        let deferred = self.deferred_tools(session).await;
        let loaded = ToolDiscoveryState::of(session).loaded_tools;
        let mut response = Message::user();
        for (request, disposition) in pending {
            let tool_name = request
                .tool_call
                .as_ref()
                .map(|tool_call| tool_call.name.as_ref())
                .unwrap_or("unknown");
            let span = tool_span(tool_name, &request.id, &session.id);
            let not_loaded = (!request_was_advertised(messages, &request))
                .then(|| deferred_tool_named(tool_name, &deferred))
                .flatten();
            let (result, unclaimed) = match disposition {
                ToolDisposition::ParseError(error) => (
                    Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                        "The tool call could not be parsed: {error}. Correct the arguments and try again."
                    ))])),
                    false,
                ),
                ToolDisposition::Execute if not_loaded.is_some() => {
                    span.record("error.type", "tool_not_loaded");
                    let name = not_loaded.as_deref().unwrap_or(tool_name);
                    let why = if !loaded.iter().any(|loaded| loaded == name) {
                        NotSent::NotLoaded
                    } else if loads_in_same_response(messages, &request) {
                        NotSent::LoadedInSameResponse
                    } else {
                        NotSent::LeftOutForLimit
                    };
                    (
                        Ok(CallToolResult::error(vec![ContentBlock::text(
                            not_loaded_message(name, why),
                        )])),
                        true,
                    )
                }
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
                            let (output, unclaimed) = if tool_call.name == FINAL_OUTPUT_TOOL_NAME
                                && !active_final_output
                            {
                                span.record("error.type", "final_output_not_defined");
                                (
                                    Err(ErrorData::new(
                                        ErrorCode::INTERNAL_ERROR,
                                        "Final output tool not defined".to_string(),
                                        None,
                                    )),
                                    false,
                                )
                            } else {
                                span.record("error.type", "tool_not_available");
                                (
                                    Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                                        "Tool '{}' is not available.",
                                        tool_call.name
                                    ))])),
                                    true,
                                )
                            };
                            let tool_response = ToolResponse {
                                id: request.id.clone(),
                                tool_result: output.clone(),
                                metadata: request.metadata.clone(),
                            };
                            run_post_tool_call_hook(
                                self.post_tool_call_hook.as_ref(),
                                session,
                                conversation,
                                &request,
                                &tool_response,
                            )
                            .instrument(span.clone())
                            .await;
                            (output, unclaimed)
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
    use crate::conversation::message::ToolRequest;
    use crate::recipe::{Recipe, Response};
    use goose_agent::tool_call_state::EXECUTABLE_KEY;
    use rmcp::model::CallToolRequestParams;
    use std::sync::Mutex;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    #[derive(Default)]
    struct RecordingHook(Mutex<Vec<(String, bool)>>);

    #[async_trait]
    impl PostToolCallHook<Session> for RecordingHook {
        async fn after_call(
            &self,
            _session: &Session,
            _conversation: &Conversation,
            request: &ToolRequest,
            response: &ToolResponse,
        ) -> Result<()> {
            let failed = response
                .tool_result
                .as_ref()
                .map_or(true, |result| result.is_error == Some(true));
            self.0.lock().unwrap().push((request.id.clone(), failed));
            Ok(())
        }
    }

    async fn run_with_hook(
        session: &Session,
        messages: Vec<Message>,
        hook: Arc<RecordingHook>,
    ) -> Result<OperationResult<GooseEffect>> {
        let (tx, _rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, CancellationToken::new());
        RemainingToolCallsOperation::new(HookManager::default(), hook, Arc::default())
            .run(session, &Conversation::new_unvalidated(messages), &emit)
            .await
    }

    async fn run(
        session: &Session,
        messages: Vec<Message>,
    ) -> Result<OperationResult<GooseEffect>> {
        run_with_hook(session, messages, Arc::default()).await
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

        let hook = Arc::new(RecordingHook::default());
        run_with_hook(&Session::default(), messages(), hook.clone()).await?;
        assert_eq!(
            *hook.0.lock().unwrap(),
            [("missing".to_string(), true), ("final".to_string(), true)]
        );

        let chat = Session {
            goose_mode: GooseMode::Chat,
            ..Session::default()
        };
        let hook = Arc::new(RecordingHook::default());
        run_with_hook(&chat, messages(), hook.clone()).await?;
        assert!(hook.0.lock().unwrap().is_empty());
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

    #[tokio::test]
    async fn a_final_output_call_is_left_to_the_recipe_step() -> Result<()> {
        let session = Session {
            recipe: Some(
                Recipe::builder()
                    .title("recipe")
                    .description("recipe")
                    .instructions("answer")
                    .response(Response {
                        json_schema: Some(serde_json::json!({ "type": "object" })),
                    })
                    .build()
                    .unwrap(),
            ),
            ..Session::default()
        };
        let messages = vec![
            Message::user().with_text("go"),
            executable_calls(&[("final", FINAL_OUTPUT_TOOL_NAME)]),
        ];

        assert!(matches!(
            run(&session, messages).await?,
            OperationResult::NotApplicable
        ));
        Ok(())
    }
}
