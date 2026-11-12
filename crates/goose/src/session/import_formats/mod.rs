//! Importers for non-goose session formats.
//!
//! Goose's native session export is a JSON-serialized [`crate::session::Session`].
//! These submodules let users also import sessions exported by other coding
//! agents — currently:
//!
//! - **Claude Code** (`.jsonl` files under `~/.claude/projects/...`)
//! - **Codex** (`.jsonl` rollouts under `~/.codex/sessions/YYYY/MM/DD/...`)
//! - **Pi** (`.jsonl` files under `~/.pi/agent/sessions/...`)
//!
//! The strategy is to convert any supported foreign format into goose's
//! native [`Session`] JSON, then hand it off to the existing
//! `SessionManager::import_session` pipeline.

use anyhow::Result;
use chrono::{DateTime, Utc};
use oi_kit_providers::conversation::token_usage::Usage;
use serde_json::{json, Map, Value};

use crate::conversation::Conversation;

pub mod claude_code;
pub mod codex;
pub mod pi;

/// Session-level fields harvested from a foreign transcript, used to build
/// the goose-native session JSON handed to `SessionManager::import_session`.
pub(crate) struct ImportedSession<'a> {
    pub session_id: &'a str,
    pub working_dir: &'a str,
    pub name: &'a str,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub usage: Usage,
    pub accumulated_cost: Option<f64>,
    pub conversation: Conversation,
}

pub(crate) fn build_session_json(session: ImportedSession) -> Value {
    let usage = serde_json::to_value(session.usage).unwrap();
    let mut obj = Map::new();
    obj.insert("id".into(), json!(session.session_id));
    obj.insert("working_dir".into(), json!(session.working_dir));
    obj.insert("name".into(), json!(session.name));
    obj.insert("user_set_name".into(), json!(false));
    obj.insert("session_type".into(), json!("user"));
    obj.insert("created_at".into(), json!(session.created_at.to_rfc3339()));
    obj.insert("updated_at".into(), json!(session.updated_at.to_rfc3339()));
    obj.insert("extension_data".into(), json!({}));
    obj.insert("usage".into(), usage.clone());
    obj.insert("accumulated_usage".into(), usage);
    obj.insert("accumulated_cost".into(), json!(session.accumulated_cost));
    obj.insert("schedule_id".into(), json!(null));
    obj.insert("recipe".into(), json!(null));
    obj.insert("user_recipe_values".into(), json!(null));
    obj.insert(
        "conversation".into(),
        serde_json::to_value(&session.conversation).unwrap(),
    );
    obj.insert(
        "message_count".into(),
        json!(session.conversation.messages().len()),
    );
    obj.insert("provider_name".into(), json!(null));
    obj.insert("model_config".into(), json!(null));
    obj.insert("goose_mode".into(), json!("auto"));
    obj.insert("archived_at".into(), json!(null));
    obj.insert("project_id".into(), json!(null));
    Value::Object(obj)
}

/// Detected import source format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    /// Native goose session export — a JSON object representing a `Session`.
    Goose,
    /// Claude Code `.jsonl` transcript (one JSON object per line, no header).
    ClaudeCode,
    /// Codex (OpenAI) `.jsonl` rollout file. First line is `{"type":"session_meta",...}`.
    Codex,
    /// Pi-mono `.jsonl` transcript (first line is `{"type":"session",...}` header).
    Pi,
}

/// Sniff the format of an import payload.
///
/// We peek at the first non-blank line:
/// - If it parses as a JSON object whose top-level has `working_dir`/`workingDir`
///   and a `conversation` (or `messages`) field, it's goose.
/// - If the *first* line is `{"type":"session", ...}` it's pi.
/// - If it's a JSON-Lines stream with per-line `type` fields like
///   `user`/`assistant`/`attachment`, it's Claude Code.
pub fn detect_format(content: &str) -> ImportFormat {
    let first_line = content.lines().find(|l| !l.trim().is_empty()).unwrap_or("");

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(first_line) {
        // Codex rollouts always start with `{"type":"session_meta",...}`.
        if v.get("type").and_then(|t| t.as_str()) == Some("session_meta") {
            return ImportFormat::Codex;
        }
        // Pi sessions start with a `{"type":"session",...}` header. Older
        // fixtures lack `version` but always have `cwd` + `id`.
        if v.get("type").and_then(|t| t.as_str()) == Some("session")
            && (v.get("version").is_some() || (v.get("cwd").is_some() && v.get("id").is_some()))
        {
            return ImportFormat::Pi;
        }
        // Claude Code lines always include a sessionId; goose's native JSON is
        // a single multi-line object whose first *parsed* line is `{` only.
        if v.is_object()
            && v.get("sessionId").is_some()
            && (v.get("type").is_some() || v.get("uuid").is_some())
        {
            return ImportFormat::ClaudeCode;
        }
    }

    // Goose's pretty-printed export starts with `{` and *eventually* contains
    // a full Session object — try to parse the entire payload.
    if serde_json::from_str::<serde_json::Value>(content)
        .ok()
        .and_then(|v| {
            v.get("working_dir")
                .or_else(|| v.get("workingDir"))
                .cloned()
        })
        .is_some()
    {
        return ImportFormat::Goose;
    }

    // Fallback: if every non-blank line is a JSON object with a `type` and
    // a `sessionId`, treat it as Claude Code.
    let mut saw_claude_marker = false;
    for line in content.lines().filter(|l| !l.trim().is_empty()).take(5) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if v.get("sessionId").is_some() {
                saw_claude_marker = true;
            }
        }
    }
    if saw_claude_marker {
        return ImportFormat::ClaudeCode;
    }

    ImportFormat::Goose
}

/// Convert any supported foreign format to a goose-native session JSON string.
pub fn convert_to_goose_session_json(content: &str) -> Result<String> {
    match detect_format(content) {
        ImportFormat::Goose => Ok(upgrade_legacy_token_fields(content)),
        ImportFormat::ClaudeCode => claude_code::convert(content),
        ImportFormat::Codex => codex::convert(content),
        ImportFormat::Pi => pi::convert(content),
    }
}

/// Exports from goose versions before `usage`/`accumulated_usage` existed
/// store token counts as flat fields.
fn upgrade_legacy_token_fields(content: &str) -> String {
    let Ok(Value::Object(mut obj)) = serde_json::from_str::<Value>(content) else {
        return content.to_string();
    };
    nest_legacy_token_fields(&mut obj);
    Value::Object(obj).to_string()
}

/// Fold pre-`usage` flat token counts into nested `usage`/`accumulated_usage`
/// objects. Shared by the export-import path and the first-run JSONL migration.
pub(crate) fn nest_legacy_token_fields(obj: &mut Map<String, Value>) {
    let nest = |obj: &Map<String, Value>, keys: [(&str, &str); 5]| -> Value {
        Value::Object(
            keys.iter()
                .map(|(from, to)| {
                    (
                        to.to_string(),
                        obj.get(*from).cloned().unwrap_or(Value::Null),
                    )
                })
                .collect(),
        )
    };

    if !obj.contains_key("usage") {
        let usage = nest(
            obj,
            [
                ("input_tokens", "input_tokens"),
                ("output_tokens", "output_tokens"),
                ("total_tokens", "total_tokens"),
                ("cache_read_tokens", "cache_read_input_tokens"),
                ("cache_write_tokens", "cache_write_input_tokens"),
            ],
        );
        obj.insert("usage".into(), usage);
    }
    if !obj.contains_key("accumulated_usage") {
        let accumulated = nest(
            obj,
            [
                ("accumulated_input_tokens", "input_tokens"),
                ("accumulated_output_tokens", "output_tokens"),
                ("accumulated_total_tokens", "total_tokens"),
                ("accumulated_cache_read_tokens", "cache_read_input_tokens"),
                ("accumulated_cache_write_tokens", "cache_write_input_tokens"),
            ],
        );
        obj.insert("accumulated_usage".into(), accumulated);
    }
}

/// Squeeze a string down to a short session-name candidate: take the first
/// non-empty line and cap it at ~80 chars.
pub(crate) fn summarize_first_line(s: &str) -> String {
    let line = s.lines().find(|l| !l.trim().is_empty()).unwrap_or(s).trim();
    if line.chars().count() <= 80 {
        line.to_string()
    } else {
        let truncated: String = line.chars().take(77).collect();
        format!("{}...", truncated)
    }
}
