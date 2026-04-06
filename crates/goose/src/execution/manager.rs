use crate::agents::{Agent, AgentConfig, GoosePlatform};
use crate::config::paths::Paths;
use crate::config::permission::PermissionManager;
use crate::config::{Config, GooseMode};
use crate::scheduler::Scheduler;
use crate::scheduler_trait::SchedulerTrait;
use crate::session::SessionManager;
use anyhow::Result;
use lru::LruCache;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::{OnceCell, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

const DEFAULT_MAX_SESSION: usize = 100;

static AGENT_MANAGER: OnceCell<Arc<AgentManager>> = OnceCell::const_new();

pub struct AgentManager {
    sessions: Arc<RwLock<LruCache<String, Arc<Agent>>>>,
    scheduler: Arc<dyn SchedulerTrait>,
    session_manager: Arc<SessionManager>,
    default_provider: Arc<RwLock<Option<Arc<dyn crate::providers::base::Provider>>>>,
    default_mode: GooseMode,
    cancel_tokens: Arc<RwLock<HashMap<String, CancellationToken>>>,
}

impl AgentManager {
    pub async fn new(
        session_manager: Arc<SessionManager>,
        schedule_file_path: std::path::PathBuf,
        max_sessions: Option<usize>,
        default_mode: GooseMode,
    ) -> Result<Self> {
        let scheduler = Scheduler::new(schedule_file_path, session_manager.clone()).await?;

        let capacity = NonZeroUsize::new(max_sessions.unwrap_or(DEFAULT_MAX_SESSION))
            .unwrap_or_else(|| NonZeroUsize::new(100).unwrap());

        let manager = Self {
            sessions: Arc::new(RwLock::new(LruCache::new(capacity))),
            scheduler,
            session_manager,
            default_provider: Arc::new(RwLock::new(None)),
            default_mode,
            cancel_tokens: Arc::new(RwLock::new(HashMap::new())),
        };

        Ok(manager)
    }

    pub async fn instance() -> Result<Arc<Self>> {
        AGENT_MANAGER
            .get_or_try_init(|| async {
                let config = Config::global();
                let max_sessions = config
                    .get_goose_max_active_agents()
                    .unwrap_or(DEFAULT_MAX_SESSION);
                let default_mode = config.get_goose_mode().unwrap_or_default();
                let schedule_file_path = Paths::data_dir().join("schedule.json");
                let session_manager = Arc::new(SessionManager::instance());
                let manager = Self::new(
                    session_manager,
                    schedule_file_path,
                    Some(max_sessions),
                    default_mode,
                )
                .await?;
                Ok(Arc::new(manager))
            })
            .await
            .cloned()
    }

    pub fn scheduler(&self) -> Arc<dyn SchedulerTrait> {
        Arc::clone(&self.scheduler)
    }

    /// Get the shared SessionManager for session-only operations
    pub fn session_manager(&self) -> &SessionManager {
        &self.session_manager
    }

    pub async fn set_default_provider(&self, provider: Arc<dyn crate::providers::base::Provider>) {
        debug!("Setting default provider on AgentManager");
        *self.default_provider.write().await = Some(provider);
    }

    pub async fn get_or_create_agent(&self, session_id: String) -> Result<Arc<Agent>> {
        {
            let mut sessions = self.sessions.write().await;
            if let Some(existing) = sessions.get(&session_id) {
                return Ok(Arc::clone(existing));
            }
        }

        let mut mode = self.default_mode;
        let permission_manager = PermissionManager::instance();

        if let Ok(session) = self.session_manager.get_session(&session_id, false).await {
            mode = session.goose_mode;
            info!(goose_mode = %mode, session_id = %session_id, "Session loaded");
        }

        let config = AgentConfig::new(
            Arc::clone(&self.session_manager),
            permission_manager,
            Some(Arc::clone(&self.scheduler)),
            mode,
            Config::global()
                .get_goose_disable_session_naming()
                .unwrap_or(false),
            GoosePlatform::GooseDesktop,
        );
        let agent = Arc::new(Agent::with_config(config));

        if let Ok(session) = self.session_manager.get_session(&session_id, false).await {
            if session.provider_name.is_some() {
                info!(
                    "Restoring evicted session {} (provider: {:?})",
                    session_id, session.provider_name
                );
                if let Err(e) = agent.restore_provider_from_session(&session).await {
                    tracing::warn!(
                        "Failed to restore provider for session {}: {}",
                        session_id,
                        e
                    );
                }
            }
            agent.load_extensions_from_session(&session).await;
        }

        if agent.provider().await.is_err() {
            if let Some(provider) = &*self.default_provider.read().await {
                agent
                    .update_provider(Arc::clone(provider), &session_id)
                    .await?;
                provider
                    .update_mode(&session_id, mode)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to propagate mode to provider: {}", e))?;
            }
        }

        let mut sessions = self.sessions.write().await;
        if let Some(existing) = sessions.get(&session_id) {
            Ok(Arc::clone(existing))
        } else {
            sessions.put(session_id, agent.clone());
            Ok(agent)
        }
    }

    pub async fn remove_session(&self, session_id: &str) -> Result<()> {
        if let Some(token) = self.cancel_tokens.write().await.remove(session_id) {
            token.cancel();
        }
        let mut sessions = self.sessions.write().await;
        sessions
            .pop(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session {} not found", session_id))?;
        info!("Removed session {}", session_id);
        Ok(())
    }

    pub async fn has_session(&self, session_id: &str) -> bool {
        self.sessions.read().await.contains(session_id)
    }

    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
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

    /// List session IDs that currently have active agents loaded
    pub async fn list_active_session_ids(&self) -> Vec<String> {
        self.sessions
            .read()
            .await
            .iter()
            .map(|(id, _)| id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use tempfile::TempDir;

    use test_case::test_case;

    use crate::config::GooseMode;
    use crate::execution::SessionExecutionMode;
    use crate::session::SessionManager;

    use super::AgentManager;

    async fn create_test_manager(temp_dir: &TempDir) -> AgentManager {
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let schedule_path = temp_dir.path().join("schedule.json");
        AgentManager::new(
            session_manager,
            schedule_path,
            Some(100),
            GooseMode::default(),
        )
        .await
        .unwrap()
    }

    #[test]
    fn test_execution_mode_constructors() {
        assert_eq!(
            SessionExecutionMode::chat(),
            SessionExecutionMode::Interactive
        );
        assert_eq!(
            SessionExecutionMode::scheduled(),
            SessionExecutionMode::Background
        );

        let parent = "parent-123".to_string();
        assert_eq!(
            SessionExecutionMode::task(parent.clone()),
            SessionExecutionMode::SubTask {
                parent_session: parent
            }
        );
    }

    #[tokio::test]
    async fn test_session_isolation() {
        let temp_dir = TempDir::new().unwrap();
        let manager = create_test_manager(&temp_dir).await;

        let session1 = uuid::Uuid::new_v4().to_string();
        let session2 = uuid::Uuid::new_v4().to_string();

        let agent1 = manager.get_or_create_agent(session1.clone()).await.unwrap();

        let agent2 = manager.get_or_create_agent(session2.clone()).await.unwrap();

        // Different sessions should have different agents
        assert!(!Arc::ptr_eq(&agent1, &agent2));

        // Getting the same session should return the same agent
        let agent1_again = manager.get_or_create_agent(session1).await.unwrap();

        assert!(Arc::ptr_eq(&agent1, &agent1_again));
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
    async fn test_concurrent_access() {
        let temp_dir = TempDir::new().unwrap();
        let manager = Arc::new(create_test_manager(&temp_dir).await);
        let session = String::from("concurrent-test");

        let mut handles = vec![];
        for _ in 0..10 {
            let mgr = Arc::clone(&manager);
            let sess = session.clone();
            handles.push(tokio::spawn(async move {
                mgr.get_or_create_agent(sess).await.unwrap()
            }));
        }

        let agents: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        for agent in &agents[1..] {
            assert!(Arc::ptr_eq(&agents[0], agent));
        }

        assert_eq!(manager.session_count().await, 1);
    }

    #[tokio::test]
    async fn test_concurrent_session_creation_race_condition() {
        // Test that concurrent attempts to create the same new session ID
        // result in only one agent being created (tests double-check pattern)
        let temp_dir = TempDir::new().unwrap();
        let manager = Arc::new(create_test_manager(&temp_dir).await);
        let session_id = String::from("race-condition-test");

        // Spawn multiple tasks trying to create the same NEW session simultaneously
        let mut handles = vec![];
        for _ in 0..20 {
            let sess = session_id.clone();
            let mgr_clone = Arc::clone(&manager);
            handles.push(tokio::spawn(async move {
                mgr_clone.get_or_create_agent(sess).await.unwrap()
            }));
        }

        // Collect all agents
        let agents: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        for agent in &agents[1..] {
            assert!(
                Arc::ptr_eq(&agents[0], agent),
                "All concurrent requests should get the same agent"
            );
        }
        assert_eq!(manager.session_count().await, 1);
    }

    #[tokio::test]
    async fn test_set_default_provider() {
        use crate::providers::testprovider::TestProvider;

        let temp_dir = TempDir::new().unwrap();
        let manager = create_test_manager(&temp_dir).await;

        // Create a test provider for replaying (doesn't need inner provider)
        let temp_file = temp_dir.path().join("test_provider.json");

        // Create an empty test provider (will fail on actual use but that's ok for this test)
        std::fs::write(&temp_file, "{}").unwrap();
        let test_provider = TestProvider::new_replaying(temp_file.to_str().unwrap()).unwrap();

        manager.set_default_provider(Arc::new(test_provider)).await;

        let session = String::from("provider-test");
        let _agent = manager.get_or_create_agent(session.clone()).await.unwrap();

        assert!(manager.has_session(&session).await);
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

    #[test_case(GooseMode::Approve ; "approve")]
    #[test_case(GooseMode::Chat ; "chat")]
    #[test_case(GooseMode::SmartApprove ; "smart_approve")]
    #[tokio::test]
    async fn test_agent_inherits_session_mode(mode: GooseMode) {
        let temp_dir = TempDir::new().unwrap();
        let manager = create_test_manager(&temp_dir).await;

        let session = manager
            .session_manager()
            .create_session(
                temp_dir.path().to_path_buf(),
                "test".into(),
                crate::session::SessionType::User,
                mode,
            )
            .await
            .unwrap();

        let agent = manager.get_or_create_agent(session.id).await.unwrap();
        assert_eq!(agent.goose_mode().await, mode);
    }

    #[tokio::test]
    async fn test_session_mode_isolation() {
        let temp_dir = TempDir::new().unwrap();
        let manager = create_test_manager(&temp_dir).await;
        let sm = manager.session_manager();

        let s1 = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "s1".into(),
                crate::session::SessionType::User,
                GooseMode::Approve,
            )
            .await
            .unwrap();
        let s2 = sm
            .create_session(
                temp_dir.path().to_path_buf(),
                "s2".into(),
                crate::session::SessionType::User,
                GooseMode::Auto,
            )
            .await
            .unwrap();

        let a1 = manager.get_or_create_agent(s1.id).await.unwrap();
        let a2 = manager.get_or_create_agent(s2.id).await.unwrap();

        assert_eq!(a1.goose_mode().await, GooseMode::Approve);
        assert_eq!(a2.goose_mode().await, GooseMode::Auto);
    }
}
