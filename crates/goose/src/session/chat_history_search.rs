use crate::conversation::message::MessageContent;
use crate::session::session_manager::SessionType;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rmcp::model::Role;
use serde::Serialize;
use sqlx::{AssertSqlSafe, Pool, Sqlite};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct ChatRecallResult {
    pub session_id: String,
    pub session_description: String,
    pub session_working_dir: String,
    pub last_activity: DateTime<Utc>,
    pub total_messages_in_session: usize,
    pub messages: Vec<ChatRecallMessage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatRecallMessage {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ChatRecallResults {
    pub results: Vec<ChatRecallResult>,
    pub total_matches: usize,
}

type SqlQueryRow = (
    String,
    String,
    String,
    DateTime<Utc>,
    String,
    String,
    DateTime<Utc>,
);

type SessionMessageGroup = (
    String,
    String,
    DateTime<Utc>,
    Vec<(String, String, DateTime<Utc>)>,
);

pub struct ChatHistorySearch<'a> {
    pool: &'a Pool<Sqlite>,
    query: &'a str,
    limit: usize,
    after_date: Option<DateTime<Utc>>,
    before_date: Option<DateTime<Utc>>,
    exclude_session_id: Option<String>,
    session_types: Vec<SessionType>,
}

impl<'a> ChatHistorySearch<'a> {
    pub fn new(
        pool: &'a Pool<Sqlite>,
        query: &'a str,
        limit: Option<usize>,
        after_date: Option<DateTime<Utc>>,
        before_date: Option<DateTime<Utc>>,
        exclude_session_id: Option<String>,
        session_types: Vec<SessionType>,
    ) -> Self {
        Self {
            pool,
            query,
            limit: limit.unwrap_or(10),
            after_date,
            before_date,
            exclude_session_id,
            session_types,
        }
    }

    pub async fn execute(self) -> Result<ChatRecallResults> {
        let keywords = self.parse_keywords();
        if keywords.is_empty() {
            return Ok(ChatRecallResults {
                results: vec![],
                total_matches: 0,
            });
        }

        let rows = self.fetch_rows(&keywords).await?;
        let session_messages = self.process_rows(rows);
        let session_totals = self.get_session_totals(&session_messages).await?;
        let results = self.convert_to_results(session_messages, session_totals);

        Ok(results)
    }

    async fn fetch_rows(&self, keywords: &[String]) -> Result<Vec<SqlQueryRow>> {
        let sql = self.build_sql(keywords);
        let mut query_builder = sqlx::query_as::<_, SqlQueryRow>(AssertSqlSafe(sql));

        for keyword in keywords {
            query_builder = query_builder.bind(keyword);
        }

        if let Some(exclude_id) = &self.exclude_session_id {
            query_builder = query_builder.bind(exclude_id);
        }

        for t in &self.session_types {
            query_builder = query_builder.bind(t.to_string());
        }

        if let Some(after) = self.after_date {
            query_builder = query_builder.bind(after);
        }
        if let Some(before) = self.before_date {
            query_builder = query_builder.bind(before);
        }

        query_builder = query_builder.bind(self.limit as i64);

        Ok(query_builder.fetch_all(self.pool).await?)
    }

    fn parse_keywords(&self) -> Vec<String> {
        self.query
            .split_whitespace()
            .map(|word| format!("%{}%", word.to_lowercase()))
            .collect()
    }

    fn build_sql(&self, keywords: &[String]) -> String {
        let mut sql = String::from(
            r#"
            SELECT 
                s.id as session_id,
                s.description as session_description,
                s.working_dir as session_working_dir,
                s.created_at as session_created_at,
                m.role,
                m.content_json,
                m.timestamp
            FROM messages m
            INNER JOIN sessions s ON m.session_id = s.id
            WHERE COALESCE(
                CASE
                    WHEN json_valid(m.metadata_json)
                    THEN json_extract(m.metadata_json, '$.agentVisible')
                END,
                1
            ) = 1
            AND COALESCE(
                CASE
                    WHEN json_valid(m.metadata_json)
                    THEN json_extract(m.metadata_json, '$.turnContext')
                END,
                0
            ) = 0
            AND EXISTS (
                SELECT 1 FROM json_each(m.content_json) AS content
                WHERE json_extract(content.value, '$.type') = 'text'
                AND (
                    json_type(content.value, '$.annotations.audience') IS NULL
                    OR EXISTS (
                        SELECT 1
                        FROM json_each(content.value, '$.annotations.audience') AS audience
                        WHERE audience.value = 'assistant'
                    )
                )
                AND (
        "#,
        );

        for (i, _) in keywords.iter().enumerate() {
            if i > 0 {
                sql.push_str(" OR ");
            }
            sql.push_str("LOWER(json_extract(content.value, '$.text')) LIKE ?");
        }

        sql.push_str(
            r#"
                )
            )
        "#,
        );

        if self.exclude_session_id.is_some() {
            sql.push_str(" AND s.id != ?");
        }

        if !self.session_types.is_empty() {
            let placeholders: String = self
                .session_types
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ");
            sql.push_str(&format!(" AND s.session_type IN ({})", placeholders));
        }

        if self.after_date.is_some() {
            sql.push_str(" AND m.timestamp >= ?");
        }
        if self.before_date.is_some() {
            sql.push_str(" AND m.timestamp <= ?");
        }

        sql.push_str(" ORDER BY m.timestamp DESC LIMIT ?");

        sql
    }

    fn process_rows(&self, rows: Vec<SqlQueryRow>) -> HashMap<String, SessionMessageGroup> {
        let mut session_messages: HashMap<String, SessionMessageGroup> = HashMap::new();

        for (
            session_id,
            session_description,
            session_working_dir,
            session_created_at,
            role,
            content_json,
            timestamp,
        ) in rows
        {
            if let Ok(content_vec) = serde_json::from_str::<Vec<MessageContent>>(&content_json) {
                let agent_visible_content = content_vec
                    .into_iter()
                    .filter_map(|content| content.filter_for_audience(Role::Assistant))
                    .collect();
                let text_parts = Self::extract_text_content(agent_visible_content);

                if !text_parts.is_empty() {
                    let entry = session_messages.entry(session_id.clone()).or_insert((
                        session_description.clone(),
                        session_working_dir.clone(),
                        session_created_at,
                        Vec::new(),
                    ));
                    entry
                        .3
                        .push((role.clone(), text_parts.join("\n"), timestamp));
                }
            }
        }

        session_messages
    }

    fn extract_text_content(content_vec: Vec<MessageContent>) -> Vec<String> {
        content_vec
            .into_iter()
            .filter_map(|content| match content {
                MessageContent::Text(ref tc) => Some(tc.text.clone()),
                MessageContent::ToolRequest(ref tr) => {
                    Some(format!("[Tool: {}]", tr.to_readable_string()))
                }
                MessageContent::ToolResponse(_) => Some("[Tool Response]".to_string()),
                MessageContent::Thinking(ref t) => Some(format!("[Thinking: {}]", t.thinking)),
                _ => None,
            })
            .collect()
    }

    async fn get_session_totals(
        &self,
        session_messages: &HashMap<String, SessionMessageGroup>,
    ) -> Result<HashMap<String, usize>> {
        let mut session_totals: HashMap<String, usize> = HashMap::new();
        for session_id in session_messages.keys() {
            let count: i64 = sqlx::query_scalar(
                r#"
                SELECT COUNT(*) FROM messages m WHERE m.session_id = ?
                AND COALESCE(
                    CASE
                        WHEN json_valid(m.metadata_json)
                        THEN json_extract(m.metadata_json, '$.agentVisible')
                    END,
                    1
                ) = 1
                AND COALESCE(
                    CASE
                        WHEN json_valid(m.metadata_json)
                        THEN json_extract(m.metadata_json, '$.turnContext')
                    END,
                    0
                ) = 0
                "#,
            )
            .bind(session_id)
            .fetch_one(self.pool)
            .await
            .unwrap_or(0);
            session_totals.insert(session_id.clone(), count as usize);
        }
        Ok(session_totals)
    }

    fn convert_to_results(
        &self,
        session_messages: HashMap<String, SessionMessageGroup>,
        session_totals: HashMap<String, usize>,
    ) -> ChatRecallResults {
        let mut results: Vec<ChatRecallResult> = session_messages
            .into_iter()
            .map(
                |(session_id, (description, working_dir, _created_at, messages))| {
                    let message_vec: Vec<ChatRecallMessage> = messages
                        .into_iter()
                        .map(|(role, content, timestamp)| ChatRecallMessage {
                            role,
                            content,
                            timestamp,
                        })
                        .collect();

                    let last_activity = message_vec
                        .iter()
                        .map(|m| m.timestamp)
                        .max()
                        .unwrap_or_else(chrono::Utc::now);

                    let total_messages_in_session =
                        session_totals.get(&session_id).copied().unwrap_or(0);

                    ChatRecallResult {
                        session_id,
                        session_description: description,
                        session_working_dir: working_dir,
                        last_activity,
                        total_messages_in_session,
                        messages: message_vec,
                    }
                },
            )
            .collect();

        results.sort_by_key(|result| std::cmp::Reverse(result.last_activity));

        let total_matches = results.iter().map(|r| r.messages.len()).sum();
        ChatRecallResults {
            results,
            total_matches,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::{Message, MessageContent, MessageMetadata};
    use rmcp::model::{Annotations, TextContent};
    use sqlx::sqlite::SqlitePoolOptions;

    fn user_only_text(text: &str) -> MessageContent {
        MessageContent::Text(
            TextContent::new(text)
                .with_annotations(Annotations::default().with_audience(vec![Role::User])),
        )
    }

    async fn insert_message(pool: &Pool<Sqlite>, message: &Message, timestamp: DateTime<Utc>) {
        sqlx::query(
            r#"
            INSERT INTO messages (session_id, role, content_json, timestamp, metadata_json)
            VALUES ('session-1', ?, ?, ?, ?)
            "#,
        )
        .bind(match message.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        })
        .bind(serde_json::to_string(&message.content).unwrap())
        .bind(timestamp)
        .bind(serde_json::to_string(&message.metadata).unwrap())
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn search_projects_audience_before_matching_and_limiting() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                description TEXT NOT NULL,
                working_dir TEXT NOT NULL,
                created_at TIMESTAMP NOT NULL,
                session_type TEXT NOT NULL
            );
            CREATE TABLE messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content_json TEXT NOT NULL,
                timestamp TIMESTAMP NOT NULL,
                metadata_json TEXT
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO sessions (id, description, working_dir, created_at, session_type) VALUES ('session-1', 'test', '/tmp', ?, 'user')",
        )
        .bind(Utc::now())
        .execute(&pool)
        .await
        .unwrap();

        let now = Utc::now();
        insert_message(
            &pool,
            &Message::user().with_text("needle public"),
            now - chrono::Duration::seconds(3),
        )
        .await;
        insert_message(
            &pool,
            &Message::user()
                .with_text("haystack visible")
                .with_content(user_only_text("needle secret-only")),
            now - chrono::Duration::seconds(2),
        )
        .await;
        insert_message(
            &pool,
            &Message::user()
                .with_text("needle hidden row")
                .with_metadata(MessageMetadata::user_only()),
            now - chrono::Duration::seconds(1),
        )
        .await;

        let needle = ChatHistorySearch::new(&pool, "needle", Some(1), None, None, None, vec![])
            .execute()
            .await
            .unwrap();
        assert_eq!(needle.total_matches, 1);
        assert_eq!(needle.results[0].messages[0].content, "needle public");

        let haystack =
            ChatHistorySearch::new(&pool, "haystack", Some(10), None, None, None, vec![])
                .execute()
                .await
                .unwrap();
        assert_eq!(haystack.total_matches, 1);
        assert!(haystack.results[0].messages[0]
            .content
            .contains("haystack visible"));
        assert!(!haystack.results[0].messages[0]
            .content
            .contains("needle secret-only"));

        let hidden_only =
            ChatHistorySearch::new(&pool, "secret-only", Some(10), None, None, None, vec![])
                .execute()
                .await
                .unwrap();
        assert_eq!(hidden_only.total_matches, 0);

        insert_message(
            &pool,
            &Message::user()
                .with_text("<turn-context>needle in operational context</turn-context>")
                .with_metadata(MessageMetadata::agent_only().with_turn_context()),
            now,
        )
        .await;
        let needle = ChatHistorySearch::new(&pool, "needle", Some(10), None, None, None, vec![])
            .execute()
            .await
            .unwrap();
        assert_eq!(
            needle.total_matches, 1,
            "turn-context events are operational context, not searchable conversation"
        );
        assert_eq!(
            needle.results[0].total_messages_in_session, 2,
            "session totals must not count turn-context or user-only rows"
        );
    }
}
