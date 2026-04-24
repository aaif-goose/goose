use crate::conversation::message::{Message, MessageContent};
use crate::model::ModelConfig;
use crate::providers::formats::anthropic::{thinking_effort, thinking_type, ThinkingType};
use crate::providers::utils::{
    convert_image, detect_image_path, extract_reasoning_effort, is_openai_responses_model,
    is_valid_function_name, load_image_file, safely_parse_json, sanitize_function_name,
    ImageFormat,
};
use anyhow::{anyhow, Error};
use rmcp::model::{
    object, AnnotateAble, CallToolRequestParams, Content, ErrorCode, ErrorData, RawContent,
    ResourceContents, Role, Tool,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::borrow::Cow;

#[derive(Serialize)]
struct DatabricksMessage {
    content: Value,
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

fn format_text_content(text: &str, image_format: &ImageFormat) -> (Vec<Value>, bool) {
    let mut items = vec![json!({"type": "text", "text": text})];
    let has_image = if let Some(path) = detect_image_path(text) {
        if let Ok(image) = load_image_file(path) {
            items.push(convert_image(&image, image_format));
        }
        true
    } else {
        false
    };
    (items, has_image)
}

fn format_tool_response(
    response: &crate::conversation::message::ToolResponse,
    image_format: &ImageFormat,
) -> Vec<DatabricksMessage> {
    let mut result = Vec::new();

    match &response.tool_result {
        Ok(call_result) => {
            let abridged: Vec<_> = call_result.content.iter().map(|c| c.raw.clone()).collect();

            let mut tool_content = Vec::new();
            let mut image_messages = Vec::new();

            for content in abridged {
                match content {
                    RawContent::Image(image) => {
                        tool_content.push(Content::text(
                            "This tool result included an image that is uploaded in the next message.",
                        ));
                        image_messages.push(DatabricksMessage {
                            role: "user".to_string(),
                            content: [convert_image(&image.no_annotation(), image_format)].into(),
                            tool_calls: None,
                            tool_call_id: None,
                        });
                    }
                    RawContent::Resource(resource) => {
                        let text = match &resource.resource {
                            ResourceContents::TextResourceContents { text, .. } => text.clone(),
                            _ => String::new(),
                        };
                        tool_content.push(Content::text(text));
                    }
                    _ => tool_content.push(content.no_annotation()),
                }
            }

            let tool_response_content: Value = json!(tool_content
                .iter()
                .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                .collect::<Vec<String>>()
                .join(" "));

            result.push(DatabricksMessage {
                content: tool_response_content,
                role: "tool".to_string(),
                tool_call_id: Some(response.id.clone()),
                tool_calls: None,
            });
            result.extend(image_messages);
        }
        Err(e) => {
            result.push(DatabricksMessage {
                role: "tool".to_string(),
                content: format!("The tool call returned the following error:\n{}", e).into(),
                tool_call_id: Some(response.id.clone()),
                tool_calls: None,
            });
        }
    }

    result
}

/// Convert internal Message format to Databricks' API message specification
///   Databricks is mostly OpenAI compatible, but has some differences (reasoning type, etc)
///   some openai compatible endpoints use the anthropic image spec at the content level
///   even though the message structure is otherwise following openai, the enum switches this
fn format_messages(messages: &[Message], image_format: &ImageFormat) -> Vec<DatabricksMessage> {
    let mut result = Vec::new();
    for message in messages {
        let mut converted = DatabricksMessage {
            content: Value::Null,
            role: match message.role {
                Role::User => "user".to_string(),
                Role::Assistant => "assistant".to_string(),
            },
            tool_calls: None,
            tool_call_id: None,
        };

        let mut content_array = Vec::new();
        let mut has_tool_calls = false;
        let mut has_multiple_content = false;

        for content in &message.content {
            match content {
                MessageContent::Text(text) => {
                    if !text.text.is_empty() {
                        let (items, multi) = format_text_content(&text.text, image_format);
                        content_array.extend(items);
                        has_multiple_content |= multi;
                    }
                }
                MessageContent::Thinking(content) => {
                    has_multiple_content = true;
                    content_array.push(json!({
                        "type": "reasoning",
                        "summary": [{
                            "type": "summary_text",
                            "text": content.thinking,
                            "signature": content.signature
                        }]
                    }));
                }
                MessageContent::RedactedThinking(content) => {
                    has_multiple_content = true;
                    content_array.push(json!({
                        "type": "reasoning",
                        "summary": [{"type": "summary_encrypted_text", "data": content.data}]
                    }));
                }
                MessageContent::ToolRequest(request) => {
                    has_tool_calls = true;
                    match &request.tool_call {
                        Ok(tool_call) => {
                            let sanitized_name = sanitize_function_name(&tool_call.name);
                            let arguments_str = tool_call
                                .arguments
                                .as_ref()
                                .map(|args| {
                                    serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string())
                                })
                                .unwrap_or_else(|| "{}".to_string());

                            let tool_calls = converted.tool_calls.get_or_insert_default();
                            let mut tool_call_json = json!({
                                "id": request.id,
                                "type": "function",
                                "function": {
                                    "name": sanitized_name,
                                    "arguments": arguments_str,
                                }
                            });

                            if let Some(metadata) = &request.metadata {
                                for (key, value) in metadata {
                                    tool_call_json[key] = value.clone();
                                }
                            }

                            tool_calls.push(tool_call_json);
                        }
                        Err(e) => {
                            content_array
                                .push(json!({"type": "text", "text": format!("Error: {}", e)}));
                        }
                    }
                }
                MessageContent::ToolResponse(response) => {
                    result.extend(format_tool_response(response, image_format));
                }
                MessageContent::Image(image) => {
                    content_array.push(convert_image(image, image_format));
                }
                MessageContent::FrontendToolRequest(req) => {
                    let text = match &req.tool_call {
                        Ok(tool_call) => format!(
                            "Frontend tool request: {} ({})",
                            tool_call.name,
                            serde_json::to_string_pretty(&tool_call.arguments).unwrap()
                        ),
                        Err(e) => format!("Frontend tool request error: {}", e),
                    };
                    content_array.push(json!({"type": "text", "text": text}));
                }
                MessageContent::SystemNotification(_)
                | MessageContent::ToolConfirmationRequest(_)
                | MessageContent::ActionRequired(_) => {}
            }
        }

        if !content_array.is_empty() {
            converted.content = if content_array.len() == 1
                && !has_multiple_content
                && content_array[0]["type"] == "text"
            {
                json!(content_array[0]["text"])
            } else {
                json!(content_array)
            };
        }

        if !content_array.is_empty() || has_tool_calls {
            result.push(converted);
        }
    }

    result
}

fn apply_claude_thinking_config(payload: &mut Value, model_config: &ModelConfig) {
    let obj = payload.as_object_mut().unwrap();

    match thinking_type(model_config) {
        ThinkingType::Adaptive => {
            obj.insert("thinking".to_string(), json!({ "type": "adaptive" }));
            obj.insert(
                "output_config".to_string(),
                json!({ "effort": thinking_effort(model_config).to_string() }),
            );
            obj.insert(
                "max_completion_tokens".to_string(),
                json!(model_config.max_output_tokens()),
            );
        }
        ThinkingType::Enabled => {
            let budget_tokens = model_config
                .get_config_param::<i32>("budget_tokens", "CLAUDE_THINKING_BUDGET")
                .unwrap_or(16000)
                .max(1024);

            let max_tokens = model_config.max_output_tokens() + budget_tokens;
            obj.insert("max_tokens".to_string(), json!(max_tokens));
            obj.insert(
                "thinking".to_string(),
                json!({
                    "type": "enabled",
                    "budget_tokens": budget_tokens
                }),
            );
            obj.insert("temperature".to_string(), json!(2));
        }
        ThinkingType::Disabled => {
            if let Some(temp) = model_config.temperature {
                obj.insert("temperature".to_string(), json!(temp));
            }
            obj.insert(
                "max_completion_tokens".to_string(),
                json!(model_config.max_output_tokens()),
            );
        }
    }
}

pub fn format_tools(tools: &[Tool], model_name: &str) -> anyhow::Result<Vec<Value>> {
    let mut tool_names = std::collections::HashSet::new();
    let mut result = Vec::new();

    let is_gemini = model_name.contains("gemini");

    for tool in tools {
        if !tool_names.insert(&tool.name) {
            return Err(anyhow!("Duplicate tool name: {}", tool.name));
        }

        let has_properties = tool
            .input_schema
            .get("properties")
            .and_then(|v| v.as_object())
            .is_some_and(|p| !p.is_empty());

        let function_def = if is_gemini {
            let mut def = json!({
                "name": tool.name,
                "description": tool.description,
            });
            if has_properties {
                def["parametersJsonSchema"] = json!(tool.input_schema);
            }
            def
        } else {
            let mut def = json!({
                "name": tool.name,
                "description": tool.description,
            });
            if has_properties {
                def["parameters"] = json!(tool.input_schema);
            }
            def
        };

        result.push(json!({
            "type": "function",
            "function": function_def,
        }));
    }

    Ok(result)
}

/// Convert Databricks' API response to internal Message format
#[allow(clippy::too_many_lines)]
pub fn response_to_message(response: &Value) -> anyhow::Result<Message> {
    let original = &response["choices"][0]["message"];
    let mut content = Vec::new();

    // Handle array-based content
    if let Some(content_array) = original.get("content").and_then(|c| c.as_array()) {
        for content_item in content_array {
            match content_item.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(text) = content_item.get("text").and_then(|t| t.as_str()) {
                        content.push(MessageContent::text(text));
                    }
                }
                Some("reasoning") => {
                    if let Some(summary_array) =
                        content_item.get("summary").and_then(|s| s.as_array())
                    {
                        for summary in summary_array {
                            match summary.get("type").and_then(|t| t.as_str()) {
                                Some("summary_text") => {
                                    let text = summary
                                        .get("text")
                                        .and_then(|t| t.as_str())
                                        .unwrap_or_default();
                                    let signature = summary
                                        .get("signature")
                                        .and_then(|s| s.as_str())
                                        .unwrap_or_default();
                                    content.push(MessageContent::thinking(text, signature));
                                }
                                Some("summary_encrypted_text") => {
                                    if let Some(data) = summary.get("data").and_then(|d| d.as_str())
                                    {
                                        content.push(MessageContent::redacted_thinking(data));
                                    }
                                }
                                _ => continue,
                            }
                        }
                    }
                }
                _ => continue,
            }
        }
    } else if let Some(text) = original.get("content").and_then(|t| t.as_str()) {
        // Handle legacy single string content
        content.push(MessageContent::text(text));
    }

    // Handle tool calls
    if let Some(tool_calls) = original.get("tool_calls") {
        if let Some(tool_calls_array) = tool_calls.as_array() {
            for tool_call in tool_calls_array {
                let id = tool_call["id"].as_str().unwrap_or_default().to_string();
                let function_name = tool_call["function"]["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();

                // Get the raw arguments string from the LLM.
                let arguments_str = tool_call["function"]["arguments"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();

                // If arguments_str is empty, default to an empty JSON object string.
                let arguments_str = if arguments_str.is_empty() {
                    "{}".to_string()
                } else {
                    arguments_str
                };

                if !is_valid_function_name(&function_name) {
                    let error = ErrorData {
                        code: ErrorCode::INVALID_REQUEST,
                        message: Cow::from(format!(
                            "The provided function name '{}' had invalid characters, it must match this regex [a-zA-Z0-9_-]+",
                            function_name
                        )),
                        data: None,
                    };
                    content.push(MessageContent::tool_request(id, Err(error)));
                } else {
                    match safely_parse_json(&arguments_str) {
                        Ok(params) => {
                            content.push(MessageContent::tool_request(
                                id,
                                Ok(CallToolRequestParams::new(function_name)
                                    .with_arguments(object(params))),
                            ));
                        }
                        Err(e) => {
                            let error = ErrorData {
                                code: ErrorCode::INVALID_PARAMS,
                                message: Cow::from(format!(
                                    "Could not interpret tool use parameters for id {}: {}. Raw arguments: '{}'",
                                    id, e, arguments_str
                                )),
                                data: None,
                            };
                            content.push(MessageContent::tool_request(id, Err(error)));
                        }
                    }
                }
            }
        }
    }

    Ok(Message::new(
        Role::Assistant,
        chrono::Utc::now().timestamp(),
        content,
    ))
}

/// Check if the model name indicates a Claude/Anthropic model that supports cache control.
fn is_claude_model(model_name: &str) -> bool {
    model_name.contains("claude")
}

/// Add Anthropic-style cache_control fields to the request payload for Claude models.
/// This enables prompt caching to reduce costs when using Claude via Databricks.
///
/// Cache control is added to:
/// - The system message
/// - The last two user messages (for incremental caching across turns)
/// - The last tool definition (so all tools are cached as a single prefix)
pub fn apply_cache_control_for_claude(payload: &mut Value) {
    if let Some(messages_spec) = payload
        .as_object_mut()
        .and_then(|obj| obj.get_mut("messages"))
        .and_then(|messages| messages.as_array_mut())
    {
        // Add cache_control to the last two user messages for incremental caching.
        // The last message gets cached so future turns can read from it.
        // The second-to-last user message is also cached to read from the previous cache.
        let mut user_count = 0;
        for message in messages_spec.iter_mut().rev() {
            if message.get("role") == Some(&json!("user")) {
                if let Some(content) = message.get_mut("content") {
                    if let Some(content_str) = content.as_str() {
                        *content = json!([{
                            "type": "text",
                            "text": content_str,
                            "cache_control": { "type": "ephemeral" }
                        }]);
                    } else if let Some(content_array) = content.as_array_mut() {
                        // Content is already an array, add cache_control to the last element
                        if let Some(last_content) = content_array.last_mut() {
                            if let Some(obj) = last_content.as_object_mut() {
                                obj.insert(
                                    "cache_control".to_string(),
                                    json!({ "type": "ephemeral" }),
                                );
                            }
                        }
                    }
                }
                user_count += 1;
                if user_count >= 2 {
                    break;
                }
            }
        }

        // Add cache_control to the system message
        if let Some(system_message) = messages_spec
            .iter_mut()
            .find(|msg| msg.get("role") == Some(&json!("system")))
        {
            if let Some(content) = system_message.get_mut("content") {
                if let Some(content_str) = content.as_str() {
                    *system_message = json!({
                        "role": "system",
                        "content": [{
                            "type": "text",
                            "text": content_str,
                            "cache_control": { "type": "ephemeral" }
                        }]
                    });
                }
            }
        }
    }

    // Add cache_control to the last tool definition
    if let Some(tools_spec) = payload
        .as_object_mut()
        .and_then(|obj| obj.get_mut("tools"))
        .and_then(|tools| tools.as_array_mut())
    {
        if let Some(last_tool) = tools_spec.last_mut() {
            if let Some(function) = last_tool.get_mut("function") {
                if let Some(obj) = function.as_object_mut() {
                    obj.insert("cache_control".to_string(), json!({ "type": "ephemeral" }));
                }
            }
        }
    }
}

/// Validates and fixes tool schemas to ensure they have proper parameter structure.
/// If parameters exist, ensures they have properties and required fields, or removes parameters entirely.
pub fn validate_tool_schemas(tools: &mut [Value]) {
    for tool in tools.iter_mut() {
        if let Some(function) = tool.get_mut("function") {
            if let Some(parameters) = function.get_mut("parameters") {
                if parameters.is_object() {
                    ensure_valid_json_schema(parameters);
                }
            }
        }
    }
}

/// Ensures that the given JSON value follows the expected JSON Schema structure.
fn ensure_valid_json_schema(schema: &mut Value) {
    if let Some(params_obj) = schema.as_object_mut() {
        // Check if this is meant to be an object type schema
        let is_object_type = params_obj
            .get("type")
            .and_then(|t| t.as_str())
            .is_none_or(|t| t == "object"); // Default to true if no type is specified

        // Only apply full schema validation to object types
        if is_object_type {
            // Ensure required fields exist with default values
            params_obj.entry("properties").or_insert_with(|| json!({}));
            params_obj.entry("required").or_insert_with(|| json!([]));
            params_obj.entry("type").or_insert_with(|| json!("object"));

            // Recursively validate properties if it exists
            if let Some(properties) = params_obj.get_mut("properties") {
                if let Some(properties_obj) = properties.as_object_mut() {
                    for (_key, prop) in properties_obj.iter_mut() {
                        if prop.is_object()
                            && prop.get("type").and_then(|t| t.as_str()) == Some("object")
                        {
                            ensure_valid_json_schema(prop);
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
pub fn create_request(
    model_config: &ModelConfig,
    system: &str,
    messages: &[Message],
    tools: &[Tool],
    image_format: &ImageFormat,
) -> anyhow::Result<Value, Error> {
    if model_config.model_name.starts_with("o1-mini") {
        return Err(anyhow!(
            "o1-mini model is not currently supported since goose uses tool calling and o1-mini does not support it. Please use o1 or o3 models instead."
        ));
    }

    let (model_name, reasoning_effort) = extract_reasoning_effort(&model_config.model_name);
    let is_openai_reasoning_model = is_openai_responses_model(&model_name);

    let system_message = DatabricksMessage {
        role: "system".to_string(),
        content: system.into(),
        tool_calls: None,
        tool_call_id: None,
    };

    let messages_spec = format_messages(messages, image_format);
    let mut tools_spec = if !tools.is_empty() {
        format_tools(tools, &model_config.model_name)?
    } else {
        vec![]
    };

    // Validate tool schemas
    validate_tool_schemas(&mut tools_spec);

    let mut messages_array = vec![system_message];
    messages_array.extend(messages_spec);

    let mut payload = json!({
        "model": model_name,
        "messages": messages_array
    });

    if let Some(effort) = reasoning_effort {
        payload
            .as_object_mut()
            .unwrap()
            .insert("reasoning_effort".to_string(), json!(effort));
    }

    if !tools_spec.is_empty() {
        payload
            .as_object_mut()
            .unwrap()
            .insert("tools".to_string(), json!(tools_spec));
    }

    if is_claude_model(&model_config.model_name) {
        apply_claude_thinking_config(&mut payload, model_config);
    } else {
        // open ai reasoning models currently don't support temperature
        if !is_openai_reasoning_model {
            if let Some(temp) = model_config.temperature {
                payload
                    .as_object_mut()
                    .unwrap()
                    .insert("temperature".to_string(), json!(temp));
            }
        }

        payload.as_object_mut().unwrap().insert(
            "max_completion_tokens".to_string(),
            json!(model_config.max_output_tokens()),
        );
    }

    // Apply cache control for Claude models to enable prompt caching
    if is_claude_model(&model_config.model_name) {
        apply_cache_control_for_claude(&mut payload);
    }

    // Add request_params to the payload (e.g., anthropic_beta for extended context)
    if let Some(params) = &model_config.request_params {
        if let Some(obj) = payload.as_object_mut() {
            for (key, value) in params {
                obj.insert(key.clone(), value.clone());
            }
        }
    }

    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::Message;
    use rmcp::model::CallToolResult;
    use rmcp::object;
    use serde_json::json;

    const OPENAI_TOOL_USE_RESPONSE: &str = r#"{
        "choices": [{
            "role": "assistant",
            "message": {
                "tool_calls": [{
                    "id": "1",
                    "function": {
                        "name": "example_fn",
                        "arguments": "{\"param\": \"value\"}"
                    }
                }]
            }
        }],
        "usage": {
            "input_tokens": 10,
            "output_tokens": 25,
            "total_tokens": 35
        }
    }"#;

    #[test]
    fn test_format_messages() -> anyhow::Result<()> {
        let message = Message::user().with_text("Hello");
        let spec = format_messages(&[message], &ImageFormat::OpenAi);

        assert_eq!(spec.len(), 1);
        assert_eq!(spec[0].role, "user");
        assert_eq!(spec[0].content, "Hello");
        Ok(())
    }

    #[test]
    fn test_format_tools() -> anyhow::Result<()> {
        let tool = Tool::new(
            "test_tool",
            "A test tool",
            object!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Test parameter"
                    }
                },
                "required": ["input"]
            }),
        );

        let spec = format_tools(std::slice::from_ref(&tool), "gpt-4o")?;
        assert_eq!(
            spec[0]["function"]["parameters"]["$schema"],
            "http://json-schema.org/draft-07/schema#"
        );

        let spec = format_tools(std::slice::from_ref(&tool), "gemini-2-5-flash")?;
        assert!(spec[0]["function"].get("parametersJsonSchema").is_some());
        assert_eq!(
            spec[0]["function"]["parametersJsonSchema"]["type"],
            "object"
        );

        let spec = format_tools(&[tool], "databricks-gemini-3-pro")?;
        assert!(spec[0]["function"].get("parametersJsonSchema").is_some());
        assert_eq!(
            spec[0]["function"]["parametersJsonSchema"]["type"],
            "object"
        );

        Ok(())
    }

    #[test]
    fn test_format_messages_complex() -> anyhow::Result<()> {
        let mut messages = vec![
            Message::assistant().with_text("Hello!"),
            Message::user().with_text("How are you?"),
            Message::assistant().with_tool_request(
                "tool1",
                Ok(CallToolRequestParams::new("example")
                    .with_arguments(object!({"param1": "value1"}))),
            ),
        ];

        let tool_id = if let MessageContent::ToolRequest(request) = &messages[2].content[0] {
            &request.id
        } else {
            panic!("should be tool request");
        };

        messages.push(Message::user().with_tool_response(
            tool_id,
            Ok(CallToolResult::success(vec![Content::text("Result")])),
        ));

        let as_value =
            serde_json::to_value(format_messages(&messages, &ImageFormat::OpenAi)).unwrap();
        let spec = as_value.as_array().unwrap();

        assert_eq!(spec.len(), 4);
        assert_eq!(spec[0]["role"], "assistant");
        assert_eq!(spec[0]["content"], "Hello!");
        assert_eq!(spec[1]["role"], "user");
        assert_eq!(spec[1]["content"], "How are you?");
        assert_eq!(spec[2]["role"], "assistant");
        assert!(spec[2]["tool_calls"].is_array());
        assert_eq!(spec[3]["role"], "tool");
        assert_eq!(spec[3]["content"], "Result");
        assert_eq!(spec[3]["tool_call_id"], spec[2]["tool_calls"][0]["id"]);

        Ok(())
    }

    #[test]
    fn test_format_messages_multiple_content() -> anyhow::Result<()> {
        let mut messages = vec![Message::assistant().with_tool_request(
            "tool1",
            Ok(CallToolRequestParams::new("example").with_arguments(object!({"param1": "value1"}))),
        )];

        let tool_id = if let MessageContent::ToolRequest(request) = &messages[0].content[0] {
            &request.id
        } else {
            panic!("should be tool request");
        };

        messages.push(Message::user().with_tool_response(
            tool_id,
            Ok(CallToolResult::success(vec![Content::text("Result")])),
        ));

        let as_value =
            serde_json::to_value(format_messages(&messages, &ImageFormat::OpenAi)).unwrap();
        let spec = as_value.as_array().unwrap();

        assert_eq!(spec.len(), 2);
        assert_eq!(spec[0]["role"], "assistant");
        assert!(spec[0]["tool_calls"].is_array());
        assert_eq!(spec[1]["role"], "tool");
        assert_eq!(spec[1]["content"], "Result");
        assert_eq!(spec[1]["tool_call_id"], spec[0]["tool_calls"][0]["id"]);

        Ok(())
    }

    #[test]
    fn test_format_tools_duplicate() -> anyhow::Result<()> {
        let tool1 = Tool::new(
            "test_tool",
            "Test tool",
            object!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Test parameter"
                    }
                },
                "required": ["input"]
            }),
        );

        let tool2 = Tool::new(
            "test_tool",
            "Test tool",
            object!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string",
                        "description": "Test parameter"
                    }
                },
                "required": ["input"]
            }),
        );

        let result = format_tools(&[tool1, tool2], "gpt-4o");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Duplicate tool name"));

        Ok(())
    }

    #[test]
    fn test_format_messages_with_image_path() -> anyhow::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let png_path = temp_dir.path().join("test.png");
        let png_data = [
            0x89, 0x50, 0x4E, 0x47, // PNG magic number
            0x0D, 0x0A, 0x1A, 0x0A, // PNG header
            0x00, 0x00, 0x00, 0x0D, // Rest of fake PNG data
        ];
        std::fs::write(&png_path, png_data)?;
        let png_path_str = png_path.to_str().unwrap();

        // Create message with image path
        let message = Message::user().with_text(format!("Here is an image: {}", png_path_str));
        let as_value =
            serde_json::to_value(format_messages(&[message], &ImageFormat::OpenAi)).unwrap();
        let spec = as_value.as_array().unwrap();

        assert_eq!(spec.len(), 1);
        assert_eq!(spec[0]["role"], "user");

        // Content should be an array with text and image
        let content = spec[0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "text");
        assert!(content[0]["text"].as_str().unwrap().contains(png_path_str));
        assert_eq!(content[1]["type"], "image_url");
        assert!(content[1]["image_url"]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));

        Ok(())
    }

    #[test]
    fn test_response_to_message_text() -> anyhow::Result<()> {
        let response = json!({
            "choices": [{
                "role": "assistant",
                "message": {
                    "content": "Hello from John Cena!"
                }
            }],
            "usage": {
                "input_tokens": 10,
                "output_tokens": 25,
                "total_tokens": 35
            }
        });

        let message = response_to_message(&response)?;
        assert_eq!(message.content.len(), 1);
        if let MessageContent::Text(text) = &message.content[0] {
            assert_eq!(text.text, "Hello from John Cena!");
        } else {
            panic!("Expected Text content");
        }
        assert!(matches!(message.role, Role::Assistant));

        Ok(())
    }

    #[test]
    fn test_response_to_message_valid_toolrequest() -> anyhow::Result<()> {
        let response: Value = serde_json::from_str(OPENAI_TOOL_USE_RESPONSE)?;
        let message = response_to_message(&response)?;

        assert_eq!(message.content.len(), 1);
        if let MessageContent::ToolRequest(request) = &message.content[0] {
            let tool_call = request.tool_call.as_ref().unwrap();
            assert_eq!(tool_call.name, "example_fn");
            assert_eq!(tool_call.arguments, Some(object!({"param": "value"})));
        } else {
            panic!("Expected ToolRequest content");
        }

        Ok(())
    }

    #[test]
    fn test_response_to_message_invalid_func_name() -> anyhow::Result<()> {
        let mut response: Value = serde_json::from_str(OPENAI_TOOL_USE_RESPONSE)?;
        response["choices"][0]["message"]["tool_calls"][0]["function"]["name"] =
            json!("invalid fn");

        let message = response_to_message(&response)?;

        if let MessageContent::ToolRequest(request) = &message.content[0] {
            match &request.tool_call {
                Err(ErrorData {
                    code: ErrorCode::INVALID_REQUEST,
                    message: msg,
                    data: None,
                }) => {
                    assert!(msg.starts_with("The provided function name"));
                }
                _ => panic!("Expected ToolNotFound error"),
            }
        } else {
            panic!("Expected ToolRequest content");
        }

        Ok(())
    }

    #[test]
    fn test_response_to_message_json_decode_error() -> anyhow::Result<()> {
        let mut response: Value = serde_json::from_str(OPENAI_TOOL_USE_RESPONSE)?;
        response["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"] =
            json!("invalid json {");

        let message = response_to_message(&response)?;

        if let MessageContent::ToolRequest(request) = &message.content[0] {
            match &request.tool_call {
                Err(ErrorData {
                    code: ErrorCode::INVALID_PARAMS,
                    message: msg,
                    data: None,
                }) => {
                    assert!(msg.starts_with("Could not interpret tool use parameters"));
                }
                _ => panic!("Expected InvalidParameters error"),
            }
        } else {
            panic!("Expected ToolRequest content");
        }

        Ok(())
    }

    #[test]
    fn test_response_to_message_empty_argument() -> anyhow::Result<()> {
        let mut response: Value = serde_json::from_str(OPENAI_TOOL_USE_RESPONSE)?;
        response["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"] =
            serde_json::Value::String("".to_string());

        let message = response_to_message(&response)?;

        if let MessageContent::ToolRequest(request) = &message.content[0] {
            let tool_call = request.tool_call.as_ref().unwrap();
            assert_eq!(tool_call.name, "example_fn");
            assert_eq!(tool_call.arguments, Some(object!({})));
        } else {
            panic!("Expected ToolRequest content");
        }

        Ok(())
    }

    #[test]
    fn test_create_request_gpt_4o() -> anyhow::Result<()> {
        // Test default medium reasoning effort for O3 model
        let model_config = ModelConfig {
            model_name: "gpt-4o".to_string(),
            context_limit: Some(4096),
            temperature: None,
            max_tokens: Some(1024),
            toolshim: false,
            toolshim_model: None,
            fast_model_config: None,
            request_params: None,
            reasoning: None,
        };
        let request = create_request(&model_config, "system", &[], &[], &ImageFormat::OpenAi)?;
        let obj = request.as_object().unwrap();
        let expected = json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "system",
                    "content": "system"
                }
            ],
            "max_completion_tokens": 1024
        });

        for (key, value) in expected.as_object().unwrap() {
            assert_eq!(obj.get(key).unwrap(), value);
        }

        Ok(())
    }

    #[test]
    fn test_create_request_reasoning_effort() -> anyhow::Result<()> {
        let model_config = ModelConfig {
            model_name: "o3-mini-high".to_string(),
            context_limit: Some(4096),
            temperature: None,
            max_tokens: Some(1024),
            toolshim: false,
            toolshim_model: None,
            fast_model_config: None,
            request_params: None,
            reasoning: None,
        };
        let request = create_request(&model_config, "system", &[], &[], &ImageFormat::OpenAi)?;
        assert_eq!(request["reasoning_effort"], "high");
        Ok(())
    }

    #[test]
    fn test_create_request_reasoning_effort_xhigh() -> anyhow::Result<()> {
        let model_config = ModelConfig {
            model_name: "o3-xhigh".to_string(),
            context_limit: Some(4096),
            temperature: None,
            max_tokens: Some(1024),
            toolshim: false,
            toolshim_model: None,
            fast_model_config: None,
            request_params: None,
            reasoning: None,
        };
        let request = create_request(&model_config, "system", &[], &[], &ImageFormat::OpenAi)?;
        assert_eq!(request["model"], "o3");
        assert_eq!(request["reasoning_effort"], "xhigh");
        Ok(())
    }

    #[test]
    fn test_create_request_reasoning_effort_none() -> anyhow::Result<()> {
        let model_config = ModelConfig {
            model_name: "o3-none".to_string(),
            context_limit: Some(4096),
            temperature: None,
            max_tokens: Some(1024),
            toolshim: false,
            toolshim_model: None,
            fast_model_config: None,
            request_params: None,
            reasoning: None,
        };
        let request = create_request(&model_config, "system", &[], &[], &ImageFormat::OpenAi)?;
        assert_eq!(request["model"], "o3");
        assert_eq!(request["reasoning_effort"], "none");
        Ok(())
    }

    #[test]
    fn test_create_request_reasoning_effort_for_prefixed_gpt5_model() -> anyhow::Result<()> {
        let model_config = ModelConfig {
            model_name: "databricks-gpt-5.4-high".to_string(),
            context_limit: Some(4096),
            temperature: None,
            max_tokens: Some(1024),
            toolshim: false,
            toolshim_model: None,
            fast_model_config: None,
            request_params: None,
            reasoning: None,
        };
        let request = create_request(&model_config, "system", &[], &[], &ImageFormat::OpenAi)?;
        assert_eq!(request["model"], "databricks-gpt-5.4");
        assert_eq!(request["reasoning_effort"], "high");
        Ok(())
    }

    #[test]
    fn test_create_request_adaptive_thinking_for_46_models() -> anyhow::Result<()> {
        let _guard = env_lock::lock_env([
            ("CLAUDE_THINKING_TYPE", Some("adaptive")),
            ("CLAUDE_THINKING_EFFORT", Some("low")),
            ("CLAUDE_THINKING_ENABLED", None::<&str>),
            ("CLAUDE_THINKING_BUDGET", None::<&str>),
        ]);

        let mut model_config = ModelConfig::new_or_fail("databricks-claude-opus-4-6");
        model_config.max_tokens = Some(4096);

        let request = create_request(&model_config, "system", &[], &[], &ImageFormat::OpenAi)?;

        assert_eq!(request["thinking"]["type"], "adaptive");
        assert_eq!(request["output_config"]["effort"], "low");
        assert!(request.get("temperature").is_none());
        assert_eq!(request["max_completion_tokens"], 4096);
        assert!(request.get("max_tokens").is_none());

        Ok(())
    }

    #[test]
    fn test_create_request_enabled_thinking_with_budget() -> anyhow::Result<()> {
        let _guard = env_lock::lock_env([
            ("CLAUDE_THINKING_TYPE", None::<&str>),
            ("CLAUDE_THINKING_ENABLED", None::<&str>),
            ("CLAUDE_THINKING_BUDGET", Some("10000")),
        ]);

        let mut model_config = ModelConfig::new_or_fail("databricks-claude-3-7-sonnet");
        model_config.max_tokens = Some(4096);
        model_config = model_config.with_request_params(Some(std::collections::HashMap::from([(
            "thinking_type".to_string(),
            json!("enabled"),
        )])));

        let request = create_request(&model_config, "system", &[], &[], &ImageFormat::OpenAi)?;

        assert_eq!(request["thinking"]["type"], "enabled");
        assert_eq!(request["thinking"]["budget_tokens"], 10000);
        assert_eq!(request["max_tokens"], 14096);
        assert_eq!(request["temperature"], 2);
        assert!(request.get("max_completion_tokens").is_none());

        Ok(())
    }

    #[test]
    fn test_response_to_message_claude_thinking() -> anyhow::Result<()> {
        let response = json!({
            "model": "us.anthropic.claude-3-7-sonnet-20250219-v1:0",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": [
                        {
                            "type": "reasoning",
                            "summary": [
                                {
                                    "type": "summary_text",
                                    "text": "Test thinking content",
                                    "signature": "test-signature"
                                }
                            ]
                        },
                        {
                            "type": "text",
                            "text": "Regular text content"
                        }
                    ]
                },
                "index": 0,
                "finish_reason": "stop"
            }]
        });

        let message = response_to_message(&response)?;
        assert_eq!(message.content.len(), 2);

        if let MessageContent::Thinking(thinking) = &message.content[0] {
            assert_eq!(thinking.thinking, "Test thinking content");
            assert_eq!(thinking.signature, "test-signature");
        } else {
            panic!("Expected Thinking content");
        }

        if let MessageContent::Text(text) = &message.content[1] {
            assert_eq!(text.text, "Regular text content");
        } else {
            panic!("Expected Text content");
        }

        Ok(())
    }

    #[test]
    fn test_response_to_message_claude_encrypted_thinking() -> anyhow::Result<()> {
        let response = json!({
            "model": "claude-3-7-sonnet-20250219",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": [
                        {
                            "type": "reasoning",
                            "summary": [
                                {
                                    "type": "summary_encrypted_text",
                                    "data": "E23sQFCkYIARgCKkATCHitsdf327Ber3v4NYUq2"
                                }
                            ]
                        },
                        {
                            "type": "text",
                            "text": "Regular text content"
                        }
                    ]
                },
                "index": 0,
                "finish_reason": "stop"
            }]
        });

        let message = response_to_message(&response)?;
        assert_eq!(message.content.len(), 2);

        if let MessageContent::RedactedThinking(redacted) = &message.content[0] {
            assert_eq!(redacted.data, "E23sQFCkYIARgCKkATCHitsdf327Ber3v4NYUq2");
        } else {
            panic!("Expected RedactedThinking content");
        }

        if let MessageContent::Text(text) = &message.content[1] {
            assert_eq!(text.text, "Regular text content");
        } else {
            panic!("Expected Text content");
        }

        Ok(())
    }

    #[test]
    fn test_format_messages_tool_request_with_none_arguments() -> anyhow::Result<()> {
        // Test that tool calls with None arguments are formatted as "{}" string
        let message = Message::assistant()
            .with_tool_request("tool1", Ok(CallToolRequestParams::new("test_tool")));

        let spec = format_messages(&[message], &ImageFormat::OpenAi);
        let as_value = serde_json::to_value(spec)?;
        let spec_array = as_value.as_array().unwrap();

        assert_eq!(spec_array.len(), 1);
        assert_eq!(spec_array[0]["role"], "assistant");
        assert!(spec_array[0]["tool_calls"].is_array());

        let tool_call = &spec_array[0]["tool_calls"][0];
        assert_eq!(tool_call["id"], "tool1");
        assert_eq!(tool_call["type"], "function");
        assert_eq!(tool_call["function"]["name"], "test_tool");
        // This should be the string "{}", not null
        assert_eq!(tool_call["function"]["arguments"], "{}");

        Ok(())
    }

    #[test]
    fn test_format_messages_tool_request_with_some_arguments() -> anyhow::Result<()> {
        // Test that tool calls with Some arguments are properly JSON-serialized
        let message = Message::assistant().with_tool_request(
            "tool1",
            Ok(CallToolRequestParams::new("test_tool")
                .with_arguments(object!({"param": "value", "number": 42}))),
        );

        let spec = format_messages(&[message], &ImageFormat::OpenAi);
        let as_value = serde_json::to_value(spec)?;
        let spec_array = as_value.as_array().unwrap();

        assert_eq!(spec_array.len(), 1);
        assert_eq!(spec_array[0]["role"], "assistant");
        assert!(spec_array[0]["tool_calls"].is_array());

        let tool_call = &spec_array[0]["tool_calls"][0];
        assert_eq!(tool_call["id"], "tool1");
        assert_eq!(tool_call["type"], "function");
        assert_eq!(tool_call["function"]["name"], "test_tool");
        // This should be a JSON string representation
        let args_str = tool_call["function"]["arguments"].as_str().unwrap();
        let parsed_args: Value = serde_json::from_str(args_str)?;
        assert_eq!(parsed_args["param"], "value");
        assert_eq!(parsed_args["number"], 42);

        Ok(())
    }

    #[test]
    fn test_is_claude_model() {
        assert!(is_claude_model("databricks-claude-sonnet-4"));
        assert!(is_claude_model("databricks-claude-3-7-sonnet"));
        assert!(is_claude_model("claude-sonnet-4"));
        assert!(is_claude_model("goose-claude-sonnet"));
        assert!(!is_claude_model("gpt-4o"));
        assert!(!is_claude_model("gemini-2-5-flash"));
        assert!(!is_claude_model("databricks-meta-llama-3-3-70b"));
    }

    #[test]
    fn test_apply_cache_control_for_claude_system_message() -> anyhow::Result<()> {
        let mut payload = json!({
            "model": "databricks-claude-sonnet-4",
            "messages": [
                {
                    "role": "system",
                    "content": "You are a helpful assistant."
                },
                {
                    "role": "user",
                    "content": "Hello"
                }
            ]
        });

        apply_cache_control_for_claude(&mut payload);

        let messages = payload["messages"].as_array().unwrap();
        let system_msg = &messages[0];

        // System message content should be converted to array with cache_control
        assert!(system_msg["content"].is_array());
        let content = system_msg["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "You are a helpful assistant.");
        assert_eq!(content[0]["cache_control"]["type"], "ephemeral");

        Ok(())
    }

    #[test]
    fn test_apply_cache_control_for_claude_user_messages() -> anyhow::Result<()> {
        let mut payload = json!({
            "model": "databricks-claude-sonnet-4",
            "messages": [
                {
                    "role": "system",
                    "content": "You are helpful"
                },
                {
                    "role": "user",
                    "content": "First question"
                },
                {
                    "role": "assistant",
                    "content": "First answer"
                },
                {
                    "role": "user",
                    "content": "Second question"
                },
                {
                    "role": "assistant",
                    "content": "Second answer"
                },
                {
                    "role": "user",
                    "content": "Third question"
                }
            ]
        });

        apply_cache_control_for_claude(&mut payload);

        let messages = payload["messages"].as_array().unwrap();

        // First user message should NOT have cache_control (only last 2)
        let first_user = &messages[1];
        assert_eq!(first_user["content"], "First question");

        // Second-to-last user message should have cache_control
        let second_user = &messages[3];
        assert!(second_user["content"].is_array());
        assert_eq!(
            second_user["content"][0]["cache_control"]["type"],
            "ephemeral"
        );

        // Last user message should have cache_control
        let last_user = &messages[5];
        assert!(last_user["content"].is_array());
        assert_eq!(
            last_user["content"][0]["cache_control"]["type"],
            "ephemeral"
        );

        Ok(())
    }

    #[test]
    fn test_apply_cache_control_for_claude_tools() -> anyhow::Result<()> {
        let mut payload = json!({
            "model": "databricks-claude-sonnet-4",
            "messages": [
                {
                    "role": "system",
                    "content": "You are helpful"
                }
            ],
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "tool1",
                        "description": "First tool"
                    }
                },
                {
                    "type": "function",
                    "function": {
                        "name": "tool2",
                        "description": "Second tool"
                    }
                }
            ]
        });

        apply_cache_control_for_claude(&mut payload);

        let tools = payload["tools"].as_array().unwrap();

        // First tool should NOT have cache_control
        assert!(tools[0]["function"].get("cache_control").is_none());

        // Last tool should have cache_control
        assert_eq!(tools[1]["function"]["cache_control"]["type"], "ephemeral");

        Ok(())
    }

    #[test]
    fn test_format_messages_with_thought_signature_metadata() -> anyhow::Result<()> {
        let mut metadata = serde_json::Map::new();
        metadata.insert(
            "thoughtSignature".to_string(),
            json!("sig_abc123_test_signature"),
        );

        let message = Message::assistant().with_tool_request_with_metadata(
            "tool1",
            Ok(CallToolRequestParams::new("test_tool").with_arguments(object!({"param": "value"}))),
            Some(&metadata),
            None,
        );

        let spec = format_messages(&[message], &ImageFormat::OpenAi);
        let as_value = serde_json::to_value(spec)?;
        let spec_array = as_value.as_array().unwrap();

        assert_eq!(spec_array.len(), 1);
        let tool_call = &spec_array[0]["tool_calls"][0];
        assert_eq!(tool_call["id"], "tool1");
        assert_eq!(tool_call["function"]["name"], "test_tool");
        assert_eq!(tool_call["thoughtSignature"], "sig_abc123_test_signature");

        Ok(())
    }

    #[test]
    fn test_create_request_claude_has_cache_control() -> anyhow::Result<()> {
        let model_config = ModelConfig {
            model_name: "databricks-claude-sonnet-4".to_string(),
            context_limit: Some(200000),
            temperature: None,
            max_tokens: Some(8192),
            toolshim: false,
            toolshim_model: None,
            fast_model_config: None,
            request_params: None,
            reasoning: None,
        };

        let messages = vec![
            Message::user().with_text("Hello"),
            Message::assistant().with_text("Hi there!"),
            Message::user().with_text("How are you?"),
        ];

        let tool = Tool::new(
            "test_tool",
            "A test tool",
            object!({
                "type": "object",
                "properties": {}
            }),
        );

        let request = create_request(
            &model_config,
            "You are helpful",
            &messages,
            &[tool],
            &ImageFormat::OpenAi,
        )?;

        // Verify system message has cache_control
        let messages_arr = request["messages"].as_array().unwrap();
        let system_msg = &messages_arr[0];
        assert!(system_msg["content"].is_array());
        assert_eq!(
            system_msg["content"][0]["cache_control"]["type"],
            "ephemeral"
        );

        // Verify last tool has cache_control
        let tools = request["tools"].as_array().unwrap();
        assert_eq!(tools[0]["function"]["cache_control"]["type"], "ephemeral");

        Ok(())
    }

    #[test]
    fn test_create_request_non_claude_no_cache_control() -> anyhow::Result<()> {
        let model_config = ModelConfig {
            model_name: "gpt-4o".to_string(),
            context_limit: Some(128000),
            temperature: None,
            max_tokens: Some(4096),
            toolshim: false,
            toolshim_model: None,
            fast_model_config: None,
            request_params: None,
            reasoning: None,
        };

        let messages = vec![Message::user().with_text("Hello")];

        let tool = Tool::new(
            "test_tool",
            "A test tool",
            object!({
                "type": "object",
                "properties": {}
            }),
        );

        let request = create_request(
            &model_config,
            "You are helpful",
            &messages,
            &[tool],
            &ImageFormat::OpenAi,
        )?;

        // Verify system message does NOT have cache_control (it's a plain string)
        let messages_arr = request["messages"].as_array().unwrap();
        let system_msg = &messages_arr[0];
        assert!(system_msg["content"].is_string());

        // Verify tool does NOT have cache_control
        let tools = request["tools"].as_array().unwrap();
        assert!(tools[0]["function"].get("cache_control").is_none());

        Ok(())
    }

    #[test]
    fn test_format_messages_with_multiple_metadata_fields() -> anyhow::Result<()> {
        let mut metadata = serde_json::Map::new();
        metadata.insert("thoughtSignature".to_string(), json!("sig_top_level"));
        metadata.insert(
            "extra_content".to_string(),
            json!({
                "google": {
                    "thought_signature": "sig_nested"
                }
            }),
        );
        metadata.insert("custom_field".to_string(), json!("custom_value"));

        let message = Message::assistant().with_tool_request_with_metadata(
            "tool1",
            Ok(CallToolRequestParams::new("test_tool")),
            Some(&metadata),
            None,
        );

        let spec = format_messages(&[message], &ImageFormat::OpenAi);
        let as_value = serde_json::to_value(spec)?;
        let spec_array = as_value.as_array().unwrap();

        let tool_call = &spec_array[0]["tool_calls"][0];
        assert_eq!(tool_call["thoughtSignature"], "sig_top_level");
        assert_eq!(
            tool_call["extra_content"]["google"]["thought_signature"],
            "sig_nested"
        );
        assert_eq!(tool_call["custom_field"], "custom_value");

        Ok(())
    }
}
