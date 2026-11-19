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
    thinking::ThinkFilter,
};
use async_stream::try_stream;
use chrono;
use futures::{Stream, TryStreamExt};
use regex::Regex;
use rmcp::model::{object, CallToolRequestParams, ErrorCode, ErrorData, Role};
use serde_json::Value;
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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

const FUNCTION: &str = "<function=";
const FUNCTION_END: &str = "</function>";
const PARAMETER: &str = "<parameter=";
const PARAMETER_END: &str = "</parameter>";
const ESCAPE: char = '\u{E000}';
const ESCAPED_ESCAPE: char = 'e';
const ESCAPED_LT: char = 'l';

fn push_guarded(output: &mut String, text: &str, hide_lt: bool) {
    for c in text.chars() {
        if c == ESCAPE {
            output.extend([ESCAPE, ESCAPED_ESCAPE]);
        } else if hide_lt && c == '<' {
            output.extend([ESCAPE, ESCAPED_LT]);
        } else {
            output.push(c);
        }
    }
}

fn decode_guarded(text: &str) -> String {
    let mut decoded = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != ESCAPE {
            decoded.push(c);
            continue;
        }
        match chars.next() {
            Some(ESCAPED_ESCAPE) => decoded.push(ESCAPE),
            Some(ESCAPED_LT) => decoded.push('<'),
            Some(c) => decoded.extend([ESCAPE, c]),
            None => decoded.push(ESCAPE),
        }
    }
    decoded
}

/// Hides every `<` inside XML tool-call parameter values from the OpenAI parser's inline
/// `<think>` filter, so think tags in tool arguments are not taken as reasoning.
/// Calls and parameters only open and close at tags the filter leaves outside reasoning.
#[derive(Default)]
struct ParameterGuard {
    think_filter: ThinkFilter,
    /// A trailing bare `<` or `</`, which `ThinkFilter` would flush before it can see `think>`.
    pending: String,
    content_tail: String,
    /// Raw text since the last `<`, to find tags split across chunks.
    token: String,
    in_call: bool,
    in_parameter: bool,
    encoded: Arc<AtomicBool>,
}

impl ParameterGuard {
    fn guard_line(&mut self, line: String) -> String {
        let Some(mut chunk) = line
            .strip_prefix("data:")
            .and_then(|data| serde_json::from_str::<Value>(data.trim()).ok())
        else {
            return line;
        };
        let mut changed = false;
        for pointer in [
            "/choices/0/delta/reasoning_content",
            "/choices/0/delta/reasoning",
        ] {
            if let Some(text) = chunk.pointer_mut(pointer) {
                if let Some(raw) = text.as_str() {
                    let mut guarded = String::with_capacity(raw.len());
                    push_guarded(&mut guarded, raw, false);
                    if guarded != raw {
                        *text = Value::String(guarded);
                        changed = true;
                    }
                }
            }
        }
        // The OpenAI parser ignores content on a delta that carries native tool calls.
        let has_native_tool_calls = chunk
            .pointer("/choices/0/delta/tool_calls")
            .and_then(Value::as_array)
            .is_some_and(|calls| !calls.is_empty());
        if !has_native_tool_calls {
            let texts: Vec<&mut Value> = match chunk.pointer_mut("/choices/0/delta/content") {
                Some(Value::Array(parts)) => parts
                    .iter_mut()
                    .filter(|part| part["type"] == "text")
                    .filter_map(|part| part.get_mut("text"))
                    .collect(),
                Some(text) => vec![text],
                None => Vec::new(),
            };
            for text in texts {
                if let Some((raw, guarded)) = text.as_str().map(|raw| (raw, self.guard(raw))) {
                    if guarded != raw {
                        *text = Value::String(guarded);
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.encoded.store(true, Ordering::Relaxed);
            format!("data: {chunk}")
        } else {
            line
        }
    }

    fn guard(&mut self, text: &str) -> String {
        let mut guarded = String::with_capacity(text.len());
        let mut rest = text;
        while !rest.is_empty() {
            // Each piece holds at most one `<`, at its start.
            let end = rest
                .match_indices('<')
                .map(|(i, _)| i)
                .find(|&i| i > 0)
                .unwrap_or(rest.len());
            let (piece, after) = rest.split_at_checked(end).unwrap_or((rest, ""));
            rest = after;
            if piece.starts_with('<') {
                self.token.clear();
            }
            let before = self.token.len();
            self.token.push_str(piece);
            let completes = |tag: &str| before < tag.len() && self.token.starts_with(tag);

            if self.in_parameter {
                push_guarded(&mut guarded, piece, true);
                self.in_parameter = !completes(PARAMETER_END);
                continue;
            }
            let tags: &[&str] = if self.in_call {
                &[PARAMETER, FUNCTION_END]
            } else {
                &[FUNCTION]
            };
            let tag = tags.iter().copied().find(|tag| completes(tag));
            let (head, tail) = tag
                .and_then(|tag| piece.split_at_checked(tag.len() - before))
                .unwrap_or(("", piece));
            push_guarded(&mut guarded, piece, false);
            self.filter(head);
            if let Some(tag) = tag.filter(|tag| self.content_tail.ends_with(tag)) {
                self.in_call = tag != FUNCTION_END;
                self.in_parameter = tag == PARAMETER;
                // A later marker counts only if it is emitted outside reasoning afresh.
                self.content_tail.clear();
            }
            if !self.in_parameter {
                self.filter(tail);
            }
        }
        guarded
    }

    /// Feeds raw text outside parameter values to a mirror of the OpenAI parser's filter.
    fn filter(&mut self, text: &str) {
        self.pending.push_str(text);
        let held = ["</", "<"]
            .into_iter()
            .find(|prefix| self.pending.ends_with(prefix))
            .map_or(0, str::len);
        let held = self.pending.split_off(self.pending.len() - held);
        let ready = std::mem::replace(&mut self.pending, held);
        self.content_tail
            .push_str(&self.think_filter.push(&ready).content);
        if let Some((keep, _)) = self
            .content_tail
            .char_indices()
            .rev()
            .nth(PARAMETER_END.len())
        {
            self.content_tail.drain(..keep);
        }
    }
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
    S: Stream<Item = anyhow::Result<String>> + Unpin + MaybeSend + 'static,
{
    let mut guard = ParameterGuard::default();
    let encoded = guard.encoded.clone();
    let stream = stream.map_ok(move |line| guard.guard_line(line));

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

            if let Some(mut message) = message_opt {
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

                if encoded.load(Ordering::Relaxed) {
                    for content in &mut message.content {
                        match content {
                            MessageContentBlock::Text(text) => {
                                text.text = decode_guarded(&text.text);
                            }
                            MessageContentBlock::Thinking(thinking) => {
                                thinking.thinking = decode_guarded(&thinking.thinking);
                            }
                            _ => {}
                        }
                    }
                }
                yield (Some(message), usage);
            } else if usage.is_some() && !xml_detected {
                yield (None, usage);
            }
        }

        if xml_detected && !accumulated_text.is_empty() {
            if encoded.load(Ordering::Relaxed) {
                accumulated_text = decode_guarded(&accumulated_text);
            }
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
                    vec![MessageContentBlock::text(accumulated_text)],
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

    async fn streamed_tool_names(lines: Vec<anyhow::Result<String>>) -> Vec<String> {
        use futures::StreamExt;
        let messages: Vec<Message> =
            response_to_streaming_message_ollama(tokio_stream::iter(lines))
                .filter_map(|item| async move { item.unwrap().0 })
                .collect()
                .await;
        messages
            .iter()
            .flat_map(|m| &m.content)
            .filter_map(|c| match c {
                MessageContentBlock::ToolRequest(r) => Some(match &r.tool_call {
                    Ok(call) => call.name.to_string(),
                    Err(e) => format!("ERR: {}", e.message),
                }),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn test_streaming_marker_inside_think_does_not_swallow_real_call() {
        let content =
            "<think>Consider <function=foo but skip it</think><function=real_tool></function>";
        let delta = json!({"choices": [{"delta": {"content": content}, "index": 0}]});
        let lines = vec![Ok(format!("data: {delta}")), Ok("data: [DONE]".to_string())];
        assert_eq!(streamed_tool_names(lines).await, vec!["real_tool"]);
    }

    #[tokio::test]
    async fn test_streaming_xml_fallback_reads_text_part_arrays() {
        let delta = json!({"choices": [{"delta": {"content": [{"type": "text", "text": "<function=real_tool></function>"}]}, "index": 0}]});
        let lines = vec![Ok(format!("data: {delta}")), Ok("data: [DONE]".to_string())];
        assert_eq!(streamed_tool_names(lines).await, vec!["real_tool"]);
    }
    #[tokio::test]
    async fn test_streaming_skips_reasoning_calls_after_first_call() {
        let content =
            "<function=read></function><think>Do not call <function=delete></function></think>";
        let delta = json!({"choices": [{"delta": {"content": content}, "index": 0}]});
        let lines = vec![Ok(format!("data: {delta}")), Ok("data: [DONE]".to_string())];
        assert_eq!(streamed_tool_names(lines).await, vec!["read"]);
    }

    #[tokio::test]
    async fn test_streaming_marker_in_think_attribute_is_not_a_call() {
        let content =
            "<think example=\"<function=wrong></function>\">plan</think><function=real></function>";
        let delta = json!({"choices": [{"delta": {"content": content}, "index": 0}]});
        let lines = vec![Ok(format!("data: {delta}")), Ok("data: [DONE]".to_string())];
        assert_eq!(streamed_tool_names(lines).await, vec!["real"]);
    }

    #[test]
    fn test_parse_xml_tool_calls_single() {
        let file_text = "hello \u{E000}/parameter> world";
        let content = format!(
            r#"<function=developer__text_editor>
<parameter=command>write</parameter>
<parameter=path>/tmp/test.txt</parameter>
<parameter=file_text>{file_text}</parameter>
</function>"#
        );

        let (prefix, tool_calls) = parse_xml_tool_calls(&content);

        assert!(prefix.is_none(), "Should have no prefix");
        assert_eq!(tool_calls.len(), 1, "Should have 1 tool call");

        if let MessageContentBlock::ToolRequest(request) = &tool_calls[0] {
            let tool_call = request.tool_call.as_ref().unwrap();
            assert_eq!(tool_call.name, "developer__text_editor");
            let args = tool_call.arguments.as_ref().unwrap();
            assert_eq!(args.get("command").unwrap(), "write");
            assert_eq!(args.get("path").unwrap(), "/tmp/test.txt");
            assert_eq!(args.get("file_text").unwrap(), file_text);
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

    fn chunked_lines(content: &str, size: usize) -> Vec<anyhow::Result<String>> {
        let chars: Vec<char> = content.chars().collect();
        let mut lines: Vec<_> = chars
            .chunks(size)
            .map(|chunk| {
                let text: String = chunk.iter().collect();
                let delta = json!({"choices": [{"delta": {"content": text}, "index": 0}]});
                Ok(format!("data: {delta}"))
            })
            .collect();
        lines.push(Ok("data: [DONE]".to_string()));
        lines
    }

    #[tokio::test]
    async fn test_streaming_keeps_any_think_tag_in_parameters_at_every_chunk_split() {
        use futures::StreamExt;

        let open = "a \u{E000}/parameter> b <think> c <think x=\">\"> d <THINKING> e";
        let close = "e </think> f </parameter g";
        let content = format!(
            "<function=edit><parameter=open>{open}</parameter><parameter=close>{close}</parameter></function>"
        );
        for size in 1..=content.len() {
            let messages: Vec<Message> = response_to_streaming_message_ollama(tokio_stream::iter(
                chunked_lines(&content, size),
            ))
            .filter_map(|item| async move { item.unwrap().0 })
            .collect()
            .await;
            let args: Vec<_> = messages
                .iter()
                .flat_map(|m| &m.content)
                .filter_map(|c| match c {
                    MessageContentBlock::ToolRequest(r) => r.tool_call.as_ref().ok(),
                    _ => None,
                })
                .map(|call| call.arguments.clone().unwrap())
                .collect();
            assert_eq!(args.len(), 1, "chunk size {size}");
            assert_eq!(args[0]["open"], open, "chunk size {size}");
            assert_eq!(args[0]["close"], close, "chunk size {size}");
        }
    }

    #[tokio::test]
    async fn test_streaming_unparsed_call_keeps_think_tags_in_text() {
        use futures::StreamExt;

        let content = "<function=edit><parameter=text>keep <think> and </think></parameter>";
        let messages: Vec<Message> =
            response_to_streaming_message_ollama(tokio_stream::iter(chunked_lines(content, 15)))
                .filter_map(|item| async move { item.unwrap().0 })
                .collect()
                .await;
        let text: String = messages
            .iter()
            .flat_map(|m| &m.content)
            .filter_map(|c| match c {
                MessageContentBlock::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(text, content);
    }

    #[tokio::test]
    async fn test_streaming_unparsed_xml_keeps_structured_reasoning_precedence() {
        use futures::StreamExt;

        for field in ["reasoning_content", "reasoning"] {
            let reasoning = json!({"choices": [{"delta": {field: "structured plan"}, "index": 0}]});
            let mut lines = vec![Ok(format!("data: {reasoning}"))];
            lines.extend(chunked_lines(
                "Use <function= syntax. <think>plan</think>answer",
                15,
            ));
            let messages: Vec<Message> =
                response_to_streaming_message_ollama(tokio_stream::iter(lines))
                    .filter_map(|item| async move { item.unwrap().0 })
                    .collect()
                    .await;
            let blocks: Vec<_> = messages.iter().flat_map(|m| &m.content).collect();
            let thinking: Vec<_> = blocks
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
            assert_eq!(thinking, vec!["structured plan"], "{field}");
            assert_eq!(text, "Use <function= syntax. answer", "{field}");
        }
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
        assert!(!items
            .iter()
            .filter_map(|(message, _)| message.as_ref())
            .flat_map(|m| &m.content)
            .any(|c| matches!(c, MessageContentBlock::Text(_))));

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

    #[tokio::test]
    async fn test_streaming_reasoning_in_call_body_cannot_override_parameters() {
        use futures::StreamExt;

        let content = "<function=developer__shell><parameter=command>pwd</parameter><think>Reject <parameter=command>echo rejected</parameter></think></function>";
        let delta = json!({"choices": [{"delta": {"content": content}, "index": 0}]});
        let lines: Vec<anyhow::Result<String>> =
            vec![Ok(format!("data: {delta}")), Ok("data: [DONE]".to_string())];
        let messages: Vec<Message> =
            response_to_streaming_message_ollama(tokio_stream::iter(lines))
                .filter_map(|item| async move { item.unwrap().0 })
                .collect()
                .await;

        let commands: Vec<_> = messages
            .iter()
            .flat_map(|m| &m.content)
            .filter_map(|c| match c {
                MessageContentBlock::ToolRequest(r) => r.tool_call.as_ref().ok(),
                _ => None,
            })
            .map(|call| call.arguments.as_ref().unwrap()["command"].clone())
            .collect();
        assert_eq!(commands, vec![json!("pwd")]);
    }

    #[tokio::test]
    async fn test_streaming_closing_tag_in_call_reasoning_does_not_end_call() {
        use futures::StreamExt;

        let content = "<function=developer__shell><think>Close with </function> after the arguments.</think><parameter=command>pwd</parameter></function>";
        let delta = json!({"choices": [{"delta": {"content": content}, "index": 0}]});
        let lines: Vec<anyhow::Result<String>> =
            vec![Ok(format!("data: {delta}")), Ok("data: [DONE]".to_string())];
        let messages: Vec<Message> =
            response_to_streaming_message_ollama(tokio_stream::iter(lines))
                .filter_map(|item| async move { item.unwrap().0 })
                .collect()
                .await;

        let commands: Vec<_> = messages
            .iter()
            .flat_map(|m| &m.content)
            .filter_map(|c| match c {
                MessageContentBlock::ToolRequest(r) => r.tool_call.as_ref().ok(),
                _ => None,
            })
            .map(|call| call.arguments.as_ref().unwrap().get("command").cloned())
            .collect();
        assert_eq!(commands, vec![Some(json!("pwd"))]);
    }

    #[tokio::test]
    async fn test_streaming_parameter_marker_in_reasoning_after_parameter_is_not_a_parameter() {
        use futures::StreamExt;

        let content = "<function=developer__shell><parameter=command>pwd</parameter><think>Use <parameter=other> later</think></function>";
        let delta = json!({"choices": [{"delta": {"content": content}, "index": 0}]});
        let lines: Vec<anyhow::Result<String>> =
            vec![Ok(format!("data: {delta}")), Ok("data: [DONE]".to_string())];
        let messages: Vec<Message> =
            response_to_streaming_message_ollama(tokio_stream::iter(lines))
                .filter_map(|item| async move { item.unwrap().0 })
                .collect()
                .await;

        let arguments: Vec<_> = messages
            .iter()
            .flat_map(|m| &m.content)
            .filter_map(|c| match c {
                MessageContentBlock::ToolRequest(r) => r.tool_call.as_ref().ok(),
                _ => None,
            })
            .map(|call| call.arguments.clone())
            .collect();
        assert_eq!(arguments, vec![Some(object(json!({"command": "pwd"})))]);
    }

    #[tokio::test]
    async fn test_streaming_mixed_reasoning_message_keeps_literal_text() {
        use futures::StreamExt;

        let content = "<function=edit><parameter=text>a < b</parameter></function>";
        let reasoning = "plan \u{E000}l";
        for field in ["reasoning_content", "reasoning"] {
            let delta =
                json!({"choices": [{"delta": {field: reasoning, "content": content}, "index": 0}]});
            let lines: Vec<anyhow::Result<String>> =
                vec![Ok(format!("data: {delta}")), Ok("data: [DONE]".to_string())];
            let messages: Vec<Message> =
                response_to_streaming_message_ollama(tokio_stream::iter(lines))
                    .filter_map(|item| async move { item.unwrap().0 })
                    .collect()
                    .await;

            let text: String = messages
                .iter()
                .flat_map(|m| &m.content)
                .filter_map(|c| match c {
                    MessageContentBlock::Text(t) => Some(t.text.as_str()),
                    _ => None,
                })
                .collect();
            let thinking: String = messages
                .iter()
                .flat_map(|m| &m.content)
                .filter_map(|c| match c {
                    MessageContentBlock::Thinking(t) => Some(t.thinking.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(text, content, "{field}");
            assert_eq!(thinking, reasoning, "{field}");
        }
    }

    #[tokio::test]
    async fn test_streaming_native_call_keeps_text_mentioning_xml_marker() {
        use futures::StreamExt;

        let text = json!({"choices": [{"delta": {"content": "Native calls replace <function= markup."}, "index": 0}]});
        let native = json!({"choices": [{"delta": {"tool_calls": [{
            "index": 0,
            "id": "call_123",
            "type": "function",
            "function": {"name": "correct_tool", "arguments": "{\"a\": \"b\"}"}
        }]}, "index": 0, "finish_reason": "tool_calls"}]});
        let lines: Vec<anyhow::Result<String>> = vec![
            Ok(format!("data: {text}")),
            Ok(format!("data: {native}")),
            Ok("data: [DONE]".to_string()),
        ];
        let messages: Vec<Message> =
            response_to_streaming_message_ollama(tokio_stream::iter(lines))
                .filter_map(|item| async move { item.unwrap().0 })
                .collect()
                .await;

        let blocks: Vec<_> = messages.iter().flat_map(|m| &m.content).collect();
        let text: String = blocks
            .iter()
            .filter_map(|c| match c {
                MessageContentBlock::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect();
        let tool_names: Vec<_> = blocks
            .iter()
            .filter_map(|c| match c {
                MessageContentBlock::ToolRequest(r) => {
                    Some(r.tool_call.as_ref().unwrap().name.to_string())
                }
                _ => None,
            })
            .collect();
        assert_eq!(text, "Native calls replace <function= markup.");
        assert_eq!(tool_names, vec!["correct_tool"]);
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

    #[tokio::test]
    async fn test_streaming_bare_lt_before_split_think_tag_keeps_call() {
        use futures::StreamExt;

        let lines: Vec<anyhow::Result<String>> = [
            "<function=developer__shell><parameter=command>pwd</parameter><",
            "think>Use <parameter=other> later</think></function>",
        ]
        .iter()
        .map(|content| {
            let delta = json!({"choices": [{"delta": {"content": content}, "index": 0}]});
            Ok(format!("data: {delta}"))
        })
        .chain([Ok("data: [DONE]".to_string())])
        .collect();
        let messages: Vec<Message> =
            response_to_streaming_message_ollama(tokio_stream::iter(lines))
                .filter_map(|item| async move { item.unwrap().0 })
                .collect()
                .await;

        let arguments: Vec<_> = messages
            .iter()
            .flat_map(|m| &m.content)
            .filter_map(|c| match c {
                MessageContentBlock::ToolRequest(r) => r.tool_call.as_ref().ok(),
                _ => None,
            })
            .map(|call| call.arguments.as_ref().unwrap().get("command").cloned())
            .collect();
        assert_eq!(arguments, vec![Some(json!("pwd"))]);
    }

    #[tokio::test]
    async fn test_streaming_text_keeps_private_use_character() {
        use futures::StreamExt;

        let content = "Glyph \u{E000} stays.";
        let delta = json!({"choices": [{"delta": {"content": content}, "index": 0}]});
        let lines: Vec<anyhow::Result<String>> =
            vec![Ok(format!("data: {delta}")), Ok("data: [DONE]".to_string())];
        let messages: Vec<Message> =
            response_to_streaming_message_ollama(tokio_stream::iter(lines))
                .filter_map(|item| async move { item.unwrap().0 })
                .collect()
                .await;

        let text: String = messages
            .iter()
            .flat_map(|m| &m.content)
            .filter_map(|c| match c {
                MessageContentBlock::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(text, content);
    }
}
