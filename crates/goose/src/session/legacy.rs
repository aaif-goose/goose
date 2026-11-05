use crate::conversation::message::MessageMetadata;
use crate::conversation::Conversation;
use crate::session::Session;
use anyhow::Result;
use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};
use std::fs;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;

pub fn list_sessions(session_dir: &PathBuf) -> Result<Vec<(String, PathBuf)>> {
    let entries = fs::read_dir(session_dir)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "jsonl") {
                let name = path.file_stem()?.to_string_lossy().to_string();
                Some((name, path))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    Ok(entries)
}

pub fn load_session(session_name: &str, session_path: &Path) -> Result<Session> {
    let file = fs::File::open(session_path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to open session file {}: {}",
            session_path.display(),
            e
        )
    })?;

    let file_metadata = file.metadata()?;

    if file_metadata.len() > MAX_FILE_SIZE {
        return Err(anyhow::anyhow!("Session file too large"));
    }
    if file_metadata.len() == 0 {
        return Err(anyhow::anyhow!("Empty session file"));
    }

    let modified_time = file_metadata.modified().unwrap_or(SystemTime::now());
    let created_time = file_metadata
        .created()
        .unwrap_or_else(|_| parse_session_timestamp(session_name).unwrap_or(modified_time));

    let reader = io::BufReader::new(file);
    let mut lines = reader.lines();
    let mut messages = Vec::new();
    let mut session = Session {
        id: session_name.to_string(),
        ..Default::default()
    };

    if let Some(Ok(line)) = lines.next() {
        let mut metadata_json: serde_json::Value = serde_json::from_str(&line)
            .map_err(|_| anyhow::anyhow!("Invalid session metadata JSON"))?;

        if let Some(obj) = metadata_json.as_object_mut() {
            obj.entry("id").or_insert(serde_json::json!(session_name));
            obj.entry("created_at")
                .or_insert(serde_json::json!(DateTime::<Utc>::from(created_time)));
            obj.entry("updated_at")
                .or_insert(serde_json::json!(DateTime::<Utc>::from(modified_time)));
            obj.entry("extension_data").or_insert(serde_json::json!({}));
            obj.entry("message_count").or_insert(serde_json::json!(0));
            obj.entry("working_dir").or_insert(serde_json::json!(""));

            crate::session::import_formats::nest_legacy_token_fields(obj);

            if let Some(desc) = obj.get_mut("description") {
                if let Some(desc_str) = desc.as_str() {
                    *desc = serde_json::json!(desc_str
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" "));
                }
            }
        }
        session = serde_json::from_value(metadata_json)?;
        session.id = session_name.to_string();
    }

    for line in lines.map_while(Result::ok) {
        if let Ok(mut message_json) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(obj) = message_json.as_object_mut() {
                obj.entry("metadata")
                    .or_insert(serde_json::to_value(MessageMetadata::default())?);
            }
            if let Ok(message) = serde_json::from_value(message_json) {
                messages.push(message);
            }
        }
    }

    if !messages.is_empty() {
        session.conversation = Some(Conversation::new_unvalidated(messages));
    }

    Ok(session)
}

fn parse_session_timestamp(session_name: &str) -> Option<SystemTime> {
    NaiveDateTime::parse_from_str(session_name, "%Y%m%d_%H%M%S")
        .ok()
        .and_then(|dt| Local.from_local_datetime(&dt).single())
        .map(SystemTime::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose_provider_types::data::Role;
    use tempfile::TempDir;

    #[test]
    fn test_load_legacy_session_without_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let session_path = temp_dir.path().join("20240101_120000.jsonl");

        let legacy_content = r#"{"description":"test","id":"20240101_120000","created_at":"2024-01-01T12:00:00Z","updated_at":"2024-01-01T12:00:00Z","extension_data":{},"message_count":0}
{"id":"msg1","role":"user","created":1704110400,"content":[{"type":"text","text":"Hello"}]}
{"id":"msg2","role":"assistant","created":1704110401,"content":[{"type":"text","text":"Hi there"}]}"#;

        fs::write(&session_path, legacy_content).unwrap();

        let session = load_session("20240101_120000", &session_path).unwrap();

        assert_eq!(session.id, "20240101_120000");
        let conversation = session.conversation.as_ref().unwrap();
        let messages = conversation.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[1].role, Role::Assistant);
    }

    #[test]
    fn test_load_legacy_session_preserves_flat_token_fields() {
        let temp_dir = TempDir::new().unwrap();
        let session_path = temp_dir.path().join("20240101_120000.jsonl");

        let legacy_content = r#"{"description":"test","id":"20240101_120000","created_at":"2024-01-01T12:00:00Z","updated_at":"2024-01-01T12:00:00Z","extension_data":{},"message_count":0,"input_tokens":11,"output_tokens":22,"total_tokens":33,"accumulated_input_tokens":111,"accumulated_output_tokens":222,"accumulated_total_tokens":333}
{"id":"msg1","role":"user","created":1704110400,"content":[{"type":"text","text":"Hello"}]}"#;

        fs::write(&session_path, legacy_content).unwrap();

        let session = load_session("20240101_120000", &session_path).unwrap();

        assert_eq!(session.usage.input_tokens, Some(11));
        assert_eq!(session.usage.output_tokens, Some(22));
        assert_eq!(session.usage.total_tokens, Some(33));
        assert_eq!(session.accumulated_usage.input_tokens, Some(111));
        assert_eq!(session.accumulated_usage.output_tokens, Some(222));
        assert_eq!(session.accumulated_usage.total_tokens, Some(333));
    }
}
