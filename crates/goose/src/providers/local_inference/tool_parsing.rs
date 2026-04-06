use crate::conversation::message::{Message, MessageContent};
use rmcp::model::{CallToolRequestParams, Tool};
use serde_json::{json, Value};
use std::borrow::Cow;
use uuid::Uuid;

pub(super) fn compact_tools_json(tools: &[Tool]) -> Option<String> {
    let compact: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description.as_ref().map(|d| d.as_ref()).unwrap_or(""),
                }
            })
        })
        .collect();
    serde_json::to_string(&compact).ok()
}

/// Split generated text into (content, tool_calls_json).
/// Looks for the last top-level JSON object containing `"tool_calls"`.
/// Returns the text before it as content, and the JSON string if found.
#[allow(clippy::string_slice)]
pub(super) fn split_content_and_tool_calls(text: &str) -> (String, Option<String>) {
    let trimmed = text.trim_end();
    if !trimmed.ends_with('}') {
        return (text.to_string(), None);
    }

    // Scan backwards for the matching '{' of the final '}'.
    // We only match on ASCII braces so `start` is always a char boundary.
    let bytes = trimmed.as_bytes();
    let mut depth = 0i32;
    let mut json_start = None;
    for i in (0..bytes.len()).rev() {
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                depth -= 1;
                if depth == 0 {
                    json_start = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }

    let Some(start) = json_start else {
        return (text.to_string(), None);
    };

    let json_str = &trimmed[start..];
    let parsed: Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return (text.to_string(), None),
    };

    if parsed
        .get("tool_calls")
        .and_then(|v| v.as_array())
        .is_none()
    {
        return (text.to_string(), None);
    }

    let content = trimmed[..start].trim_end().to_string();
    (content, Some(json_str.to_string()))
}

/// Return the byte length of text that is safe to stream.
/// Everything before the last unmatched top-level `{` is safe — the `{` could
/// be the start of a tool-call JSON block still being generated.
/// If all braces are balanced the entire text is safe.
pub(super) fn safe_stream_end(text: &str) -> usize {
    // Hold back from the start of any incomplete <tool_call> tag.
    // If we find an unmatched opening, nothing from that point should be streamed.
    let xml_hold = text.find("<tool_call>").unwrap_or(text.len());

    let bytes = text.as_bytes();
    let mut safe_end = bytes.len();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => {
                if depth == 0 {
                    safe_end = i;
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    safe_end = i + 1;
                }
            }
            _ => {
                if depth == 0 {
                    safe_end = i + 1;
                }
            }
        }
    }

    // Also hold back a partial `<tool_call` prefix at the end of the text.
    // The tag is 11 chars; if the last N chars are a prefix of `<tool_call>`, hold them.
    let tag = b"<tool_call>";
    let tail_hold = {
        let mut hold = safe_end;
        let check_len = tag.len().min(bytes.len());
        for start in (safe_end.saturating_sub(check_len))..safe_end {
            let tail = &bytes[start..safe_end];
            if tag.starts_with(tail) {
                hold = start;
                break;
            }
        }
        hold
    };

    safe_end.min(xml_hold).min(tail_hold)
}

/// Extract tool call messages from a JSON object containing "tool_calls".
/// Handles both the model's native format (name/arguments at top level)
/// and the OpenAI format (function.name/function.arguments).
pub(super) fn extract_tool_call_messages(tool_calls_json: &str, message_id: &str) -> Vec<Message> {
    let parsed: Value = match serde_json::from_str(tool_calls_json) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    let Some(tool_calls) = parsed.get("tool_calls").and_then(|v| v.as_array()) else {
        return vec![];
    };

    let mut messages = Vec::new();
    for tc in tool_calls {
        // Try OpenAI format first: {"function": {"name": ..., "arguments": ...}, "id": ...}
        // Then model's native format: {"name": ..., "arguments": {...}, "id": ...}
        let (name, arguments) = if let Some(func) = tc.get("function") {
            let n = func.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args_str = func
                .get("arguments")
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let args: Option<serde_json::Map<String, Value>> = serde_json::from_str(args_str).ok();
            (n.to_string(), args)
        } else {
            let n = tc.get("name").and_then(|v| v.as_str()).unwrap_or("");
            // Arguments may be an object directly (model format) or a string (OAI format)
            let args = if let Some(obj) = tc.get("arguments").and_then(|v| v.as_object()) {
                Some(obj.clone())
            } else if let Some(s) = tc.get("arguments").and_then(|v| v.as_str()) {
                serde_json::from_str(s).ok()
            } else {
                None
            };
            (n.to_string(), args)
        };

        if name.is_empty() {
            continue;
        }

        let id = tc
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let tool_call = CallToolRequestParams::new(Cow::Owned(name))
            .with_arguments(arguments.unwrap_or_default());

        let mut msg = Message::assistant();
        msg.content
            .push(MessageContent::tool_request(id, Ok(tool_call)));
        msg.id = Some(message_id.to_string());
        messages.push(msg);
    }

    messages
}

/// Parse XML-style tool calls used by models like qwen3-coder.
/// Format:
/// ```text
/// <tool_call>
/// <function=tool_name>
/// <parameter=param1>value1</parameter>
/// <parameter=param2>value2</parameter>
/// </function>
/// </tool_call>
/// ```
/// Returns (content_before_tool_calls, vec_of_tool_calls) or None if no XML tool calls found.
#[allow(clippy::type_complexity)]
pub(super) fn split_content_and_xml_tool_calls(
    text: &str,
) -> Option<(String, Vec<(String, serde_json::Map<String, Value>)>)> {
    let (content, first_block_and_rest) = text.split_once("<tool_call>")?;
    let content = content.trim_end().to_string();
    let mut tool_calls = Vec::new();

    // Process the first block, then keep splitting on subsequent <tool_call> tags
    let mut remaining = first_block_and_rest;
    loop {
        // Split off the block up to </tool_call> (or take the rest if unclosed)
        let (block, after_close) = remaining
            .split_once("</tool_call>")
            .unwrap_or((remaining, ""));

        if let Some(tool_call) = parse_single_xml_tool_call(block) {
            tool_calls.push(tool_call);
        }

        // Try to find the next <tool_call> in what remains
        match after_close.split_once("<tool_call>") {
            Some((_between, next_remaining)) => remaining = next_remaining,
            None => break,
        }
    }

    if tool_calls.is_empty() {
        None
    } else {
        Some((content, tool_calls))
    }
}

fn parse_single_xml_tool_call(block: &str) -> Option<(String, serde_json::Map<String, Value>)> {
    // Try <function=NAME><parameter=K>V</parameter>...</function> format first
    if let Some(result) = parse_xml_function_format(block) {
        return Some(result);
    }
    // Try GLM-style: TOOL_NAME<arg_key>K</arg_key><arg_value>V</arg_value>...
    parse_xml_arg_key_value_format(block)
}

fn parse_xml_function_format(block: &str) -> Option<(String, serde_json::Map<String, Value>)> {
    let (_, after_func_eq) = block.split_once("<function=")?;
    let (func_name, func_body) = after_func_eq.split_once('>')?;
    let func_name = func_name.trim().to_string();

    let mut args = serde_json::Map::new();
    let mut rest = func_body;

    while let Some((_, after_param_eq)) = rest.split_once("<parameter=") {
        let Some((param_name, after_name_close)) = after_param_eq.split_once('>') else {
            break;
        };
        let param_name = param_name.trim().to_string();

        let (value, after_value) = after_name_close
            .split_once("</parameter>")
            .unwrap_or((after_name_close, ""));
        let value = value.trim();

        let json_value =
            serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
        args.insert(param_name, json_value);

        rest = after_value;
    }

    Some((func_name, args))
}

/// Parse GLM-style tool calls: `NAME<arg_key>K</arg_key><arg_value>V</arg_value>...`
/// Also handles zero-argument calls like just `NAME`.
fn parse_xml_arg_key_value_format(block: &str) -> Option<(String, serde_json::Map<String, Value>)> {
    let func_name_end = block.find("<arg_key>").unwrap_or(block.len());
    // Safe: find returns a byte offset at the start of an ASCII '<' character,
    // and block.len() is always a valid boundary.
    #[allow(clippy::string_slice)]
    let func_name = block[..func_name_end].trim().to_string();
    if func_name.is_empty() {
        return None;
    }

    let mut args = serde_json::Map::new();
    #[allow(clippy::string_slice)]
    let mut rest = &block[func_name_end..];

    while let Some((_, after_key_open)) = rest.split_once("<arg_key>") {
        let Some((key, after_key_close)) = after_key_open.split_once("</arg_key>") else {
            break;
        };
        let key = key.trim().to_string();

        let Some((_, after_val_open)) = after_key_close.split_once("<arg_value>") else {
            break;
        };
        let (value, after_val_close) = after_val_open
            .split_once("</arg_value>")
            .unwrap_or((after_val_open, ""));
        let value = value.trim();

        let json_value =
            serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
        args.insert(key, json_value);

        rest = after_val_close;
    }

    Some((func_name, args))
}

pub(super) fn extract_xml_tool_call_messages(
    tool_calls: Vec<(String, serde_json::Map<String, Value>)>,
    message_id: &str,
) -> Vec<Message> {
    tool_calls
        .into_iter()
        .map(|(name, args)| {
            let tool_call = if args.is_empty() {
                CallToolRequestParams::new(Cow::Owned(name))
            } else {
                CallToolRequestParams::new(Cow::Owned(name)).with_arguments(args)
            };
            let mut msg = Message::assistant();
            msg.content.push(MessageContent::tool_request(
                Uuid::new_v4().to_string(),
                Ok(tool_call),
            ));
            msg.id = Some(message_id.to_string());
            msg
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SHELL_TOOL: &str = "developer__shell";

    #[test]
    fn test_parse_xml_tool_call_single() {
        let text = "I'll search for that.\n\n<tool_call>\n<function=search__files>\n<parameter=pattern>local.*inference</parameter>\n</function>\n</tool_call>";
        let result = split_content_and_xml_tool_calls(text);
        assert!(result.is_some());
        let (content, calls) = result.unwrap();
        assert_eq!(content, "I'll search for that.");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "search__files");
        assert_eq!(calls[0].1.get("pattern").unwrap(), "local.*inference");
    }

    #[test]
    fn test_parse_xml_tool_call_multiple_params() {
        let text = "<tool_call>\n<function=developer__shell>\n<parameter=command>ls -la</parameter>\n<parameter=timeout>30</parameter>\n</function>\n</tool_call>";
        let result = split_content_and_xml_tool_calls(text);
        assert!(result.is_some());
        let (content, calls) = result.unwrap();
        assert!(content.is_empty());
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, SHELL_TOOL);
        assert_eq!(calls[0].1.get("command").unwrap(), "ls -la");
        // 30 should be parsed as a number
        assert_eq!(calls[0].1.get("timeout").unwrap(), &json!(30));
    }

    #[test]
    fn test_parse_xml_tool_call_no_tool_call() {
        let text = "Just some regular text with no tool calls.";
        assert!(split_content_and_xml_tool_calls(text).is_none());
    }

    #[test]
    fn test_parse_xml_tool_call_multiple_calls() {
        let text = "Doing two things:\n<tool_call>\n<function=foo__bar>\n<parameter=x>1</parameter>\n</function>\n</tool_call>\n<tool_call>\n<function=baz__qux>\n<parameter=y>hello</parameter>\n</function>\n</tool_call>";
        let result = split_content_and_xml_tool_calls(text);
        assert!(result.is_some());
        let (content, calls) = result.unwrap();
        assert_eq!(content, "Doing two things:");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, "foo__bar");
        assert_eq!(calls[1].0, "baz__qux");
    }

    #[test]
    fn test_parse_xml_tool_call_multiline_value() {
        let text = "<tool_call>\n<function=developer__write_file>\n<parameter=path>test.py</parameter>\n<parameter=content>def hello():\n    print(\"world\")</parameter>\n</function>\n</tool_call>";
        let result = split_content_and_xml_tool_calls(text);
        assert!(result.is_some());
        let (_content, calls) = result.unwrap();
        assert_eq!(calls[0].0, "developer__write_file");
        assert_eq!(
            calls[0].1.get("content").unwrap(),
            "def hello():\n    print(\"world\")"
        );
    }

    #[test]
    fn test_safe_stream_end_holds_back_tool_call_tag() {
        let text = "Some text before <tool_call>\n<function=foo>";
        let safe = safe_stream_end(text);
        assert!(safe <= text.find("<tool_call>").unwrap());
    }

    #[test]
    fn test_safe_stream_end_holds_back_partial_tag() {
        let text = "Some text <tool_ca";
        let safe = safe_stream_end(text);
        // Should hold back the partial tag
        assert!(safe <= text.find('<').unwrap());
    }

    #[test]
    fn test_parse_glm_style_tool_call() {
        let text = "<tool_call>developer__shell<arg_key>command</arg_key><arg_value>ls -la</arg_value></tool_call>";
        let result = split_content_and_xml_tool_calls(text);
        assert!(result.is_some());
        let (content, calls) = result.unwrap();
        assert!(content.is_empty());
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, SHELL_TOOL);
        assert_eq!(calls[0].1.get("command").unwrap(), "ls -la");
    }

    #[test]
    fn test_parse_glm_style_tool_call_no_args() {
        let text = "Some text\n<tool_call>load</tool_call>";
        let result = split_content_and_xml_tool_calls(text);
        assert!(result.is_some());
        let (content, calls) = result.unwrap();
        assert_eq!(content, "Some text");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "load");
        assert!(calls[0].1.is_empty());
    }

    #[test]
    fn test_parse_glm_style_tool_call_multiple_args() {
        let text = "Let me check.\n<tool_call>execute_typescript<arg_key>code</arg_key><arg_value>async function run() { return 1; }</arg_value><arg_key>tool_graph</arg_key><arg_value>[{\"tool\": \"shell\"}]</arg_value></tool_call>";
        let result = split_content_and_xml_tool_calls(text);
        assert!(result.is_some());
        let (content, calls) = result.unwrap();
        assert_eq!(content, "Let me check.");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "execute_typescript");
        assert_eq!(
            calls[0].1.get("code").unwrap(),
            "async function run() { return 1; }"
        );
        // tool_graph should be parsed as JSON array
        assert!(calls[0].1.get("tool_graph").unwrap().is_array());
    }

    #[test]
    fn test_extract_xml_tool_call_messages() {
        let calls = vec![(
            SHELL_TOOL.to_string(),
            serde_json::Map::from_iter(vec![("command".to_string(), json!("ls"))]),
        )];
        let msgs = extract_xml_tool_call_messages(calls, "test-id");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].id, Some("test-id".to_string()));
        match &msgs[0].content[0] {
            MessageContent::ToolRequest(req) => {
                let call = req.tool_call.as_ref().unwrap();
                assert_eq!(&*call.name, SHELL_TOOL);
                assert_eq!(
                    call.arguments.as_ref().unwrap().get("command").unwrap(),
                    "ls"
                );
            }
            _ => panic!("Expected ToolRequest"),
        }
    }

    #[test]
    fn test_split_content_and_tool_calls_with_tool() {
        let text = "Here is the result.\n{\"tool_calls\": [{\"function\": {\"name\": \"shell\", \"arguments\": \"{}\"}, \"id\": \"abc\"}]}";
        let (content, tc) = split_content_and_tool_calls(text);
        assert_eq!(content, "Here is the result.");
        assert!(tc.is_some());
        let parsed: serde_json::Value = serde_json::from_str(&tc.unwrap()).unwrap();
        assert_eq!(parsed["tool_calls"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_split_content_and_tool_calls_no_tool() {
        let text = "Just regular text, no JSON.";
        let (content, tc) = split_content_and_tool_calls(text);
        assert_eq!(content, text);
        assert!(tc.is_none());
    }

    #[test]
    fn test_split_content_and_tool_calls_json_without_tool_calls_key() {
        let text = "{\"key\": \"value\"}";
        let (content, tc) = split_content_and_tool_calls(text);
        assert_eq!(content, text);
        assert!(tc.is_none());
    }

    #[test]
    fn test_extract_tool_call_messages_openai_format() {
        let json_str = r#"{"tool_calls": [{"function": {"name": "developer__shell", "arguments": "{\"command\": \"ls\"}"}, "id": "call-1"}]}"#;
        let msgs = extract_tool_call_messages(json_str, "msg-1");
        assert_eq!(msgs.len(), 1);
        match &msgs[0].content[0] {
            MessageContent::ToolRequest(req) => {
                let call = req.tool_call.as_ref().unwrap();
                assert_eq!(&*call.name, SHELL_TOOL);
                assert_eq!(
                    call.arguments.as_ref().unwrap().get("command").unwrap(),
                    "ls"
                );
            }
            _ => panic!("Expected ToolRequest"),
        }
    }

    #[test]
    fn test_extract_tool_call_messages_native_format() {
        let json_str = r#"{"tool_calls": [{"name": "developer__shell", "arguments": {"command": "ls"}, "id": "call-2"}]}"#;
        let msgs = extract_tool_call_messages(json_str, "msg-2");
        assert_eq!(msgs.len(), 1);
        match &msgs[0].content[0] {
            MessageContent::ToolRequest(req) => {
                let call = req.tool_call.as_ref().unwrap();
                assert_eq!(&*call.name, SHELL_TOOL);
            }
            _ => panic!("Expected ToolRequest"),
        }
    }

    #[test]
    fn test_extract_tool_call_messages_invalid_json() {
        assert!(extract_tool_call_messages("not json", "msg-3").is_empty());
    }

    #[test]
    fn test_extract_tool_call_messages_empty_name_skipped() {
        let json_str = r#"{"tool_calls": [{"name": "", "arguments": {}, "id": "x"}]}"#;
        assert!(extract_tool_call_messages(json_str, "msg-4").is_empty());
    }

    #[test]
    fn test_compact_tools_json_produces_minimal_output() {
        use rmcp::model::Tool;
        use rmcp::object;

        let tools = vec![Tool::new(
            "developer__shell".to_string(),
            "Run shell commands".to_string(),
            object!({"type": "object", "properties": {"command": {"type": "string"}}}),
        )];
        let result = compact_tools_json(&tools);
        assert!(result.is_some());
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed.len(), 1);
        let func = &parsed[0]["function"];
        assert_eq!(func["name"], "developer__shell");
        assert_eq!(func["description"], "Run shell commands");
        // Should not contain full parameter schemas
        assert!(func.get("parameters").is_none());
    }

    #[test]
    fn test_compact_tools_json_empty() {
        let result = compact_tools_json(&[]);
        assert!(result.is_some());
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(parsed.is_empty());
    }

    #[test]
    fn test_safe_stream_end_balanced_braces() {
        let text = "Result: {\"key\": \"value\"} done";
        assert_eq!(safe_stream_end(text), text.len());
    }

    #[test]
    fn test_safe_stream_end_unbalanced_open_brace() {
        let text = "Some text {\"tool_calls\": [";
        assert_eq!(safe_stream_end(text), "Some text ".len());
    }

    #[test]
    fn test_safe_stream_end_empty() {
        assert_eq!(safe_stream_end(""), 0);
    }

    #[test]
    fn test_safe_stream_end_no_braces() {
        let text = "plain text here";
        assert_eq!(safe_stream_end(text), text.len());
    }
}
