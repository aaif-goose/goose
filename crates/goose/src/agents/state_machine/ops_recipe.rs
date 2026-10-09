//! Applies recipe commands and enforces their structured final output.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ContentBlock, Tool};
use tracing_futures::Instrument;

use crate::agents::final_output_tool::{
    structured_output_unsupported_message, FinalOutputTool, FINAL_OUTPUT_CONTINUATION_MESSAGE,
    FINAL_OUTPUT_TOOL_NAME,
};
use crate::agents::state_machine::ops_toolcalling::{
    emit_post_tool_use, pending_advertised_tool_requests, run_pre_tool_hooks, tool_span,
    ToolDisposition,
};
use crate::agents::state_machine::{
    applied, ends_turn, last_effective_role, messages_since_kickoff, not_applicable, yielded_with,
    ConversationEffect, Emitter, GooseEffect, Operation, OperationResult, SlashCommand,
};
use crate::agents::tool_execution::CHAT_MODE_TOOL_SKIPPED_RESPONSE;
use crate::config::GooseMode;
use crate::conversation::message::Message;
use crate::conversation::{Conversation, EffectiveRole};
use crate::hooks::HookManager;
use crate::providers::base::Provider;
use crate::session::Session;

pub(crate) fn final_output_tool(session: &Session) -> Result<Option<FinalOutputTool>> {
    session
        .recipe
        .as_ref()
        .and_then(|recipe| recipe.response.clone())
        .map(FinalOutputTool::try_new)
        .transpose()
        .map_err(|error| anyhow!(error))
}

pub(crate) fn recipe_prompt_parts(session: &Session) -> Result<Vec<(String, String)>> {
    let instructions = session
        .recipe
        .as_ref()
        .and_then(|recipe| recipe.instructions.clone())
        .map(|instructions| ("recipe".to_string(), instructions));
    let final_output =
        final_output_tool(session)?.map(|tool| ("final_output".to_string(), tool.system_prompt()));
    Ok(instructions.into_iter().chain(final_output).collect())
}

pub struct RecipeOperation {
    provider: Arc<dyn Provider>,
    hook_manager: HookManager,
    response: std::sync::Mutex<Option<Message>>,
}

impl RecipeOperation {
    pub fn new(provider: Arc<dyn Provider>, hook_manager: HookManager) -> Self {
        Self {
            provider,
            hook_manager,
            response: std::sync::Mutex::default(),
        }
    }

    fn take_response(&self) -> Option<Message> {
        self.response.lock().unwrap().take()
    }

    async fn command_error(
        &self,
        conversation: &Conversation,
        message: String,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let command = messages_since_kickoff(conversation)?
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("recipe command conversation has no kickoff message"))?;
        let message_id = command
            .id
            .clone()
            .ok_or_else(|| anyhow!("Persisted slash command message has no id"))?;
        let command = command.with_visibility(true, false);
        let response = Message::assistant()
            .with_text(message)
            .with_visibility(true, false);
        emit.message(command).await;
        let response = emit.message(response).await;
        yielded_with([
            ConversationEffect::SetMessageVisibility {
                message_id,
                user_visible: true,
                agent_visible: false,
            }
            .into(),
            response.into(),
        ])
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for RecipeOperation {
    fn name(&self) -> &'static str {
        "recipe"
    }

    async fn finalize_cancellation(
        &self,
        _session: &Session,
        _conversation: &Conversation,
        emit: &Emitter,
    ) -> Vec<GooseEffect> {
        match self.take_response() {
            Some(response) => vec![emit.message(response).await.into()],
            None => Vec::new(),
        }
    }

    async fn run_command(
        &self,
        command: &SlashCommand<'_>,
        _session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let (recipe, prompt) = match crate::slash_commands::recipe_slash_command::resolve_command(
            command.command,
            command.params_str,
        ) {
            Ok(Some(recipe)) => recipe,
            Ok(None) => return not_applicable(),
            Err(error) => return self.command_error(conversation, error, emit).await,
        };

        if let Some(response) = recipe.response.clone() {
            if let Err(error) = FinalOutputTool::try_new(response) {
                return self
                    .command_error(conversation, format!("Recipe is not valid: {error}"), emit)
                    .await;
            }
        }

        #[cfg(feature = "telemetry")]
        crate::posthog::emit_custom_slash_command_used();

        let command_message = messages_since_kickoff(conversation)?
            .first()
            .ok_or_else(|| anyhow!("recipe command conversation has no kickoff message"))?;
        let message_id = command_message
            .id
            .clone()
            .ok_or_else(|| anyhow!("Persisted slash command message has no id"))?;
        applied([
            ConversationEffect::SetMessageVisibility {
                message_id,
                user_visible: true,
                agent_visible: false,
            }
            .into(),
            GooseEffect::SetRecipe(Box::new(Some(recipe))),
            Message::user()
                .with_text(prompt)
                .with_visibility(false, true)
                .into(),
        ])
    }

    async fn inference_tools(&self, session: &Session) -> Result<Vec<Tool>> {
        Ok(final_output_tool(session)?
            .as_ref()
            .map(FinalOutputTool::tool)
            .into_iter()
            .collect())
    }

    async fn prompt_parts(
        &self,
        session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        recipe_prompt_parts(session)
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let Some(mut final_output) = final_output_tool(session)? else {
            return not_applicable();
        };

        if !self.provider.supports_builtin_tools() {
            return self
                .command_error(
                    conversation,
                    structured_output_unsupported_message(self.provider.get_name()),
                    emit,
                )
                .await;
        }

        let messages = messages_since_kickoff(conversation)?;
        let pending = pending_advertised_tool_requests(messages).into_iter().find(
            |(request, disposition)| {
                *disposition == ToolDisposition::Execute
                    && request
                        .tool_call
                        .as_ref()
                        .is_ok_and(|tool_call| tool_call.name == FINAL_OUTPUT_TOOL_NAME)
            },
        );
        if let Some((request, _)) = pending {
            if session.goose_mode == GooseMode::Chat {
                let mut response = Message::user();
                response.add_tool_response_with_metadata(
                    request.id,
                    Ok(CallToolResult::success(vec![ContentBlock::text(
                        CHAT_MODE_TOOL_SKIPPED_RESPONSE,
                    )])),
                    request.metadata.as_ref(),
                );
                let response = emit.message(response).await;
                return applied([response.into()]);
            }
            if FinalOutputTool::has_unanswered_siblings(messages, &request.id) {
                return not_applicable();
            }

            let tool_call = request
                .tool_call
                .map_err(|error| anyhow!("final output tool call could not be parsed: {error}"))?;
            let span = tool_span(&tool_call.name, &request.id, &session.id);
            // `recipe__final_output` is executed here rather than by
            // ToolExecutionOperation, which is registered after this one. Run the
            // same hook lifecycle it would have run there.
            let tool_input = tool_call
                .arguments
                .as_ref()
                .map(|arguments| serde_json::Value::Object(arguments.clone()));
            let pre_tool = run_pre_tool_hooks(
                &self.hook_manager,
                session,
                &request.id,
                &tool_call.name,
                tool_input.as_ref(),
            )
            .instrument(span.clone())
            .await;
            let output = match &pre_tool {
                // A denial returns before execution and emits no post event, the
                // same shape ToolExecutionOperation has: its dispatch returns the
                // denial before the post-hook wrapper is ever applied.
                Err(denial) => Err(denial.clone()),
                Ok(()) => {
                    let result = final_output
                        .execute_tool_call(tool_call.clone())
                        .instrument(span.clone())
                        .await;
                    let output = result.result.instrument(span.clone()).await;
                    match &output {
                        Ok(result) if result.is_error == Some(true) => {
                            span.record("error.type", "tool_error");
                        }
                        Err(_) => {
                            span.record("error.type", "tool_execution_error");
                        }
                        _ => {}
                    }
                    output
                }
            };
            let mut response = Message::user().with_generated_id_if_missing();
            response.add_tool_response_with_metadata(
                request.id.clone(),
                output.clone(),
                request.metadata.as_ref(),
            );
            *self.response.lock().unwrap() = Some(response);
            if pre_tool.is_ok() {
                // Post event carries the same tool_call_id as the pre events.
                // The large-response rewrite ToolExecutionOperation applies is
                // deliberately not reused: the recipe's structured output is
                // the deliverable, not a payload to offload to a temp file.
                emit_post_tool_use(
                    &self.hook_manager,
                    &session.id,
                    &session.working_dir.to_string_lossy(),
                    &tool_call.name,
                    &request.id,
                    tool_input.as_ref(),
                    &output,
                )
                .instrument(span.clone())
                .await;
            }
            let response = self.take_response().expect("recipe response held");
            let response = emit.message(response).await;
            return applied([response.into()]);
        }

        if let Some(output) = FinalOutputTool::successful_output(messages) {
            if last_effective_role(messages)? == EffectiveRole::Tool {
                let message = Message::assistant().with_text(output);
                let message = emit.message(message).await;
                return applied([message.into()]);
            }
            return not_applicable();
        }

        if ends_turn(messages) {
            let message = Message::user()
                .with_text(FINAL_OUTPUT_CONTINUATION_MESSAGE)
                .agent_only();
            let message = emit.message(message).await;
            return applied([message.into()]);
        }

        not_applicable()
    }
}
