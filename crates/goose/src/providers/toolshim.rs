//! # ToolShim Module
//!
//! The ToolShim module provides a reusable component for interpreting and augmenting LLM outputs with tool calls,
//! regardless of whether the underlying model natively supports tool/function calling.
//!
//! ## Overview
//!
//! ToolShim addresses the challenge of working with models that don't natively support tools by:
//!
//! 1. Taking the text output from any LLM
//! 2. Sending it to a separate "interpreter" model (which can be the same or different model)
//! 3. Using a model to extract tool call intentions into the appropriate format
//! 4. Converting the outputs of the interpreter model into proper tool call structs
//! 5. Augmenting the original message with the extracted tool calls
//!
//! ## Key Components
//!
//! ### ToolInterpreter Trait
//!
//! The core of ToolShim is the `ToolInterpreter` trait, which defines the interface for any model that can interpret text and extract tool calls.
//!
//! ### Implementations
//!
//! The module provides an implementation for Ollama:
//!
//! - `OllamaInterpreter`: Uses Ollama's structured output API to interpret tool calls
//!
//! ### Helper Functions
//!
//! - `augment_message_with_tool_calls`: A utility function that takes any message, extracts text content, sends it to an interpreter, and adds any detected tool calls back to the message.
//!

use super::errors::ProviderError;
use super::ollama::OLLAMA_DEFAULT_PORT;
use super::ollama::OLLAMA_HOST;
use crate::conversation::message::{Message, MessageContent};
use crate::conversation::Conversation;
use crate::model::ModelConfig;
use crate::providers::base::DEFAULT_PROVIDER_TIMEOUT_SECS;
use crate::providers::formats::openai::create_request;
use anyhow::Result;
use reqwest::Client;
use rmcp::model::{object, CallToolRequestParams, RawContent, Tool};
use serde_json::{json, Value};
use std::ops::Deref;
use std::time::Duration;
use uuid::Uuid;

/// Default model to use for tool interpretation
pub const DEFAULT_INTERPRETER_MODEL_OLLAMA: &str = "mistral-nemo";

/// Environment variables that affect behavior:
/// - GOOSE_TOOLSHIM: When set to "true" or "1", enables using the tool shim in the standard OllamaProvider (default: false)
/// - GOOSE_TOOLSHIM_OLLAMA_MODEL: Ollama model to use as the tool interpreter (default: DEFAULT_INTERPRETER_MODEL)
/// A trait for models that can interpret text into structured tool call JSON format
#[async_trait::async_trait]
pub trait ToolInterpreter {
    /// Interpret potential tool calls from text and convert them to proper tool call JSON format
    async fn interpret_to_tool_calls(
        &self,
        content: &str,
        tools: &[Tool],
    ) -> Result<Vec<CallToolRequestParams>, ProviderError>;
}

/// Ollama-specific implementation of the ToolInterpreter trait
pub struct OllamaInterpreter {
    client: Client,
    base_url: String,
}

impl OllamaInterpreter {
    pub fn new() -> Result<Self, ProviderError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_PROVIDER_TIMEOUT_SECS))
            .build()
            .expect("Failed to create HTTP client");

        let base_url = Self::get_ollama_base_url()?;

        Ok(Self { client, base_url })
    }

    /// Get the Ollama base URL from existing config or use default values
    fn get_ollama_base_url() -> Result<String, ProviderError> {
        let config = crate::config::Config::global();
        let host: String = config
            .get_param("OLLAMA_HOST")
            .unwrap_or_else(|_| OLLAMA_HOST.to_string());

        // Format the URL correctly with http:// prefix if needed
        let base = if host.starts_with("http://") || host.starts_with("https://") {
            &host
        } else {
            &format!("http://{}", host)
        };

        let mut base_url = url::Url::parse(base)
            .map_err(|e| ProviderError::RequestFailed(format!("Invalid base URL: {e}")))?;

        // Set the default port if missing
        // Don't add default port if:
        // 1. URL explicitly ends with standard ports (:80 or :443)
        // 2. URL uses HTTPS (which implicitly uses port 443)
        let explicit_default_port = host.ends_with(":80") || host.ends_with(":443");
        let is_https = base_url.scheme() == "https";

        if base_url.port().is_none() && !explicit_default_port && !is_https {
            base_url.set_port(Some(OLLAMA_DEFAULT_PORT)).map_err(|_| {
                ProviderError::RequestFailed("Failed to set default port".to_string())
            })?;
        }

        Ok(base_url.to_string())
    }

    fn tool_structured_output_format_schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "tool_calls": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": {
                                "type": "string",
                                "description": "The name of the tool to call"
                            },
                            "arguments": {
                                "type": "object",
                                "description": "The arguments to pass to the tool"
                            }
                        },
                        "required": ["name", "arguments"]
                    }
                }
            },
            "required": ["tool_calls"]
        })
    }

    async fn post_structured(
        &self,
        system_prompt: &str,
        format_instruction: &str,
        format_schema: Value,
        model: &str,
    ) -> Result<Value, ProviderError> {
        let base_url = self.base_url.trim_end_matches('/');
        let url = format!("{}/api/chat", base_url);

        let mut messages = Vec::new();
        let user_message = Message::user().with_text(format_instruction);
        messages.push(user_message);

        let model_config = ModelConfig::new(model)
            .map_err(|e| ProviderError::RequestFailed(format!("Model config error: {e}")))?
            .with_canonical_limits("ollama");

        let mut payload = create_request(
            &model_config,
            system_prompt,
            &messages,
            &[], // No tools
            &super::utils::ImageFormat::OpenAi,
            false,
        )?;

        payload["stream"] = json!(false); // needed for the /api/chat endpoint to work
        payload["format"] = format_schema;

        tracing::info!(
            "Tool interpreter payload: {}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );

        let response = self.client.post(&url).json(&payload).send().await?;

        if !response.status().is_success() {
            let status = response.status();

            let error_text = match response.text().await {
                Ok(text) => text,
                Err(_) => "Could not read error response".to_string(),
            };

            return Err(ProviderError::RequestFailed(format!(
                "Ollama structured API returned error status {}: {}",
                status, error_text
            )));
        }

        let response_json: Value = response.json().await.map_err(|e| {
            ProviderError::RequestFailed(format!(
                "Failed to parse Ollama structured API response: {e}"
            ))
        })?;

        Ok(response_json)
    }

    fn process_interpreter_response(
        response: &Value,
    ) -> Result<Vec<CallToolRequestParams>, ProviderError> {
        let mut tool_calls = Vec::new();
        tracing::info!(
            "Tool interpreter response is {}",
            serde_json::to_string_pretty(&response).unwrap_or_default()
        );
        // Extract tool_calls array from the response
        if response.get("message").is_some() && response["message"].get("content").is_some() {
            let content = response["message"]["content"].as_str().unwrap_or_default();

            // Try to parse the content as JSON
            if let Ok(content_json) = serde_json::from_str::<Value>(content) {
                // Check for the format with tool_calls array inside an object
                if content_json.is_object() && content_json.get("tool_calls").is_some() {
                    // Process each tool call in the array
                    if let Some(tool_calls_array) = content_json["tool_calls"].as_array() {
                        for item in tool_calls_array {
                            if item.is_object()
                                && item.get("name").is_some()
                                && item.get("arguments").is_some()
                            {
                                let name = item["name"].as_str().unwrap_or_default().to_string();
                                let arguments = item["arguments"].clone();

                                // Add the tool call to our result vector
                                tool_calls.push(
                                    CallToolRequestParams::new(name)
                                        .with_arguments(object(arguments)),
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(tool_calls)
    }
}

#[async_trait::async_trait]
impl ToolInterpreter for OllamaInterpreter {
    async fn interpret_to_tool_calls(
        &self,
        last_assistant_msg: &str,
        tools: &[Tool],
    ) -> Result<Vec<CallToolRequestParams>, ProviderError> {
        if tools.is_empty() {
            return Ok(vec![]);
        }

        // Create the system prompt
        let system_prompt = "If there is detectable JSON-formatted tool requests, write them into valid JSON tool calls in the following format:
{{
  \"tool_calls\": [
    {{
      \"name\": \"tool_name\",
      \"arguments\": {{
        \"param1\": \"value1\",
        \"param2\": \"value2\"
      }}
    }}
  ]
}}

Otherwise, if no JSON tool requests are provided, use the no-op tool:
{{
  \"tool_calls\": [
    {{
    \"name\": \"noop\",
      \"arguments\": {{
      }}
    }}]
}}
";

        // Create enhanced content with instruction to output tool calls as JSON
        let format_instruction = format!("{}\nRequest: {}\n\n", system_prompt, last_assistant_msg);

        // Define the JSON schema for tool call format
        let format_schema = OllamaInterpreter::tool_structured_output_format_schema();

        // Determine which model to use for interpretation (from env var or default)
        let interpreter_model = std::env::var("GOOSE_TOOLSHIM_OLLAMA_MODEL")
            .unwrap_or_else(|_| DEFAULT_INTERPRETER_MODEL_OLLAMA.to_string());

        // Make a call to ollama with structured output
        let interpreter_response = self
            .post_structured("", &format_instruction, format_schema, &interpreter_model)
            .await?;

        // Process the interpreter response to get tool calls directly
        let tool_calls = OllamaInterpreter::process_interpreter_response(&interpreter_response)?;

        Ok(tool_calls)
    }
}

/// Creates a string containing formatted tool information
pub fn format_tool_info(tools: &[Tool]) -> String {
    let mut tool_info = String::new();
    for tool in tools {
        tool_info.push_str(&format!(
            "Tool Name: {}\nSchema: {}\nDescription: {:?}\n\n",
            tool.name,
            serde_json::to_string_pretty(&tool.input_schema).unwrap_or_default(),
            tool.description
        ));
    }
    tool_info
}

/// Convert messages containing ToolRequest/ToolResponse to text messages for toolshim mode
/// This is necessary because some providers (like Bedrock) validate that tool_use/tool_result
/// blocks can only exist when tools are defined, but in toolshim mode we pass empty tools
pub fn convert_tool_messages_to_text(messages: &[Message]) -> Conversation {
    let converted_messages: Vec<Message> = messages
        .iter()
        .map(|message| {
            let mut new_content = Vec::new();
            let mut has_tool_content = false;

            for content in &message.content {
                match content {
                    MessageContent::ToolRequest(req) => {
                        has_tool_content = true;
                        // Convert tool request to text format
                        let text = if let Ok(tool_call) = &req.tool_call {
                            format!(
                                "Using tool: {}\n{{\n  \"name\": \"{}\",\n  \"arguments\": {}\n}}",
                                tool_call.name,
                                tool_call.name,
                                serde_json::to_string_pretty(&tool_call.arguments)
                                    .unwrap_or_default()
                            )
                        } else {
                            "Tool request failed".to_string()
                        };
                        new_content.push(MessageContent::text(text));
                    }
                    MessageContent::ToolResponse(res) => {
                        has_tool_content = true;
                        // Convert tool response to text format
                        let text = match &res.tool_result {
                            Ok(result) => {
                                let text_contents: Vec<String> = result
                                    .content
                                    .iter()
                                    .filter_map(|c| match c.deref() {
                                        RawContent::Text(t) => Some(t.text.clone()),
                                        _ => None,
                                    })
                                    .collect();
                                format!("Tool result:\n{}", text_contents.join("\n"))
                            }
                            Err(e) => format!("Tool error: {}", e),
                        };
                        new_content.push(MessageContent::text(text));
                    }
                    _ => {
                        // Keep other content types as-is
                        new_content.push(content.clone());
                    }
                }
            }

            if has_tool_content {
                Message::new(message.role.clone(), message.created, new_content)
            } else {
                message.clone()
            }
        })
        .collect();

    Conversation::new_unvalidated(converted_messages)
}

/// Modifies the system prompt to include tool usage instructions when tool interpretation is enabled
pub fn modify_system_prompt_for_tool_json(system_prompt: &str, tools: &[Tool]) -> String {
    let tool_info = format_tool_info(tools);

    format!(
        "{}\n\n{}\n\nBreak down your task into smaller steps and do one step and tool call at a time. Do not try to use multiple tools at once. If you want to use a tool, tell the user what tool to use by specifying the tool in this JSON format\n{{\n  \"name\": \"tool_name\",\n  \"arguments\": {{\n    \"parameter1\": \"value1\",\n    \"parameter2\": \"value2\"\n }}\n}}. After you get the tool result back, consider the result and then proceed to do the next step and tool call if required.",
        system_prompt,
        tool_info
    )
}

/// Helper function to augment a message with tool calls if any are detected
pub async fn augment_message_with_tool_calls<T: ToolInterpreter>(
    interpreter: &T,
    message: Message,
    tools: &[Tool],
) -> Result<Message, ProviderError> {
    // If there are no tools or the message is empty, return the original message
    if tools.is_empty() {
        return Ok(message);
    }

    // Extract content from the message
    let content_opt = message.content.iter().find_map(|content| {
        if let MessageContent::Text(text) = content {
            Some(text.text.as_str())
        } else {
            None
        }
    });

    // If there's no text content or it's already a tool request, return the original message
    let content = match content_opt {
        Some(text) => text,
        None => return Ok(message),
    };

    // Check if there's already a tool request
    if message
        .content
        .iter()
        .any(|content| matches!(content, MessageContent::ToolRequest(_)))
    {
        return Ok(message);
    }

    // Use the interpreter to convert the content to tool calls
    let tool_calls = interpreter.interpret_to_tool_calls(content, tools).await?;

    // If no tool calls were detected, return the original message
    if tool_calls.is_empty() {
        return Ok(message);
    }

    // Add each tool call to the message
    let mut final_message = message;
    for tool_call in tool_calls {
        if tool_call.name != "noop" {
            // do not actually execute noop tool
            let id = Uuid::new_v4().to_string();
            final_message = final_message.with_tool_request(id, Ok(tool_call));
        }
    }

    Ok(final_message)
}
