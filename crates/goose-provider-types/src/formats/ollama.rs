//! Ollama-specific response handling with XML tool call fallback.
//!
//! Some models running through Ollama (notably Qwen3-coder) output XML-style tool calls
//! when given many tools (6+), instead of using the native JSON tool_calls format.
//! This module wraps the standard OpenAI response parsing with XML fallback logic,
//! isolating this behavior to the Ollama provider only.
//!
//! Known affected models:
//! - qwen3-coder
//! - qwen3-coder-32b

use crate::conversation::message::{Message, MessageContentBlock};
use crate::{
    conversation::token_usage::ProviderUsage,
    formats::openai::{self, is_valid_function_name},
};
use async_stream::try_stream;
use chrono;
use futures::Stream;
use regex::Regex;
use rmcp::model::{object, CallToolRequestParams, ErrorCode, ErrorData, Role};
use serde_json::Value;
use std::borrow::Cow;
use uuid::Uuid;

pub use crate::formats::openai::{
    create_request, format_messages, format_tools, get_usage, validate_tool_schemas,
};

/// Parse XML-style tool calls from content (Ollama/Qwen3-coder fallback format).
///
/// Format: `<function=name><parameter=key>value</parameter>...</function>`
///
/// Returns a tuple of (prefix_text, tool_calls) where prefix_text is any text before the first function tag.
pub fn parse_xml_tool_calls(content: &str) -> (Option<String>, Vec<MessageContentBlock>) {
    let mut tool_calls = Vec::new();

    let function_re = Regex::new(r"<function=([^>]+)>([\s\S]*?)</function>").unwrap();
    let param_re = Regex::new(r"<parameter=([^>]+)>([\s\S]*?)</parameter>").unwrap();

    let prefix = content
        .find("<function=")
        .and_then(|idx| content.get(..idx))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    for func_cap in function_re.captures_iter(content) {
        let function_name = func_cap[1].trim().to_string();
        let function_body = &func_cap[2];
        let mut arguments = serde_json::Map::new();
        for param_cap in param_re.captures_iter(function_body) {
            let param_name = param_cap[1].trim().to_string();
            let param_value = param_cap[2].trim().to_string();
            arguments.insert(param_name, serde_json::Value::String(param_value));
        }

        let id = Uuid::new_v4().to_string();

        if is_valid_function_name(&function_name) {
            tool_calls.push(MessageContentBlock::tool_request(
                id,
                Ok(CallToolRequestParams::new(function_name)
                    .with_arguments(object(serde_json::Value::Object(arguments)))),
            ));
        } else {
            let error = ErrorData {
                code: ErrorCode::INVALID_REQUEST,
                message: Cow::from(format!(
                    "The provided function name '{}' had invalid characters, it must match this regex [a-zA-Z0-9_-]+",
                    function_name
                )),
                data: None,
            };
            tool_calls.push(MessageContentBlock::tool_request(id, Err(error)));
        }
    }

    (prefix, tool_calls)
}

/// Convert OpenAI's API response to internal Message format, with XML tool call fallback.
///
/// This wraps the standard OpenAI response parsing and adds XML fallback for models
/// like Qwen3-coder that output XML tool calls when given many tools.
pub fn response_to_message(response: &Value) -> anyhow::Result<Message> {
    let message = openai::response_to_message(response)?;

    let has_tool_requests = message
        .content
        .iter()
        .any(|c| matches!(c, MessageContentBlock::ToolRequest(_)));

    if has_tool_requests {
        return Ok(message);
    }

    let original = response
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|m| m.get("message"));

    if let Some(original) = original {
        if let Some(text) = original.get("content").and_then(|c| c.as_str()) {
            if text.contains("<function=") {
                let (prefix, xml_tool_calls) = parse_xml_tool_calls(text);
                if !xml_tool_calls.is_empty() {
                    let mut content = Vec::new();
                    if let Some(prefix_text) = prefix {
                        content.push(MessageContentBlock::text(prefix_text));
                    }
                    content.extend(xml_tool_calls);

                    return Ok(Message::new(
                        Role::Assistant,
                        chrono::Utc::now().timestamp(),
                        content,
                    ));
                }
            }
        }
    }

    Ok(message)
}

/// Extract text content from a message's content items.
fn extract_text_from_message(message: &Message) -> String {
    message
        .content
        .iter()
        .filter_map(|c| {
            if let MessageContentBlock::Text(text) = c {
                Some(text.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Check if a message contains only text content (no tool requests/responses).
fn is_text_only_message(message: &Message) -> bool {
    message
        .content
        .iter()
        .all(|c| matches!(c, MessageContentBlock::Text(_)))
}

/// Streaming message handler with XML tool call post-processing for Ollama.
///
/// This wraps the standard OpenAI streaming handler and post-processes messages
/// to detect and parse XML tool calls. When XML markers are detected in text
/// messages, it buffers them until the stream completes, then parses and emits
/// the tool calls.
///
/// This approach avoids exposing any internal types from openai.rs.
pub fn response_to_streaming_message_ollama<S>(
    stream: S,
) -> impl Stream<Item = anyhow::Result<(Option<Message>, Option<ProviderUsage>)>> + 'static
where
    S: Stream<Item = anyhow::Result<String>> + Unpin + Send + 'static,
{
    try_stream! {
        use futures::StreamExt;

        let base_stream = openai::response_to_streaming_message(stream);
        let mut base_stream = std::pin::pin!(base_stream);

        let mut accumulated_text = String::new();
        let mut xml_detected = false;
        let mut buffered_usage: Option<ProviderUsage> = None;

        while let Some(result) = base_stream.next().await {
            let (message_opt, usage) = result?;
            if usage.is_some() {
                buffered_usage = usage.clone();
            }

            if let Some(message) = message_opt {
                if is_text_only_message(&message) {
                    let text = extract_text_from_message(&message);
                    accumulated_text.push_str(&text);

                    if !xml_detected && accumulated_text.contains("<function=") {
                        xml_detected = true;
                    }

                    if xml_detected {
                        continue;
                    }
                }

                yield (Some(message), usage);
            } else if usage.is_some() && !xml_detected {
                yield (None, usage);
            }
        }

        if xml_detected && !accumulated_text.is_empty() {
            let (prefix, xml_tool_calls) = parse_xml_tool_calls(&accumulated_text);

            if !xml_tool_calls.is_empty() {
                let mut contents = Vec::new();
                if let Some(prefix_text) = prefix {
                    contents.push(MessageContentBlock::text(prefix_text));
                }
                contents.extend(xml_tool_calls);

                let msg = Message::new(
                    Role::Assistant,
                    chrono::Utc::now().timestamp(),
                    contents,
                );

                yield (Some(msg), buffered_usage);
            } else {
                let msg = Message::new(
                    Role::Assistant,
                    chrono::Utc::now().timestamp(),
                    vec![MessageContentBlock::text(&accumulated_text)],
                )
                .with_generated_id();

                yield (Some(msg), buffered_usage);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_xml_tool_calls_single() {
        let content = r#"<function=developer__text_editor>
<parameter=command>write</parameter>
<parameter=path>/tmp/test.txt</parameter>
<parameter=file_text>hello world</parameter>
</function>"#;

        let (prefix, tool_calls) = parse_xml_tool_calls(content);

        assert!(prefix.is_none(), "Should have no prefix");
        assert_eq!(tool_calls.len(), 1, "Should have 1 tool call");

        if let MessageContentBlock::ToolRequest(request) = &tool_calls[0] {
            let tool_call = request.tool_call.as_ref().unwrap();
            assert_eq!(tool_call.name, "developer__text_editor");
            let args = tool_call.arguments.as_ref().unwrap();
            assert_eq!(args.get("command").unwrap(), "write");
            assert_eq!(args.get("path").unwrap(), "/tmp/test.txt");
            assert_eq!(args.get("file_text").unwrap(), "hello world");
        } else {
            panic!("Expected ToolRequest content");
        }
    }

    #[test]
    fn test_parse_xml_tool_calls_with_prefix() {
        let content = r#"I'll create the file for you.

<function=developer__text_editor>
<parameter=command>write</parameter>
<parameter=path>/tmp/test.txt</parameter>
</function>"#;

        let (prefix, tool_calls) = parse_xml_tool_calls(content);

        assert_eq!(
            prefix,
            Some("I'll create the file for you.".to_string()),
            "Should have prefix text"
        );
        assert_eq!(tool_calls.len(), 1, "Should have 1 tool call");
    }

    #[test]
    fn test_parse_xml_tool_calls_multiple() {
        let content = r#"<function=developer__shell>
<parameter=command>ls -la</parameter>
</function>
<function=developer__text_editor>
<parameter=command>view</parameter>
<parameter=path>/tmp/test.txt</parameter>
</function>"#;

        let (prefix, tool_calls) = parse_xml_tool_calls(content);

        assert!(prefix.is_none());
        assert_eq!(tool_calls.len(), 2, "Should have 2 tool calls");

        if let MessageContentBlock::ToolRequest(request) = &tool_calls[0] {
            let tool_call = request.tool_call.as_ref().unwrap();
            assert_eq!(tool_call.name, "developer__shell");
        } else {
            panic!("Expected ToolRequest content");
        }

        if let MessageContentBlock::ToolRequest(request) = &tool_calls[1] {
            let tool_call = request.tool_call.as_ref().unwrap();
            assert_eq!(tool_call.name, "developer__text_editor");
        } else {
            panic!("Expected ToolRequest content");
        }
    }

    #[test]
    fn test_parse_xml_tool_calls_no_match() {
        let content = "This is just regular text without any tool calls.";

        let (prefix, tool_calls) = parse_xml_tool_calls(content);

        assert!(prefix.is_none());
        assert!(tool_calls.is_empty(), "Should have no tool calls");
    }

    #[test]
    fn test_parse_xml_tool_calls_qwen_format() {
        // Test the exact format observed from Qwen3-coder via Ollama
        let content = r#"I'll create a file at /tmp/hello.txt with the content "hello".

<function=developer__text_editor>
<parameter=command>
write
</parameter>
<parameter=path>
/tmp/hello.txt
</parameter>
<parameter=file_text>
hello
</parameter>
</function>
</tool_call>"#;

        let (prefix, tool_calls) = parse_xml_tool_calls(content);

        assert!(prefix.is_some(), "Should have prefix");
        assert_eq!(tool_calls.len(), 1, "Should have 1 tool call");

        if let MessageContentBlock::ToolRequest(request) = &tool_calls[0] {
            let tool_call = request.tool_call.as_ref().unwrap();
            assert_eq!(tool_call.name, "developer__text_editor");
            let args = tool_call.arguments.as_ref().unwrap();
            assert_eq!(args.get("command").unwrap(), "write");
            assert_eq!(args.get("path").unwrap(), "/tmp/hello.txt");
            assert_eq!(args.get("file_text").unwrap(), "hello");
        } else {
            panic!("Expected ToolRequest content");
        }
    }

    #[tokio::test]
    async fn test_response_to_message_xml_fallback() -> anyhow::Result<()> {
        use futures::StreamExt;

        // Test that response_to_message falls back to XML parsing when no JSON tool_calls
        let response = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "<function=developer__shell>\n<parameter=command>ls</parameter>\n</function>"
                }
            }]
        });

        let message = response_to_message(&response)?;

        assert_eq!(message.content.len(), 1);
        if let MessageContentBlock::ToolRequest(request) = &message.content[0] {
            let tool_call = request.tool_call.as_ref().unwrap();
            assert_eq!(tool_call.name, "developer__shell");
        } else {
            panic!("Expected ToolRequest content from XML parsing");
        }

        let response_lines = r#"data: {"id":"ollama-source-id","model":"test-model","choices":[{"delta":{"role":"assistant","content":"literal <function=not-a-tool"},"index":0,"finish_reason":null}],"object":"chat.completion.chunk","created":123}
data: {"id":"ollama-source-id","model":"test-model","choices":[{"delta":{"content":" should remain text"},"index":0,"finish_reason":"stop"}],"object":"chat.completion.chunk","created":124}
data: [DONE]"#;
        let lines = response_lines.lines().map(|s| Ok(s.to_string()));
        let response_stream = tokio_stream::iter(lines);
        let mut messages = std::pin::pin!(response_to_streaming_message_ollama(response_stream));

        let (message, usage) = messages
            .next()
            .await
            .expect("expected invalid XML fallback message")?;
        let usage = usage.expect("expected buffered response metadata");
        assert_eq!(usage.response_id.as_deref(), Some("ollama-source-id"));
        assert_eq!(usage.finish_reasons, Some(vec!["stop".to_string()]));
        let message = message.expect("expected invalid XML fallback message");
        assert_eq!(message.role, Role::Assistant);
        assert_eq!(message.content.len(), 1);
        let MessageContentBlock::Text(text) = &message.content[0] else {
            panic!("expected invalid XML fallback to remain text-only");
        };
        assert_eq!(text.text, "literal <function=not-a-tool should remain text");
        let message_id = message
            .id
            .as_deref()
            .expect("invalid XML fallback message should have an ID");
        assert!(message_id.starts_with("msg_"));
        assert!(messages.next().await.is_none());

        Ok(())
    }

    #[test]
    fn test_response_to_message_prefers_json_over_xml() -> anyhow::Result<()> {
        // Test that JSON tool_calls take precedence over XML in content
        let response = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "<function=wrong_tool>\n<parameter=x>y</parameter>\n</function>",
                    "tool_calls": [{
                        "id": "call_123",
                        "function": {
                            "name": "correct_tool",
                            "arguments": "{\"a\": \"b\"}"
                        }
                    }]
                }
            }]
        });

        let message = response_to_message(&response)?;

        // Should have both text (from content) and tool request (from tool_calls)
        // The XML in content should NOT be parsed since we have JSON tool_calls
        let tool_requests: Vec<_> = message
            .content
            .iter()
            .filter(|c| matches!(c, MessageContentBlock::ToolRequest(_)))
            .collect();

        assert_eq!(tool_requests.len(), 1);
        if let MessageContentBlock::ToolRequest(request) = tool_requests[0] {
            let tool_call = request.tool_call.as_ref().unwrap();
            assert_eq!(tool_call.name, "correct_tool");
        } else {
            panic!("Expected ToolRequest");
        }

        Ok(())
    }
}
