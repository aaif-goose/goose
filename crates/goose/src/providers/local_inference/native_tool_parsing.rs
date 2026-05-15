use crate::conversation::message::{Message, MessageContent};
use crate::providers::errors::ProviderError;
use crate::providers::utils::{is_valid_function_name, safely_parse_json};
use rmcp::model::{object, CallToolRequestParams, ErrorCode, ErrorData};
use serde_json::{json, Value};
use std::borrow::Cow;
use uuid::Uuid;

pub(crate) fn message_from_native_tool_text(
    generated_text: &str,
    message_id: &str,
) -> Result<Option<Message>, ProviderError> {
    let mut content = Vec::new();

    if let Some(message) = parse_openai_message_json(generated_text) {
        append_text(&mut content, message.get("content"));
        append_tool_calls(&mut content, message.get("tool_calls"));
    } else if let Some(tool_calls) = parse_tool_calls_json(generated_text) {
        append_tool_calls(&mut content, Some(&tool_calls));
    } else if generated_text.contains("<function=") {
        let (prefix, tool_calls) = parse_xml_tool_calls(generated_text);
        if let Some(prefix) = prefix {
            content.push(MessageContent::text(prefix));
        }
        content.extend(tool_calls);
    } else {
        return Ok(None);
    }

    if content
        .iter()
        .any(|content| matches!(content, MessageContent::ToolRequest(_)))
    {
        let mut message = Message::new(
            rmcp::model::Role::Assistant,
            chrono::Utc::now().timestamp(),
            content,
        );
        message.id = Some(message_id.to_string());
        Ok(Some(message))
    } else {
        Ok(None)
    }
}

fn parse_openai_message_json(generated_text: &str) -> Option<Value> {
    json_candidates(generated_text).into_iter().find(|value| {
        value
            .get("tool_calls")
            .and_then(|tool_calls| tool_calls.as_array())
            .is_some_and(|tool_calls| !tool_calls.is_empty())
    })
}

fn parse_tool_calls_json(generated_text: &str) -> Option<Value> {
    for value in json_candidates(generated_text) {
        if value.as_array().is_some() {
            return Some(value);
        }
        if let Some(tool_calls) = value.get("tool_calls") {
            return Some(tool_calls.clone());
        }
        if value.get("name").is_some() || value.get("function").is_some() {
            return Some(Value::Array(vec![value]));
        }
    }
    None
}

fn json_candidates(text: &str) -> Vec<Value> {
    let mut candidates = Vec::new();
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        candidates.push(value);
    }

    for (open, close) in [('{', '}'), ('[', ']')] {
        let starts = text.match_indices(open).map(|(idx, _)| idx);
        for start in starts {
            let mut depth = 0i32;
            let mut in_string = false;
            let mut escaped = false;
            for (offset, ch) in text[start..].char_indices() {
                if escaped {
                    escaped = false;
                    continue;
                }
                if ch == '\\' && in_string {
                    escaped = true;
                    continue;
                }
                if ch == '"' {
                    in_string = !in_string;
                    continue;
                }
                if in_string {
                    continue;
                }
                if ch == open {
                    depth += 1;
                } else if ch == close {
                    depth -= 1;
                    if depth == 0 {
                        let end = start + offset + ch.len_utf8();
                        if let Ok(value) = serde_json::from_str::<Value>(&text[start..end]) {
                            candidates.push(value);
                        }
                        break;
                    }
                }
            }
        }
    }

    candidates
}

fn append_text(content: &mut Vec<MessageContent>, value: Option<&Value>) {
    if let Some(text) = value.and_then(|value| value.as_str()) {
        if !text.is_empty() {
            content.push(MessageContent::text(text));
        }
    }
}

fn append_tool_calls(content: &mut Vec<MessageContent>, value: Option<&Value>) {
    let Some(tool_calls) = value.and_then(|value| value.as_array()) else {
        return;
    };

    for tool_call in tool_calls {
        content.push(tool_call_content(tool_call));
    }
}

fn tool_call_content(tool_call: &Value) -> MessageContent {
    let id = tool_call
        .get("id")
        .and_then(|id| id.as_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let function = tool_call.get("function").unwrap_or(tool_call);
    let name = function
        .get("name")
        .and_then(|name| name.as_str())
        .unwrap_or_default()
        .to_string();

    if !is_valid_function_name(&name) {
        return MessageContent::tool_request(
            id,
            Err(ErrorData {
                code: ErrorCode::INVALID_REQUEST,
                message: Cow::from(format!(
                    "The provided function name '{}' had invalid characters, it must match this regex [a-zA-Z0-9_-]+",
                    name
                )),
                data: None,
            }),
        );
    }

    let arguments = function
        .get("arguments")
        .or_else(|| tool_call.get("arguments"))
        .cloned()
        .unwrap_or_else(|| json!({}));

    let raw_arguments = arguments.to_string();
    let parsed = match arguments {
        Value::String(arguments) if arguments.trim().is_empty() => Ok(json!({})),
        Value::String(arguments) => safely_parse_json(&arguments),
        Value::Object(_) => Ok(arguments),
        Value::Null => Ok(json!({})),
        other => Ok(other),
    };

    match parsed {
        Ok(params) => MessageContent::tool_request(
            id,
            Ok(CallToolRequestParams::new(name).with_arguments(object(params))),
        ),
        Err(error) => {
            let message = format!(
                "Could not interpret tool use parameters for id {}: {}. Raw arguments: '{}'",
                id, error, raw_arguments
            );
            MessageContent::tool_request(
                id,
                Err(ErrorData {
                    code: ErrorCode::INVALID_PARAMS,
                    message: Cow::from(message),
                    data: None,
                }),
            )
        }
    }
}

fn parse_xml_tool_calls(content: &str) -> (Option<String>, Vec<MessageContent>) {
    let function_re = regex::Regex::new(r"<function=([^>]+)>([\s\S]*?)</function>").unwrap();
    let param_re = regex::Regex::new(r"<parameter=([^>]+)>([\s\S]*?)</parameter>").unwrap();

    let prefix = content
        .find("<function=")
        .and_then(|idx| content.get(..idx))
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string);

    let mut tool_calls = Vec::new();
    for func_cap in function_re.captures_iter(content) {
        let function_name = func_cap[1].trim().to_string();
        let function_body = &func_cap[2];
        let mut arguments = serde_json::Map::new();
        for param_cap in param_re.captures_iter(function_body) {
            arguments.insert(
                param_cap[1].trim().to_string(),
                Value::String(param_cap[2].trim().to_string()),
            );
        }
        tool_calls.push(tool_call_content(&json!({
            "function": {
                "name": function_name,
                "arguments": arguments,
            }
        })));
    }

    (prefix, tool_calls)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_count(message: &Message) -> usize {
        message
            .content
            .iter()
            .filter(|content| matches!(content, MessageContent::ToolRequest(_)))
            .count()
    }

    #[test]
    fn parses_openai_message_tool_calls() {
        let text = r#"{"content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"developer__shell","arguments":"{\"command\":\"pwd\"}"}}]}"#;
        let message = message_from_native_tool_text(text, "msg").unwrap().unwrap();
        assert_eq!(tool_count(&message), 1);
    }

    #[test]
    fn parses_top_level_tool_calls() {
        let text = r#"{"tool_calls":[{"name":"developer__shell","arguments":{"command":"pwd"}}]}"#;
        let message = message_from_native_tool_text(text, "msg").unwrap().unwrap();
        assert_eq!(tool_count(&message), 1);
    }

    #[test]
    fn parses_xml_tool_calls() {
        let text = r#"<function=developer__shell><parameter=command>pwd</parameter></function>"#;
        let message = message_from_native_tool_text(text, "msg").unwrap().unwrap();
        assert_eq!(tool_count(&message), 1);
    }
}
