use crate::config::paths::Paths;
use crate::config::GooseMode;
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::model::ModelConfig;
use crate::providers::base::{Provider, MSG_COUNT_FOR_SESSION_NAME_GENERATION};
use crate::recipe::Recipe;
use crate::session::extension_data::ExtensionData;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rmcp::model::Role;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use tracing::{info, warn};
use utoipa::ToSchema;

pub const CURRENT_SCHEMA_VERSION: i32 = 9;
pub const SESSIONS_FOLDER: &str = "sessions";
pub const DB_NAME: &str = "sessions.db";

#[derive(
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    ToSchema,
    PartialEq,
    Eq,
    Default,
    strum::Display,
    strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SessionType {
    #[default]
    User,
    Scheduled,
    SubAgent,
    Hidden,
    Terminal,
    Gateway,
    Acp,
}

static SESSION_STORAGE: LazyLock<Arc<SessionStorage>> =
    LazyLock::new(|| Arc::new(SessionStorage::new(Paths::data_dir())));

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Session {
    pub id: String,
    #[schema(value_type = String)]
    pub working_dir: PathBuf,
    #[serde(alias = "description")]
    pub name: String,
    #[serde(default)]
    pub user_set_name: bool,
    #[serde(default)]
    pub session_type: SessionType,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub extension_data: ExtensionData,
    pub total_tokens: Option<i32>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub accumulated_total_tokens: Option<i32>,
    pub accumulated_input_tokens: Option<i32>,
    pub accumulated_output_tokens: Option<i32>,
    pub schedule_id: Option<String>,
    pub recipe: Option<Recipe>,
    pub user_recipe_values: Option<HashMap<String, String>>,
    pub conversation: Option<Conversation>,
    pub message_count: usize,
    pub provider_name: Option<String>,
    pub model_config: Option<ModelConfig>,
    #[serde(default)]
    pub goose_mode: GooseMode,
}

pub struct SessionUpdateBuilder<'a> {
    session_manager: &'a SessionManager,
    session_id: String,
    name: Option<String>,
    user_set_name: Option<bool>,
    session_type: Option<SessionType>,
    working_dir: Option<PathBuf>,
    extension_data: Option<ExtensionData>,
    total_tokens: Option<Option<i32>>,
    input_tokens: Option<Option<i32>>,
    output_tokens: Option<Option<i32>>,
    accumulated_total_tokens: Option<Option<i32>>,
    accumulated_input_tokens: Option<Option<i32>>,
    accumulated_output_tokens: Option<Option<i32>>,
    schedule_id: Option<Option<String>>,
    recipe: Option<Option<Recipe>>,
    user_recipe_values: Option<Option<HashMap<String, String>>>,
    provider_name: Option<Option<String>>,
    model_config: Option<Option<ModelConfig>>,
    goose_mode: Option<GooseMode>,
}

#[derive(Serialize, ToSchema, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SessionInsights {
    pub total_sessions: usize,
    pub total_tokens: i64,
}

impl<'a> SessionUpdateBuilder<'a> {
    fn new(session_manager: &'a SessionManager, session_id: String) -> Self {
        Self {
            session_manager,
            session_id,
            name: None,
            user_set_name: None,
            session_type: None,
            working_dir: None,
            extension_data: None,
            total_tokens: None,
            input_tokens: None,
            output_tokens: None,
            accumulated_total_tokens: None,
            accumulated_input_tokens: None,
            accumulated_output_tokens: None,
            schedule_id: None,
            recipe: None,
            user_recipe_values: None,
            provider_name: None,
            model_config: None,
            goose_mode: None,
        }
    }

    pub async fn apply(self) -> Result<()> {
        self.session_manager.apply_update_inner(self).await
    }

    pub fn user_provided_name(mut self, name: impl Into<String>) -> Self {
        let name = name.into().trim().to_string();
        if !name.is_empty() {
            self.name = Some(name);
            self.user_set_name = Some(true);
        }
        self
    }

    pub fn system_generated_name(mut self, name: impl Into<String>) -> Self {
        let name = name.into().trim().to_string();
        if !name.is_empty() {
            self.name = Some(name);
            self.user_set_name = Some(false);
        }
        self
    }

    pub fn session_type(mut self, session_type: SessionType) -> Self {
        self.session_type = Some(session_type);
        self
    }

    pub fn working_dir(mut self, working_dir: PathBuf) -> Self {
        self.working_dir = Some(working_dir);
        self
    }

    pub fn extension_data(mut self, data: ExtensionData) -> Self {
        self.extension_data = Some(data);
        self
    }

    pub fn total_tokens(mut self, tokens: Option<i32>) -> Self {
        self.total_tokens = Some(tokens);
        self
    }

    pub fn input_tokens(mut self, tokens: Option<i32>) -> Self {
        self.input_tokens = Some(tokens);
        self
    }

    pub fn output_tokens(mut self, tokens: Option<i32>) -> Self {
        self.output_tokens = Some(tokens);
        self
    }

    pub fn accumulated_total_tokens(mut self, tokens: Option<i32>) -> Self {
        self.accumulated_total_tokens = Some(tokens);
        self
    }

    pub fn accumulated_input_tokens(mut self, tokens: Option<i32>) -> Self {
        self.accumulated_input_tokens = Some(tokens);
        self
    }

    pub fn accumulated_output_tokens(mut self, tokens: Option<i32>) -> Self {
        self.accumulated_output_tokens = Some(tokens);
        self
    }

    pub fn schedule_id(mut self, schedule_id: Option<String>) -> Self {
        self.schedule_id = Some(schedule_id);
        self
    }

    pub fn recipe(mut self, recipe: Option<Recipe>) -> Self {
        self.recipe = Some(recipe);
        self
    }

    pub fn user_recipe_values(
        mut self,
        user_recipe_values: Option<HashMap<String, String>>,
    ) -> Self {
        self.user_recipe_values = Some(user_recipe_values);
        self
    }

    pub fn provider_name(mut self, provider_name: impl Into<String>) -> Self {
        self.provider_name = Some(Some(provider_name.into()));
        self
    }

    pub fn model_config(mut self, model_config: ModelConfig) -> Self {
        self.model_config = Some(Some(model_config));
        self
    }

    pub fn clear_model_config(mut self) -> Self {
        self.model_config = Some(None);
        self
    }

    pub fn goose_mode(mut self, mode: GooseMode) -> Self {
        self.goose_mode = Some(mode);
        self
    }
}

pub struct SessionManager {
    storage: Arc<SessionStorage>,
}

impl SessionManager {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            storage: Arc::new(SessionStorage::new(data_dir)),
        }
    }

    pub fn instance() -> Self {
        Self {
            storage: Arc::clone(&SESSION_STORAGE),
        }
    }

    pub fn storage(&self) -> &Arc<SessionStorage> {
        &self.storage
    }

    pub async fn create_session(
        &self,
        working_dir: PathBuf,
        name: String,
        session_type: SessionType,
        goose_mode: GooseMode,
    ) -> Result<Session> {
        self.storage
            .create_session(working_dir, name, session_type, goose_mode)
            .await
    }

    pub async fn get_session(&self, id: &str, include_messages: bool) -> Result<Session> {
        self.storage.get_session(id, include_messages).await
    }

    pub fn update(&self, id: &str) -> SessionUpdateBuilder<'_> {
        SessionUpdateBuilder::new(self, id.to_string())
    }

    async fn apply_update_inner(&self, builder: SessionUpdateBuilder<'_>) -> Result<()> {
        self.storage.apply_update(builder).await
    }

    pub async fn add_message(&self, id: &str, message: &Message) -> Result<()> {
        self.storage.add_message(id, message).await
    }

    pub async fn replace_conversation(&self, id: &str, conversation: &Conversation) -> Result<()> {
        self.storage.replace_conversation(id, conversation).await
    }

    pub async fn list_sessions(&self) -> Result<Vec<Session>> {
        self.storage.list_sessions().await
    }

    pub async fn list_sessions_by_types(&self, types: &[SessionType]) -> Result<Vec<Session>> {
        self.storage.list_sessions_by_types(Some(types)).await
    }

    pub async fn list_all_sessions(&self) -> Result<Vec<Session>> {
        self.storage.list_sessions_by_types(None).await
    }

    pub async fn delete_session(&self, id: &str) -> Result<()> {
        self.storage.delete_session(id).await
    }

    pub async fn get_insights(&self) -> Result<SessionInsights> {
        self.storage
            .get_insights(&[SessionType::User, SessionType::Scheduled])
            .await
    }

    pub async fn export_session(&self, id: &str) -> Result<String> {
        self.storage.export_session(id).await
    }

    pub async fn import_session(
        &self,
        json: &str,
        session_type_override: Option<SessionType>,
    ) -> Result<Session> {
        self.storage
            .import_session(self, json, session_type_override)
            .await
    }

    pub async fn copy_session(&self, session_id: &str, new_name: String) -> Result<Session> {
        self.storage.copy_session(self, session_id, new_name).await
    }

    pub async fn truncate_conversation(&self, session_id: &str, timestamp: i64) -> Result<()> {
        self.storage
            .truncate_conversation(session_id, timestamp)
            .await
    }

    pub async fn maybe_update_name(&self, id: &str, provider: Arc<dyn Provider>) -> Result<()> {
        let session = self.get_session(id, true).await?;

        if session.user_set_name {
            return Ok(());
        }

        let conversation = session
            .conversation
            .ok_or_else(|| anyhow::anyhow!("No messages found"))?;

        let user_message_count = conversation
            .messages()
            .iter()
            .filter(|m| matches!(m.role, Role::User))
            .count();

        if user_message_count <= MSG_COUNT_FOR_SESSION_NAME_GENERATION {
            let name = provider.generate_session_name(id, &conversation).await?;
            self.update(id).system_generated_name(name).apply().await
        } else {
            Ok(())
        }
    }

    pub async fn search_chat_history(
        &self,
        query: &str,
        limit: Option<usize>,
        after_date: Option<chrono::DateTime<chrono::Utc>>,
        before_date: Option<chrono::DateTime<chrono::Utc>>,
        exclude_session_id: Option<String>,
        session_types: Vec<SessionType>,
    ) -> Result<crate::session::chat_history_search::ChatRecallResults> {
        self.storage
            .search_chat_history(
                query,
                limit,
                after_date,
                before_date,
                exclude_session_id,
                session_types,
            )
            .await
    }

    pub async fn update_message_metadata<F>(id: &str, message_id: &str, f: F) -> Result<()>
    where
        F: FnOnce(
            crate::conversation::message::MessageMetadata,
        ) -> crate::conversation::message::MessageMetadata,
    {
        Self::instance()
            .storage
            .update_message_metadata(id, message_id, f)
            .await
    }
}

pub struct SessionStorage {
    pool: Pool<Sqlite>,
    initialized: tokio::sync::OnceCell<()>,
    session_dir: PathBuf,
}

fn role_to_string(role: &Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

impl Default for Session {
    fn default() -> Self {
        Self {
            id: String::new(),
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            name: String::new(),
            user_set_name: false,
            session_type: SessionType::default(),
            created_at: Default::default(),
            updated_at: Default::default(),
            extension_data: ExtensionData::default(),
            total_tokens: None,
            input_tokens: None,
            output_tokens: None,
            accumulated_total_tokens: None,
            accumulated_input_tokens: None,
            accumulated_output_tokens: None,
            schedule_id: None,
            recipe: None,
            user_recipe_values: None,
            conversation: None,
            message_count: 0,
            provider_name: None,
            model_config: None,
            goose_mode: GooseMode::default(),
        }
    }
}

impl Session {
    pub fn without_messages(mut self) -> Self {
        self.conversation = None;
        self
    }
}

impl sqlx::FromRow<'_, sqlx::sqlite::SqliteRow> for Session {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;

        let recipe_json: Option<String> = row.try_get("recipe_json")?;
        let recipe = recipe_json.and_then(|json| serde_json::from_str(&json).ok());

        let user_recipe_values_json: Option<String> = row.try_get("user_recipe_values_json")?;
        let user_recipe_values =
            user_recipe_values_json.and_then(|json| serde_json::from_str(&json).ok());

        let model_config_json: Option<String> = row.try_get("model_config_json").ok().flatten();
        let model_config = model_config_json.and_then(|json| serde_json::from_str(&json).ok());

        let name: String = {
            let name_val: String = row.try_get("name").unwrap_or_default();
            if !name_val.is_empty() {
                name_val
            } else {
                row.try_get("description").unwrap_or_default()
            }
        };

        let user_set_name = row.try_get("user_set_name").unwrap_or(false);

        let session_type_str: String = row
            .try_get("session_type")
            .unwrap_or_else(|_| "user".to_string());
        let session_type = session_type_str.parse().unwrap_or_default();

        Ok(Session {
            id: row.try_get("id")?,
            working_dir: PathBuf::from(row.try_get::<String, _>("working_dir")?),
            name,
            user_set_name,
            session_type,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            extension_data: serde_json::from_str(&row.try_get::<String, _>("extension_data")?)
                .unwrap_or_default(),
            total_tokens: row.try_get("total_tokens")?,
            input_tokens: row.try_get("input_tokens")?,
            output_tokens: row.try_get("output_tokens")?,
            accumulated_total_tokens: row.try_get("accumulated_total_tokens")?,
            accumulated_input_tokens: row.try_get("accumulated_input_tokens")?,
            accumulated_output_tokens: row.try_get("accumulated_output_tokens")?,
            schedule_id: row.try_get("schedule_id")?,
            recipe,
            user_recipe_values,
            conversation: None,
            message_count: row.try_get("message_count").unwrap_or(0) as usize,
            provider_name: row.try_get("provider_name").ok().flatten(),
            model_config,
            goose_mode: row
                .try_get::<String, _>("goose_mode")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default(),
        })
    }
}

impl SessionStorage {
    fn create_pool(path: &Path) -> Pool<Sqlite> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create session database directory");
        }

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .busy_timeout(std::time::Duration::from_secs(30))
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

        SqlitePoolOptions::new().connect_lazy_with(options)
    }

    pub fn new(data_dir: PathBuf) -> Self {
        let session_dir = data_dir.join(SESSIONS_FOLDER);
        let db_path = session_dir.join(DB_NAME);
        Self {
            pool: Self::create_pool(&db_path),
            initialized: tokio::sync::OnceCell::new(),
            session_dir,
        }
    }

    async fn pool(&self) -> Result<&Pool<Sqlite>> {
        self.initialized
            .get_or_try_init(|| async {
                let schema_exists = sqlx::query_scalar::<_, bool>(
                    r#"SELECT EXISTS (SELECT name FROM sqlite_master WHERE type='table' AND name='schema_version')"#,
                )
                .fetch_one(&self.pool)
                .await
                .unwrap_or(false);

                if schema_exists {
                    Self::run_migrations(&self.pool).await?;
                } else {
                    Self::create_schema(&self.pool).await?;
                    if let Err(e) = Self::import_legacy(&self.pool, &self.session_dir).await {
                        warn!("Failed to import some legacy sessions: {}", e);
                    }
                }
                Ok::<(), anyhow::Error>(())
            })
            .await?;
        Ok(&self.pool)
    }

    pub async fn create(session_dir: &Path) -> Result<Self> {
        let storage = Self::new(session_dir.to_path_buf());
        Self::create_schema(&storage.pool).await?;
        Ok(storage)
    }

    async fn create_schema(pool: &Pool<Sqlite>) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE schema_version (
                version INTEGER PRIMARY KEY,
                applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
        "#,
        )
        .execute(pool)
        .await?;

        sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
            .bind(CURRENT_SCHEMA_VERSION)
            .execute(pool)
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL DEFAULT '',
                description TEXT NOT NULL DEFAULT '',
                user_set_name BOOLEAN DEFAULT FALSE,
                session_type TEXT NOT NULL DEFAULT 'user',
                working_dir TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                extension_data TEXT DEFAULT '{}',
                total_tokens INTEGER,
                input_tokens INTEGER,
                output_tokens INTEGER,
                accumulated_total_tokens INTEGER,
                accumulated_input_tokens INTEGER,
                accumulated_output_tokens INTEGER,
                schedule_id TEXT,
                recipe_json TEXT,
                user_recipe_values_json TEXT,
                provider_name TEXT,
                model_config_json TEXT,
                goose_mode TEXT NOT NULL DEFAULT 'auto'
            )
        "#,
        )
        .execute(pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                message_id TEXT,
                session_id TEXT NOT NULL REFERENCES sessions(id),
                role TEXT NOT NULL,
                content_json TEXT NOT NULL,
                created_timestamp INTEGER NOT NULL,
                timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                tokens INTEGER,
                metadata_json TEXT
            )
        "#,
        )
        .execute(pool)
        .await?;

        sqlx::query("CREATE INDEX idx_messages_session ON messages(session_id)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX idx_messages_timestamp ON messages(timestamp)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX idx_messages_message_id ON messages(message_id)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX idx_sessions_updated ON sessions(updated_at DESC)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX idx_sessions_type ON sessions(session_type)")
            .execute(pool)
            .await?;

        Ok(())
    }

    async fn import_legacy(pool: &Pool<Sqlite>, session_dir: &PathBuf) -> Result<()> {
        use crate::session::legacy;

        let sessions = match legacy::list_sessions(session_dir) {
            Ok(sessions) => sessions,
            Err(_) => {
                warn!("No legacy sessions found to import");
                return Ok(());
            }
        };

        if sessions.is_empty() {
            return Ok(());
        }

        let mut imported_count = 0;
        let mut failed_count = 0;

        for (session_name, session_path) in sessions {
            match legacy::load_session(&session_name, &session_path) {
                Ok(session) => match Self::import_legacy_session(pool, &session).await {
                    Ok(_) => {
                        imported_count += 1;
                        info!("  ✓ Imported: {}", session_name);
                    }
                    Err(e) => {
                        failed_count += 1;
                        info!("  ✗ Failed to import {}: {}", session_name, e);
                    }
                },
                Err(e) => {
                    failed_count += 1;
                    info!("  ✗ Failed to load {}: {}", session_name, e);
                }
            }
        }

        info!(
            "Import complete: {} successful, {} failed",
            imported_count, failed_count
        );
        Ok(())
    }

    async fn import_legacy_session(pool: &Pool<Sqlite>, session: &Session) -> Result<()> {
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        let recipe_json = match &session.recipe {
            Some(recipe) => Some(serde_json::to_string(recipe)?),
            None => None,
        };

        let user_recipe_values_json = match &session.user_recipe_values {
            Some(user_recipe_values) => Some(serde_json::to_string(user_recipe_values)?),
            None => None,
        };

        let model_config_json = match &session.model_config {
            Some(model_config) => Some(serde_json::to_string(model_config)?),
            None => None,
        };

        sqlx::query(
            r#"
        INSERT INTO sessions (
            id, name, user_set_name, session_type, working_dir, created_at, updated_at, extension_data,
            total_tokens, input_tokens, output_tokens,
            accumulated_total_tokens, accumulated_input_tokens, accumulated_output_tokens,
            schedule_id, recipe_json, user_recipe_values_json,
            provider_name, model_config_json, goose_mode
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        )
        .bind(&session.id)
        .bind(&session.name)
        .bind(session.user_set_name)
        .bind(session.session_type.to_string())
        .bind(&*session.working_dir.to_string_lossy())
        .bind(session.created_at)
        .bind(session.updated_at)
        .bind(serde_json::to_string(&session.extension_data)?)
        .bind(session.total_tokens)
        .bind(session.input_tokens)
        .bind(session.output_tokens)
        .bind(session.accumulated_total_tokens)
        .bind(session.accumulated_input_tokens)
        .bind(session.accumulated_output_tokens)
        .bind(&session.schedule_id)
        .bind(recipe_json)
        .bind(user_recipe_values_json)
        .bind(&session.provider_name)
        .bind(model_config_json)
        .bind(session.goose_mode.to_string())
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        if let Some(conversation) = &session.conversation {
            Self::replace_conversation_inner(pool, &session.id, conversation).await?;
        }
        Ok(())
    }

    async fn run_migrations(pool: &Pool<Sqlite>) -> Result<()> {
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        let current_version = Self::get_schema_version(&mut tx).await?;

        if current_version < CURRENT_SCHEMA_VERSION {
            info!(
                "Running database migrations from v{} to v{}...",
                current_version, CURRENT_SCHEMA_VERSION
            );

            for version in (current_version + 1)..=CURRENT_SCHEMA_VERSION {
                info!("  Applying migration v{}...", version);
                Self::apply_migration(&mut tx, version).await?;
                Self::update_schema_version(&mut tx, version).await?;
                info!("  ✓ Migration v{} complete", version);
            }

            info!("All migrations complete");
        }

        tx.commit().await?;
        Ok(())
    }

    async fn get_schema_version(tx: &mut sqlx::Transaction<'_, Sqlite>) -> Result<i32> {
        let table_exists = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT name FROM sqlite_master
                WHERE type='table' AND name='schema_version'
            )
        "#,
        )
        .fetch_one(&mut **tx)
        .await?;

        if !table_exists {
            return Ok(0);
        }

        let version = sqlx::query_scalar::<_, i32>("SELECT MAX(version) FROM schema_version")
            .fetch_one(&mut **tx)
            .await?;

        Ok(version)
    }

    async fn update_schema_version(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        version: i32,
    ) -> Result<()> {
        sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
            .bind(version)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn apply_migration(tx: &mut sqlx::Transaction<'_, Sqlite>, version: i32) -> Result<()> {
        match version {
            1 => {
                sqlx::query(
                    r#"
                    CREATE TABLE IF NOT EXISTS schema_version (
                        version INTEGER PRIMARY KEY,
                        applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                    )
                "#,
                )
                .execute(&mut **tx)
                .await?;
            }
            2 => {
                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN user_recipe_values_json TEXT
                "#,
                )
                .execute(&mut **tx)
                .await?;
            }
            3 => {
                sqlx::query(
                    r#"
                    ALTER TABLE messages ADD COLUMN metadata_json TEXT
                "#,
                )
                .execute(&mut **tx)
                .await?;
            }
            4 => {
                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN name TEXT DEFAULT ''
                "#,
                )
                .execute(&mut **tx)
                .await?;

                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN user_set_name BOOLEAN DEFAULT FALSE
                "#,
                )
                .execute(&mut **tx)
                .await?;
            }
            5 => {
                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN session_type TEXT NOT NULL DEFAULT 'user'
                "#,
                )
                .execute(&mut **tx)
                .await?;

                sqlx::query("CREATE INDEX idx_sessions_type ON sessions(session_type)")
                    .execute(&mut **tx)
                    .await?;
            }
            6 => {
                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN provider_name TEXT
                "#,
                )
                .execute(&mut **tx)
                .await?;

                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN model_config_json TEXT
                "#,
                )
                .execute(&mut **tx)
                .await?;
            }
            7 => {
                sqlx::query(
                    r#"
                    ALTER TABLE messages ADD COLUMN message_id TEXT
                "#,
                )
                .execute(&mut **tx)
                .await?;

                sqlx::query(
                    r#"
                    UPDATE messages
                    SET message_id = 'msg_' || session_id || '_' || id
                "#,
                )
                .execute(&mut **tx)
                .await?;

                sqlx::query("CREATE INDEX idx_messages_message_id ON messages(message_id)")
                    .execute(&mut **tx)
                    .await?;
            }
            8 => {
                sqlx::query(
                    r#"
                    ALTER TABLE sessions ADD COLUMN goose_mode TEXT NOT NULL DEFAULT 'auto'
                "#,
                )
                .execute(&mut **tx)
                .await?;
            }
            9 => {
                sqlx::query(
                    r#"
                    UPDATE sessions
                    SET session_type = 'acp'
                    WHERE session_type = 'user'
                      AND name = 'ACP Session'
                      AND user_set_name = FALSE
                "#,
                )
                .execute(&mut **tx)
                .await?;
            }
            _ => {
                anyhow::bail!("Unknown migration version: {}", version);
            }
        }

        Ok(())
    }

    async fn create_session(
        &self,
        working_dir: PathBuf,
        name: String,
        session_type: SessionType,
        goose_mode: GooseMode,
    ) -> Result<Session> {
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        let today = chrono::Utc::now().format("%Y%m%d").to_string();
        let session = sqlx::query_as(
            r#"
                INSERT INTO sessions (id, name, user_set_name, session_type, working_dir, extension_data, goose_mode)
                VALUES (
                    ? || '_' || CAST(COALESCE((
                        SELECT MAX(CAST(SUBSTR(id, 10) AS INTEGER))
                        FROM sessions
                        WHERE id LIKE ? || '_%'
                    ), 0) + 1 AS TEXT),
                    ?,
                    FALSE,
                    ?,
                    ?,
                    '{}',
                    ?
                )
                RETURNING *
                "#,
        )
            .bind(&today)
            .bind(&today)
            .bind(&name)
            .bind(session_type.to_string())
            .bind(&*working_dir.to_string_lossy())
            .bind(goose_mode.to_string())
            .fetch_one(&mut *tx)
            .await?;

        tx.commit().await?;
        #[cfg(feature = "telemetry")]
        crate::posthog::emit_session_started();
        Ok(session)
    }

    async fn get_session(&self, id: &str, include_messages: bool) -> Result<Session> {
        let pool = self.pool().await?;
        let mut session = sqlx::query_as::<_, Session>(
            r#"
        SELECT id, working_dir, name, description, user_set_name, session_type, created_at, updated_at, extension_data,
               total_tokens, input_tokens, output_tokens,
               accumulated_total_tokens, accumulated_input_tokens, accumulated_output_tokens,
               schedule_id, recipe_json, user_recipe_values_json,
               provider_name, model_config_json, goose_mode
        FROM sessions
        WHERE id = ?
    "#,
        )
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Session not found"))?;

        if include_messages {
            let conv = self.get_conversation(&session.id).await?;
            session.message_count = conv.messages().len();
            session.conversation = Some(conv);
        } else {
            let count =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM messages WHERE session_id = ?")
                    .bind(&session.id)
                    .fetch_one(pool)
                    .await? as usize;
            session.message_count = count;
        }

        Ok(session)
    }

    #[allow(clippy::too_many_lines)]
    async fn apply_update(&self, builder: SessionUpdateBuilder<'_>) -> Result<()> {
        let mut updates = Vec::new();
        let mut query = String::from("UPDATE sessions SET ");

        macro_rules! add_update {
            ($field:expr, $name:expr) => {
                if $field.is_some() {
                    if !updates.is_empty() {
                        query.push_str(", ");
                    }
                    updates.push($name);
                    query.push_str($name);
                    query.push_str(" = ?");
                }
            };
        }

        add_update!(builder.name, "name");
        add_update!(builder.user_set_name, "user_set_name");
        add_update!(builder.session_type, "session_type");
        add_update!(builder.working_dir, "working_dir");
        add_update!(builder.extension_data, "extension_data");
        add_update!(builder.total_tokens, "total_tokens");
        add_update!(builder.input_tokens, "input_tokens");
        add_update!(builder.output_tokens, "output_tokens");
        add_update!(builder.accumulated_total_tokens, "accumulated_total_tokens");
        add_update!(builder.accumulated_input_tokens, "accumulated_input_tokens");
        add_update!(
            builder.accumulated_output_tokens,
            "accumulated_output_tokens"
        );
        add_update!(builder.schedule_id, "schedule_id");
        add_update!(builder.recipe, "recipe_json");
        add_update!(builder.user_recipe_values, "user_recipe_values_json");
        add_update!(builder.provider_name, "provider_name");
        add_update!(builder.model_config, "model_config_json");
        add_update!(builder.goose_mode, "goose_mode");

        if updates.is_empty() {
            return Ok(());
        }

        query.push_str(", ");
        query.push_str("updated_at = datetime('now') WHERE id = ?");

        let mut q = sqlx::query(&query);

        if let Some(name) = builder.name {
            q = q.bind(name);
        }
        if let Some(user_set_name) = builder.user_set_name {
            q = q.bind(user_set_name);
        }
        if let Some(session_type) = builder.session_type {
            q = q.bind(session_type.to_string());
        }
        if let Some(wd) = builder.working_dir {
            q = q.bind(wd.to_string_lossy().to_string());
        }
        if let Some(ed) = builder.extension_data {
            q = q.bind(serde_json::to_string(&ed)?);
        }
        if let Some(tt) = builder.total_tokens {
            q = q.bind(tt);
        }
        if let Some(it) = builder.input_tokens {
            q = q.bind(it);
        }
        if let Some(ot) = builder.output_tokens {
            q = q.bind(ot);
        }
        if let Some(att) = builder.accumulated_total_tokens {
            q = q.bind(att);
        }
        if let Some(ait) = builder.accumulated_input_tokens {
            q = q.bind(ait);
        }
        if let Some(aot) = builder.accumulated_output_tokens {
            q = q.bind(aot);
        }
        if let Some(sid) = builder.schedule_id {
            q = q.bind(sid);
        }
        if let Some(recipe) = builder.recipe {
            let recipe_json = recipe.map(|r| serde_json::to_string(&r)).transpose()?;
            q = q.bind(recipe_json);
        }
        if let Some(user_recipe_values) = builder.user_recipe_values {
            let user_recipe_values_json = user_recipe_values
                .map(|urv| serde_json::to_string(&urv))
                .transpose()?;
            q = q.bind(user_recipe_values_json);
        }
        if let Some(provider_name) = builder.provider_name {
            q = q.bind(provider_name);
        }
        if let Some(model_config) = builder.model_config {
            let model_config_json = model_config
                .map(|mc| serde_json::to_string(&mc))
                .transpose()?;
            q = q.bind(model_config_json);
        }
        if let Some(goose_mode) = builder.goose_mode {
            q = q.bind(goose_mode.to_string());
        }

        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        q = q.bind(&builder.session_id);
        q.execute(&mut *tx).await?;

        tx.commit().await?;
        Ok(())
    }

    async fn get_conversation(&self, session_id: &str) -> Result<Conversation> {
        let pool = self.pool().await?;
        let rows = sqlx::query_as::<_, (String, String, i64, Option<String>, Option<String>)>(
            // Order by created_timestamp, then by id to break ties. created_timestamp is in seconds,
            // so messages created in the same second (e.g., tool request and response) need to
            // maintain their insertion order via the auto-increment id.
            "SELECT role, content_json, created_timestamp, metadata_json, message_id FROM messages WHERE session_id = ? ORDER BY created_timestamp, id",
        )
            .bind(session_id)
            .fetch_all(pool)
            .await?;

        let mut messages = Vec::new();
        for (role_str, content_json, created_timestamp, metadata_json, message_id) in
            rows.into_iter()
        {
            let role = match role_str.as_str() {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                _ => continue,
            };

            let content = serde_json::from_str(&content_json)?;
            let metadata = metadata_json
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default();

            let mut message = Message::new(role, created_timestamp, content);
            message.metadata = metadata;
            if let Some(id) = message_id {
                message = message.with_id(id);
            }
            messages.push(message);
        }

        Ok(Conversation::new_unvalidated(messages))
    }

    async fn add_message(&self, session_id: &str, message: &Message) -> Result<()> {
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        let metadata_json = serde_json::to_string(&message.metadata)?;

        let message_id = message
            .id
            .clone()
            .unwrap_or_else(|| format!("msg_{}_{}", session_id, uuid::Uuid::new_v4()));

        sqlx::query(
            r#"
            INSERT INTO messages (message_id, session_id, role, content_json, created_timestamp, metadata_json)
            VALUES (?, ?, ?, ?, ?, ?)
        "#,
        )
        .bind(message_id)
        .bind(session_id)
        .bind(role_to_string(&message.role))
        .bind(serde_json::to_string(&message.content)?)
        .bind(message.created)
        .bind(metadata_json)
        .execute(&mut *tx)
        .await?;

        sqlx::query("UPDATE sessions SET updated_at = datetime('now') WHERE id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn replace_conversation_inner(
        pool: &Pool<Sqlite>,
        session_id: &str,
        conversation: &Conversation,
    ) -> Result<()> {
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        sqlx::query("DELETE FROM messages WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        for message in conversation.messages() {
            let metadata_json = serde_json::to_string(&message.metadata)?;

            let message_id = message
                .id
                .clone()
                .unwrap_or_else(|| format!("msg_{}_{}", session_id, uuid::Uuid::new_v4()));

            sqlx::query(
                r#"
            INSERT INTO messages (message_id, session_id, role, content_json, created_timestamp, metadata_json)
            VALUES (?, ?, ?, ?, ?, ?)
        "#,
            )
            .bind(message_id)
            .bind(session_id)
            .bind(role_to_string(&message.role))
            .bind(serde_json::to_string(&message.content)?)
            .bind(message.created)
            .bind(metadata_json)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn replace_conversation(
        &self,
        session_id: &str,
        conversation: &Conversation,
    ) -> Result<()> {
        let pool = self.pool().await?;
        Self::replace_conversation_inner(pool, session_id, conversation).await
    }

    async fn list_sessions_by_types(&self, types: Option<&[SessionType]>) -> Result<Vec<Session>> {
        let (where_clause, binds): (String, Vec<String>) = match types {
            Some(t) if !t.is_empty() => {
                let placeholders: String = t.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
                (
                    format!("WHERE s.session_type IN ({})", placeholders),
                    t.iter().map(|t| t.to_string()).collect(),
                )
            }
            Some(_) => return Ok(Vec::new()),
            None => (String::new(), Vec::new()),
        };

        let query = format!(
            r#"
            SELECT s.id, s.working_dir, s.name, s.description, s.user_set_name, s.session_type, s.created_at, s.updated_at, s.extension_data,
                   s.total_tokens, s.input_tokens, s.output_tokens,
                   s.accumulated_total_tokens, s.accumulated_input_tokens, s.accumulated_output_tokens,
                   s.schedule_id, s.recipe_json, s.user_recipe_values_json,
                   s.provider_name, s.model_config_json, s.goose_mode,
                   COUNT(m.id) as message_count
            FROM sessions s
            INNER JOIN messages m ON s.id = m.session_id
            {}
            GROUP BY s.id
            ORDER BY s.updated_at DESC
            "#,
            where_clause
        );

        let mut q = sqlx::query_as::<_, Session>(&query);
        for b in &binds {
            q = q.bind(b);
        }

        let pool = self.pool().await?;
        q.fetch_all(pool).await.map_err(Into::into)
    }

    async fn list_sessions(&self) -> Result<Vec<Session>> {
        self.list_sessions_by_types(Some(&[SessionType::User, SessionType::Scheduled]))
            .await
    }

    async fn delete_session(&self, session_id: &str) -> Result<()> {
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        let exists =
            sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?)")
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await?;

        if !exists {
            return Err(anyhow::anyhow!("Session not found"));
        }

        sqlx::query("DELETE FROM messages WHERE session_id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn get_insights(&self, types: &[SessionType]) -> Result<SessionInsights> {
        if types.is_empty() {
            return Ok(SessionInsights {
                total_sessions: 0,
                total_tokens: 0,
            });
        }

        let placeholders: String = types.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let query = format!(
            r#"
            SELECT COUNT(*) as total_sessions,
                   COALESCE(SUM(COALESCE(accumulated_total_tokens, total_tokens, 0)), 0) as total_tokens
            FROM sessions
            WHERE session_type IN ({})
            "#,
            placeholders
        );

        let pool = self.pool().await?;
        let mut q = sqlx::query_as::<_, (i64, Option<i64>)>(&query);
        for t in types {
            q = q.bind(t.to_string());
        }

        let row = q.fetch_one(pool).await?;

        Ok(SessionInsights {
            total_sessions: row.0 as usize,
            total_tokens: row.1.unwrap_or(0),
        })
    }

    async fn export_session(&self, id: &str) -> Result<String> {
        let session = self.get_session(id, true).await?;
        serde_json::to_string_pretty(&session).map_err(Into::into)
    }

    async fn import_session(
        &self,
        session_manager: &SessionManager,
        json: &str,
        session_type_override: Option<SessionType>,
    ) -> Result<Session> {
        let import: Session = serde_json::from_str(json)?;

        let session = self
            .create_session(
                import.working_dir.clone(),
                import.name.clone(),
                session_type_override.unwrap_or(import.session_type),
                import.goose_mode,
            )
            .await?;

        let mut builder = session_manager
            .update(&session.id)
            .extension_data(import.extension_data)
            .total_tokens(import.total_tokens)
            .input_tokens(import.input_tokens)
            .output_tokens(import.output_tokens)
            .accumulated_total_tokens(import.accumulated_total_tokens)
            .accumulated_input_tokens(import.accumulated_input_tokens)
            .accumulated_output_tokens(import.accumulated_output_tokens)
            .schedule_id(import.schedule_id)
            .recipe(import.recipe)
            .user_recipe_values(import.user_recipe_values);

        if import.user_set_name {
            builder = builder.user_provided_name(import.name.clone());
        }

        builder.apply().await?;

        if let Some(conversation) = import.conversation {
            self.replace_conversation(&session.id, &conversation)
                .await?;
        }

        self.get_session(&session.id, true).await
    }

    async fn copy_session(
        &self,
        session_manager: &SessionManager,
        session_id: &str,
        new_name: String,
    ) -> Result<Session> {
        let original_session = self.get_session(session_id, true).await?;

        let new_session = self
            .create_session(
                original_session.working_dir.clone(),
                new_name,
                original_session.session_type,
                original_session.goose_mode,
            )
            .await?;

        let mut builder = session_manager
            .update(&new_session.id)
            .extension_data(original_session.extension_data)
            .schedule_id(original_session.schedule_id)
            .recipe(original_session.recipe)
            .user_recipe_values(original_session.user_recipe_values);

        // Preserve provider, model config, and goose_mode from original session
        if let Some(provider_name) = original_session.provider_name {
            builder = builder.provider_name(provider_name);
        }

        if let Some(model_config) = original_session.model_config {
            builder = builder.model_config(model_config);
        }

        builder = builder.goose_mode(original_session.goose_mode);

        builder.apply().await?;

        if let Some(conversation) = original_session.conversation {
            self.replace_conversation(&new_session.id, &conversation)
                .await?;
        }

        self.get_session(&new_session.id, true).await
    }

    async fn truncate_conversation(&self, session_id: &str, timestamp: i64) -> Result<()> {
        let pool = self.pool().await?;
        sqlx::query("DELETE FROM messages WHERE session_id = ? AND created_timestamp >= ?")
            .bind(session_id)
            .bind(timestamp)
            .execute(pool)
            .await?;

        Ok(())
    }

    async fn search_chat_history(
        &self,
        query: &str,
        limit: Option<usize>,
        after_date: Option<chrono::DateTime<chrono::Utc>>,
        before_date: Option<chrono::DateTime<chrono::Utc>>,
        exclude_session_id: Option<String>,
        session_types: Vec<SessionType>,
    ) -> Result<crate::session::chat_history_search::ChatRecallResults> {
        use crate::session::chat_history_search::ChatHistorySearch;

        let pool = self.pool().await?;
        ChatHistorySearch::new(
            pool,
            query,
            limit,
            after_date,
            before_date,
            exclude_session_id,
            session_types,
        )
        .execute()
        .await
    }

    async fn update_message_metadata<F>(
        &self,
        session_id: &str,
        message_id: &str,
        f: F,
    ) -> Result<()>
    where
        F: FnOnce(
            crate::conversation::message::MessageMetadata,
        ) -> crate::conversation::message::MessageMetadata,
    {
        let pool = self.pool().await?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;

        let current_metadata_json = sqlx::query_scalar::<_, String>(
            "SELECT metadata_json FROM messages WHERE message_id = ? AND session_id = ?",
        )
        .bind(message_id)
        .bind(session_id)
        .fetch_one(&mut *tx)
        .await?;

        let current_metadata: crate::conversation::message::MessageMetadata =
            serde_json::from_str(&current_metadata_json)?;

        let new_metadata = f(current_metadata);
        let metadata_json = serde_json::to_string(&new_metadata)?;

        sqlx::query(
            "UPDATE messages SET metadata_json = ? WHERE message_id = ? AND session_id = ?",
        )
        .bind(metadata_json)
        .bind(message_id)
        .bind(session_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::{Message, MessageContent};
    use tempfile::TempDir;
    use test_case::test_case;

    const NUM_CONCURRENT_SESSIONS: i32 = 10;

    async fn run_lock_upgrade_attempt(
        pool: Pool<Sqlite>,
        session_id: String,
        begin_statement: &'static str,
        worker_id: i32,
        barrier: Option<Arc<tokio::sync::Barrier>>,
    ) -> anyhow::Result<()> {
        let mut tx = pool.begin_with(begin_statement).await?;

        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions WHERE id = ?")
            .bind(&session_id)
            .fetch_one(&mut *tx)
            .await?;

        if let Some(barrier) = barrier {
            barrier.wait().await;
        }

        sqlx::query("UPDATE sessions SET total_tokens = ? WHERE id = ?")
            .bind(worker_id)
            .bind(&session_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn run_lock_upgrade_race(
        pool: Pool<Sqlite>,
        session_id: String,
        begin_statement: &'static str,
        use_barrier: bool,
    ) -> Vec<anyhow::Result<()>> {
        let barrier = if use_barrier {
            Some(Arc::new(tokio::sync::Barrier::new(2)))
        } else {
            None
        };
        let mut handles = Vec::new();

        for worker_id in 0..2 {
            let pool = pool.clone();
            let session_id = session_id.clone();
            let barrier = barrier.clone();
            handles.push(tokio::spawn(async move {
                run_lock_upgrade_attempt(pool, session_id, begin_statement, worker_id, barrier)
                    .await
            }));
        }

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.expect("lock-upgrade task panicked"));
        }
        results
    }

    #[tokio::test]
    async fn test_begin_immediate_prevents_lock_upgrade_deadlock() {
        let temp_dir = TempDir::new().unwrap();
        let session_manager = SessionManager::new(temp_dir.path().to_path_buf());

        let session = session_manager
            .create_session(
                PathBuf::from("/tmp/lock-upgrade-test"),
                "Lock Upgrade Session".to_string(),
                SessionType::User,
                GooseMode::default(),
            )
            .await
            .unwrap();

        let pool = session_manager.storage().pool.clone();

        let results = run_lock_upgrade_race(pool.clone(), session.id.clone(), "BEGIN", true).await;
        assert!(
            results.iter().any(Result::is_err),
            "BEGIN (DEFERRED) should cause SQLITE_BUSY when two tasks try to upgrade SHARED → RESERVED"
        );

        let results = run_lock_upgrade_race(pool, session.id, "BEGIN IMMEDIATE", false).await;
        assert!(
            results.iter().all(Result::is_ok),
            "BEGIN IMMEDIATE should serialize contention without SQLITE_BUSY: {:?}",
            results
                .iter()
                .filter_map(|r| r.as_ref().err().map(ToString::to_string))
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_concurrent_session_creation() {
        let temp_dir = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));

        let mut handles = vec![];

        for i in 0..NUM_CONCURRENT_SESSIONS {
            let sm = Arc::clone(&session_manager);
            let handle = tokio::spawn(async move {
                let working_dir = PathBuf::from(format!("/tmp/test_{}", i));
                let description = format!("Test session {}", i);

                let session = sm
                    .create_session(
                        working_dir.clone(),
                        description,
                        SessionType::User,
                        GooseMode::default(),
                    )
                    .await
                    .unwrap();

                sm.add_message(
                    &session.id,
                    &Message {
                        id: None,
                        role: Role::User,
                        created: chrono::Utc::now().timestamp_millis(),
                        content: vec![MessageContent::text("hello world")],
                        metadata: Default::default(),
                    },
                )
                .await
                .unwrap();

                sm.add_message(
                    &session.id,
                    &Message {
                        id: None,
                        role: Role::Assistant,
                        created: chrono::Utc::now().timestamp_millis(),
                        content: vec![MessageContent::text("sup world?")],
                        metadata: Default::default(),
                    },
                )
                .await
                .unwrap();

                sm.update(&session.id)
                    .user_provided_name(format!("Updated session {}", i))
                    .total_tokens(Some(100 * i))
                    .apply()
                    .await
                    .unwrap();

                let updated = sm.get_session(&session.id, true).await.unwrap();
                assert_eq!(updated.message_count, 2);
                assert_eq!(updated.total_tokens, Some(100 * i));

                session.id
            });
            handles.push(handle);
        }

        let mut results = vec![];
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        assert_eq!(results.len(), NUM_CONCURRENT_SESSIONS as usize);

        let unique_ids: std::collections::HashSet<_> = results.iter().collect();
        assert_eq!(unique_ids.len(), NUM_CONCURRENT_SESSIONS as usize);

        let sessions = session_manager.list_sessions().await.unwrap();
        assert_eq!(sessions.len(), NUM_CONCURRENT_SESSIONS as usize);

        for session in &sessions {
            assert_eq!(session.message_count, 2);
            assert!(session.name.starts_with("Updated session"));
        }

        let insights = session_manager.get_insights().await.unwrap();
        assert_eq!(insights.total_sessions, NUM_CONCURRENT_SESSIONS as usize);
        let expected_tokens = 100 * NUM_CONCURRENT_SESSIONS * (NUM_CONCURRENT_SESSIONS - 1) / 2;
        assert_eq!(insights.total_tokens, expected_tokens as i64);
    }

    #[tokio::test]
    async fn test_export_import_roundtrip() {
        const DESCRIPTION: &str = "Original session";
        const TOTAL_TOKENS: i32 = 500;
        const INPUT_TOKENS: i32 = 300;
        const OUTPUT_TOKENS: i32 = 200;
        const ACCUMULATED_TOKENS: i32 = 1000;
        const USER_MESSAGE: &str = "test message";
        const ASSISTANT_MESSAGE: &str = "test response";

        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let original = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                DESCRIPTION.to_string(),
                SessionType::User,
                GooseMode::default(),
            )
            .await
            .unwrap();

        sm.update(&original.id)
            .total_tokens(Some(TOTAL_TOKENS))
            .input_tokens(Some(INPUT_TOKENS))
            .output_tokens(Some(OUTPUT_TOKENS))
            .accumulated_total_tokens(Some(ACCUMULATED_TOKENS))
            .apply()
            .await
            .unwrap();

        sm.add_message(
            &original.id,
            &Message {
                id: None,
                role: Role::User,
                created: chrono::Utc::now().timestamp_millis(),
                content: vec![MessageContent::text(USER_MESSAGE)],
                metadata: Default::default(),
            },
        )
        .await
        .unwrap();

        sm.add_message(
            &original.id,
            &Message {
                id: None,
                role: Role::Assistant,
                created: chrono::Utc::now().timestamp_millis(),
                content: vec![MessageContent::text(ASSISTANT_MESSAGE)],
                metadata: Default::default(),
            },
        )
        .await
        .unwrap();

        let exported = sm.export_session(&original.id).await.unwrap();
        let imported = sm.import_session(&exported, None).await.unwrap();

        assert_ne!(imported.id, original.id);
        assert_eq!(imported.name, DESCRIPTION);
        assert_eq!(imported.working_dir, PathBuf::from("/tmp/test"));
        assert_eq!(imported.total_tokens, Some(TOTAL_TOKENS));
        assert_eq!(imported.input_tokens, Some(INPUT_TOKENS));
        assert_eq!(imported.output_tokens, Some(OUTPUT_TOKENS));
        assert_eq!(imported.accumulated_total_tokens, Some(ACCUMULATED_TOKENS));
        assert_eq!(imported.message_count, 2);

        let conversation = imported.conversation.unwrap();
        assert_eq!(conversation.messages().len(), 2);
        assert_eq!(conversation.messages()[0].role, Role::User);
        assert_eq!(conversation.messages()[1].role, Role::Assistant);
    }

    #[tokio::test]
    async fn test_list_sessions_filters_by_type() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let user_session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "User session".to_string(),
                SessionType::User,
                GooseMode::default(),
            )
            .await
            .unwrap();

        sm.add_message(
            &user_session.id,
            &Message {
                id: None,
                role: Role::User,
                created: chrono::Utc::now().timestamp_millis(),
                content: vec![MessageContent::text("hello world")],
                metadata: Default::default(),
            },
        )
        .await
        .unwrap();

        let acp_session = sm
            .create_session(
                PathBuf::from("/tmp/test"),
                "ACP session".to_string(),
                SessionType::Acp,
                GooseMode::default(),
            )
            .await
            .unwrap();

        sm.add_message(
            &acp_session.id,
            &Message {
                id: None,
                role: Role::User,
                created: chrono::Utc::now().timestamp_millis(),
                content: vec![MessageContent::text("hello acp")],
                metadata: Default::default(),
            },
        )
        .await
        .unwrap();

        let default_sessions = sm.list_sessions().await.unwrap();
        assert_eq!(default_sessions.len(), 1);
        assert_eq!(default_sessions[0].name, "User session");

        let acp_sessions = sm
            .list_sessions_by_types(&[SessionType::Acp])
            .await
            .unwrap();
        assert_eq!(acp_sessions.len(), 1);
        assert_eq!(acp_sessions[0].name, "ACP session");
    }

    #[tokio::test]
    async fn test_import_session_with_description_field() {
        const OLD_FORMAT_JSON: &str = r#"{
            "id": "20240101_1",
            "description": "Old format session",
            "user_set_name": true,
            "working_dir": "/tmp/test",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z",
            "extension_data": {},
            "message_count": 0
        }"#;

        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let imported = sm.import_session(OLD_FORMAT_JSON, None).await.unwrap();

        assert_eq!(imported.name, "Old format session");
        assert!(imported.user_set_name);
        assert_eq!(imported.working_dir, PathBuf::from("/tmp/test"));
    }

    #[test_case(GooseMode::Approve)]
    #[test_case(GooseMode::SmartApprove)]
    #[test_case(GooseMode::Chat)]
    #[tokio::test]
    async fn test_goose_mode_persists(mode: GooseMode) {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "test".into(),
                SessionType::User,
                mode,
            )
            .await
            .unwrap();

        let reloaded = sm.get_session(&session.id, false).await.unwrap();
        assert_eq!(reloaded.goose_mode, mode);
    }

    #[tokio::test]
    async fn test_goose_mode_update() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "test".into(),
                SessionType::User,
                GooseMode::default(),
            )
            .await
            .unwrap();

        sm.update(&session.id)
            .goose_mode(GooseMode::Approve)
            .apply()
            .await
            .unwrap();

        let reloaded = sm.get_session(&session.id, false).await.unwrap();
        assert_eq!(reloaded.goose_mode, GooseMode::Approve);
    }

    #[tokio::test]
    async fn test_goose_mode_malformed_defaults_to_auto() {
        let temp_dir = TempDir::new().unwrap();
        let sm = SessionManager::new(temp_dir.path().to_path_buf());

        let session = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "test".into(),
                SessionType::User,
                GooseMode::Approve,
            )
            .await
            .unwrap();

        let pool = &sm.storage().pool;
        sqlx::query("UPDATE sessions SET goose_mode = 'garbage' WHERE id = ?")
            .bind(&session.id)
            .execute(pool)
            .await
            .unwrap();

        let reloaded = sm.get_session(&session.id, false).await.unwrap();
        assert_eq!(reloaded.goose_mode, GooseMode::default());
    }

    #[tokio::test]
    async fn test_acp_session_migration() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join(SESSIONS_FOLDER).join(DB_NAME);

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }

        let pool = SqlitePoolOptions::new()
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(&db_path)
                    .create_if_missing(true),
            )
            .await
            .unwrap();

        SessionStorage::create_schema(&pool).await.unwrap();

        // Demote the schema back to v8 to simulate a database
        // that has never seen migration 9.
        sqlx::query("UPDATE schema_version SET version = 8")
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO sessions (id, name, user_set_name, session_type, working_dir, extension_data, goose_mode)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("user_id")
        .bind("User Session")
        .bind(false)
        .bind("user")
        .bind("/tmp")
        .bind("{}")
        .bind("auto")
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO sessions (id, name, user_set_name, session_type, working_dir, extension_data, goose_mode)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("acp_id")
        .bind("ACP Session")
        .bind(false)
        .bind("user")
        .bind("/tmp")
        .bind("{}")
        .bind("auto")
        .execute(&pool)
        .await
        .unwrap();

        pool.close().await;

        let sm = SessionManager::new(temp_dir.path().to_path_buf());
        sm.storage().pool().await.unwrap(); // Triggers migration

        let user_session = sm.storage().get_session("user_id", false).await.unwrap();
        assert_eq!(user_session.session_type, SessionType::User);

        let acp_session = sm.storage().get_session("acp_id", false).await.unwrap();
        assert_eq!(acp_session.session_type, SessionType::Acp);
    }
}
