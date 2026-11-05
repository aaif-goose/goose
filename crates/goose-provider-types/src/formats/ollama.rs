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
use crate::maybe_send::MaybeSend;
#[cfg(not(feature = "std"))]
use crate::prelude::*;
use crate::{
    conversation::token_usage::ProviderUsage,
    formats::openai::{self, is_valid_function_name},
};

use futures::Stream;

use crate::data::{object, CallToolRequestParams, ErrorCode, ErrorData, Role};
use alloc::borrow::Cow;
use serde_json::Value;

pub use crate::formats::openai::{
    create_request, format_messages, format_tools, get_usage, validate_tool_schemas,
};

/// Parse XML-style tool calls from content (Ollama/Qwen3-coder fallback format).
///
/// Format: `<function=name><parameter=key>value</parameter>...</function>`
///
/// Returns a tuple of (prefix_text, tool_calls) where prefix_text is any text before the first function tag.
#[cfg(feature = "std")]
pub fn parse_xml_tool_calls(content: &str) -> (Option<String>, Vec<MessageContentBlock>) {
    parse_xml_tool_calls_with_host(&crate::runtime::SystemHost, content)
}

pub fn parse_xml_tool_calls_with_host(
    host: &dyn crate::runtime::HostServices,
    content: &str,
) -> (Option<String>, Vec<MessageContentBlock>) {
    let mut tool_calls = Vec::new();

    let prefix = content
        .find("<function=")
        .and_then(|idx| content.get(..idx))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    for (name, function_body) in xml_elements(content, "function") {
        let function_name = name.trim().to_string();

        let mut arguments = serde_json::Map::new();
        for (name, value) in xml_elements(function_body, "parameter") {
            let param_name = name.trim().to_string();
            let param_value = value.trim().to_string();
            arguments.insert(param_name, serde_json::Value::String(param_value));
        }

        let id = host.new_id();

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
#[cfg(feature = "std")]
pub fn response_to_message(response: &Value) -> anyhow::Result<Message> {
    response_to_message_with_host(&crate::runtime::SystemHost, response)
}

pub fn response_to_message_with_host(
    host: &dyn crate::runtime::HostServices,
    response: &Value,
) -> anyhow::Result<Message> {
    let message = openai::response_to_message_with_host(host, response)?;

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
                let (prefix, xml_tool_calls) = parse_xml_tool_calls_with_host(host, text);
                if !xml_tool_calls.is_empty() {
                    let mut content = Vec::new();
                    if let Some(prefix_text) = prefix {
                        content.push(MessageContentBlock::text(prefix_text));
                    }
                    content.extend(xml_tool_calls);

                    return Ok(Message::new(
                        Role::Assistant,
                        host.unix_timestamp(),
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
#[cfg(feature = "std")]
pub fn response_to_streaming_message_ollama<S>(
    stream: S,
) -> impl Stream<Item = anyhow::Result<(Option<Message>, Option<ProviderUsage>)>> + 'static
where
    S: Stream<Item = anyhow::Result<String>> + Unpin + MaybeSend + 'static,
{
    response_to_streaming_message_ollama_with_host(&crate::runtime::SystemHost, stream)
}

pub fn response_to_streaming_message_ollama_with_host<'a, S>(
    host: &'a dyn crate::runtime::HostServices,
    stream: S,
) -> impl Stream<Item = anyhow::Result<(Option<Message>, Option<ProviderUsage>)>> + 'a
where
    S: Stream<Item = anyhow::Result<String>> + Unpin + MaybeSend + 'a,
{
    crate::stream::try_stream(|mut emit| async move {
        use futures::StreamExt;

        let base_stream = openai::response_to_streaming_message_with_host(host, stream);
        let mut base_stream = core::pin::pin!(base_stream);

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

                emit.send((Some(message), usage)).await;
            } else if usage.is_some() && !xml_detected {
                emit.send((None, usage)).await;
            }
        }

        if xml_detected && !accumulated_text.is_empty() {
            let (prefix, xml_tool_calls) = parse_xml_tool_calls_with_host(host, &accumulated_text);

            if !xml_tool_calls.is_empty() {
                let mut contents = Vec::new();
                if let Some(prefix_text) = prefix {
                    contents.push(MessageContentBlock::text(prefix_text));
                }
                contents.extend(xml_tool_calls);

                let msg = Message::new(Role::Assistant, host.unix_timestamp(), contents);

                emit.send((Some(msg), buffered_usage)).await;
            } else {
                let msg = Message::new(
                    Role::Assistant,
                    host.unix_timestamp(),
                    vec![MessageContentBlock::text(&accumulated_text)],
                )
                .with_generated_id_with_host(host);

                emit.send((Some(msg), buffered_usage)).await;
            }
        }

        Ok::<(), anyhow::Error>(())
    })
}

fn xml_elements<'a>(mut text: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    let open = format!("<{tag}=");
    let close = format!("</{tag}>");
    let mut elements = Vec::new();
    while let Some(start) = text.find(&open) {
        text = text.get(start + open.len()..).unwrap();
        let Some(name_end) = text.find('>') else {
            break;
        };
        let name = text.get(..name_end).unwrap();
        text = text.get(name_end + 1..).unwrap();
        let Some(end) = text.find(&close) else {
            break;
        };
        if !name.is_empty() {
            elements.push((name, text.get(..end).unwrap()));
        }
        text = text.get(end + close.len()..).unwrap();
    }
    elements
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
        let mut messages = core::pin::pin!(response_to_streaming_message_ollama(response_stream));

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
