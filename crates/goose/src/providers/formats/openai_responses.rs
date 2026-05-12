use crate::conversation::message::{Message, MessageContent};
use crate::mcp_utils::extract_text_from_resource;
use crate::model::ModelConfig;
use crate::providers::base::{ProviderUsage, Usage};
use crate::providers::utils::{extract_reasoning_effort, is_openai_responses_model};
use anyhow::{anyhow, Error};
use async_stream::try_stream;
use chrono;
use futures::Stream;
use rmcp::model::{object, CallToolRequestParams, RawContent, Role, Tool};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::ops::Deref;

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponsesApiResponse {
    pub id: String,
    pub object: String,
    pub created_at: i64,
    pub status: String,
    pub model: String,
    pub output: Vec<ResponseOutputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ResponseReasoningInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ResponseUsage>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub struct SummaryText {
    pub text: String,
}

fn reasoning_from_summary(summary: &[SummaryText]) -> Option<MessageContent> {
    let text: String = summary
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() {
        None
    } else {
        Some(MessageContent::thinking(text, ""))
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ResponseOutputItem {
    Reasoning {
        id: String,
        #[serde(default)]
        summary: Vec<SummaryText>,
    },
    Message {
        id: String,
        status: String,
        role: String,
        content: Vec<ResponseContentBlock>,
    },
    FunctionCall {
        id: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
        name: String,
        arguments: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ResponseContentBlock {
    OutputText {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Vec<Value>>,
    },
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseReasoningInfo {
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseUsage {
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub total_tokens: i32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ResponsesStreamEvent {
    #[serde(rename = "response.created")]
    ResponseCreated {
        sequence_number: i32,
        response: ResponseMetadata,
    },
    #[serde(rename = "response.in_progress")]
    ResponseInProgress {
        sequence_number: i32,
        response: ResponseMetadata,
    },
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded {
        sequence_number: i32,
        output_index: i32,
        item: ResponseOutputItemInfo,
    },
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded {
        sequence_number: i32,
        item_id: String,
        output_index: i32,
        content_index: i32,
        part: ContentPart,
    },
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        sequence_number: i32,
        item_id: String,
        output_index: i32,
        content_index: i32,
        delta: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<Value>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.output_item.done")]
    OutputItemDone {
        sequence_number: i32,
        output_index: i32,
        item: ResponseOutputItemInfo,
    },
    #[serde(rename = "response.content_part.done")]
    ContentPartDone {
        sequence_number: i32,
        item_id: String,
        output_index: i32,
        content_index: i32,
        part: ContentPart,
    },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {
        sequence_number: i32,
        item_id: String,
        output_index: i32,
        content_index: i32,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<Value>>,
    },
    #[serde(rename = "response.completed")]
    ResponseCompleted {
        sequence_number: i32,
        response: ResponseMetadata,
    },
    #[serde(rename = "response.failed")]
    ResponseFailed { sequence_number: i32, error: Value },
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta {
        sequence_number: i32,
        item_id: String,
        output_index: i32,
        delta: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone {
        sequence_number: i32,
        item_id: String,
        output_index: i32,
        arguments: String,
    },
    #[serde(rename = "error")]
    Error { error: Value },
    #[serde(rename = "keepalive")]
    Keepalive {
        #[serde(default)]
        sequence_number: Option<i32>,
    },
}

fn is_known_responses_stream_event_type(event_type: &str) -> bool {
    matches!(
        event_type,
        "response.created"
            | "response.in_progress"
            | "response.output_item.added"
            | "response.content_part.added"
            | "response.output_text.delta"
            | "response.output_item.done"
            | "response.content_part.done"
            | "response.output_text.done"
            | "response.completed"
            | "response.failed"
            | "response.function_call_arguments.delta"
            | "response.function_call_arguments.done"
            | "error"
            | "keepalive"
    )
}

fn parse_responses_stream_event(data_line: &str) -> anyhow::Result<Option<ResponsesStreamEvent>> {
    let raw_event: Value = serde_json::from_str(data_line).map_err(|e| {
        anyhow!(
            "Failed to parse Responses stream event: {}: {:?}",
            e,
            data_line
        )
    })?;

    let Some(event_type) = raw_event.get("type").and_then(Value::as_str) else {
        return Ok(None);
    };

    if !is_known_responses_stream_event_type(event_type) {
        return Ok(None);
    }

    let event = serde_json::from_value(raw_event).map_err(|e| {
        anyhow!(
            "Failed to parse Responses stream event: {}: {:?}",
            e,
            data_line
        )
    })?;
    Ok(Some(event))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseMetadata {
    pub id: String,
    pub object: String,
    pub created_at: i64,
    pub status: String,
    pub model: String,
    pub output: Vec<ResponseOutputItemInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ResponseUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ResponseReasoningInfo>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ResponseOutputItemInfo {
    Reasoning {
        id: String,
        #[serde(default)]
        summary: Vec<SummaryText>,
    },
    Message {
        id: String,
        status: String,
        role: String,
        content: Vec<ContentPart>,
    },
    FunctionCall {
        id: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        call_id: Option<String>,
        name: String,
        arguments: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ContentPart {
    OutputText {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        annotations: Option<Vec<Value>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<Value>>,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
}

fn add_message_items(input_items: &mut Vec<Value>, messages: &[Message]) {
    for message in messages.iter().filter(|m| m.is_agent_visible()) {
        let role = match message.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };

        let mut text_items = Vec::new();

        for content in &message.content {
            match content {
                MessageContent::Text(text) if !text.text.is_empty() => {
                    let content_type = if message.role == Role::Assistant {
                        "output_text"
                    } else {
                        "input_text"
                    };
                    text_items.push(json!({
                        "type": content_type,
                        "text": text.text
                    }));
                }
                MessageContent::ToolRequest(request) if message.role == Role::Assistant => {
                    if !text_items.is_empty() {
                        input_items.push(json!({
                            "role": role,
                            "content": text_items
                        }));
                        text_items = Vec::new();
                    }

                    if let Ok(tool_call) = &request.tool_call {
                        let arguments_str = tool_call
                            .arguments
                            .as_ref()
                            .map(|args| {
                                serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string())
                            })
                            .unwrap_or_else(|| "{}".to_string());

                        tracing::debug!(
                            "Replaying function_call with call_id: {}, name: {}",
                            request.id,
                            tool_call.name
                        );
                        input_items.push(json!({
                            "type": "function_call",
                            "call_id": request.id,
                            "name": tool_call.name,
                            "arguments": arguments_str
                        }));
                    }
                }
                MessageContent::ToolResponse(response) => {
                    if !text_items.is_empty() {
                        input_items.push(json!({
                            "role": role,
                            "content": text_items
                        }));
                        text_items = Vec::new();
                    }

                    match &response.tool_result {
                        Ok(contents) => {
                            let has_images = contents
                                .content
                                .iter()
                                .any(|c| matches!(c.deref(), RawContent::Image(_)));

                            let output = if has_images {
                                json!(contents
                                    .content
                                    .iter()
                                    .map(|c| match c.deref() {
                                        RawContent::Text(t) => json!({
                                            "type": "input_text", "text": t.text
                                        }),
                                        RawContent::Resource(r) => json!({
                                            "type": "input_text",
                                            "text": extract_text_from_resource(&r.resource)
                                        }),
                                        RawContent::Image(image) => json!({
                                            "type": "input_image",
                                            "image_url": format!(
                                                "data:{};base64,{}",
                                                image.mime_type, image.data
                                            )
                                        }),
                                        RawContent::Audio(_) => json!({
                                            "type": "input_text", "text": "[Audio content]"
                                        }),
                                        RawContent::ResourceLink(_) => json!({
                                            "type": "input_text", "text": "[Resource link]"
                                        }),
                                    })
                                    .collect::<Vec<Value>>())
                            } else {
                                json!(contents
                                    .content
                                    .iter()
                                    .filter_map(|c| match c.deref() {
                                        RawContent::Text(t) => Some(t.text.clone()),
                                        RawContent::Resource(r) => {
                                            Some(extract_text_from_resource(&r.resource))
                                        }
                                        RawContent::Audio(_) => Some("[Audio content]".into()),
                                        RawContent::ResourceLink(_) => {
                                            Some("[Resource link]".into())
                                        }
                                        RawContent::Image(_) => None,
                                    })
                                    .collect::<Vec<String>>()
                                    .join("\n"))
                            };

                            input_items.push(json!({
                                "type": "function_call_output",
                                "call_id": response.id,
                                "output": output
                            }));
                        }
                        Err(error_data) => {
                            tracing::debug!(
                                "Sending function_call_output error with call_id: {}",
                                response.id
                            );
                            input_items.push(json!({
                                "type": "function_call_output",
                                "call_id": response.id,
                                "output": format!("Error: {}", error_data.message)
                            }));
                        }
                    }
                }
                _ => {}
            }
        }

        if !text_items.is_empty() {
            input_items.push(json!({
                "role": role,
                "content": text_items
            }));
        }
    }
}

pub fn create_responses_request(
    model_config: &ModelConfig,
    system: &str,
    messages: &[Message],
    tools: &[Tool],
) -> anyhow::Result<Value, Error> {
    let mut input_items = Vec::new();

    if !system.is_empty() {
        input_items.push(json!({
            "role": "system",
            "content": [{
                "type": "input_text",
                "text": system
            }]
        }));
    }

    add_message_items(&mut input_items, messages);

    let (model_name, reasoning_effort) = extract_reasoning_effort(&model_config.model_name);
    // All models routed here are responses-capable; temperature is rejected
    // by the API for reasoning models regardless of whether an explicit
    // effort suffix was provided.
    let is_reasoning_model = is_openai_responses_model(&model_name);

    let mut payload = json!({
        "model": model_name,
        "input": input_items,
        "store": false,
    });

    if let Some(effort) = reasoning_effort {
        payload.as_object_mut().unwrap().insert(
            "reasoning".to_string(),
            json!({
                "effort": effort,
                "summary": "auto",
            }),
        );
    }

    if !tools.is_empty() {
        let tools_spec: Vec<Value> = tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                    "strict": false,
                })
            })
            .collect();

        payload
            .as_object_mut()
            .unwrap()
            .insert("tools".to_string(), json!(tools_spec));
    }

    if !is_reasoning_model {
        if let Some(temp) = model_config.temperature {
            payload
                .as_object_mut()
                .unwrap()
                .insert("temperature".to_string(), json!(temp));
        }
    }

    payload.as_object_mut().unwrap().insert(
        "max_output_tokens".to_string(),
        json!(model_config.max_output_tokens()),
    );

    Ok(payload)
}

pub fn responses_api_to_message(response: &ResponsesApiResponse) -> anyhow::Result<Message> {
    let mut content = Vec::new();

    for item in &response.output {
        match item {
            ResponseOutputItem::Reasoning { summary, .. } => {
                content.extend(reasoning_from_summary(summary));
            }
            ResponseOutputItem::Message {
                content: msg_content,
                ..
            } => {
                for block in msg_content {
                    match block {
                        ResponseContentBlock::OutputText { text, .. } => {
                            if !text.is_empty() {
                                content.push(MessageContent::text(text));
                            }
                        }
                        ResponseContentBlock::ToolCall { id, name, input } => {
                            content.push(MessageContent::tool_request(
                                id.clone(),
                                Ok(CallToolRequestParams::new(name.clone())
                                    .with_arguments(object(input.clone()))),
                            ));
                        }
                    }
                }
            }
            ResponseOutputItem::FunctionCall {
                id,
                call_id,
                name,
                arguments,
                ..
            } => {
                let request_id = call_id.as_ref().unwrap_or(id).clone();
                let parsed_args = if arguments.is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(arguments).unwrap_or_else(|_| json!({}))
                };

                content.push(MessageContent::tool_request(
                    request_id,
                    Ok(CallToolRequestParams::new(name.clone())
                        .with_arguments(object(parsed_args))),
                ));
            }
        }
    }

    let mut message = Message::new(Role::Assistant, chrono::Utc::now().timestamp(), content);

    message = message.with_id(response.id.clone());

    Ok(message)
}

pub fn get_responses_usage(response: &ResponsesApiResponse) -> Usage {
    response.usage.as_ref().map_or_else(Usage::default, |u| {
        Usage::new(
            Some(u.input_tokens),
            Some(u.output_tokens),
            Some(u.total_tokens),
        )
    })
}

fn process_streaming_output_items(
    output_items: Vec<ResponseOutputItemInfo>,
    is_text_response: bool,
) -> Vec<MessageContent> {
    let mut content = Vec::new();

    for item in output_items {
        match item {
            ResponseOutputItemInfo::Reasoning { summary, .. } => {
                content.extend(reasoning_from_summary(&summary));
            }
            ResponseOutputItemInfo::Message { content: parts, .. } => {
                for part in parts {
                    match part {
                        ContentPart::OutputText { text, .. } => {
                            if !text.is_empty() && !is_text_response {
                                content.push(MessageContent::text(&text));
                            }
                        }
                        ContentPart::ToolCall {
                            id,
                            name,
                            arguments,
                        } => {
                            let parsed_args = if arguments.is_empty() {
                                json!({})
                            } else {
                                serde_json::from_str(&arguments).unwrap_or_else(|_| json!({}))
                            };

                            content.push(MessageContent::tool_request(
                                id,
                                Ok(CallToolRequestParams::new(name)
                                    .with_arguments(object(parsed_args))),
                            ));
                        }
                    }
                }
            }
            ResponseOutputItemInfo::FunctionCall {
                id,
                call_id,
                name,
                arguments,
                ..
            } => {
                let request_id = call_id.unwrap_or(id);
                let parsed_args = if arguments.is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(&arguments).unwrap_or_else(|_| json!({}))
                };

                content.push(MessageContent::tool_request(
                    request_id,
                    Ok(CallToolRequestParams::new(name).with_arguments(object(parsed_args))),
                ));
            }
        }
    }

    content
}

pub fn responses_api_to_streaming_message<S>(
    mut stream: S,
) -> impl Stream<Item = anyhow::Result<(Option<Message>, Option<ProviderUsage>)>> + 'static
where
    S: Stream<Item = anyhow::Result<String>> + Unpin + Send + 'static,
{
    try_stream! {
        use futures::StreamExt;

        let mut accumulated_text = String::new();
        let mut response_id: Option<String> = None;
        let mut model_name: Option<String> = None;
        let mut final_usage: Option<ProviderUsage> = None;
        let mut output_items: Vec<ResponseOutputItemInfo> = Vec::new();
        let mut is_text_response = false;

        'outer: while let Some(response) = stream.next().await {
            let response_str = response?;

            // Skip empty lines
            if response_str.trim().is_empty() {
                continue;
            }
            if response_str.starts_with(':') {
                continue;
            }

            // Parse SSE format: "event: <type>\ndata: <json>"
            // For now, we only care about the data line
            // SSE spec allows both "data: value" and "data:value" (space after colon is optional)
            let data_line = if response_str.starts_with("data: ") {
                response_str.strip_prefix("data: ").unwrap()
            } else if response_str.starts_with("data:") {
                response_str.strip_prefix("data:").unwrap()
            } else if response_str.starts_with("event: ") || response_str.starts_with("event:") {
                // Skip event type lines
                continue;
            } else {
                // Try to parse as-is when there's no prefix
                &response_str
            };

            if data_line == "[DONE]" {
                break 'outer;
            }

            let Some(event) = parse_responses_stream_event(data_line)? else {
                continue;
            };

            match event {
                ResponsesStreamEvent::ResponseCreated { response, .. } |
                ResponsesStreamEvent::ResponseInProgress { response, .. } => {
                    response_id = Some(response.id);
                    model_name = Some(response.model);
                }

                ResponsesStreamEvent::OutputTextDelta { delta, .. } => {
                    is_text_response = true;
                    if !delta.is_empty() {
                        accumulated_text.push_str(&delta);

                        // Yield incremental text updates for true streaming
                        let mut msg = Message::new(
                            Role::Assistant,
                            chrono::Utc::now().timestamp(),
                            vec![MessageContent::text(&delta)],
                        );

                        // Add ID so desktop client knows these deltas are part of the same message
                        if let Some(id) = &response_id {
                            msg = msg.with_id(id.clone());
                        }

                        yield (Some(msg), None);
                    }
                }

                ResponsesStreamEvent::OutputItemDone { item, .. } => {
                    output_items.push(item);
                }

                ResponsesStreamEvent::OutputTextDone { .. } => {
                    // Text is already complete from deltas, this is just a summary event
                }

                ResponsesStreamEvent::ResponseCompleted { response, .. } => {
                    let model = model_name.as_ref().unwrap_or(&response.model);
                    let usage = response.usage.as_ref().map_or_else(
                        Usage::default,
                        |u| Usage::new(
                            Some(u.input_tokens),
                            Some(u.output_tokens),
                            Some(u.total_tokens),
                        ),
                    );
                    final_usage = Some(ProviderUsage {
                        usage,
                        model: model.clone(),
                    });

                    // For complete output, use the response output items
                    if !response.output.is_empty() {
                        output_items = response.output;
                    }

                    break 'outer;
                }

                ResponsesStreamEvent::FunctionCallArgumentsDelta { .. } => {
                    // Function call arguments are being streamed, but we'll get the complete
                    // arguments in the OutputItemDone event, so we can ignore deltas for now
                }

                ResponsesStreamEvent::FunctionCallArgumentsDone { .. } => {
                    // Arguments are complete, will be in the OutputItemDone event
                }

                ResponsesStreamEvent::ResponseFailed { error, .. } => {
                    Err(anyhow!("Responses API failed: {:?}", error))?;
                }

                ResponsesStreamEvent::Error { error } => {
                    Err(anyhow!("Responses API error: {:?}", error))?;
                }

                _ => {
                    // Ignore other event types (OutputItemAdded, ContentPartAdded, ContentPartDone)
                }
            }
        }

        // Process final output items and yield usage data
        let content = process_streaming_output_items(output_items, is_text_response);

        if !content.is_empty() {
            let mut message = Message::new(Role::Assistant, chrono::Utc::now().timestamp(), content);
            if let Some(id) = response_id {
                message = message.with_id(id);
            }
            yield (Some(message), final_usage);
        } else if let Some(usage) = final_usage {
            yield (None, Some(usage));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::MessageContent;
    use crate::model::ModelConfig;
    use futures::StreamExt;
    use rmcp::model::CallToolRequestParams;
    use rmcp::object;

    #[tokio::test]
    async fn test_responses_stream_ignores_keepalive_event() -> anyhow::Result<()> {
        let lines = vec![
            r#"data: {"type":"response.created","sequence_number":1,"response":{"id":"resp_1","object":"response","created_at":1737368310,"status":"in_progress","model":"gpt-5.2-pro","output":[]}}"#.to_string(),
            r#"data: {"type":"keepalive"}"#.to_string(),
            r#"data: {"type":"response.output_text.delta","sequence_number":2,"item_id":"msg_1","output_index":0,"content_index":0,"delta":"Hello"}"#.to_string(),
            r#"data: {"type":"response.output_text.delta","sequence_number":3,"item_id":"msg_1","output_index":0,"content_index":0,"delta":" world"}"#.to_string(),
            r#"data: {"type":"response.completed","sequence_number":4,"response":{"id":"resp_1","object":"response","created_at":1737368310,"status":"completed","model":"gpt-5.2-pro","output":[],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14}}}"#.to_string(),
            "data: [DONE]".to_string(),
        ];

        let response_stream = tokio_stream::iter(lines.into_iter().map(Ok));
        let messages = responses_api_to_streaming_message(response_stream);
        futures::pin_mut!(messages);

        let mut text_parts = Vec::new();
        let mut usage: Option<ProviderUsage> = None;

        while let Some(item) = messages.next().await {
            let (message, maybe_usage) = item?;
            if let Some(msg) = message {
                for content in msg.content {
                    if let MessageContent::Text(text) = content {
                        text_parts.push(text.text.clone());
                    }
                }
            }
            if let Some(final_usage) = maybe_usage {
                usage = Some(final_usage);
            }
        }

        assert_eq!(text_parts.concat(), "Hello world");
        let usage = usage.expect("usage should be present at completion");
        assert_eq!(usage.model, "gpt-5.2-pro");
        assert_eq!(usage.usage.input_tokens, Some(10));
        assert_eq!(usage.usage.output_tokens, Some(4));
        assert_eq!(usage.usage.total_tokens, Some(14));

        Ok(())
    }

    #[test]
    fn test_responses_api_to_message_captures_reasoning_summary() -> anyhow::Result<()> {
        let response: ResponsesApiResponse = serde_json::from_value(serde_json::json!({
            "id": "resp_1",
            "object": "response",
            "created_at": 1737368310,
            "status": "completed",
            "model": "gpt-5",
            "output": [
                {
                    "type": "reasoning",
                    "id": "rs_1",
                    "summary": [
                        { "type": "summary_text", "text": "Thinking about the question..." },
                        { "type": "summary_text", "text": "The answer is straightforward." }
                    ]
                },
                {
                    "type": "message",
                    "id": "msg_1",
                    "status": "completed",
                    "role": "assistant",
                    "content": [
                        { "type": "output_text", "text": "The capital of France is Paris." }
                    ]
                }
            ]
        }))?;

        let message = responses_api_to_message(&response)?;

        let thinking = message.content.iter().find_map(|c| c.as_thinking());
        assert!(thinking.is_some(), "should contain thinking content");
        assert_eq!(
            thinking.unwrap().thinking,
            "Thinking about the question...\nThe answer is straightforward."
        );

        let text = message.content.iter().find_map(|c| c.as_text());
        assert_eq!(text, Some("The capital of France is Paris."));

        Ok(())
    }

    #[tokio::test]
    async fn test_responses_stream_captures_reasoning_summary() -> anyhow::Result<()> {
        let reasoning_item = serde_json::json!({
            "type": "reasoning",
            "id": "rs_1",
            "summary": [
                { "type": "summary_text", "text": "Let me think step by step." }
            ]
        });
        let message_item = serde_json::json!({
            "type": "message",
            "id": "msg_1",
            "status": "completed",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": "Paris." }]
        });

        let lines = vec![
            format!(
                r#"data: {{"type":"response.created","sequence_number":1,"response":{{"id":"resp_1","object":"response","created_at":1737368310,"status":"in_progress","model":"gpt-5","output":[]}}}}"#
            ),
            format!(
                r#"data: {{"type":"response.output_text.delta","sequence_number":2,"item_id":"msg_1","output_index":1,"content_index":0,"delta":"Paris."}}"#
            ),
            format!(
                r#"data: {{"type":"response.output_item.done","sequence_number":3,"output_index":0,"item":{}}}"#,
                serde_json::to_string(&reasoning_item)?
            ),
            format!(
                r#"data: {{"type":"response.output_item.done","sequence_number":4,"output_index":1,"item":{}}}"#,
                serde_json::to_string(&message_item)?
            ),
            format!(
                r#"data: {{"type":"response.completed","sequence_number":5,"response":{{"id":"resp_1","object":"response","created_at":1737368310,"status":"completed","model":"gpt-5","output":[{},{}],"usage":{{"input_tokens":10,"output_tokens":5,"total_tokens":15}}}}}}"#,
                serde_json::to_string(&reasoning_item)?,
                serde_json::to_string(&message_item)?
            ),
            "data: [DONE]".to_string(),
        ];

        let response_stream = tokio_stream::iter(lines.into_iter().map(Ok));
        let messages = responses_api_to_streaming_message(response_stream);
        futures::pin_mut!(messages);

        let mut thinking_parts = Vec::new();
        let mut text_parts = Vec::new();

        while let Some(item) = messages.next().await {
            let (message, _) = item?;
            if let Some(msg) = message {
                for content in msg.content {
                    match &content {
                        MessageContent::Thinking(t) => thinking_parts.push(t.thinking.clone()),
                        MessageContent::Text(t) => text_parts.push(t.text.clone()),
                        _ => {}
                    }
                }
            }
        }

        assert!(
            !thinking_parts.is_empty(),
            "should capture thinking from stream"
        );
        assert_eq!(thinking_parts.join(""), "Let me think step by step.");
        assert!(text_parts.concat().contains("Paris."));

        Ok(())
    }

    #[tokio::test]
    async fn test_responses_stream_error_event_still_returns_error() -> anyhow::Result<()> {
        let lines = vec![
            r#"data: {"type":"error","error":{"message":"boom"}}"#.to_string(),
            "data: [DONE]".to_string(),
        ];

        let response_stream = tokio_stream::iter(lines.into_iter().map(Ok));
        let messages = responses_api_to_streaming_message(response_stream);
        futures::pin_mut!(messages);

        let first = messages
            .next()
            .await
            .expect("stream should emit an error item");

        assert!(first.is_err());
        assert!(first
            .expect_err("expected error")
            .to_string()
            .contains("Responses API error"));

        Ok(())
    }

    #[test]
    fn test_history_preserves_chronological_order() {
        let model_config = ModelConfig {
            model_name: "gpt-5.2-codex".to_string(),
            context_limit: None,
            temperature: None,
            max_tokens: None,
            toolshim: false,
            toolshim_model: None,
            fast_model_config: None,
            request_params: None,
            reasoning: None,
        };

        let messages = vec![
            Message::assistant()
                .with_text("I'll create that file.")
                .with_tool_request(
                    "call_1",
                    Ok(CallToolRequestParams::new("shell")
                        .with_arguments(object!({"command": "echo hello"}))),
                ),
            Message::assistant()
                .with_text("Now let me verify.")
                .with_tool_request(
                    "call_2",
                    Ok(CallToolRequestParams::new("shell")
                        .with_arguments(object!({"command": "cat file.txt"}))),
                ),
        ];

        let result = create_responses_request(&model_config, "", &messages, &[]).unwrap();
        let input = result["input"].as_array().unwrap();

        let types: Vec<&str> = input
            .iter()
            .map(|item| {
                item.get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| item["role"].as_str().unwrap())
            })
            .collect();

        assert_eq!(
            types,
            vec!["assistant", "function_call", "assistant", "function_call"]
        );
    }

    #[test]
    fn test_responses_api_to_message_uses_call_id_for_tool_request_id() {
        let response = ResponsesApiResponse {
            id: "resp_1".to_string(),
            object: "response".to_string(),
            created_at: 0,
            status: "completed".to_string(),
            model: "gpt-5.3-codex".to_string(),
            output: vec![ResponseOutputItem::FunctionCall {
                id: "fc_123".to_string(),
                status: "completed".to_string(),
                call_id: Some("call_abc".to_string()),
                name: "test__get_person_zip_code".to_string(),
                arguments: r#"{"name":"Alice Burns"}"#.to_string(),
            }],
            reasoning: None,
            usage: None,
        };

        let message = responses_api_to_message(&response).unwrap();
        assert_eq!(message.content.len(), 1);
        let MessageContent::ToolRequest(tool_request) = &message.content[0] else {
            panic!("expected tool request content");
        };
        assert_eq!(tool_request.id, "call_abc");
    }

    #[test]
    fn test_deserialize_reasoning_info_with_null_effort() {
        let json = r#"{"effort": null}"#;
        let info: ResponseReasoningInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.effort, None);
        assert_eq!(info.summary, None);
    }

    #[test]
    fn test_deserialize_reasoning_info_with_effort() {
        let json = r#"{"effort": "high", "summary": "Thought deeply"}"#;
        let info: ResponseReasoningInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.effort.as_deref(), Some("high"));
        assert_eq!(info.summary.as_deref(), Some("Thought deeply"));
    }

    #[test]
    fn test_responses_tools_include_strict_false() {
        let model_config = ModelConfig {
            model_name: "gpt-5.4".to_string(),
            context_limit: None,
            temperature: None,
            max_tokens: None,
            toolshim: false,
            toolshim_model: None,
            fast_model_config: None,
            request_params: None,
            reasoning: None,
        };

        let tool = Tool::new(
            "shell",
            "Execute a shell command",
            object!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command to run"
                    }
                },
                "required": ["command"]
            }),
        );

        let result =
            create_responses_request(&model_config, "You are helpful.", &[], &[tool]).unwrap();
        let tools = result["tools"]
            .as_array()
            .expect("tools should be an array");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["strict"], json!(false),
            "Responses API defaults strict to true, but MCP tool schemas are not strict-compatible; must explicitly set strict: false");
    }

    #[test]
    fn test_responses_request_with_explicit_effort_suffix() {
        for (model_name, expected_model, expected_effort) in [
            ("gpt-5.4-xhigh", "gpt-5.4", "xhigh"),
            ("databricks-gpt-5.4-high", "databricks-gpt-5.4", "high"),
            ("databricks-o3-none", "databricks-o3", "none"),
        ] {
            let model_config = ModelConfig {
                model_name: model_name.to_string(),
                context_limit: None,
                temperature: None,
                max_tokens: None,
                toolshim: false,
                toolshim_model: None,
                fast_model_config: None,
                request_params: None,
                reasoning: None,
            };

            let result =
                create_responses_request(&model_config, "You are helpful.", &[], &[]).unwrap();

            assert_eq!(
                result["model"], expected_model,
                "unexpected model for {model_name}"
            );
            assert_eq!(
                result["reasoning"]["effort"], expected_effort,
                "unexpected effort for {model_name}"
            );
            assert_eq!(result["reasoning"]["summary"], "auto");
        }
    }

    #[test]
    fn test_responses_request_without_effort_suffix_omits_reasoning() {
        for model_name in ["gpt-5.4", "o3", "gpt-5-nano"] {
            let model_config = ModelConfig {
                model_name: model_name.to_string(),
                context_limit: None,
                temperature: None,
                max_tokens: None,
                toolshim: false,
                toolshim_model: None,
                fast_model_config: None,
                request_params: None,
                reasoning: None,
            };

            let result =
                create_responses_request(&model_config, "You are helpful.", &[], &[]).unwrap();

            assert_eq!(result["model"], model_name, "model should be unchanged");
            assert!(
                result.get("reasoning").is_none(),
                "reasoning should be omitted for {model_name} without explicit effort suffix"
            );
        }
    }
}
