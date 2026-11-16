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
use crate::{
    conversation::token_usage::ProviderUsage,
    formats::openai::{self, is_valid_function_name},
    thinking::{split_think_blocks, ThinkFilter},
};
use async_stream::try_stream;
use chrono;
use futures::{Stream, TryStreamExt};
use regex::Regex;
use rmcp::model::{object, CallToolRequestParams, ErrorCode, ErrorData, Role};
use serde_json::Value;
use std::borrow::Cow;
use std::sync::{Arc, Mutex};
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

/// Check if a message contains only text content (no tool requests/responses).
fn is_text_only_message(message: &Message) -> bool {
    message
        .content
        .iter()
        .all(|c| matches!(c, MessageContentBlock::Text(_)))
}

const XML_TOOL_CALL_MARKER: &str = "<function=";

/// Raw assistant text seen on the stream, before any `<think>` filtering.
#[derive(Default)]
struct RawContent {
    text: String,
    xml_start: Option<usize>,
}

impl RawContent {
    /// Records the line's text and withholds everything from the first XML tool call onward,
    /// so the OpenAI parser's inline `<think>` filter cannot rewrite tool parameter values.
    fn withhold_xml_tool_calls(&mut self, line: String) -> String {
        let Some(data) = line.strip_prefix("data:") else {
            return line;
        };
        let Ok(mut chunk) = serde_json::from_str::<Value>(data.trim()) else {
            return line;
        };
        let Some(content) = chunk.pointer_mut("/choices/0/delta/content") else {
            return line;
        };
        let Some(text) = content.as_str() else {
            return line;
        };

        let chunk_start = self.text.len();
        self.text.push_str(text);
        if self.xml_start.is_none() {
            let search_from = self
                .text
                .floor_char_boundary(chunk_start.saturating_sub(XML_TOOL_CALL_MARKER.len()));
            self.xml_start = self
                .text
                .get(search_from..)
                .and_then(|rest| rest.find(XML_TOOL_CALL_MARKER))
                .map(|i| search_from + i);
        }
        let Some(xml_start) = self.xml_start else {
            return line;
        };

        let kept = text
            .get(..xml_start.saturating_sub(chunk_start))
            .unwrap_or_default()
            .to_string();
        *content = Value::String(kept);
        format!("data: {chunk}")
    }
}

/// Streaming message handler with XML tool call post-processing for Ollama.
///
/// This wraps the standard OpenAI streaming handler and post-processes messages
/// to detect and parse XML tool calls. Text from the first XML tool call onward is
/// kept back from the OpenAI handler and parsed from the raw stream once it completes,
/// so tool parameter values arrive exactly as the model wrote them.
///
/// This approach avoids exposing any internal types from openai.rs.
pub fn response_to_streaming_message_ollama<S>(
    stream: S,
) -> impl Stream<Item = anyhow::Result<(Option<Message>, Option<ProviderUsage>)>> + 'static
where
    S: Stream<Item = anyhow::Result<String>> + Unpin + MaybeSend + 'static,
{
    let raw_content = Arc::new(Mutex::new(RawContent::default()));
    let tee = Arc::clone(&raw_content);
    let stream = stream.map_ok(move |line| {
        tee.lock()
            .expect("raw content lock poisoned")
            .withhold_xml_tool_calls(line)
    });

    try_stream! {
        use futures::StreamExt;

        let base_stream = openai::response_to_streaming_message(stream);
        let mut base_stream = std::pin::pin!(base_stream);

        let mut xml_detected = false;
        let mut native_tool_call_emitted = false;
        let mut buffered_usage: Option<ProviderUsage> = None;

        while let Some(result) = base_stream.next().await {
            let (message_opt, usage) = result?;
            xml_detected = xml_detected
                || raw_content
                    .lock()
                    .expect("raw content lock poisoned")
                    .xml_start
                    .is_some();

            if let Some(message) =
                message_opt.filter(|m| !(xml_detected && is_text_only_message(m)))
            {
                native_tool_call_emitted = native_tool_call_emitted
                    || message
                        .content
                        .iter()
                        .any(|c| matches!(c, MessageContentBlock::ToolRequest(_)));
                yield (Some(message), usage);
            } else if xml_detected {
                buffered_usage = usage.or(buffered_usage);
            } else if usage.is_some() {
                yield (None, usage);
            }
        }

        let raw = std::mem::take(&mut *raw_content.lock().expect("raw content lock poisoned"));
        // Native tool calls take precedence over XML in content, as in `response_to_message`.
        let xml_start = raw.xml_start.filter(|_| !native_tool_call_emitted);
        if let Some((leading_raw, xml_raw)) = xml_start.and_then(|i| raw.text.split_at_checked(i)) {
            let (_, xml_tool_calls) = parse_xml_tool_calls(xml_raw);

            if !xml_tool_calls.is_empty() {
                let (leading_text, _) = split_think_blocks(leading_raw);
                let mut contents = Vec::new();
                let prefix_text = leading_text.trim();
                if !prefix_text.is_empty() {
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
                // No tool parameters to protect, so the withheld text is think-filtered as usual;
                // the leading thinking was already emitted by the OpenAI parser.
                let mut think_filter = ThinkFilter::new();
                let mut text = think_filter.push(leading_raw).content;
                let rest = think_filter.push(xml_raw);
                let tail = think_filter.finish();
                text.push_str(&rest.content);
                text.push_str(&tail.content);
                let thinking = rest.thinking + &tail.thinking;

                let mut contents = Vec::new();
                if !thinking.is_empty() {
                    contents.push(MessageContentBlock::thinking(thinking, ""));
                }
                if !text.is_empty() {
                    contents.push(MessageContentBlock::text(text));
                }
                let msg = Message::new(Role::Assistant, chrono::Utc::now().timestamp(), contents)
                    .with_generated_id();

                yield (Some(msg), buffered_usage);
            }
        } else if buffered_usage.is_some() {
            yield (None, buffered_usage);
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

    #[tokio::test]
    async fn test_streaming_xml_fallback_keeps_think_tags_in_parameters() -> anyhow::Result<()> {
        use futures::StreamExt;

        let content = "<think>plan</think>Writing it.\n<function=developer__write>\n<parameter=path>tags.js</parameter>\n<parameter=content>const openTag = '<think>';\nconst closeTag = '</think>';</parameter>\n</function>";
        let mut lines: Vec<anyhow::Result<String>> = content
            .chars()
            .collect::<Vec<_>>()
            .chunks(5)
            .map(|chunk| {
                let delta = json!({"choices": [{"delta": {"content": chunk.iter().collect::<String>()}, "index": 0}]});
                Ok(format!("data: {delta}"))
            })
            .collect();
        lines.push(Ok("data: [DONE]".to_string()));
        let messages: Vec<Message> =
            response_to_streaming_message_ollama(tokio_stream::iter(lines))
                .filter_map(|item| async move { item.unwrap().0 })
                .collect()
                .await;

        let thinking: Vec<_> = messages
            .iter()
            .flat_map(|m| &m.content)
            .filter_map(|c| match c {
                MessageContentBlock::Thinking(t) => Some(t.thinking.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(thinking, vec!["plan"]);

        let tool_call = messages
            .iter()
            .flat_map(|m| &m.content)
            .find_map(|c| match c {
                MessageContentBlock::ToolRequest(request) => request.tool_call.as_ref().ok(),
                _ => None,
            })
            .expect("expected an XML tool call");
        let args = tool_call.arguments.as_ref().unwrap();
        assert_eq!(
            args.get("content").unwrap(),
            "const openTag = '<think>';\nconst closeTag = '</think>';"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_streaming_prefers_json_over_xml() -> anyhow::Result<()> {
        use futures::StreamExt;

        let delta = json!({"choices": [{"delta": {
            "content": "<function=wrong_tool>\n<parameter=x>y</parameter>\n</function>",
            "tool_calls": [{
                "index": 0,
                "id": "call_123",
                "type": "function",
                "function": {"name": "correct_tool", "arguments": "{\"a\": \"b\"}"}
            }]
        }, "index": 0, "finish_reason": "tool_calls"}]});
        let usage_frame = json!({"model": "test-model", "choices": [], "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}});
        let lines: Vec<anyhow::Result<String>> = vec![
            Ok(format!("data: {delta}")),
            Ok(format!("data: {usage_frame}")),
            Ok("data: [DONE]".to_string()),
        ];
        let items: Vec<_> = response_to_streaming_message_ollama(tokio_stream::iter(lines))
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<anyhow::Result<_>>()?;

        let input_tokens: Vec<_> = items
            .iter()
            .filter_map(|(_, usage)| usage.as_ref()?.usage.input_tokens)
            .collect();
        assert_eq!(input_tokens, vec![10]);

        let tool_names: Vec<_> = items
            .iter()
            .filter_map(|(message, _)| message.as_ref())
            .flat_map(|m| &m.content)
            .filter_map(|c| match c {
                MessageContentBlock::ToolRequest(request) => {
                    Some(request.tool_call.as_ref().unwrap().name.to_string())
                }
                _ => None,
            })
            .collect();
        assert_eq!(tool_names, vec!["correct_tool"]);

        Ok(())
    }

    #[tokio::test]
    async fn test_streaming_unparsed_xml_inside_think_stays_thinking() -> anyhow::Result<()> {
        use futures::StreamExt;

        let deltas = [
            "<think>Consider ",
            "<function=foo but ",
            "skip it</th",
            "ink>answer",
        ];
        let mut lines: Vec<anyhow::Result<String>> = deltas
            .iter()
            .map(|content| {
                let delta = json!({"choices": [{"delta": {"content": content}, "index": 0}]});
                Ok(format!("data: {delta}"))
            })
            .collect();
        lines.push(Ok("data: [DONE]".to_string()));
        let messages: Vec<Message> =
            response_to_streaming_message_ollama(tokio_stream::iter(lines))
                .filter_map(|item| async move { item.unwrap().0 })
                .collect()
                .await;

        let blocks: Vec<_> = messages.iter().flat_map(|m| &m.content).collect();
        let thinking: String = blocks
            .iter()
            .filter_map(|c| match c {
                MessageContentBlock::Thinking(t) => Some(t.thinking.as_str()),
                _ => None,
            })
            .collect();
        let text: String = blocks
            .iter()
            .filter_map(|c| match c {
                MessageContentBlock::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(thinking, "Consider <function=foo but skip it");
        assert_eq!(text, "answer");

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
