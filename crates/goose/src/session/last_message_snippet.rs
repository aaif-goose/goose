use crate::conversation::message::Message;
use crate::session::session_manager::Session;
use anyhow::Result;
use rmcp::model::Role;
use sqlx::{AssertSqlSafe, Pool, Sqlite};
use std::collections::HashMap;

const LAST_MESSAGE_SNIPPET_MAX_CHARS: usize = 128;
const RECENT_MESSAGE_SNIPPET_SCAN_LIMIT: usize = 8;

#[derive(Debug, sqlx::FromRow)]
struct RecentMessageRow {
    row_id: i64,
    session_id: String,
    role: String,
    content_json: String,
    created_timestamp: i64,
    metadata_json: Option<String>,
    message_id: Option<String>,
}

pub(super) async fn hydrate_last_message_snippets(
    pool: &Pool<Sqlite>,
    sessions: &mut [Session],
) -> Result<()> {
    if sessions.is_empty() {
        return Ok(());
    }

    let session_ids = sessions
        .iter()
        .map(|session| session.id.clone())
        .collect::<Vec<_>>();
    let mut snippets = HashMap::with_capacity(session_ids.len());

    let rows = recent_message_rows(pool, &session_ids).await?;

    for row in rows {
        if snippets.contains_key(&row.session_id) {
            continue;
        }

        let session_id = row.session_id.clone();
        let Some(message) = message_from_recent_row(row)? else {
            continue;
        };
        if let Some(snippet) = message_snippet(&message, LAST_MESSAGE_SNIPPET_MAX_CHARS) {
            snippets.insert(session_id, snippet);
        }
    }

    for session in sessions {
        session.last_message_snippet = snippets.remove(&session.id);
    }

    Ok(())
}

async fn recent_message_rows(
    pool: &Pool<Sqlite>,
    session_ids: &[String],
) -> Result<Vec<RecentMessageRow>> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }

    let branch = r#"
        SELECT row_id, session_id, role, content_json, created_timestamp, metadata_json, message_id
        FROM (
            SELECT id AS row_id, session_id, role, content_json, created_timestamp, metadata_json, message_id
            FROM messages
            WHERE session_id = ?
            ORDER BY created_timestamp DESC, id DESC
            LIMIT ?
        )
    "#;
    let sql = std::iter::repeat_n(branch, session_ids.len())
        .collect::<Vec<_>>()
        .join(" UNION ALL ");

    let mut query = sqlx::query_as::<_, RecentMessageRow>(AssertSqlSafe(sql));
    for session_id in session_ids {
        query = query
            .bind(session_id)
            .bind(RECENT_MESSAGE_SNIPPET_SCAN_LIMIT as i64);
    }

    let mut rows = query.fetch_all(pool).await?;
    rows.sort_by(|left, right| {
        left.session_id
            .cmp(&right.session_id)
            .then_with(|| right.created_timestamp.cmp(&left.created_timestamp))
            .then_with(|| right.row_id.cmp(&left.row_id))
    });
    Ok(rows)
}

fn message_from_recent_row(row: RecentMessageRow) -> Result<Option<Message>> {
    let role = match row.role.as_str() {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        _ => return Ok(None),
    };

    let content = match serde_json::from_str(&row.content_json) {
        Ok(content) => content,
        Err(_) => return Ok(None),
    };
    let metadata = row
        .metadata_json
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();

    let mut message = Message::new(role, row.created_timestamp, content);
    message.metadata = metadata;
    if let Some(id) = row.message_id {
        message = message.with_id(id);
    }
    Ok(Some(message))
}

/// Build a bounded, single-line snippet from user-visible message text.
///
/// Tool-request, tool-response, thinking, image-only, and assistant-audience
/// blocks collapse to an empty string and return `None`. Internal whitespace
/// and newlines are collapsed to single spaces, and the result includes at most
/// `max_chars` characters of content; if truncated, a trailing `…` is appended
/// so it can be rendered verbatim by clients.
fn message_snippet(message: &Message, max_chars: usize) -> Option<String> {
    if !message.metadata.user_visible {
        return None;
    }

    let text = message
        .content
        .iter()
        .filter_map(|content| content.filter_for_audience(Role::User))
        .filter_map(|content| content.as_text().map(|text| text.to_string()))
        .collect::<Vec<_>>()
        .join("\n");
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }

    let mut chars = normalized.chars();
    let mut result: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        let end = result.trim_end().len();
        result.truncate(end);
        result.push('…');
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GooseMode;
    use crate::conversation::message::{MessageContent, MessageMetadata};
    use crate::session::session_manager::{
        SessionListFilters, SessionListPageQuery, SessionManager, SessionType,
    };
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    async fn snippet_session(sm: &SessionManager) -> String {
        sm.create_session(
            PathBuf::from("/tmp/snippet"),
            "Snippet session".to_string(),
            SessionType::User,
            GooseMode::default(),
        )
        .await
        .unwrap()
        .id
    }

    async fn listed_snippets(sm: &SessionManager) -> HashMap<String, Option<String>> {
        let types = [SessionType::User];
        sm.list_sessions_paged(SessionListPageQuery {
            filters: SessionListFilters {
                types: Some(&types),
                working_dir: Some(Path::new("/tmp/snippet")),
                ..Default::default()
            },
            cursor: None,
            page_size: 100,
            include_last_message_snippet: true,
        })
        .await
        .unwrap()
        .sessions
        .into_iter()
        .map(|session| (session.id, session.last_message_snippet))
        .collect()
    }

    async fn hydrated_snippet_of(sm: &SessionManager, id: &str) -> Option<String> {
        let mut snippets = listed_snippets(sm).await;
        snippets.remove(id).unwrap()
    }

    fn message_at(mut message: Message, created: i64) -> Message {
        message.created = created;
        message
    }

    fn assistant_audience_text(text: &str) -> MessageContent {
        use rmcp::model::TextContent;

        MessageContent::Text(TextContent::new(text.to_string()).with_annotations(
            rmcp::model::Annotations::default().with_audience(vec![Role::Assistant]),
        ))
    }

    #[test]
    fn test_message_snippet_collapses_whitespace_and_truncates() {
        use rmcp::model::CallToolRequestParams;

        let collapsed = Message::user().with_text("  hello\n\nworld\t  again  ");
        assert_eq!(
            message_snippet(&collapsed, 20).as_deref(),
            Some("hello world again")
        );

        let exact = Message::user().with_text("one two three four x");
        assert_eq!(
            message_snippet(&exact, 20).as_deref(),
            Some("one two three four x")
        );

        let long = Message::user().with_text("abcde fghij klmno p qrstuv");
        assert_eq!(
            message_snippet(&long, 20).as_deref(),
            Some("abcde fghij klmno p…")
        );

        let blob = Message::user().with_text("x".repeat(5000));
        let s = message_snippet(&blob, 20).unwrap();
        assert_eq!(s.chars().count(), 21);
        assert!(s.ends_with('…'));

        let tool =
            Message::assistant().with_tool_request("t1", Ok(CallToolRequestParams::new("shell")));
        assert_eq!(message_snippet(&tool, 20), None);

        let thinking = Message::assistant().with_thinking("internal reasoning", "sig");
        assert_eq!(message_snippet(&thinking, 20), None);

        let agent_only = Message::assistant()
            .with_text("hidden summary")
            .with_metadata(MessageMetadata::agent_only());
        assert_eq!(message_snippet(&agent_only, 20), None);
    }

    #[test]
    fn test_message_snippet_ignores_assistant_audience_text_blocks() {
        let mixed = Message::user()
            .with_content(assistant_audience_text("assistant-only preprompt"))
            .with_text("visible prompt");

        assert_eq!(
            message_snippet(&mixed, 128).as_deref(),
            Some("visible prompt")
        );

        let only_assistant =
            Message::user().with_content(assistant_audience_text("assistant-only details"));

        assert_eq!(message_snippet(&only_assistant, 128), None);
    }

    #[tokio::test]
    async fn test_live_last_message_snippet_reads_text_append() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = snippet_session(&sm).await;

        assert_eq!(hydrated_snippet_of(&sm, &id).await, None);

        sm.add_message(&id, &Message::user().with_text("hello there world"))
            .await
            .unwrap();

        assert_eq!(
            hydrated_snippet_of(&sm, &id).await.as_deref(),
            Some("hello there world")
        );

        let message = Message::user()
            .with_content(assistant_audience_text("assistant-only preprompt"))
            .with_text("visible prompt");
        sm.add_message(&id, &message).await.unwrap();

        assert_eq!(
            hydrated_snippet_of(&sm, &id).await.as_deref(),
            Some("visible prompt")
        );
    }

    #[tokio::test]
    async fn test_live_last_message_snippet_ignores_tool_messages() {
        use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};

        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = snippet_session(&sm).await;

        sm.add_message(&id, &Message::user().with_text("real text message"))
            .await
            .unwrap();
        sm.add_message(
            &id,
            &Message::assistant()
                .with_text("hidden context summary")
                .with_metadata(MessageMetadata::agent_only()),
        )
        .await
        .unwrap();
        sm.add_message(
            &id,
            &Message::assistant().with_tool_request("t1", Ok(CallToolRequestParams::new("shell"))),
        )
        .await
        .unwrap();
        sm.add_message(
            &id,
            &Message::user().with_tool_response(
                "t1",
                Ok(CallToolResult::success(vec![ContentBlock::text("done")])),
            ),
        )
        .await
        .unwrap();
        sm.add_message(&id, &Message::assistant().with_thinking("pondering", "sig"))
            .await
            .unwrap();

        assert_eq!(
            hydrated_snippet_of(&sm, &id).await.as_deref(),
            Some("real text message")
        );
    }

    #[tokio::test]
    async fn test_live_last_message_snippet_reads_replaced_conversation() {
        use crate::conversation::Conversation;
        use rmcp::model::CallToolRequestParams;

        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = snippet_session(&sm).await;

        sm.add_message(&id, &Message::user().with_text("stale original message"))
            .await
            .unwrap();

        let conversation = Conversation::new_unvalidated(vec![
            Message::user().with_text("first user prompt"),
            Message::assistant().with_text("assistant reply here"),
            Message::assistant().with_tool_request("t1", Ok(CallToolRequestParams::new("shell"))),
            Message::assistant()
                .with_text("hidden compacted summary")
                .with_metadata(MessageMetadata::agent_only()),
        ]);
        sm.replace_conversation(&id, &conversation).await.unwrap();

        assert_eq!(
            hydrated_snippet_of(&sm, &id).await.as_deref(),
            Some("assistant reply here")
        );
    }

    #[tokio::test]
    async fn test_live_last_message_snippet_reads_truncated_conversation() {
        use rmcp::model::CallToolRequestParams;

        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = snippet_session(&sm).await;

        let previous = message_at(Message::user().with_text("previous remaining text"), 1_000);
        let tool = message_at(
            Message::assistant().with_tool_request("t1", Ok(CallToolRequestParams::new("shell"))),
            2_000,
        );
        let latest = message_at(
            Message::assistant().with_text("latest text to remove"),
            3_000,
        );
        let hidden = message_at(
            Message::assistant()
                .with_text("hidden compacted summary")
                .with_metadata(MessageMetadata::agent_only()),
            2_500,
        );

        sm.add_message(&id, &previous).await.unwrap();
        sm.add_message(&id, &tool).await.unwrap();
        sm.add_message(&id, &hidden).await.unwrap();
        sm.add_message(&id, &latest).await.unwrap();

        assert_eq!(
            hydrated_snippet_of(&sm, &id).await.as_deref(),
            Some("latest text to remove")
        );

        sm.truncate_conversation(&id, 3_000).await.unwrap();

        assert_eq!(
            hydrated_snippet_of(&sm, &id).await.as_deref(),
            Some("previous remaining text")
        );
    }

    #[tokio::test]
    async fn test_live_last_message_snippet_null_after_truncate_without_text_messages() {
        use rmcp::model::CallToolRequestParams;

        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = snippet_session(&sm).await;

        let tool = message_at(
            Message::assistant().with_tool_request("t1", Ok(CallToolRequestParams::new("shell"))),
            500,
        );
        let text = message_at(Message::user().with_text("only text to remove"), 1_000);

        sm.add_message(&id, &tool).await.unwrap();
        sm.add_message(&id, &text).await.unwrap();

        assert_eq!(
            hydrated_snippet_of(&sm, &id).await.as_deref(),
            Some("only text to remove")
        );

        sm.truncate_conversation(&id, 1_000).await.unwrap();

        assert_eq!(hydrated_snippet_of(&sm, &id).await, None);
    }

    #[tokio::test]
    async fn test_live_last_message_snippets_read_from_recent_messages() {
        use rmcp::model::CallToolRequestParams;

        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let visible_id = snippet_session(&sm).await;

        sm.add_message(
            &visible_id,
            &Message::user().with_text("**raw** _markdown_ subtitle"),
        )
        .await
        .unwrap();
        for index in 0..3 {
            sm.add_message(
                &visible_id,
                &Message::assistant()
                    .with_text(format!("hidden summary {index}"))
                    .with_metadata(MessageMetadata::agent_only()),
            )
            .await
            .unwrap();
        }
        let empty_id = snippet_session(&sm).await;
        sm.add_message(
            &empty_id,
            &Message::assistant().with_tool_request("t1", Ok(CallToolRequestParams::new("shell"))),
        )
        .await
        .unwrap();
        sm.add_message(
            &empty_id,
            &Message::assistant()
                .with_text("hidden only")
                .with_metadata(MessageMetadata::agent_only()),
        )
        .await
        .unwrap();
        let by_id = listed_snippets(&sm).await;

        assert_eq!(
            by_id
                .get(&visible_id)
                .and_then(|snippet| snippet.as_deref()),
            Some("**raw** _markdown_ subtitle")
        );
        assert_eq!(by_id.get(&empty_id), Some(&None));
    }

    #[tokio::test]
    async fn test_live_last_message_snippet_skips_unparseable_recent_rows() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = snippet_session(&sm).await;

        sm.add_message(
            &id,
            &message_at(Message::user().with_text("older visible text"), 1_000),
        )
        .await
        .unwrap();

        let pool = sm.storage().pool().await.unwrap();
        sqlx::query(
            r#"
            INSERT INTO messages (message_id, session_id, role, content_json, created_timestamp, metadata_json)
            VALUES (?, ?, ?, ?, ?, ?)
        "#,
        )
        .bind("invalid-content")
        .bind(&id)
        .bind("assistant")
        .bind("not valid json")
        .bind(2_000_i64)
        .bind("{}")
        .execute(pool)
        .await
        .unwrap();

        assert_eq!(
            hydrated_snippet_of(&sm, &id).await.as_deref(),
            Some("older visible text")
        );
    }

    #[tokio::test]
    async fn test_live_last_message_snippets_stays_bounded() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        let id = snippet_session(&sm).await;

        sm.add_message(&id, &Message::user().with_text("older visible text"))
            .await
            .unwrap();
        for index in 0..RECENT_MESSAGE_SNIPPET_SCAN_LIMIT {
            sm.add_message(
                &id,
                &Message::assistant()
                    .with_text(format!("hidden summary {index}"))
                    .with_metadata(MessageMetadata::agent_only()),
            )
            .await
            .unwrap();
        }
        assert_eq!(hydrated_snippet_of(&sm, &id).await, None);
    }
}
