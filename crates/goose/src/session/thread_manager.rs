use super::session_manager::{role_to_string, SessionStorage};
use crate::conversation::message::Message;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rmcp::model::Role;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub name: String,
    pub user_set_name: bool,
    pub working_dir: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
    pub metadata: ThreadMetadata,
    #[serde(default)]
    pub current_session_id: Option<String>,
    #[serde(default)]
    pub message_count: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreadMetadata {
    #[serde(default)]
    pub persona_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model_name: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

pub struct ThreadManager {
    storage: Arc<SessionStorage>,
}

const THREAD_SELECT: &str = "\
    SELECT t.id, t.name, t.user_set_name, t.working_dir, t.created_at, t.updated_at, \
    t.archived_at, t.metadata_json, \
    (SELECT s.id FROM sessions s WHERE s.thread_id = t.id ORDER BY s.created_at DESC LIMIT 1) as current_session_id, \
    (SELECT COUNT(*) FROM thread_messages WHERE thread_id = t.id) as message_count \
    FROM threads t";

type ThreadRow = (
    String,
    String,
    bool,
    Option<String>,
    DateTime<Utc>,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    String,
    Option<String>,
    i64,
);

fn thread_from_row(
    (
        id,
        name,
        user_set_name,
        working_dir,
        created_at,
        updated_at,
        archived_at,
        metadata_json,
        current_session_id,
        message_count,
    ): ThreadRow,
) -> Result<Thread> {
    let metadata: ThreadMetadata = serde_json::from_str(&metadata_json).unwrap_or_default();

    Ok(Thread {
        id,
        name,
        user_set_name,
        working_dir,
        created_at,
        updated_at,
        archived_at,
        metadata,
        current_session_id,
        message_count,
    })
}

impl ThreadManager {
    pub fn new(storage: Arc<SessionStorage>) -> Self {
        Self { storage }
    }

    pub async fn create_thread(
        &self,
        name: Option<String>,
        metadata: Option<ThreadMetadata>,
        working_dir: Option<String>,
    ) -> Result<Thread> {
        let pool = self.storage.pool().await?;
        let id = uuid::Uuid::new_v4().to_string();
        let name = name.unwrap_or_else(|| "New Chat".to_string());
        let meta = metadata.unwrap_or_default();
        let metadata_json = serde_json::to_string(&meta)?;

        sqlx::query(
            "INSERT INTO threads (id, name, user_set_name, working_dir, metadata_json) VALUES (?, ?, FALSE, ?, ?)",
        )
        .bind(&id)
        .bind(&name)
        .bind(&working_dir)
        .bind(&metadata_json)
        .execute(pool)
        .await?;

        self.get_thread(&id).await
    }

    pub async fn get_thread(&self, id: &str) -> Result<Thread> {
        let pool = self.storage.pool().await?;
        let sql = format!("{} WHERE t.id = ?", THREAD_SELECT);
        let row = sqlx::query_as::<_, ThreadRow>(&sql)
            .bind(id)
            .fetch_one(pool)
            .await?;

        thread_from_row(row)
    }

    pub async fn update_thread(
        &self,
        id: &str,
        name: Option<String>,
        user_set_name: Option<bool>,
        metadata: Option<ThreadMetadata>,
    ) -> Result<Thread> {
        let pool = self.storage.pool().await?;
        let mut sets = Vec::new();

        if name.is_some() {
            sets.push("name = ?");
            sets.push("user_set_name = ?");
        }
        if metadata.is_some() {
            sets.push("metadata_json = ?");
        }

        if !sets.is_empty() {
            let sql = format!(
                "UPDATE threads SET {}, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
                sets.join(", ")
            );
            let mut q = sqlx::query(&sql);
            if let Some(ref n) = name {
                q = q.bind(n);
                q = q.bind(user_set_name.unwrap_or(true));
            }
            if let Some(ref meta) = metadata {
                q = q.bind(serde_json::to_string(meta)?);
            }
            q = q.bind(id);
            q.execute(pool).await?;
        }

        self.get_thread(id).await
    }

    pub async fn list_threads(&self, include_archived: bool) -> Result<Vec<Thread>> {
        let pool = self.storage.pool().await?;
        let sql = if include_archived {
            format!("{} ORDER BY t.updated_at DESC", THREAD_SELECT)
        } else {
            format!(
                "{} WHERE t.archived_at IS NULL ORDER BY t.updated_at DESC",
                THREAD_SELECT
            )
        };
        let rows = sqlx::query_as::<_, ThreadRow>(&sql).fetch_all(pool).await?;

        rows.into_iter().map(thread_from_row).collect()
    }

    pub async fn archive_thread(&self, id: &str) -> Result<Thread> {
        let pool = self.storage.pool().await?;
        sqlx::query("UPDATE threads SET archived_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        self.get_thread(id).await
    }

    pub async fn unarchive_thread(&self, id: &str) -> Result<Thread> {
        let pool = self.storage.pool().await?;
        sqlx::query(
            "UPDATE threads SET archived_at = NULL, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        )
        .bind(id)
        .execute(pool)
        .await?;
        self.get_thread(id).await
    }

    pub async fn update_metadata(
        &self,
        id: &str,
        f: impl FnOnce(&mut ThreadMetadata),
    ) -> Result<Thread> {
        let thread = self.get_thread(id).await?;
        let mut meta = thread.metadata;
        f(&mut meta);
        self.update_thread(id, None, None, Some(meta)).await
    }

    pub async fn update_working_dir(&self, id: &str, working_dir: &str) -> Result<()> {
        let pool = self.storage.pool().await?;
        sqlx::query(
            "UPDATE threads SET working_dir = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        )
        .bind(working_dir)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete_thread(&self, id: &str) -> Result<()> {
        let pool = self.storage.pool().await?;
        let mut tx = pool.begin().await?;

        sqlx::query("DELETE FROM thread_messages WHERE thread_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "DELETE FROM messages WHERE session_id IN (SELECT id FROM sessions WHERE thread_id = ?)",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM sessions WHERE thread_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM threads WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn append_message(
        &self,
        thread_id: &str,
        session_id: Option<&str>,
        message: &Message,
    ) -> Result<Message> {
        let pool = self.storage.pool().await?;
        let role_str = role_to_string(&message.role);
        let metadata_json = serde_json::to_string(&message.metadata)?;

        // When the incoming message is text-only, try to coalesce it with the
        // last stored row if that row has the same role and is also text-only.
        // This avoids storing one row per streaming token while keeping the UI
        // streaming path unchanged (callers still forward every chunk).
        if message.has_only_text_content() && !message.content.is_empty() {
            let new_text = message.as_concat_text();

            let maybe_last = sqlx::query_as::<_, (i64, String, String, String, String)>(
                "SELECT id, message_id, role, content_json, metadata_json \
                 FROM thread_messages \
                 WHERE thread_id = ? \
                 ORDER BY id DESC LIMIT 1",
            )
            .bind(thread_id)
            .fetch_optional(pool)
            .await?;

            if let Some((
                row_id,
                existing_msg_id,
                last_role,
                last_content_json,
                last_metadata_json,
            )) = maybe_last
            {
                if last_role == role_str
                    && last_metadata_json == metadata_json
                    && is_text_only_json(&last_content_json)
                {
                    // Append text into the existing row's single text element.
                    let updated_json = append_text_json(&last_content_json, &new_text)?;
                    sqlx::query("UPDATE thread_messages SET content_json = ? WHERE id = ?")
                        .bind(&updated_json)
                        .bind(row_id)
                        .execute(pool)
                        .await?;

                    sqlx::query("UPDATE threads SET updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                        .bind(thread_id)
                        .execute(pool)
                        .await?;

                    let mut stored = message.clone();
                    stored.id = Some(existing_msg_id);
                    return Ok(stored);
                }
            }
        }

        // Default path: insert a new row.
        let content_json = serde_json::to_string(&message.content)?;

        let message_id = message
            .id
            .clone()
            .unwrap_or_else(|| format!("tmsg_{}", uuid::Uuid::new_v4()));

        sqlx::query(
            "INSERT INTO thread_messages (thread_id, session_id, message_id, role, content_json, created_timestamp, metadata_json) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(thread_id)
        .bind(session_id)
        .bind(&message_id)
        .bind(role_str)
        .bind(&content_json)
        .bind(message.created)
        .bind(&metadata_json)
        .execute(pool)
        .await?;

        sqlx::query("UPDATE threads SET updated_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(thread_id)
            .execute(pool)
            .await?;

        let mut stored = message.clone();
        stored.id = Some(message_id);
        Ok(stored)
    }

    pub async fn fork_thread(&self, source_thread_id: &str) -> Result<Thread> {
        let source = self.get_thread(source_thread_id).await?;
        let pool = self.storage.pool().await?;

        let new_id = uuid::Uuid::new_v4().to_string();
        let name = format!("Fork of {}", source.name);
        let metadata_json = serde_json::to_string(&source.metadata)?;

        sqlx::query(
            "INSERT INTO threads (id, name, user_set_name, working_dir, metadata_json) VALUES (?, ?, FALSE, ?, ?)",
        )
        .bind(&new_id)
        .bind(&name)
        .bind(&source.working_dir)
        .bind(&metadata_json)
        .execute(pool)
        .await?;

        // Copy all thread messages
        sqlx::query(
            "INSERT INTO thread_messages (thread_id, session_id, message_id, role, content_json, created_timestamp, metadata_json) \
             SELECT ?, session_id, 'tmsg_' || hex(randomblob(16)), role, content_json, created_timestamp, metadata_json \
             FROM thread_messages WHERE thread_id = ? ORDER BY id ASC",
        )
        .bind(&new_id)
        .bind(source_thread_id)
        .execute(pool)
        .await?;

        self.get_thread(&new_id).await
    }

    pub async fn list_messages(&self, thread_id: &str) -> Result<Vec<Message>> {
        let pool = self.storage.pool().await?;
        let rows = sqlx::query_as::<_, (Option<String>, String, Option<String>, String, i64, String)>(
            "SELECT message_id, role, session_id, content_json, created_timestamp, metadata_json FROM thread_messages WHERE thread_id = ? ORDER BY id ASC",
        )
        .bind(thread_id)
        .fetch_all(pool)
        .await?;

        let mut messages = Vec::new();
        for (message_id, role_str, _session_id, content_json, created_timestamp, metadata_json) in
            rows
        {
            let role = match role_str.as_str() {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                _ => continue,
            };
            let content = serde_json::from_str(&content_json)?;
            let metadata = serde_json::from_str(&metadata_json).unwrap_or_default();

            let mut msg = Message::new(role, created_timestamp, content);
            msg.metadata = metadata;
            if let Some(id) = message_id {
                msg = msg.with_id(id);
            }
            messages.push(msg);
        }

        Ok(messages)
    }
}

/// Check whether a `content_json` string represents a single text-only element.
/// Avoids a full deserialize by inspecting the JSON structure directly.
fn is_text_only_json(content_json: &str) -> bool {
    let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(content_json) else {
        return false;
    };
    items.len() == 1
        && items[0].get("type").and_then(|v| v.as_str()) == Some("text")
        && items[0].get("text").is_some()
}

/// Append `new_text` to the single text element in a text-only `content_json` array.
fn append_text_json(content_json: &str, new_text: &str) -> anyhow::Result<String> {
    let mut items: Vec<serde_json::Value> = serde_json::from_str(content_json)?;
    if let Some(text_val) = items.get_mut(0).and_then(|v| v.get_mut("text")) {
        let existing = text_val.as_str().unwrap_or("");
        *text_val = serde_json::Value::String(format!("{}{}", existing, new_text));
    }
    Ok(serde_json::to_string(&items)?)
}
