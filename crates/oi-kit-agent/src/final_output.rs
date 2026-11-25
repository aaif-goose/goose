use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use indoc::formatdoc;
use oi_kit_provider_types::conversation::message::{Message, MessageContent, ToolRequest};
use oi_kit_provider_types::conversation::Conversation;
use oi_kit_provider_types::maybe_send::{MaybeSend, MaybeSync};
use rmcp::model::{
    Annotations, CallToolResult, ContentBlock, ErrorCode, ErrorData, JsonObject, Role, TextContent,
    Tool, ToolAnnotations,
};
use serde_json::Value;

use crate::machine::MachineSession;
use crate::operation::{
    applied, ends_turn, messages_since_kickoff, not_applicable, yielded_with, ConversationEffect,
    Emitter, Operation, OperationResult,
};

pub const FINAL_OUTPUT_TOOL_NAME: &str = "final_output";
pub const FINAL_OUTPUT_SUCCESS_MESSAGE: &str = "Final output successfully collected.";
pub const FINAL_OUTPUT_CONTINUATION_MESSAGE: &str =
    "You MUST call the `final_output` tool NOW with the final output for the user.";
pub const FINAL_OUTPUT_NOT_ALONE_MESSAGE: &str =
    "final_output was not accepted because it was called together with other tools. \
     Read the results of those tools, then call final_output on its own.";

const DEFAULT_UNSUPPORTED_MESSAGE: &str =
    "This session asks for structured final output, but the provider can't receive the \
     `final_output` tool, so the model can never provide it. Run it with a different provider.";

pub trait FinalOutputSchemaSource<S>: MaybeSend + MaybeSync {
    fn json_schema(&self, session: &S) -> Option<Value>;
}

pub struct FinalOutputTool {
    json_schema: Value,
    validator: jsonschema::Validator,
}

impl FinalOutputTool {
    pub fn try_new(json_schema: Value) -> Result<Self, String> {
        let schema = json_schema
            .as_object()
            .ok_or_else(|| "json_schema must be an object".to_string())?;
        if schema.is_empty() {
            return Err("empty json_schema is not allowed".to_string());
        }
        let validator =
            jsonschema::validator_for(&json_schema).map_err(|error| error.to_string())?;
        Ok(Self {
            json_schema,
            validator,
        })
    }

    pub fn latest_turn_output(conversation: &Conversation) -> Option<String> {
        messages_since_kickoff(conversation)
            .ok()
            .and_then(Self::successful_output)
    }

    pub fn successful_output(messages: &[Message]) -> Option<String> {
        let successful_responses: HashSet<&str> = messages
            .iter()
            .flat_map(|message| &message.content)
            .filter_map(|content| match content {
                MessageContent::ToolResponse(response)
                    if response.tool_result.as_ref().is_ok_and(|result| {
                        result.is_error != Some(true)
                            && result.content.iter().any(|content| {
                                content
                                    .as_text()
                                    .is_some_and(|text| text.text == FINAL_OUTPUT_SUCCESS_MESSAGE)
                            })
                    }) =>
                {
                    Some(response.id.as_str())
                }
                _ => None,
            })
            .collect();

        messages
            .iter()
            .rev()
            .flat_map(|message| message.content.iter().rev())
            .find_map(|content| match content {
                MessageContent::ToolRequest(request)
                    if successful_responses.contains(request.id.as_str()) =>
                {
                    request.tool_call.as_ref().ok().and_then(|tool_call| {
                        (tool_call.name == FINAL_OUTPUT_TOOL_NAME).then(|| {
                            Value::Object(tool_call.arguments.clone().unwrap_or_default())
                                .to_string()
                        })
                    })
                }
                _ => None,
            })
    }

    fn pretty_schema(&self) -> String {
        serde_json::to_string_pretty(&self.json_schema).unwrap()
    }

    pub fn tool(&self) -> Tool {
        let instructions = formatdoc! {r#"
            The final_output tool collects the final output for the user and provides validation for structured JSON final output against a predefined schema.

            This final_output tool MUST be called with the final output for the user.

            Purpose:
            - Collects the final output for the user
            - Ensures that final outputs conform to the expected JSON structure
            - Provides clear validation feedback when outputs don't match the schema

            Usage:
            - Call the `final_output` tool with your JSON final output passed as the argument.
            - Call it on its own, in a reply with no other tool calls, after you have read the results of your other tool calls.

            The expected JSON schema format is:

            {}

            When validation fails, you'll receive:
            - Specific validation errors
            - The expected format
        "#, self.pretty_schema()};

        Tool::new(
            FINAL_OUTPUT_TOOL_NAME.to_string(),
            instructions,
            self.json_schema.as_object().unwrap().clone(),
        )
        .annotate(
            ToolAnnotations::with_title("Final Output".to_string())
                .read_only(false)
                .destructive(false)
                .idempotent(true)
                .open_world(false),
        )
    }

    pub fn system_prompt(&self) -> String {
        formatdoc! {r#"
            # Final Output Instructions

            You MUST use the `final_output` tool to collect the final output for the user rather than providing the output directly in your response.
            The final output MUST be a valid JSON object that is provided to the `final_output` tool when called and it must match the following schema:

            {}

            ----
        "#, self.pretty_schema()}
    }

    pub fn validate(&self, arguments: Option<JsonObject>) -> Result<String, ErrorData> {
        let output = Value::from(arguments);
        let validation_errors: Vec<String> = self
            .validator
            .iter_errors(&output)
            .map(|error| format!("- {}: {}", error.instance_path(), error))
            .collect();

        if validation_errors.is_empty() {
            Ok(serde_json::to_string(&output).unwrap())
        } else {
            Err(invalid_params(format!(
                "Validation failed:\n{}\n\nExpected format:\n{}\n\nPlease correct your output to match the expected JSON schema and try again.",
                validation_errors.join("\n"),
                self.pretty_schema()
            )))
        }
    }
}

fn invalid_params(message: String) -> ErrorData {
    ErrorData::new(ErrorCode::INVALID_PARAMS, message, None)
}

fn success_result() -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::Text(
        TextContent::new(FINAL_OUTPUT_SUCCESS_MESSAGE)
            .with_annotations(Annotations::default().with_audience(vec![Role::Assistant])),
    )])
}

fn tool_span(tool_name: &str, tool_call_id: &str, session_id: &str) -> tracing::Span {
    tracing::info_span!(
        target: "goose::state_machine",
        "execute_tool",
        "gen_ai.operation.name" = "execute_tool",
        "gen_ai.tool.name" = %tool_name,
        "gen_ai.tool.call.id" = %tool_call_id,
        "gen_ai.conversation.id" = %session_id,
        "gen_ai.tool.call.arguments" = tracing::field::Empty,
        "gen_ai.tool.call.result" = tracing::field::Empty,
        "error.type" = tracing::field::Empty,
        session.id = %session_id,
    )
}

fn pending_final_output_request(messages: &[Message]) -> Option<ToolRequest> {
    let answered: HashSet<&str> = messages
        .iter()
        .flat_map(Message::get_tool_response_ids)
        .collect();
    messages
        .iter()
        .filter(|message| message.role == Role::Assistant)
        .flat_map(|message| &message.content)
        .filter_map(MessageContent::as_tool_request)
        .find(|request| {
            !answered.contains(request.id.as_str())
                && !request.was_executed_externally()
                && request
                    .tool_call
                    .as_ref()
                    .is_ok_and(|tool_call| tool_call.name == FINAL_OUTPUT_TOOL_NAME)
        })
        .cloned()
}

fn assistant_block_bounds(messages: &[Message], message_index: usize) -> (usize, usize) {
    let start = (0..message_index)
        .rev()
        .take_while(|index| messages[*index].role == Role::Assistant)
        .last()
        .unwrap_or(message_index);
    let end = (message_index + 1..messages.len())
        .take_while(|index| messages[*index].role == Role::Assistant)
        .last()
        .map_or(message_index + 1, |index| index + 1);
    (start, end)
}

fn has_sibling_tool_calls(messages: &[Message], request_id: &str) -> bool {
    let Some(message_index) = messages.iter().position(|message| {
        message
            .content
            .iter()
            .filter_map(MessageContent::as_tool_request)
            .any(|request| request.id == request_id)
    }) else {
        return false;
    };
    let (start, end) = assistant_block_bounds(messages, message_index);
    messages[start..end]
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(MessageContent::as_tool_request)
        .any(|request| request.id != request_id && !request.was_executed_externally())
}

fn stop_with_error<E>(
    conversation: &Conversation,
    message: String,
    emit: &Emitter,
) -> Result<OperationResult<E>>
where
    E: From<ConversationEffect>,
{
    let kickoff = messages_since_kickoff(conversation)?
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("conversation has no kickoff message"))?;
    let message_id = kickoff
        .id
        .clone()
        .ok_or_else(|| anyhow!("kickoff message has no id"))?;
    let kickoff = kickoff.with_visibility(true, false);
    emit.record_failure(message.clone());
    let response = Message::assistant()
        .with_text(message)
        .with_visibility(true, false);
    emit.message(kickoff);
    let response = emit.message(response);
    yielded_with(
        [
            ConversationEffect::SetMessageVisibility {
                message_id,
                user_visible: true,
                agent_visible: false,
            },
            ConversationEffect::from(response),
        ]
        .map(E::from),
    )
}

pub struct FinalOutputOperation<S> {
    source: Arc<dyn FinalOutputSchemaSource<S>>,
    supports_builtin_tools: bool,
    unsupported_message: String,
}

impl<S> FinalOutputOperation<S> {
    pub fn new(source: Arc<dyn FinalOutputSchemaSource<S>>, supports_builtin_tools: bool) -> Self {
        Self {
            source,
            supports_builtin_tools,
            unsupported_message: DEFAULT_UNSUPPORTED_MESSAGE.to_string(),
        }
    }

    pub fn with_unsupported_message(mut self, message: impl Into<String>) -> Self {
        self.unsupported_message = message.into();
        self
    }

    fn final_output_tool(&self, session: &S) -> Result<Option<FinalOutputTool>> {
        self.source
            .json_schema(session)
            .map(FinalOutputTool::try_new)
            .transpose()
            .map_err(|error| anyhow!(error))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for FinalOutputOperation<S>
where
    S: MachineSession + 'static,
    E: From<ConversationEffect> + MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        "final_output"
    }

    async fn inference_tools(&self, session: &S) -> Result<Vec<Tool>> {
        Ok(self
            .final_output_tool(session)?
            .as_ref()
            .map(FinalOutputTool::tool)
            .into_iter()
            .collect())
    }

    async fn prompt_parts(
        &self,
        session: &S,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        Ok(self
            .final_output_tool(session)?
            .map(|tool| ("final_output".to_string(), tool.system_prompt()))
            .into_iter()
            .collect())
    }

    async fn run(
        &self,
        session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let Some(final_output) = self.final_output_tool(session)? else {
            return not_applicable();
        };

        if !self.supports_builtin_tools {
            return stop_with_error(conversation, self.unsupported_message.clone(), emit);
        }

        let messages = messages_since_kickoff(conversation)?;
        if let Some(request) = pending_final_output_request(messages) {
            let mut response = Message::user();
            if has_sibling_tool_calls(messages, &request.id) {
                response.add_tool_response_with_metadata(
                    request.id,
                    Err(ErrorData::new(
                        ErrorCode::INVALID_REQUEST,
                        FINAL_OUTPUT_NOT_ALONE_MESSAGE,
                        None,
                    )),
                    request.metadata.as_ref(),
                );
                let response = emit.message(response);
                return applied([E::from(response.into())]);
            }

            let tool_call = request
                .tool_call
                .clone()
                .map_err(|error| anyhow!("final output tool call could not be parsed: {error}"))?;
            let span = tool_span(&tool_call.name, &request.id, session.id());
            let (result, accepted) =
                match span.in_scope(|| final_output.validate(tool_call.arguments)) {
                    Ok(accepted) => (Ok(success_result()), Some(accepted)),
                    Err(error) => {
                        span.record("error.type", "tool_execution_error");
                        (Err(error), None)
                    }
                };
            response.add_tool_response_with_metadata(request.id, result, request.metadata.as_ref());
            let response = emit.message(response);
            let Some(accepted) = accepted else {
                return applied([E::from(response.into())]);
            };
            let accepted = emit.message(Message::assistant().with_text(accepted));
            return applied([E::from(response.into()), E::from(accepted.into())]);
        }

        if FinalOutputTool::successful_output(messages).is_some() {
            return not_applicable();
        }

        if ends_turn(messages) {
            let message = Message::user()
                .with_text(FINAL_OUTPUT_CONTINUATION_MESSAGE)
                .agent_only();
            let message = emit.message(message);
            return applied([E::from(message.into())]);
        }

        not_applicable()
    }
}
