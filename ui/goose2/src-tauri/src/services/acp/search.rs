use std::collections::HashSet;

use serde::Serialize;
use serde_json::{Map, Value};

use super::GooseAcpManager;

const SNIPPET_PREFIX_BYTES: usize = 40;
const SNIPPET_SUFFIX_BYTES: usize = 60;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSearchResult {
    pub session_id: String,
    pub snippet: String,
    pub message_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_role: Option<String>,
    pub match_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExportedMessage {
    id: String,
    role: Option<String>,
    searchable_texts: Vec<String>,
}

pub async fn search_sessions_via_exports(
    manager: &GooseAcpManager,
    query: &str,
    session_ids: &[String],
) -> Result<Vec<SessionSearchResult>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut seen = HashSet::new();
    let mut results = Vec::new();

    for session_id in session_ids {
        if !seen.insert(session_id.clone()) {
            continue;
        }

        let exported = manager.export_session(session_id.clone()).await?;
        if let Some(result) = search_exported_session(session_id, &exported, trimmed)? {
            results.push(result);
        }
    }

    Ok(results)
}

fn search_exported_session(
    session_id: &str,
    exported_json: &str,
    query: &str,
) -> Result<Option<SessionSearchResult>, String> {
    let root: Value = serde_json::from_str(exported_json)
        .map_err(|error| format!("Failed to parse exported session JSON: {error}"))?;
    let Some(conversation) = root.get("conversation").or_else(|| root.get("messages")) else {
        return Ok(None);
    };

    let messages = extract_messages(conversation);
    if messages.is_empty() {
        return Ok(None);
    }

    let mut first_match: Option<(String, Option<String>, String)> = None;
    let mut match_count = 0;

    for message in messages {
        for text in message.searchable_texts {
            let occurrence_count = count_occurrences(&text, query);
            if occurrence_count == 0 {
                continue;
            }

            match_count += occurrence_count;

            if first_match.is_none() {
                first_match = Some((
                    message.id.clone(),
                    message.role.clone(),
                    build_snippet(&text, query),
                ));
            }
        }
    }

    let Some((message_id, message_role, snippet)) = first_match else {
        return Ok(None);
    };

    Ok(Some(SessionSearchResult {
        session_id: session_id.to_string(),
        snippet,
        message_id,
        message_role,
        match_count,
    }))
}

fn extract_messages(value: &Value) -> Vec<ExportedMessage> {
    let mut messages = Vec::new();
    collect_messages(value, &mut messages);
    messages
}

fn collect_messages(value: &Value, messages: &mut Vec<ExportedMessage>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_messages(item, messages);
            }
        }
        Value::Object(map) => {
            if let Some(message_value) = map.get("message") {
                collect_messages(message_value, messages);
                return;
            }

            if let Some(messages_value) = map.get("messages") {
                collect_messages(messages_value, messages);
                return;
            }

            if looks_like_message(map) {
                let fallback_id = format!("message-{}", messages.len());
                if let Some(message) = extract_message(map, fallback_id) {
                    messages.push(message);
                }
            }
        }
        _ => {}
    }
}

fn looks_like_message(map: &Map<String, Value>) -> bool {
    map.contains_key("role") && (map.contains_key("content") || map.contains_key("text"))
}

fn extract_message(map: &Map<String, Value>, fallback_id: String) -> Option<ExportedMessage> {
    let role = normalize_role(map.get("role").and_then(Value::as_str));
    let mut searchable_texts = Vec::new();

    if let Some(content) = map.get("content") {
        searchable_texts.extend(extract_searchable_texts(content, role.as_deref()));
    } else if let Some(text) = map.get("text").and_then(Value::as_str) {
        if role.is_some() && !text.trim().is_empty() {
            searchable_texts.push(text.trim().to_string());
        }
    }

    if searchable_texts.is_empty() {
        return None;
    }

    Some(ExportedMessage {
        id: map
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(&fallback_id)
            .to_string(),
        role,
        searchable_texts,
    })
}

fn extract_searchable_texts(value: &Value, role: Option<&str>) -> Vec<String> {
    match value {
        Value::String(text) => role
            .filter(|supported_role| is_searchable_role(supported_role))
            .and_then(|_| normalized_text(text))
            .into_iter()
            .collect(),
        Value::Array(items) => items
            .iter()
            .flat_map(|item| extract_searchable_block_text(item, role))
            .collect(),
        Value::Object(_) => extract_searchable_block_text(value, role),
        _ => Vec::new(),
    }
}

fn extract_searchable_block_text(value: &Value, role: Option<&str>) -> Vec<String> {
    let Value::Object(map) = value else {
        return Vec::new();
    };

    let block_type = map.get("type").and_then(Value::as_str);
    let text = map.get("text").and_then(Value::as_str);

    match block_type {
        Some("text") | Some("input_text") | Some("output_text") => {
            text.and_then(normalized_text).into_iter().collect()
        }
        Some("systemNotification") | Some("system_notification") => {
            text.and_then(normalized_text).into_iter().collect()
        }
        Some("toolRequest")
        | Some("toolResponse")
        | Some("thinking")
        | Some("redactedThinking")
        | Some("reasoning")
        | Some("image") => Vec::new(),
        _ => {
            if role.is_some_and(is_searchable_role) {
                return text.and_then(normalized_text).into_iter().collect();
            }
            Vec::new()
        }
    }
}

fn normalize_role(role: Option<&str>) -> Option<String> {
    let normalized = role?.trim();
    if normalized.eq_ignore_ascii_case("user") {
        return Some("user".to_string());
    }
    if normalized.eq_ignore_ascii_case("assistant") {
        return Some("assistant".to_string());
    }
    if normalized.eq_ignore_ascii_case("system") {
        return Some("system".to_string());
    }
    None
}

fn is_searchable_role(role: &str) -> bool {
    matches!(role, "user" | "assistant" | "system")
}

fn normalized_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn count_occurrences(text: &str, query: &str) -> usize {
    let haystack = text.to_ascii_lowercase();
    let needle = query.to_ascii_lowercase();
    if needle.is_empty() {
        return 0;
    }

    let mut count = 0;
    let mut search_start = 0;

    while let Some(relative_index) = haystack[search_start..].find(&needle) {
        count += 1;
        search_start += relative_index + needle.len();
    }

    count
}

fn build_snippet(text: &str, query: &str) -> String {
    let haystack = text.to_ascii_lowercase();
    let needle = query.to_ascii_lowercase();
    let match_index = haystack.find(&needle).unwrap_or(0);

    let start = floor_char_boundary(text, match_index.saturating_sub(SNIPPET_PREFIX_BYTES));
    let end = ceil_char_boundary(
        text,
        match_index
            .saturating_add(query.len())
            .saturating_add(SNIPPET_SUFFIX_BYTES)
            .min(text.len()),
    );

    let prefix = if start > 0 { "..." } else { "" };
    let suffix = if end < text.len() { "..." } else { "" };
    let body = text.get(start..end).unwrap_or(text).trim();

    format!("{prefix}{body}{suffix}")
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn ceil_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::{build_snippet, search_exported_session, SessionSearchResult};

    #[test]
    fn finds_user_and_assistant_text_matches() {
        let exported = serde_json::json!({
            "conversation": [
                {
                    "id": "user-1",
                    "role": "user",
                    "content": [{ "type": "text", "text": "searchable user prompt" }]
                },
                {
                    "id": "assistant-1",
                    "role": "assistant",
                    "content": [{ "type": "text", "text": "assistant searchable response" }]
                }
            ]
        })
        .to_string();

        let user_result = search_exported_session("session-1", &exported, "prompt")
            .expect("search succeeds")
            .expect("user result");
        let assistant_result = search_exported_session("session-1", &exported, "response")
            .expect("search succeeds")
            .expect("assistant result");

        assert_eq!(user_result.message_id, "user-1");
        assert_eq!(user_result.message_role.as_deref(), Some("user"));
        assert_eq!(assistant_result.message_id, "assistant-1");
        assert_eq!(assistant_result.message_role.as_deref(), Some("assistant"));
    }

    #[test]
    fn includes_system_notifications() {
        let exported = serde_json::json!({
            "conversation": [
                {
                    "id": "system-1",
                    "role": "system",
                    "content": [{
                        "type": "systemNotification",
                        "text": "Compaction completed successfully"
                    }]
                }
            ]
        })
        .to_string();

        let result = search_exported_session("session-1", &exported, "completed")
            .expect("search succeeds")
            .expect("system result");

        assert_eq!(result.message_id, "system-1");
        assert_eq!(result.message_role.as_deref(), Some("system"));
    }

    #[test]
    fn skips_tool_and_reasoning_content() {
        let exported = serde_json::json!({
            "conversation": [
                {
                    "id": "assistant-1",
                    "role": "assistant",
                    "content": [
                        { "type": "toolRequest", "text": "tool request text" },
                        { "type": "toolResponse", "text": "tool response text" },
                        { "type": "thinking", "text": "private thinking" },
                        { "type": "reasoning", "text": "private reasoning" }
                    ]
                }
            ]
        })
        .to_string();

        let result =
            search_exported_session("session-1", &exported, "tool").expect("search succeeds");
        assert!(result.is_none());
    }

    #[test]
    fn skips_single_object_tool_and_reasoning_blocks() {
        let exported = serde_json::json!({
            "conversation": [
                {
                    "id": "assistant-1",
                    "role": "assistant",
                    "content": { "type": "toolResponse", "text": "tool response text" }
                },
                {
                    "id": "assistant-2",
                    "role": "assistant",
                    "content": { "type": "reasoning", "text": "private reasoning" }
                }
            ]
        })
        .to_string();

        let tool_result =
            search_exported_session("session-1", &exported, "tool").expect("search succeeds");
        let reasoning_result =
            search_exported_session("session-1", &exported, "reasoning").expect("search succeeds");

        assert!(tool_result.is_none());
        assert!(reasoning_result.is_none());
    }

    #[test]
    fn includes_single_object_text_blocks() {
        let exported = serde_json::json!({
            "conversation": [
                {
                    "id": "assistant-1",
                    "role": "assistant",
                    "content": { "type": "text", "text": "needle in a single object block" }
                }
            ]
        })
        .to_string();

        let result = search_exported_session("session-1", &exported, "needle")
            .expect("search succeeds")
            .expect("text result");

        assert_eq!(result.message_id, "assistant-1");
        assert_eq!(result.message_role.as_deref(), Some("assistant"));
    }

    #[test]
    fn counts_multiple_matches_in_one_session() {
        let exported = serde_json::json!({
            "conversation": [
                {
                    "id": "assistant-1",
                    "role": "assistant",
                    "content": [
                        { "type": "text", "text": "needle once" },
                        { "type": "text", "text": "needle twice needle" }
                    ]
                }
            ]
        })
        .to_string();

        let result = search_exported_session("session-1", &exported, "needle")
            .expect("search succeeds")
            .expect("result");

        assert_eq!(
            result,
            SessionSearchResult {
                session_id: "session-1".to_string(),
                snippet: "needle once".to_string(),
                message_id: "assistant-1".to_string(),
                message_role: Some("assistant".to_string()),
                match_count: 3,
            }
        );
    }

    #[test]
    fn builds_trimmed_snippets_around_first_match() {
        let text = "abcdefghijklmnopqrstuvwxyz0123456789prefix padding before needle and some trailing text that keeps going";
        let snippet = build_snippet(text, "needle");

        assert!(snippet.starts_with("..."));
        assert!(snippet.contains("needle and some trailing text"));
    }
}
