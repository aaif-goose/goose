//! Converter for Claude Code `.jsonl` transcript files.
//!
//! Claude Code stores each session as a JSON-Lines file under
//! `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`. Every line is a typed
//! event; the ones we care about are `user`, `assistant`, and `ai-title`.
//! Most other lines (attachments, queue operations, internal hooks) are
//! transcript noise and are skipped.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, ErrorCode, ErrorData};
use serde_json::Value;

use crate::conversation::message::Message;
use crate::conversation::Conversation;
use goose_providers::conversation::token_usage::Usage;

pub fn convert(content: &str) -> Result<String> {
    let lines: Vec<Value> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect();

    if lines.is_empty() {
        return Err(anyhow!("Claude Code import: no parseable JSON lines"));
    }

    let cwd = lines
        .iter()
        .find_map(|l| l.get("cwd").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();

    let session_id = lines
        .iter()
        .find_map(|l| l.get("sessionId").and_then(|v| v.as_str()))
        .unwrap_or("imported")
        .to_string();

    let ai_title = lines.iter().find_map(|l| {
        if l.get("type").and_then(|v| v.as_str()) == Some("ai-title") {
            l.get("aiTitle")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        } else {
            None
        }
    });

    let mut messages: Vec<Message> = Vec::new();
    let mut total_input: i64 = 0;
    let mut total_output: i64 = 0;
    let mut total_cache_read: i64 = 0;
    let mut total_cache_write: i64 = 0;
    let mut first_ts: Option<DateTime<Utc>> = None;
    let mut last_ts: Option<DateTime<Utc>> = None;
    let mut first_user_text: Option<String> = None;

    for line in &lines {
        let line_type = line.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let timestamp = line
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        if let Some(ts) = timestamp {
            first_ts.get_or_insert(ts);
            last_ts = Some(ts);
        }

        match line_type {
            "user" => {
                if let Some(msg) = convert_user_message(line, timestamp) {
                    if first_user_text.is_none() {
                        first_user_text = extract_first_text(&msg);
                    }
                    messages.push(msg);
                }
            }
            "assistant" => {
                if let Some(msg) = convert_assistant_message(line, timestamp) {
                    if let Some(usage) = line
                        .get("message")
                        .and_then(|m| m.get("usage"))
                        .and_then(|u| u.as_object())
                    {
                        let cache_write = usage
                            .get("cache_creation_input_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        let cache_read = usage
                            .get("cache_read_input_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        total_input += usage
                            .get("input_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        total_input += cache_write + cache_read;
                        total_cache_write += cache_write;
                        total_cache_read += cache_read;
                        total_output += usage
                            .get("output_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                    }
                    messages.push(msg);
                }
            }
            _ => {} // attachments, ai-title, queue-operation, etc.
        }
    }

    let name = ai_title
        .or_else(|| first_user_text.as_deref().map(super::summarize_first_line))
        .unwrap_or_else(|| format!("Imported Claude Code session {}", session_id));

    let working_dir = if cwd.is_empty() {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string())
    } else {
        cwd
    };

    let created_at = first_ts.unwrap_or_else(Utc::now);
    let updated_at = last_ts.unwrap_or(created_at);

    let conversation = Conversation::new_unvalidated(messages);

    let session_json = super::build_session_json(super::ImportedSession {
        session_id: &session_id,
        working_dir: &working_dir,
        name: &name,
        created_at,
        updated_at,
        usage: Usage::new(Some(total_input as i32), Some(total_output as i32), None)
            .with_cache_tokens(
                (total_cache_read > 0).then_some(total_cache_read as i32),
                (total_cache_write > 0).then_some(total_cache_write as i32),
            ),
        accumulated_cost: None,
        conversation,
    });

    serde_json::to_string_pretty(&session_json).map_err(Into::into)
}

fn convert_user_message(line: &Value, timestamp: Option<DateTime<Utc>>) -> Option<Message> {
    let content = line.get("message")?.get("content")?;
    let created = timestamp
        .map(|t| t.timestamp())
        .unwrap_or_else(|| Utc::now().timestamp());

    // Tool results in Claude Code live inside `user` messages with role=user
    // and content blocks of type=tool_result. Goose models tool responses the
    // same way (on a user-role message), so this maps cleanly.
    let mut msg = Message::user();
    msg.created = created;

    match content {
        Value::String(s) => {
            msg = msg.with_text(s.clone());
        }
        Value::Array(blocks) => {
            for block in blocks {
                let bt = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match bt {
                    "text" => {
                        if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                            msg = msg.with_text(t);
                        }
                    }
                    "tool_result" => {
                        let id = block
                            .get("tool_use_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let is_error = block
                            .get("is_error")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let result = build_tool_result(block.get("content"), is_error);
                        msg = msg.with_tool_response(id, result);
                    }
                    "image" => {
                        if let (Some(data), Some(mime)) = (
                            block
                                .get("source")
                                .and_then(|s| s.get("data"))
                                .and_then(|v| v.as_str()),
                            block
                                .get("source")
                                .and_then(|s| s.get("media_type"))
                                .and_then(|v| v.as_str()),
                        ) {
                            msg = msg.with_image(data, mime);
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    if msg.content.is_empty() {
        return None;
    }
    Some(msg)
}

fn convert_assistant_message(line: &Value, timestamp: Option<DateTime<Utc>>) -> Option<Message> {
    let content = line.get("message")?.get("content")?.as_array()?;
    let created = timestamp
        .map(|t| t.timestamp())
        .unwrap_or_else(|| Utc::now().timestamp());

    let mut msg = Message::assistant();
    msg.created = created;

    for block in content {
        let bt = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match bt {
            "text" => {
                if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                    if !t.is_empty() {
                        msg = msg.with_text(t);
                    }
                }
            }
            "thinking" => {
                let t = block.get("thinking").and_then(|v| v.as_str()).unwrap_or("");
                let sig = block
                    .get("signature")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !t.is_empty() {
                    msg = msg.with_thinking(t, sig);
                }
            }
            "tool_use" => {
                let id = block
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown_tool");
                let args = block
                    .get("input")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();
                let params = CallToolRequestParams::new(name.to_string()).with_arguments(args);
                msg = msg.with_tool_request(id, Ok(params));
            }
            _ => {}
        }
    }

    if msg.content.is_empty() {
        return None;
    }
    Some(msg)
}

fn build_tool_result(content: Option<&Value>, is_error: bool) -> Result<CallToolResult, ErrorData> {
    let text = match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| {
                let bt = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match bt {
                    "text" => b.get("text").and_then(|v| v.as_str()).map(str::to_string),
                    "tool_reference" => b
                        .get("tool_name")
                        .and_then(|v| v.as_str())
                        .map(|n| format!("[tool_reference: {}]", n)),
                    _ => Some(serde_json::to_string(b).unwrap_or_default()),
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
        None => String::new(),
    };

    if is_error {
        Err(ErrorData::new(ErrorCode::INTERNAL_ERROR, text, None))
    } else {
        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }
}

fn extract_first_text(msg: &Message) -> Option<String> {
    use crate::conversation::message::MessageContent;
    for c in &msg.content {
        if let MessageContent::Text(t) = c {
            return Some(t.text.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_tool_use_and_result() {
        let jsonl = r#"{"type":"user","sessionId":"s","uuid":"u1","timestamp":"2026-01-01T00:00:00.000Z","cwd":"/tmp","message":{"role":"user","content":"do it"}}
{"type":"assistant","sessionId":"s","uuid":"u2","timestamp":"2026-01-01T00:00:01.000Z","cwd":"/tmp","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"bash","input":{"command":"ls"}}]}}
{"type":"user","sessionId":"s","uuid":"u3","timestamp":"2026-01-01T00:00:02.000Z","cwd":"/tmp","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":[{"type":"text","text":"file.txt"}]}]}}"#;

        let json = convert(jsonl).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        let msgs = v["conversation"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        // assistant message should contain a toolRequest
        let assistant = &msgs[1];
        let content = assistant["content"].as_array().unwrap();
        assert!(content.iter().any(|c| c["type"] == "toolRequest"));
        // user response should contain a toolResponse
        let resp = &msgs[2];
        let content = resp["content"].as_array().unwrap();
        assert!(content.iter().any(|c| c["type"] == "toolResponse"));
    }

    #[test]
    fn sanitizes_unicode_tags_in_tool_result() {
        let jsonl = serde_json::json!({
            "type": "user",
            "sessionId": "s",
            "uuid": "u1",
            "timestamp": "2026-01-01T00:00:00Z",
            "cwd": "/tmp",
            "message": {
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "toolu_1",
                    "content": [{"type": "text", "text": "visible\u{E0041}世界"}]
                }]
            }
        })
        .to_string();

        let json = convert(&jsonl).unwrap();

        assert!(json.contains("visible世界"));
        assert!(!json.contains('\u{E0041}'));
    }

    #[test]
    fn sanitizes_unicode_tags_in_tool_result_error() {
        let jsonl = serde_json::json!({
            "type": "user",
            "sessionId": "s",
            "uuid": "u1",
            "timestamp": "2026-01-01T00:00:00Z",
            "cwd": "/tmp",
            "message": {
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "toolu_1",
                    "is_error": true,
                    "content": "failed\u{E0041}café"
                }]
            }
        })
        .to_string();

        let json = convert(&jsonl).unwrap();

        assert!(json.contains("failedcafé"));
        assert!(!json.contains('\u{E0041}'));
    }

    #[test]
    fn emits_cache_token_breakdown() {
        let jsonl = r#"{"type":"user","sessionId":"s","uuid":"u1","timestamp":"2026-01-01T00:00:01Z","cwd":"/tmp","message":{"role":"user","content":"hi"}}
{"type":"assistant","sessionId":"s","uuid":"u2","timestamp":"2026-01-01T00:00:02Z","message":{"role":"assistant","content":[{"type":"text","text":"hello"}],"usage":{"input_tokens":7,"cache_creation_input_tokens":1000,"cache_read_input_tokens":5000,"output_tokens":50}}}"#;
        let json = convert(jsonl).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["usage"]["input_tokens"], 6007); // 7 + 1000 + 5000
        assert_eq!(v["usage"]["output_tokens"], 50);
        assert_eq!(v["usage"]["cache_read_input_tokens"], 5000);
        assert_eq!(v["usage"]["cache_write_input_tokens"], 1000);
        assert_eq!(v["accumulated_usage"]["cache_read_input_tokens"], 5000);
        assert_eq!(v["accumulated_usage"]["cache_write_input_tokens"], 1000);
    }

    #[test]
    fn skips_unknown_lines() {
        let jsonl = r#"{"type":"attachment","sessionId":"s","uuid":"u0","timestamp":"2026-01-01T00:00:00Z"}
{"type":"queue-operation","sessionId":"s","timestamp":"2026-01-01T00:00:00Z"}
{"type":"user","sessionId":"s","uuid":"u1","timestamp":"2026-01-01T00:00:01Z","cwd":"/tmp","message":{"role":"user","content":"hi"}}"#;
        let json = convert(jsonl).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["message_count"], 1);
    }
}
