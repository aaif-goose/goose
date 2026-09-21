//! Experimental on-device provider. The native bridge produces one turn and never
//! executes tools; both goose agent loops use the usual tool request/response path.
use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use goose_apple_foundation_models as afm;
use rmcp::model::{CallToolRequestParams, Role, Tool};
use serde_json::Value;

use crate::{
    base::{
        stream_from_single_message, MessageStream, ModelInfo, Provider, ProviderDescriptor,
        ProviderMetadata,
    },
    canonical::catalog::{
        ProviderSetupCategory, ProviderSetupGroup, ProviderSetupMetadata, ProviderSetupMethod,
    },
    conversation::{
        message::{Message, MessageContentBlock},
        token_usage::{CostSource, ProviderUsage, Usage},
    },
    errors::ProviderError,
    model::ModelConfig,
};

pub const PROVIDER_NAME: &str = "apple-foundation-models";
pub const MODEL_NAME: &str = "system";

pub struct AppleFoundationModelsProvider {
    context_size: usize,
}

impl AppleFoundationModelsProvider {
    pub fn is_supported() -> bool {
        afm::is_supported()
    }

    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            context_size: afm::model_info().map_err(provider_error)?.context_size,
        })
    }
}

impl ProviderDescriptor for AppleFoundationModelsProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            PROVIDER_NAME,
            "Apple Foundation Models (experimental)",
            "On-device text and tool calling on macOS 27 with Apple Intelligence. No API key required.",
            MODEL_NAME,
            vec![MODEL_NAME],
            "https://developer.apple.com/documentation/foundationmodels",
            vec![],
        )
        .with_setup(ProviderSetupMetadata::new(
            ProviderSetupCategory::Model,
            ProviderSetupMethod::None,
            ProviderSetupGroup::Additional,
        ))
        .with_setup_steps(vec!["Requires macOS 27 on Apple silicon with Apple Intelligence enabled."])
    }
}

#[async_trait]
impl Provider for AppleFoundationModelsProvider {
    fn get_name(&self) -> &str {
        PROVIDER_NAME
    }

    async fn stream(
        &self,
        model: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let request = create_request(model, system, messages, tools)?;
        let response = afm::generate(request).await.map_err(provider_error)?;
        let (message, usage) = convert_response(response, tools)?;
        Ok(stream_from_single_message(message, usage))
    }

    async fn get_context_limit(&self, _model: &str, override_limit: Option<usize>) -> usize {
        override_limit
            .unwrap_or(self.context_size)
            .min(self.context_size)
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![MODEL_NAME.into()])
    }

    async fn fetch_model_info(&self, model: &str) -> Result<ModelInfo, ProviderError> {
        check_model(model)?;
        Ok(ModelInfo::with_cost(
            MODEL_NAME,
            self.context_size,
            0.0,
            0.0,
        ))
    }

    async fn fetch_supported_model_info(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(vec![self.fetch_model_info(MODEL_NAME).await?])
    }
}

fn check_model(model: &str) -> Result<(), ProviderError> {
    if model != MODEL_NAME {
        return Err(ProviderError::InvalidValue(format!(
            "Unknown Apple Foundation Models model: {model}; expected {MODEL_NAME}"
        )));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> ProviderError {
    ProviderError::InvalidValue(message.into())
}

enum ArgumentEncoding {
    Native,
    JsonText(jsonschema::Validator),
}

struct PreparedTool {
    native: afm::Tool,
    encoding: ArgumentEncoding,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonArguments {
    arguments_json: String,
}

impl PreparedTool {
    fn new(tool: &Tool) -> Result<Self, ProviderError> {
        let original = Value::Object((*tool.input_schema).clone());
        let (schema, encoding) = match afm::tool_schema(&original, &tool.name) {
            Ok(schema) => (schema, ArgumentEncoding::Native),
            Err(error) if error.kind == afm::ErrorKind::InvalidRequest => {
                let validator = jsonschema::validator_for(&original).map_err(|e| {
                    invalid(format!("Tool '{}': invalid input schema: {e}", tool.name))
                })?;
                // Apple's fixed-property schemas cannot describe arbitrary maps or
                // every JSON Schema constraint. Keep the original contract in the
                // prompt and validate the decoded arguments before goose sees a call.
                let envelope = serde_json::json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "arguments_json": {
                            "type": "string",
                            "description": format!(
                                "The complete tool arguments as a JSON object encoded in a string. \
                                 Follow this JSON Schema exactly: {original}"
                            )
                        }
                    },
                    "required": ["arguments_json"]
                });
                (
                    afm::tool_schema(&envelope, &tool.name).map_err(provider_error)?,
                    ArgumentEncoding::JsonText(validator),
                )
            }
            Err(mut error) => {
                error.message = format!("Tool '{}': {}", tool.name, error.message);
                return Err(provider_error(error));
            }
        };
        Ok(Self {
            native: afm::Tool {
                name: tool.name.to_string(),
                description: tool.description.as_deref().unwrap_or("").to_owned(),
                schema,
            },
            encoding,
        })
    }

    fn encode_history(&self, arguments: String) -> String {
        match self.encoding {
            ArgumentEncoding::Native => arguments,
            ArgumentEncoding::JsonText(_) => serde_json::json!({
                "arguments_json": arguments
            })
            .to_string(),
        }
    }

    fn decode_arguments(
        &self,
        arguments: &str,
    ) -> Result<serde_json::Map<String, Value>, ProviderError> {
        let fail = |e| {
            invalid(format!(
                "Tool '{}': invalid arguments: {e}",
                self.native.name
            ))
        };
        match &self.encoding {
            ArgumentEncoding::Native => serde_json::from_str(arguments).map_err(fail),
            ArgumentEncoding::JsonText(validator) => {
                let envelope: JsonArguments = serde_json::from_str(arguments).map_err(fail)?;
                let args: serde_json::Map<String, Value> =
                    serde_json::from_str(&envelope.arguments_json).map_err(fail)?;
                validator
                    .validate(&Value::Object(args.clone()))
                    .map_err(|e| {
                        invalid(format!(
                            "Tool '{}': arguments do not match its schema: {e}",
                            self.native.name
                        ))
                    })?;
                Ok(args)
            }
        }
    }
}

fn create_request(
    model: &ModelConfig,
    system: &str,
    messages: &[Message],
    tools: &[Tool],
) -> Result<afm::Request, ProviderError> {
    check_model(&model.model_name)?;
    if model.temperature.is_some_and(|t| !t.is_finite() || t < 0.0) {
        return Err(invalid("Temperature must be finite and nonnegative"));
    }
    let max_tokens = model
        .max_tokens
        .map(|n| {
            u32::try_from(n)
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid("max_tokens must be positive"))
        })
        .transpose()?;
    let mut names = HashSet::new();
    let tools = tools
        .iter()
        .map(|tool| {
            if !names.insert(tool.name.as_ref()) {
                return Err(invalid("Duplicate tool name"));
            }
            PreparedTool::new(tool)
        })
        .collect::<Result<Vec<_>, ProviderError>>()?;
    let mut history = Vec::new();
    let mut pending = HashMap::new();
    let mut seen_ids = HashSet::new();
    for message in messages.iter().filter(|m| m.is_agent_visible()) {
        let message = message.agent_visible_content();
        for content in &message.content {
            match content {
                MessageContentBlock::Text(text) => {
                    if !pending.is_empty() {
                        return Err(invalid(
                            "Missing tool results before the next conversation message",
                        ));
                    }
                    let entry = match message.role {
                        Role::User => afm::Entry::User {
                            text: text.text.clone(),
                        },
                        Role::Assistant => afm::Entry::Assistant {
                            text: text.text.clone(),
                        },
                    };
                    // A message may contain multiple text blocks; preserve their order in one turn.
                    match (history.last_mut(), entry) {
                        (Some(afm::Entry::User { text }), afm::Entry::User { text: next })
                        | (
                            Some(afm::Entry::Assistant { text }),
                            afm::Entry::Assistant { text: next },
                        ) => {
                            text.push('\n');
                            text.push_str(&next);
                        }
                        (_, entry) => history.push(entry),
                    }
                }
                MessageContentBlock::ToolRequest(request) => {
                    let call = request
                        .tool_call
                        .as_ref()
                        .map_err(|_| invalid("Cannot replay an unparseable tool call"))?;
                    if !seen_ids.insert(request.id.clone()) {
                        return Err(invalid("Duplicate tool-call ID in history"));
                    }
                    pending.insert(request.id.clone(), call.name.to_string());
                    let arguments =
                        serde_json::to_string(&call.arguments.clone().unwrap_or_default())
                            .map_err(|e| invalid(e.to_string()))?;
                    let arguments = if let Some(tool) =
                        tools.iter().find(|tool| tool.native.name == call.name)
                    {
                        tool.encode_history(arguments)
                    } else {
                        arguments
                    };
                    let call = afm::ToolCall {
                        id: request.id.clone(),
                        name: call.name.to_string(),
                        arguments,
                    };
                    if let Some(afm::Entry::ToolCalls { calls }) = history.last_mut() {
                        calls.push(call);
                    } else {
                        history.push(afm::Entry::ToolCalls { calls: vec![call] });
                    }
                }
                MessageContentBlock::ToolResponse(response) => {
                    let name = pending.remove(&response.id).ok_or_else(|| {
                        invalid(format!("Tool result {} has no preceding call", response.id))
                    })?;
                    // Preserve structured results, errors, and MCP content types without dropping data.
                    let text = match &response.tool_result {
                        Ok(result) => {
                            serde_json::to_string(result).map_err(|e| invalid(e.to_string()))?
                        }
                        Err(error) => {
                            serde_json::json!({"isError": true, "error": error}).to_string()
                        }
                    };
                    history.push(afm::Entry::ToolOutput {
                        id: response.id.clone(),
                        name,
                        text,
                    });
                }
                MessageContentBlock::Image(_) | MessageContentBlock::Document(_) => {
                    return Err(ProviderError::NotImplemented(
                        "The Apple Foundation Models prototype accepts text only".into(),
                    ))
                }
                MessageContentBlock::Thinking(_)
                | MessageContentBlock::RedactedThinking(_)
                | MessageContentBlock::ToolConfirmationRequest(_)
                | MessageContentBlock::ActionRequired(_)
                | MessageContentBlock::SystemNotification(_)
                | MessageContentBlock::Error(_) => {}
            }
        }
    }
    if history.is_empty() {
        return Err(invalid("A conversation is required"));
    }
    if !pending.is_empty() {
        return Err(invalid(
            "Every tool call needs a result before generation can resume",
        ));
    }
    Ok(afm::Request {
        instructions: system.into(),
        history,
        tools: tools.into_iter().map(|tool| tool.native).collect(),
        temperature: model.temperature.map(f64::from),
        max_tokens,
    })
}

fn convert_response(
    response: afm::Response,
    tools: &[Tool],
) -> Result<(Message, ProviderUsage), ProviderError> {
    let mut message = Message::assistant();
    let mut ids = HashSet::new();
    for entry in response.entries {
        match entry {
            afm::Entry::Assistant { text } => message = message.with_text(text),
            afm::Entry::ToolCalls { calls } => {
                for call in calls {
                    if call.id.is_empty() || !ids.insert(call.id.clone()) {
                        return Err(invalid("Invalid or duplicate generated tool-call ID"));
                    }
                    let tool = tools
                        .iter()
                        .find(|tool| tool.name == call.name)
                        .ok_or_else(|| {
                            invalid(format!("Model requested unknown tool: {}", call.name))
                        })?;
                    let args = PreparedTool::new(tool)?.decode_arguments(&call.arguments)?;
                    message = message.with_tool_request(
                        call.id,
                        Ok(CallToolRequestParams::new(call.name).with_arguments(args)),
                    );
                }
            }
            _ => return Err(invalid("Unexpected entry in model response")),
        }
    }
    if message.content.is_empty() {
        return Err(ProviderError::RequestFailed(
            "Empty Foundation Models response".into(),
        ));
    }
    let count =
        |n| i32::try_from(n).map_err(|_| ProviderError::UsageError("Token count overflow".into()));
    let usage = Usage::new(
        Some(count(response.input_tokens)?),
        Some(count(response.output_tokens)?),
        None,
    )
    .with_cache_tokens(Some(count(response.cached_tokens)?), None);
    let usage = ProviderUsage::new(MODEL_NAME.into(), usage).with_cost(0.0, CostSource::Estimated);
    Ok((message, usage))
}

fn provider_error(error: afm::Error) -> ProviderError {
    match error.kind {
        afm::ErrorKind::ContextLengthExceeded => {
            ProviderError::ContextLengthExceeded(error.message)
        }
        afm::ErrorKind::Refusal => ProviderError::Refusal {
            details: error.message,
            category: None,
        },
        afm::ErrorKind::InvalidRequest => invalid(error.message),
        afm::ErrorKind::Unavailable | afm::ErrorKind::Cancelled | afm::ErrorKind::Generation => {
            ProviderError::RequestFailed(error.message)
        }
    }
}

#[cfg(test)]
#[path = "../tests/apple_foundation_models/unit.rs"]
mod tests;
