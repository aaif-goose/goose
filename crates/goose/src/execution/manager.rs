use crate::agents::mcp_client::GooseMcpHostInfo;
use crate::agents::{Agent, AgentConfig, ExtensionLoadResult, GoosePlatform};
use crate::config::permission::PermissionManager;
use crate::config::Config;
use crate::scheduler_trait::SchedulerTrait;
use crate::session::{SessionManager, SessionNameUpdate};
use anyhow::Result;
use lru::LruCache;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, OnceCell, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::info;

const DEFAULT_MAX_SESSION: usize = 100;

static AGENT_MANAGER: OnceCell<Arc<AgentManager>> = OnceCell::const_new();

#[derive(Clone, Default)]
pub struct RuntimeContext {
    pub mcp_host_info: Option<GooseMcpHostInfo>,
    pub use_login_shell_path: Option<bool>,
    pub session_name_update_tx: Option<mpsc::UnboundedSender<SessionNameUpdate>>,
}

pub struct AgentManagerGetResult {
    pub agent: Arc<Agent>,
    pub extension_results: Vec<ExtensionLoadResult>,
}

/// Serves every session from one `Agent`. Loading a session restores its provider and
/// starts its extensions; the least recently used session is released once more than
/// `max_sessions` are loaded.
pub struct AgentManager {
    agent_config: AgentConfig,
    agent: OnceCell<Arc<Agent>>,
    loaded_sessions: Mutex<LruCache<String, ()>>,
    /// Concurrent first requests for a session must not each start its MCP servers (#9031).
    load_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    cancel_tokens: RwLock<HashMap<String, CancellationToken>>,
}

impl AgentManager {
    pub async fn new(agent_config: AgentConfig, max_sessions: Option<usize>) -> Result<Self> {
        let capacity = NonZeroUsize::new(max_sessions.unwrap_or(DEFAULT_MAX_SESSION))
            .unwrap_or_else(|| NonZeroUsize::new(100).unwrap());
        Ok(Self {
            agent_config,
            agent: OnceCell::new(),
            loaded_sessions: Mutex::new(LruCache::new(capacity)),
            load_locks: Mutex::new(HashMap::new()),
            cancel_tokens: RwLock::new(HashMap::new()),
        })
    }

    pub async fn instance() -> Result<Arc<Self>> {
        AGENT_MANAGER
            .get_or_try_init(|| async {
                let config = Config::global();
                let max_sessions = config
                    .get_goose_max_active_agents()
                    .unwrap_or(DEFAULT_MAX_SESSION);
                let session_manager = Arc::new(SessionManager::instance());
                let agent_config = AgentConfig::new(
                    session_manager,
                    PermissionManager::instance(),
                    None,
                    config.get_goose_disable_session_naming().unwrap_or(false),
                    GoosePlatform::GooseDesktop,
                );
                let manager = Self::new(agent_config, Some(max_sessions)).await?;
                Ok(Arc::new(manager))
            })
            .await
            .cloned()
    }

    pub fn scheduler(&self) -> Option<Arc<dyn SchedulerTrait>> {
        self.agent_config.scheduler_service.as_ref().map(Arc::clone)
    }

    /// Get the shared SessionManager for session-only operations
    pub fn session_manager(&self) -> &SessionManager {
        self.agent_config.session_manager.as_ref()
    }

    pub async fn get_or_create_agent(&self, session_id: String) -> Result<Arc<Agent>> {
        Ok(self
            .get_or_create_agent_with_runtime_context(session_id, RuntimeContext::default())
            .await?
            .agent)
    }

    /// The runtime context is the same for every call on a manager (one per ACP
    /// connection), so the first caller's context builds the shared agent.
    pub async fn get_or_create_agent_with_runtime_context(
        &self,
        session_id: String,
        runtime_context: RuntimeContext,
    ) -> Result<AgentManagerGetResult> {
        let agent = self.agent(runtime_context).await;
        if self.loaded_sessions.lock().await.get(&session_id).is_some() {
            return Ok(AgentManagerGetResult {
                agent,
                extension_results: Vec::new(),
            });
        }

        let load_lock = Arc::clone(
            self.load_locks
                .lock()
                .await
                .entry(session_id.clone())
                .or_default(),
        );
        let load_guard = load_lock.lock().await;
        let result = self.load_session(&agent, &session_id).await;
        drop(load_guard);
        drop(load_lock);
        self.prune_load_lock(&session_id).await;

        Ok(AgentManagerGetResult {
            agent,
            extension_results: result?,
        })
    }

    async fn agent(&self, runtime_context: RuntimeContext) -> Arc<Agent> {
        Arc::clone(
            self.agent
                .get_or_init(|| async {
                    let mut config = self.agent_config.clone();
                    config.mcp_host_info = runtime_context.mcp_host_info;
                    config.use_login_shell_path = runtime_context.use_login_shell_path;
                    config.session_name_update_tx = runtime_context.session_name_update_tx;
                    Arc::new(Agent::with_config(config))
                })
                .await,
        )
    }

    async fn load_session(
        &self,
        agent: &Arc<Agent>,
        session_id: &str,
    ) -> Result<Vec<ExtensionLoadResult>> {
        if self.loaded_sessions.lock().await.get(session_id).is_some() {
            return Ok(Vec::new());
        }

        let mut extension_results = Vec::new();
        if let Ok(session) = self
            .agent_config
            .session_manager
            .get_session(session_id, false)
            .await
        {
            if session.provider_name.is_some() {
                if let Err(error) = agent.restore_provider_from_session(&session).await {
                    if crate::acp::is_auth_required(&error) {
                        return Err(error);
                    }
                    tracing::warn!(
                        "Failed to restore provider for session {}: {}",
                        session_id,
                        error
                    );
                }
            }
            extension_results = agent.load_extensions_from_session(&session).await;
        }

        let evicted = self
            .loaded_sessions
            .lock()
            .await
            .push(session_id.to_string(), ());
        if let Some((evicted_id, ())) = evicted {
            agent.release_session(&evicted_id).await;
        }
        Ok(extension_results)
    }

    /// Waiters still holding the lock keep the entry; the last one out removes it.
    async fn prune_load_lock(&self, session_id: &str) {
        let mut locks = self.load_locks.lock().await;
        if locks
            .get(session_id)
            .is_some_and(|lock| Arc::strong_count(lock) == 1)
        {
            locks.remove(session_id);
        }
    }

    pub async fn remove_session(&self, session_id: &str) -> Result<()> {
        if self.loaded_sessions.lock().await.pop(session_id).is_none() {
            anyhow::bail!("Session {} not found", session_id);
        }
        self.unload(session_id).await;
        Ok(())
    }

    pub async fn remove_session_if_loaded(&self, session_id: &str) -> Result<()> {
        self.loaded_sessions.lock().await.pop(session_id);
        self.unload(session_id).await;
        Ok(())
    }

    async fn unload(&self, session_id: &str) {
        if let Some(token) = self.cancel_tokens.write().await.remove(session_id) {
            token.cancel();
        }
        if let Some(agent) = self.agent.get() {
            agent.release_session(session_id).await;
        }
        info!("Removed session {}", session_id);
    }

    pub async fn has_session(&self, session_id: &str) -> bool {
        self.loaded_sessions.lock().await.contains(session_id)
    }

    pub async fn session_count(&self) -> usize {
        self.loaded_sessions.lock().await.len()
    }

    /// Atomically check if busy and register a cancel token. Returns Err if already busy.
    pub async fn try_register_cancel_token(
        &self,
        session_id: &str,
        token: CancellationToken,
    ) -> Result<()> {
        let mut tokens = self.cancel_tokens.write().await;
        if tokens.contains_key(session_id) {
            anyhow::bail!("Session '{}' is currently busy", session_id);
        }
        tokens.insert(session_id.to_string(), token);
        Ok(())
    }

    /// Remove the cancellation token for a session (called when reply finishes)
    pub async fn unregister_cancel_token(&self, session_id: &str) {
        self.cancel_tokens.write().await.remove(session_id);
    }

    /// Cancel a running agent by triggering its cancellation token
    pub async fn cancel_session(&self, session_id: &str) -> Result<()> {
        let tokens = self.cancel_tokens.read().await;
        let token = tokens
            .get(session_id)
            .ok_or_else(|| anyhow::anyhow!("No active operation for session {}", session_id))?;
        token.cancel();
        Ok(())
    }

    /// Check if a session has an active reply in progress
    pub async fn is_session_busy(&self, session_id: &str) -> bool {
        let tokens = self.cancel_tokens.read().await;
        tokens.contains_key(session_id)
    }

    pub async fn list_active_session_ids(&self) -> Vec<String> {
        self.loaded_sessions
            .lock()
            .await
            .iter()
            .map(|(id, _)| id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use tempfile::TempDir;

    use goose_test_support::McpFixture;
    use tokio::sync::Barrier;

    use crate::agents::extension::{Envs, ExtensionConfig};
    use crate::agents::{AgentConfig, GoosePlatform};
    use crate::config::permission::PermissionManager;
    use crate::config::GooseMode;
    use crate::session::{EnabledExtensionsState, ExtensionState, SessionManager, SessionType};

    use super::AgentManager;

    async fn create_test_manager(temp_dir: &TempDir) -> AgentManager {
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let agent_config = AgentConfig::new(
            session_manager,
            PermissionManager::instance(),
            None,
            false,
            GoosePlatform::GooseDesktop,
        );
        AgentManager::new(agent_config, Some(100)).await.unwrap()
    }

    #[tokio::test]
    async fn test_session_limit() {
        let temp_dir = TempDir::new().unwrap();
        let manager = create_test_manager(&temp_dir).await;

        let sessions: Vec<_> = (0..100).map(|i| format!("session-{}", i)).collect();

        for session in &sessions {
            manager.get_or_create_agent(session.clone()).await.unwrap();
        }

        // Create a new session after cleanup
        let new_session = "new-session".to_string();
        let _new_agent = manager.get_or_create_agent(new_session).await.unwrap();

        assert_eq!(manager.session_count().await, 100);
    }

    #[tokio::test]
    async fn test_remove_session() {
        let temp_dir = TempDir::new().unwrap();
        let manager = create_test_manager(&temp_dir).await;
        let session = String::from("remove-test");

        manager.get_or_create_agent(session.clone()).await.unwrap();
        assert!(manager.has_session(&session).await);

        manager.remove_session(&session).await.unwrap();
        assert!(!manager.has_session(&session).await);

        assert!(manager.remove_session(&session).await.is_err());
    }

    #[tokio::test]
    async fn test_remove_session_if_loaded() {
        let temp_dir = TempDir::new().unwrap();
        let manager = create_test_manager(&temp_dir).await;
        let session = String::from("remove-if-loaded-test");

        manager.remove_session_if_loaded(&session).await.unwrap();

        manager.get_or_create_agent(session.clone()).await.unwrap();
        manager.remove_session_if_loaded(&session).await.unwrap();
        assert!(!manager.has_session(&session).await);
        manager.remove_session_if_loaded(&session).await.unwrap();
    }

    async fn session_with_fixture_extension(
        manager: &AgentManager,
        temp_dir: &TempDir,
        mcp: &McpFixture,
    ) -> String {
        let session = manager
            .session_manager()
            .create_session(
                temp_dir.path().to_path_buf(),
                "fixture-extension".to_string(),
                SessionType::User,
                GooseMode::default(),
            )
            .await
            .unwrap();
        let extension = ExtensionConfig::StreamableHttp {
            name: "mcp-fixture".to_string(),
            description: "MCP fixture".to_string(),
            uri: mcp.url.clone(),
            envs: Envs::default(),
            env_keys: vec![],
            headers: HashMap::new(),
            timeout: Some(30),
            socket: None,
            client_id: None,
            client_secret_key: None,
            scopes: vec![],
            bundled: Some(false),
            available_tools: vec![],
        };
        let mut extension_data = session.extension_data.clone();
        EnabledExtensionsState::new(vec![extension])
            .to_extension_data(&mut extension_data)
            .unwrap();
        manager
            .session_manager()
            .update(&session.id)
            .extension_data(extension_data)
            .apply()
            .await
            .unwrap();
        session.id
    }

    #[tokio::test]
    async fn concurrent_session_creation_initializes_extensions_once() {
        let temp_dir = TempDir::new().unwrap();
        let manager = Arc::new(create_test_manager(&temp_dir).await);
        let mcp = McpFixture::new().await;
        let session_id = session_with_fixture_extension(&manager, &temp_dir, &mcp).await;

        let callers = 20;
        let barrier = Arc::new(Barrier::new(callers));
        let mut handles = Vec::with_capacity(callers);
        for _ in 0..callers {
            let manager = Arc::clone(&manager);
            let session_id = session_id.clone();
            let barrier = Arc::clone(&barrier);
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                manager.get_or_create_agent(session_id).await.unwrap()
            }));
        }

        for result in futures::future::join_all(handles).await {
            result.unwrap();
        }
        assert_eq!(manager.session_count().await, 1);
        assert!(manager.load_locks.lock().await.is_empty());
        // One discover from one extension start; a second initialization
        // would have been a second request.
        assert_eq!(mcp.request_count(), 1);
    }

    #[tokio::test]
    async fn evicting_a_session_stops_its_extensions() {
        let temp_dir = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let agent_config = AgentConfig::new(
            session_manager,
            PermissionManager::instance(),
            None,
            false,
            GoosePlatform::GooseDesktop,
        );
        let manager = AgentManager::new(agent_config, Some(1)).await.unwrap();
        let mcp = McpFixture::new().await;
        let evicted = session_with_fixture_extension(&manager, &temp_dir, &mcp).await;

        let agent = manager.get_or_create_agent(evicted.clone()).await.unwrap();
        assert_eq!(agent.list_extensions(&evicted).await, ["mcp-fixture"]);
        manager.get_or_create_agent("next".into()).await.unwrap();

        assert!(!manager.has_session(&evicted).await);
        assert!(agent.list_extensions(&evicted).await.is_empty());
    }

    #[tokio::test]
    async fn test_eviction_updates_last_used() {
        // Test that accessing a session updates its last_used timestamp
        // and affects eviction order
        let temp_dir = TempDir::new().unwrap();
        let manager = create_test_manager(&temp_dir).await;

        let sessions: Vec<_> = (0..100).map(|i| format!("session-{}", i)).collect();

        for session in &sessions {
            manager.get_or_create_agent(session.clone()).await.unwrap();
            // Small delay to ensure different timestamps
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        // Access the first session again to update its last_used
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        manager
            .get_or_create_agent(sessions[0].clone())
            .await
            .unwrap();

        // Now create a 101st session - should evict session2 (least recently used)
        let session101 = String::from("session-101");
        manager
            .get_or_create_agent(session101.clone())
            .await
            .unwrap();

        assert!(manager.has_session(&sessions[0]).await);
        assert!(!manager.has_session(&sessions[1]).await);
        assert!(manager.has_session(&session101).await);
    }

    #[tokio::test]
    async fn test_remove_nonexistent_session_error() {
        // Test that removing a nonexistent session returns an error
        let temp_dir = TempDir::new().unwrap();
        let manager = create_test_manager(&temp_dir).await;
        let session = String::from("never-created");

        let result = manager.remove_session(&session).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_final_output_tool_restored_after_lru_eviction() {
        use crate::agents::final_output_tool::FINAL_OUTPUT_TOOL_NAME;
        use crate::recipe::{Recipe, Response};
        use serde_json::json;

        let temp_dir = TempDir::new().unwrap();
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let agent_config = AgentConfig::new(
            Arc::clone(&session_manager),
            PermissionManager::instance(),
            None,
            false,
            GoosePlatform::GooseDesktop,
        );
        let manager = AgentManager::new(agent_config, Some(1)).await.unwrap();

        let session = session_manager
            .create_session(
                temp_dir.path().to_path_buf(),
                "recipe-session".into(),
                crate::session::SessionType::User,
                GooseMode::default(),
            )
            .await
            .unwrap();

        let recipe = Recipe {
            version: "1.0.0".into(),
            title: "Test".into(),
            description: "Test recipe".into(),
            response: Some(Response {
                json_schema: Some(json!({
                    "type": "object",
                    "properties": { "result": { "type": "string" } },
                    "required": ["result"]
                })),
            }),
            instructions: None,
            prompt: None,
            extensions: None,
            settings: None,
            activities: None,
            author: None,
            parameters: None,
            sub_recipes: None,
            retry: None,
        };

        session_manager
            .update(&session.id)
            .recipe(Some(recipe))
            .apply()
            .await
            .unwrap();

        // Fill the cache (capacity 1) then evict it
        let agent = manager
            .get_or_create_agent(session.id.clone())
            .await
            .unwrap();
        let tools = agent.list_tools(&session.id, None).await;
        assert!(
            tools
                .iter()
                .any(|t| t.name.as_ref() == FINAL_OUTPUT_TOOL_NAME),
            "final_output_tool must be present on first creation"
        );

        // Evict by adding a second session
        manager
            .get_or_create_agent("evict-trigger".into())
            .await
            .unwrap();
        assert!(
            !manager.has_session(&session.id).await,
            "session should be evicted"
        );

        // Recreate agent via slow path (create_agent_locked)
        let restored_agent = manager
            .get_or_create_agent(session.id.clone())
            .await
            .unwrap();
        let tools = restored_agent.list_tools(&session.id, None).await;
        assert!(
            tools
                .iter()
                .any(|t| t.name.as_ref() == FINAL_OUTPUT_TOOL_NAME),
            "final_output_tool must be restored after LRU eviction"
        );
    }
}
