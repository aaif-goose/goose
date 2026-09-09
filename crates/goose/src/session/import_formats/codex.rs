//! Converter for Codex (OpenAI) `.jsonl` rollout files.
//!
//! Codex stores sessions under `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`.
//! Each line is `{"timestamp":..,"type":..,"payload":{..}}` with these
//! top-level `type`s:
//!
//! - `session_meta` — header (cwd, id, model, instructions, …)
//! - `response_item` — the real conversation: `message`, `reasoning`,
//!   `function_call`, `function_call_output`, `web_search_call`, …
//! - `event_msg` — UI events (`task_started`, `agent_message`, `web_search_end`).
//!   Redundant with `response_item`; skipped except to harvest token usage.
//! - `turn_context`, `compacted`, … — metadata, skipped.
//!
//! Assistant-side `response_item` payloads (`message` with `role:"assistant"`,
//! `reasoning`, `function_call`) reuse the existing OpenAI Responses API
//! types from `providers::formats::openai_responses` — so we get argument
//! parsing, reasoning summary handling, and schema validation for free.
//! User-side items (`message` with `role:"user"`, `function_call_output`,
//! `web_search_call`) are rollout-specific and handled locally.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};
use serde_json::{json, Map, Value};

use crate::conversation::message::Message;
use crate::conversation::Conversation;
use goose_providers::conversation::token_usage::Usage;
use goose_providers::formats::openai_responses::{ResponseOutputItem, ResponsesApiResponse};

pub fn convert(content: &str) -> Result<String> {
    let lines: Vec<Value> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect();

    if lines.is_empty() {
        return Err(anyhow!("Codex import: no parseable JSON lines"));
    }

    let meta = lines
        .iter()
        .find(|v| v.get("type").and_then(|t| t.as_str()) == Some("session_meta"))
        .and_then(|v| v.get("payload"));

    let cwd = meta
        .and_then(|m| m.get("cwd"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let session_id = meta
        .and_then(|m| m.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("imported")
        .to_string();

    let mut messages: Vec<Message> = Vec::new();
    let mut first_ts: Option<DateTime<Utc>> = None;
    let mut last_ts: Option<DateTime<Utc>> = None;
    let mut first_user_text: Option<String> = None;
    let mut total_input: i64 = 0;
    let mut total_output: i64 = 0;
    let mut total_cache_read: i64 = 0;

    for (line_idx, line) in lines.iter().enumerate() {
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

        if line_type == "event_msg" {
            if let Some(usage) = line
                .get("payload")
                .and_then(|p| p.get("usage"))
                .and_then(|u| u.as_object())
            {
                // Codex input_tokens already includes cached_input_tokens
                total_input += usage
                    .get("input_tokens")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                total_cache_read += usage
                    .get("cached_input_tokens")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                total_output += usage
                    .get("output_tokens")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
            }
            continue;
        }

        if line_type != "response_item" {
            continue;
        }
        let Some(payload) = line.get("payload") else {
            continue;
        };
        let pt = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let created = timestamp
            .map(|t| t.timestamp())
            .unwrap_or_else(|| Utc::now().timestamp());

        // First try the provider-defined Responses API types. These cover
        // assistant-side output items: `message` (role=assistant),
        // `reasoning`, and `function_call`. Unknown variants and user-side
        // items will fail to deserialize and fall through.
        if let Some(role) = payload.get("role").and_then(|v| v.as_str()) {
            if role == "developer" || role == "system" {
                continue; // harness-injected prompts, skip
            }
            if role == "user" {
                let text = collect_user_text(payload.get("content"));
                if !text.trim().is_empty() {
                    if first_user_text.is_none() && !is_context_blob(&text) {
                        first_user_text = Some(text.clone());
                    }
                    let mut msg = Message::user();
                    msg.created = created;
                    msg = msg.with_text(text);
                    messages.push(msg);
                }
                continue;
            }
        }

        if let Ok(item) = serde_json::from_value::<ResponseOutputItem>(payload.clone()) {
            // Wrap the single item in a stub `ResponsesApiResponse` so we can
            // reuse the existing decoder without duplicating its logic.
            let stub = ResponsesApiResponse {
                id: session_id.clone(),
                object: "response".to_string(),
                created_at: created,
                status: "completed".to_string(),
                model: String::new(),
                output: vec![item],
                reasoning: None,
                usage: None,
            };
            if let Ok(decoded) =
                goose_providers::formats::openai_responses::responses_api_to_message(&stub)
            {
                if !decoded.content.is_empty() {
                    let mut msg = Message::assistant();
                    msg.created = created;
                    for c in decoded.content {
                        msg.content.push(c);
                    }
                    messages.push(msg);
                    continue;
                }
            }
        }

        // Items the provider doesn't model: function_call_output,
        // web_search_call.
        match pt {
            "function_call_output" => {
                let call_id = payload
                    .get("call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let output = payload
                    .get("output")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or_default();
                let mut msg = Message::user();
                msg.created = created;
                msg = msg.with_tool_response(
                    call_id,
                    Ok(CallToolResult::success(vec![ContentBlock::text(output)])),
                );
                messages.push(msg);
            }
            "web_search_call" => {
                let action = payload.get("action");
                let query = action
                    .and_then(|a| a.get("query"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let url = action
                    .and_then(|a| a.get("url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let mut args = Map::new();
                if !query.is_empty() {
                    args.insert("query".into(), json!(query));
                }
                if !url.is_empty() {
                    args.insert("url".into(), json!(url));
                }
                let id = format!("codex_websearch_{}", line_idx);
                let params =
                    CallToolRequestParams::new("web_search".to_string()).with_arguments(args);
                let mut req = Message::assistant();
                req.created = created;
                req = req.with_tool_request(id.clone(), Ok(params));
                messages.push(req);

                let status = payload
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("completed");
                let mut resp = Message::user();
                resp.created = created;
                resp = resp.with_tool_response(
                    id,
                    Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                        "[web_search {}]",
                        status
                    ))])),
                );
                messages.push(resp);
            }
            _ => {}
        }
    }

    messages.retain(|m| !m.content.is_empty());

    let working_dir = if cwd.is_empty() {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string())
    } else {
        cwd
    };

    let name = first_user_text
        .as_deref()
        .map(super::summarize_first_line)
        .unwrap_or_else(|| format!("Imported Codex session {}", session_id));

    let created_at = first_ts.unwrap_or_else(Utc::now);
    let updated_at = last_ts.unwrap_or(created_at);
    let conversation = Conversation::new_unvalidated(messages);

    let session_json = super::build_session_json(super::ImportedSession {
        session_id: &session_id,
        working_dir: &working_dir,
        name: &name,
        created_at,
        updated_at,
        usage: Usage::new(
            (total_input > 0).then_some(total_input as i32),
            (total_output > 0).then_some(total_output as i32),
            None,
        )
        .with_cache_tokens(
            (total_cache_read > 0).then_some(total_cache_read as i32),
            None,
        ),
        accumulated_cost: None,
        conversation,
    });

    serde_json::to_string_pretty(&session_json).map_err(Into::into)
}

fn collect_user_text(content: Option<&Value>) -> String {
    let Some(Value::Array(blocks)) = content else {
        return content.and_then(|v| v.as_str()).unwrap_or("").to_string();
    };
    let mut parts = Vec::new();
    for block in blocks {
        let bt = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if matches!(bt, "input_text" | "text" | "output_text") {
            if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                parts.push(t.to_string());
            }
        }
    }
    parts.join("\n")
}

/// Heuristic: Codex's first "user" message is often a giant
/// `<environment_context>` / AGENTS.md blob injected by the harness rather than
/// the user's actual prompt. We still preserve it in the transcript, but it's
/// a bad source for the session name.
fn is_context_blob(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("<environment_context>")
        || t.starts_with("<app-context>")
        || t.starts_with("<permissions instructions>")
        || t.starts_with("# AGENTS.md")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_developer_and_system_messages() {
        let jsonl = r#"{"timestamp":"2026-05-22T13:37:22.526Z","type":"session_meta","payload":{"id":"abc","cwd":"/tmp"}}
{"timestamp":"2026-05-22T13:37:23.000Z","type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"<huge system prompt>"}]}}
{"timestamp":"2026-05-22T13:37:23.946Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"the real question"}]}}"#;

        let json = convert(jsonl).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["message_count"], 1);
        assert_eq!(v["name"], "the real question");
    }

    #[test]
    fn converts_function_call_and_output() {
        let jsonl = r#"{"timestamp":"2026-05-22T13:37:22Z","type":"session_meta","payload":{"id":"s","cwd":"/w"}}
{"timestamp":"2026-05-22T13:37:23Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"run ls"}]}}
{"timestamp":"2026-05-22T13:37:24Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"ls\"}","call_id":"call_1"}}
{"timestamp":"2026-05-22T13:37:25Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call_1","output":"file.txt\n"}}"#;

        let json = convert(jsonl).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        let msgs = v["conversation"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        // assistant message with a tool request, decoded via the provider
        // crate so arguments-as-JSON-string is parsed automatically
        let req_block = msgs[1]["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["type"] == "toolRequest")
            .expect("expected a toolRequest");
        assert_eq!(req_block["toolCall"]["status"], "success");
        assert_eq!(req_block["toolCall"]["value"]["arguments"]["cmd"], "ls");
        // user message with the tool response
        assert!(msgs[2]["content"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["type"] == "toolResponse"));
    }

    #[test]
    fn sanitizes_unicode_tags_in_function_call_output() {
        let jsonl = [
            serde_json::json!({
                "timestamp": "2026-05-22T13:37:22Z",
                "type": "session_meta",
                "payload": {"id": "s", "cwd": "/w"}
            })
            .to_string(),
            serde_json::json!({
                "timestamp": "2026-05-22T13:37:23Z",
                "type": "response_item",
                "payload": {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": "visible\u{E0041}世界"
                }
            })
            .to_string(),
        ]
        .join("\n");

        let json = convert(&jsonl).unwrap();

        assert!(json.contains("visible世界"));
        assert!(!json.contains('\u{E0041}'));
    }

    #[test]
    fn first_user_text_skips_context_blobs() {
        let jsonl = r#"{"timestamp":"2026-05-22T13:37:22Z","type":"session_meta","payload":{"id":"s","cwd":"/w"}}
{"timestamp":"2026-05-22T13:37:23Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>\n  <cwd>/w</cwd>\n</environment_context>"}]}}
{"timestamp":"2026-05-22T13:37:24Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"actual prompt"}]}}"#;
        let json = convert(jsonl).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["name"], "actual prompt");
        assert_eq!(v["message_count"], 2);
    }
}
