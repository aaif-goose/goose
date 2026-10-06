use crate::agents::tool_execution::ToolCallResult;
use crate::conversation::message::{Message, MessageContent};
use crate::recipe::Response;
use indoc::formatdoc;
use rmcp::model::{
    CallToolRequestParams, ContentBlock, ErrorCode, ErrorData, Tool, ToolAnnotations,
};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::HashSet;

pub const FINAL_OUTPUT_TOOL_NAME: &str = "recipe__final_output";
pub const FINAL_OUTPUT_SUCCESS_MESSAGE: &str = "Final output successfully collected.";
pub const FINAL_OUTPUT_CONTINUATION_MESSAGE: &str =
    "You MUST call the `final_output` tool NOW with the final output for the user.";

pub(crate) fn structured_output_unsupported_message(provider_name: &str) -> String {
    format!(
        "This recipe declares a structured `response`, but provider `{provider_name}` can't \
         support it because it never receives goose's built-in `final_output` tool, so the \
         model can never satisfy this recipe. Remove the entire `response` block from the recipe \
         or run it with a different provider."
    )
}

pub struct FinalOutputTool {
    pub response: Response,
    /// The final output collected for the user. It will be a single line string for easy script extraction from output.
    pub final_output: Option<String>,
}

impl FinalOutputTool {
    pub fn try_new(response: Response) -> Result<Self, String> {
        let schema_value = response
            .json_schema
            .as_ref()
            .ok_or_else(|| "json_schema is required".to_string())?;
        let schema = schema_value
            .as_object()
            .ok_or_else(|| "json_schema must be an object".to_string())?;
        if schema.is_empty() {
            return Err("empty json_schema is not allowed".to_string());
        }
        jsonschema::validator_for(schema_value).map_err(|error| error.to_string())?;

        Ok(Self {
            response,
            final_output: None,
        })
    }

    fn assistant_block_bounds(messages: &[Message], message_index: usize) -> (usize, usize) {
        let start = (0..message_index)
            .rev()
            .take_while(|index| messages[*index].role == rmcp::model::Role::Assistant)
            .last()
            .unwrap_or(message_index);
        let end = (message_index + 1..messages.len())
            .take_while(|index| messages[*index].role == rmcp::model::Role::Assistant)
            .last()
            .map_or(message_index + 1, |index| index + 1);
        (start, end)
    }

    pub(crate) fn has_unanswered_siblings(messages: &[Message], request_id: &str) -> bool {
        let answered: HashSet<&str> = messages
            .iter()
            .flat_map(|message| &message.content)
            .filter_map(|content| match content {
                MessageContent::ToolResponse(response) => Some(response.id.as_str()),
                _ => None,
            })
            .collect();
        let Some(message_index) = messages.iter().position(|message| {
            message.content.iter().any(|content| {
                matches!(
                    content,
                    MessageContent::ToolRequest(request) if request.id == request_id
                )
            })
        }) else {
            return false;
        };
        let (start, end) = Self::assistant_block_bounds(messages, message_index);
        messages[start..end]
            .iter()
            .flat_map(|message| &message.content)
            .any(|content| match content {
                // Another unanswered final-output call is not a reason to wait.
                // This operation drains them one per pass, so treating a sibling
                // final-output call as unfinished work would deadlock the pair:
                // each would wait for the other and neither would be answered.
                // Ordinary tool calls still have to finish first.
                MessageContent::ToolRequest(request) => {
                    request.id != request_id
                        && !answered.contains(request.id.as_str())
                        && !request
                            .tool_call
                            .as_ref()
                            .is_ok_and(|tool_call| tool_call.name == FINAL_OUTPUT_TOOL_NAME)
                }
                _ => false,
            })
    }

    pub(crate) fn successful_output(messages: &[Message]) -> Option<String> {
        let answered_responses: HashSet<&str> = messages
            .iter()
            .flat_map(|message| &message.content)
            .filter_map(|content| match content {
                MessageContent::ToolResponse(response) => Some(response.id.as_str()),
                _ => None,
            })
            .collect();
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

        for (message_index, message) in messages.iter().enumerate().rev() {
            let output = message
                .content
                .iter()
                .rev()
                .find_map(|content| match content {
                    MessageContent::ToolRequest(request)
                        if successful_responses.contains(request.id.as_str()) =>
                    {
                        request.tool_call.as_ref().ok().and_then(|tool_call| {
                            (tool_call.name == FINAL_OUTPUT_TOOL_NAME).then(|| {
                                serde_json::Value::Object(
                                    tool_call.arguments.clone().unwrap_or_default(),
                                )
                                .to_string()
                            })
                        })
                    }
                    _ => None,
                });
            if output.is_some() {
                let (block_start, block_end) =
                    Self::assistant_block_bounds(messages, message_index);
                let siblings_answered = messages[block_start..block_end]
                    .iter()
                    .flat_map(|message| &message.content)
                    .all(|content| match content {
                        MessageContent::ToolRequest(request) => {
                            answered_responses.contains(request.id.as_str())
                        }
                        _ => true,
                    });
                return siblings_answered.then_some(output).flatten();
            }
        }
        None
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
            
            The expected JSON schema format is:

            {}
            
            When validation fails, you'll receive:
            - Specific validation errors
            - The expected format
        "#, serde_json::to_string_pretty(self.response.json_schema.as_ref().unwrap()).unwrap()};

        Tool::new(
            FINAL_OUTPUT_TOOL_NAME.to_string(),
            instructions,
            self.response
                .json_schema
                .as_ref()
                .unwrap()
                .as_object()
                .unwrap()
                .clone(),
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
        "#, serde_json::to_string_pretty(self.response.json_schema.as_ref().unwrap()).unwrap()}
    }

    async fn validate_json_output(&self, output: &Value) -> Result<Value, String> {
        let compiled_schema =
            match jsonschema::validator_for(self.response.json_schema.as_ref().unwrap()) {
                Ok(schema) => schema,
                Err(e) => {
                    return Err(format!("Internal error: Failed to compile schema: {}", e));
                }
            };

        let validation_errors: Vec<String> = compiled_schema
            .iter_errors(output)
            .map(|error| format!("- {}: {}", error.instance_path(), error))
            .collect();

        if validation_errors.is_empty() {
            Ok(output.clone())
        } else {
            Err(format!(
                "Validation failed:\n{}\n\nExpected format:\n{}\n\nPlease correct your output to match the expected JSON schema and try again.",
                validation_errors.join("\n"),
                serde_json::to_string_pretty(self.response.json_schema.as_ref().unwrap()).unwrap_or_else(|_| "Invalid schema".to_string())
            ))
        }
    }

    pub async fn execute_tool_call(&mut self, tool_call: CallToolRequestParams) -> ToolCallResult {
        match tool_call.name.to_string().as_str() {
            FINAL_OUTPUT_TOOL_NAME => {
                let result = self.validate_json_output(&tool_call.arguments.into()).await;
                match result {
                    Ok(parsed_value) => {
                        self.final_output = Some(Self::parsed_final_output_string(parsed_value));
                        ToolCallResult::from(Ok(rmcp::model::CallToolResult::success(vec![
                            ContentBlock::text(FINAL_OUTPUT_SUCCESS_MESSAGE.to_string()),
                        ])))
                    }
                    Err(error) => ToolCallResult::from(Err(ErrorData {
                        code: ErrorCode::INVALID_PARAMS,
                        message: Cow::from(error),
                        data: None,
                    })),
                }
            }
            _ => ToolCallResult::from(Err(ErrorData {
                code: ErrorCode::INVALID_REQUEST,
                message: Cow::from(format!("Unknown tool: {}", tool_call.name)),
                data: None,
            })),
        }
    }

    // Formats the parsed JSON as a single line string so its easy to extract from the output
    fn parsed_final_output_string(parsed_json: Value) -> String {
        serde_json::to_string(&parsed_json).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::Response;
    use rmcp::model::CallToolRequestParams;
    use rmcp::object;
    use serde_json::json;

    fn create_complex_test_schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "age": {"type": "number"}
                    },
                    "required": ["name", "age"]
                },
                "tags": {
                    "type": "array",
                    "items": {"type": "string"}
                }
            },
            "required": ["user", "tags"]
        })
    }

    #[test]
    fn test_try_new_with_missing_schema() {
        let response = Response { json_schema: None };
        assert_eq!(
            FinalOutputTool::try_new(response).err().unwrap(),
            "json_schema is required"
        );
    }

    #[test]
    fn test_try_new_with_empty_schema() {
        let response = Response {
            json_schema: Some(json!({})),
        };
        assert_eq!(
            FinalOutputTool::try_new(response).err().unwrap(),
            "empty json_schema is not allowed"
        );
    }

    #[test]
    fn test_try_new_with_invalid_schema() {
        let response = Response {
            json_schema: Some(json!({
                "type": "invalid_type",
                "properties": {
                    "message": {
                        "type": "unknown_type"
                    }
                }
            })),
        };
        assert!(FinalOutputTool::try_new(response).is_err());
    }

    #[test]
    fn test_try_new_with_invalid_pattern() {
        let response = Response {
            json_schema: Some(json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "pattern": "["
                    }
                }
            })),
        };

        assert!(FinalOutputTool::try_new(response).is_err());
    }

    #[tokio::test]
    async fn test_execute_tool_call_schema_validation_failure() {
        let response = Response {
            json_schema: Some(json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string"
                    },
                    "count": {
                        "type": "number"
                    }
                },
                "required": ["message", "count"]
            })),
        };

        let mut tool = FinalOutputTool::try_new(response).unwrap();
        let tool_call =
            CallToolRequestParams::new(FINAL_OUTPUT_TOOL_NAME).with_arguments(object!({
                "message": "Hello"  // Missing required "count" field
            }));

        let result = tool.execute_tool_call(tool_call).await;
        let tool_result = result.result.await;
        assert!(tool_result.is_err());
        if let Err(error) = tool_result {
            assert!(error.to_string().contains("Validation failed"));
        }
    }

    #[tokio::test]
    async fn test_execute_tool_call_complex_valid_json() {
        let response = Response {
            json_schema: Some(create_complex_test_schema()),
        };

        let mut tool = FinalOutputTool::try_new(response).unwrap();
        let tool_call =
            CallToolRequestParams::new(FINAL_OUTPUT_TOOL_NAME).with_arguments(object!({
                "user": {
                    "name": "John",
                    "age": 30
                },
                "tags": ["developer", "rust"]
            }));

        let result = tool.execute_tool_call(tool_call).await;
        let tool_result = result.result.await;
        assert!(tool_result.is_ok());
        assert!(tool.final_output.is_some());

        let final_output = tool.final_output.unwrap();
        assert!(serde_json::from_str::<Value>(&final_output).is_ok());
        assert!(!final_output.contains('\n'));
    }
}
