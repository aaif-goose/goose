use super::session_manager::{role_to_string, SessionStorage};
use crate::conversation::message::{Message, MessageContent};
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
    #[serde(default, alias = "model_name")]
    pub model_id: Option<String>,
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

    /// Merge a JSON object patch into the `tool_meta` of the `ToolRequest` whose
    /// `id == tool_call_id` inside the message identified by `(thread_id,
    /// message_id)`. Existing keys in `tool_meta` are preserved.
    ///
    /// No-ops (returns `Ok(())`) if the row containing the tool request can't
    /// be found — callers (e.g. async title tasks) treat persistence as
    /// best-effort.
    ///
    /// `message_id` is used as a coarse filter, but multiple `thread_messages`
    /// rows can share the same `message_id` when the agent splits a single
    /// LLM response (e.g. text + tool_request) into separate
    /// `AgentEvent::Message` events. We disambiguate by walking the matching
    /// rows and picking the one whose content actually contains a
    /// `ToolRequest` with `tool_call_id`, then update only that row by its
    /// auto-incremented primary key. Without this, the title for the first
    /// tool in such a split message never persists, because `fetch_optional`
    /// returns the text-only row first and finds no matching tool call.
    pub async fn update_tool_request_meta(
        &self,
        thread_id: &str,
        message_id: &str,
        tool_call_id: &str,
        patch: serde_json::Value,
    ) -> Result<()> {
        let pool = self.storage.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        let rows = sqlx::query_as::<_, (i64, String)>(
            "SELECT id, content_json FROM thread_messages \
             WHERE thread_id = ? AND message_id = ? \
             ORDER BY id ASC",
        )
        .bind(thread_id)
        .bind(message_id)
        .fetch_all(&mut *tx)
        .await?;

        for (row_id, content_json) in rows {
            let mut content: Vec<MessageContent> = serde_json::from_str(&content_json)?;
            let mut found = false;
            for block in &mut content {
                if let MessageContent::ToolRequest(tr) = block {
                    if tr.id == tool_call_id {
                        tr.tool_meta = Some(merge_tool_meta(tr.tool_meta.take(), &patch));
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                continue;
            }

            let updated_json = serde_json::to_string(&content)?;
            sqlx::query("UPDATE thread_messages SET content_json = ? WHERE id = ?")
                .bind(updated_json)
                .bind(row_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(());
        }

        tx.commit().await?;
        Ok(())
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

/// Merge a JSON object `patch` into an existing optional object value,
/// preserving keys not present in the patch. Non-object values are replaced.
fn merge_tool_meta(
    existing: Option<serde_json::Value>,
    patch: &serde_json::Value,
) -> serde_json::Value {
    let mut base = match existing {
        Some(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    if let serde_json::Value::Object(patch_map) = patch {
        for (k, v) in patch_map {
            base.insert(k.clone(), v.clone());
        }
    }
    serde_json::Value::Object(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::{
        Message, MessageContent, ToolRequest, TOOL_META_CHAIN_SUMMARY_KEY,
        TOOL_META_EXTERNAL_DISPATCH_KEY, TOOL_META_TITLE_KEY,
    };
    use crate::session::SessionManager;
    use rmcp::model::CallToolRequestParams;
    use tempfile::TempDir;

    fn assistant_message_with_tool_request(
        tool_id: &str,
        tool_meta: Option<serde_json::Value>,
    ) -> Message {
        let tool_request = ToolRequest {
            id: tool_id.to_string(),
            tool_call: Ok(CallToolRequestParams::new("developer__shell")),
            metadata: None,
            tool_meta,
        };
        Message::new(
            Role::Assistant,
            chrono::Utc::now().timestamp_millis(),
            vec![MessageContent::ToolRequest(tool_request)],
        )
    }

    async fn fresh_thread_manager(temp: &TempDir) -> Arc<ThreadManager> {
        let session_manager = SessionManager::new(temp.path().to_path_buf());
        Arc::new(ThreadManager::new(session_manager.storage().clone()))
    }

    #[tokio::test]
    async fn update_tool_request_meta_sets_title_when_missing() {
        let temp = TempDir::new().unwrap();
        let mgr = fresh_thread_manager(&temp).await;
        let thread = mgr.create_thread(None, None, None).await.unwrap();

        let stored = mgr
            .append_message(
                &thread.id,
                None,
                &assistant_message_with_tool_request("tc-1", None),
            )
            .await
            .unwrap();
        let message_id = stored.id.clone().unwrap();

        mgr.update_tool_request_meta(
            &thread.id,
            &message_id,
            "tc-1",
            serde_json::json!({ TOOL_META_TITLE_KEY: "reading config" }),
        )
        .await
        .unwrap();

        let messages = mgr.list_messages(&thread.id).await.unwrap();
        let req = match &messages[0].content[0] {
            MessageContent::ToolRequest(r) => r,
            _ => panic!("expected tool request"),
        };
        assert_eq!(req.persisted_title(), Some("reading config"));
    }

    #[tokio::test]
    async fn update_tool_request_meta_preserves_existing_keys() {
        let temp = TempDir::new().unwrap();
        let mgr = fresh_thread_manager(&temp).await;
        let thread = mgr.create_thread(None, None, None).await.unwrap();

        let stored = mgr
            .append_message(
                &thread.id,
                None,
                &assistant_message_with_tool_request(
                    "tc-1",
                    Some(serde_json::json!({ TOOL_META_EXTERNAL_DISPATCH_KEY: true })),
                ),
            )
            .await
            .unwrap();
        let message_id = stored.id.clone().unwrap();

        mgr.update_tool_request_meta(
            &thread.id,
            &message_id,
            "tc-1",
            serde_json::json!({ TOOL_META_TITLE_KEY: "running commands" }),
        )
        .await
        .unwrap();

        let messages = mgr.list_messages(&thread.id).await.unwrap();
        let req = match &messages[0].content[0] {
            MessageContent::ToolRequest(r) => r,
            _ => panic!("expected tool request"),
        };
        assert!(
            req.is_externally_dispatched(),
            "external_dispatch key should be preserved across the merge"
        );
        assert_eq!(req.persisted_title(), Some("running commands"));
    }

    #[tokio::test]
    async fn update_tool_request_meta_overwrites_existing_value() {
        let temp = TempDir::new().unwrap();
        let mgr = fresh_thread_manager(&temp).await;
        let thread = mgr.create_thread(None, None, None).await.unwrap();

        let stored = mgr
            .append_message(
                &thread.id,
                None,
                &assistant_message_with_tool_request(
                    "tc-1",
                    Some(serde_json::json!({ TOOL_META_TITLE_KEY: "old" })),
                ),
            )
            .await
            .unwrap();
        let message_id = stored.id.clone().unwrap();

        mgr.update_tool_request_meta(
            &thread.id,
            &message_id,
            "tc-1",
            serde_json::json!({ TOOL_META_TITLE_KEY: "new" }),
        )
        .await
        .unwrap();

        let messages = mgr.list_messages(&thread.id).await.unwrap();
        let req = match &messages[0].content[0] {
            MessageContent::ToolRequest(r) => r,
            _ => panic!("expected tool request"),
        };
        assert_eq!(req.persisted_title(), Some("new"));
    }

    #[tokio::test]
    async fn update_tool_request_meta_no_op_for_unknown_message() {
        let temp = TempDir::new().unwrap();
        let mgr = fresh_thread_manager(&temp).await;
        let thread = mgr.create_thread(None, None, None).await.unwrap();

        mgr.update_tool_request_meta(
            &thread.id,
            "missing-message-id",
            "tc-1",
            serde_json::json!({ TOOL_META_TITLE_KEY: "x" }),
        )
        .await
        .expect("missing message must be a no-op, not an error");
    }

    #[tokio::test]
    async fn update_tool_request_meta_no_op_for_unknown_tool_call() {
        let temp = TempDir::new().unwrap();
        let mgr = fresh_thread_manager(&temp).await;
        let thread = mgr.create_thread(None, None, None).await.unwrap();

        let stored = mgr
            .append_message(
                &thread.id,
                None,
                &assistant_message_with_tool_request("tc-1", None),
            )
            .await
            .unwrap();
        let message_id = stored.id.clone().unwrap();

        mgr.update_tool_request_meta(
            &thread.id,
            &message_id,
            "tc-other",
            serde_json::json!({ TOOL_META_TITLE_KEY: "x" }),
        )
        .await
        .unwrap();

        let messages = mgr.list_messages(&thread.id).await.unwrap();
        let req = match &messages[0].content[0] {
            MessageContent::ToolRequest(r) => r,
            _ => panic!("expected tool request"),
        };
        assert!(
            req.persisted_title().is_none(),
            "no-match must leave tool_meta untouched"
        );
    }

    #[tokio::test]
    async fn update_tool_request_meta_targets_correct_row_when_message_id_is_shared() {
        // Regression for "first tool call in a chain consistently shows the
        // deterministic title on reload." Bedrock/Anthropic-style streaming
        // produces a single LLM message id (e.g. `msg_bdrk_…`) but the agent
        // splits it across multiple `AgentEvent::Message` events — one for
        // text, one for the trailing tool_request — and `append_message`
        // writes a separate row per event. Both rows end up with the SAME
        // `message_id`. `fetch_optional` returned the text-only row first and
        // the title never persisted.
        use crate::conversation::message::ToolRequest;
        use rmcp::model::CallToolRequestParams;

        let temp = TempDir::new().unwrap();
        let mgr = fresh_thread_manager(&temp).await;
        let thread = mgr.create_thread(None, None, None).await.unwrap();

        let shared_id = "msg_bdrk_shared".to_string();

        let mut text_only = Message::new(
            Role::Assistant,
            chrono::Utc::now().timestamp_millis(),
            vec![MessageContent::text(
                "Let me look at the project structure.",
            )],
        );
        text_only.id = Some(shared_id.clone());
        let stored_text = mgr
            .append_message(&thread.id, None, &text_only)
            .await
            .unwrap();
        assert_eq!(stored_text.id.as_deref(), Some(shared_id.as_str()));

        let mut tool_message = Message::new(
            Role::Assistant,
            chrono::Utc::now().timestamp_millis(),
            vec![MessageContent::ToolRequest(ToolRequest {
                id: "toolu_tree".to_string(),
                tool_call: Ok(CallToolRequestParams::new("tree")),
                metadata: None,
                tool_meta: None,
            })],
        );
        tool_message.id = Some(shared_id.clone());
        let stored_tool = mgr
            .append_message(&thread.id, None, &tool_message)
            .await
            .unwrap();
        assert_eq!(stored_tool.id.as_deref(), Some(shared_id.as_str()));

        mgr.update_tool_request_meta(
            &thread.id,
            &shared_id,
            "toolu_tree",
            serde_json::json!({ TOOL_META_TITLE_KEY: "exploring project structure" }),
        )
        .await
        .unwrap();

        let messages = mgr.list_messages(&thread.id).await.unwrap();
        assert_eq!(messages.len(), 2, "two distinct rows must be preserved");
        let text_msg = &messages[0];
        let tool_msg = &messages[1];
        assert!(
            matches!(&text_msg.content[0], MessageContent::Text(_)),
            "first row must remain text-only and untouched",
        );
        let tr = match &tool_msg.content[0] {
            MessageContent::ToolRequest(r) => r,
            _ => panic!("expected tool request in second row"),
        };
        assert_eq!(
            tr.persisted_title(),
            Some("exploring project structure"),
            "title must land on the row that actually contains the tool call",
        );
    }

    #[tokio::test]
    async fn update_tool_request_meta_serializes_concurrent_writes_preserving_all_keys() {
        // Regression for "occasional bad replay" when multiple persist tasks
        // (per-tool title for tc-1, per-tool title for tc-2, chain summary on
        // tc-1) race against each other for the same row's tool_meta. They
        // must serialize via BEGIN IMMEDIATE and merge rather than clobber.
        use crate::conversation::message::ToolRequest;
        use rmcp::model::CallToolRequestParams;

        let temp = TempDir::new().unwrap();
        let mgr = fresh_thread_manager(&temp).await;
        let thread = mgr.create_thread(None, None, None).await.unwrap();

        let message = Message::new(
            Role::Assistant,
            chrono::Utc::now().timestamp_millis(),
            vec![
                MessageContent::ToolRequest(ToolRequest {
                    id: "tc-1".to_string(),
                    tool_call: Ok(CallToolRequestParams::new("developer__shell")),
                    metadata: None,
                    tool_meta: None,
                }),
                MessageContent::ToolRequest(ToolRequest {
                    id: "tc-2".to_string(),
                    tool_call: Ok(CallToolRequestParams::new("developer__shell")),
                    metadata: None,
                    tool_meta: None,
                }),
            ],
        );
        let stored = mgr
            .append_message(&thread.id, None, &message)
            .await
            .unwrap();
        let message_id = stored.id.clone().unwrap();

        let m1 = mgr.clone();
        let t1 = thread.id.clone();
        let mid1 = message_id.clone();
        let h1 = tokio::spawn(async move {
            m1.update_tool_request_meta(
                &t1,
                &mid1,
                "tc-1",
                serde_json::json!({ TOOL_META_TITLE_KEY: "ran shell command" }),
            )
            .await
            .unwrap();
        });

        let m2 = mgr.clone();
        let t2 = thread.id.clone();
        let mid2 = message_id.clone();
        let h2 = tokio::spawn(async move {
            m2.update_tool_request_meta(
                &t2,
                &mid2,
                "tc-2",
                serde_json::json!({ TOOL_META_TITLE_KEY: "ran another shell command" }),
            )
            .await
            .unwrap();
        });

        let m3 = mgr.clone();
        let t3 = thread.id.clone();
        let mid3 = message_id.clone();
        let h3 = tokio::spawn(async move {
            m3.update_tool_request_meta(
                &t3,
                &mid3,
                "tc-1",
                serde_json::json!({
                    TOOL_META_CHAIN_SUMMARY_KEY: { "summary": "inspected codebase", "count": 2 },
                }),
            )
            .await
            .unwrap();
        });

        h1.await.unwrap();
        h2.await.unwrap();
        h3.await.unwrap();

        let messages = mgr.list_messages(&thread.id).await.unwrap();
        let content = &messages[0].content;
        let tc1 = match &content[0] {
            MessageContent::ToolRequest(r) => r,
            _ => panic!("expected tool request"),
        };
        let tc2 = match &content[1] {
            MessageContent::ToolRequest(r) => r,
            _ => panic!("expected tool request"),
        };

        assert_eq!(
            tc1.persisted_title(),
            Some("ran shell command"),
            "concurrent writes must not drop tc-1's title",
        );
        let chain_summary = tc1
            .persisted_chain_summary()
            .expect("tc-1 must keep its chain summary");
        assert_eq!(chain_summary.summary, "inspected codebase");
        assert_eq!(chain_summary.count, 2);
        assert_eq!(
            tc2.persisted_title(),
            Some("ran another shell command"),
            "concurrent writes must not drop tc-2's title",
        );
    }

    #[tokio::test]
    async fn update_tool_request_meta_persists_chain_summary_object() {
        let temp = TempDir::new().unwrap();
        let mgr = fresh_thread_manager(&temp).await;
        let thread = mgr.create_thread(None, None, None).await.unwrap();

        let stored = mgr
            .append_message(
                &thread.id,
                None,
                &assistant_message_with_tool_request(
                    "tc-1",
                    Some(serde_json::json!({ TOOL_META_TITLE_KEY: "first step" })),
                ),
            )
            .await
            .unwrap();
        let message_id = stored.id.clone().unwrap();

        mgr.update_tool_request_meta(
            &thread.id,
            &message_id,
            "tc-1",
            serde_json::json!({
                TOOL_META_CHAIN_SUMMARY_KEY: { "summary": "applied dark mode polish", "count": 4 },
            }),
        )
        .await
        .unwrap();

        let messages = mgr.list_messages(&thread.id).await.unwrap();
        let req = match &messages[0].content[0] {
            MessageContent::ToolRequest(r) => r,
            _ => panic!("expected tool request"),
        };
        let chain = req
            .persisted_chain_summary()
            .expect("chain summary should be present");
        assert_eq!(chain.summary, "applied dark mode polish");
        assert_eq!(chain.count, 4);
        assert_eq!(req.persisted_title(), Some("first step"));
    }
}
