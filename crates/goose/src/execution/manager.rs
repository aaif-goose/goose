use crate::agents::{GoosePlatform, StateMachineServices, StateMachineServicesConfig};
use crate::config::permission::PermissionManager;
use crate::config::Config;
use crate::scheduler_trait::SchedulerTrait;
use crate::session::SessionManager;
use anyhow::Result;
use lru::LruCache;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};
use tracing::warn;

const DEFAULT_MAX_SESSION: usize = 100;

static AGENT_MANAGER: OnceCell<Arc<AgentManager>> = OnceCell::const_new();

/// Serves every session from one `Agent`. A session's first use restores its provider;
/// once more than `max_sessions` are loaded the least recently used idle one is released.
pub struct AgentManager {
    agent: Arc<StateMachineServices>,
    max_sessions: usize,
    loaded_sessions: Mutex<LruCache<String, ()>>,
    /// Concurrent first requests for a session restore its provider once.
    load_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl AgentManager {
    pub fn new(agent_config: StateMachineServicesConfig, max_sessions: Option<usize>) -> Self {
        Self {
            agent: Arc::new(StateMachineServices::with_config(agent_config)),
            max_sessions: max_sessions.unwrap_or(DEFAULT_MAX_SESSION).max(1),
            loaded_sessions: Mutex::new(LruCache::unbounded()),
            load_locks: Mutex::new(HashMap::new()),
        }
    }

    pub async fn instance() -> Result<Arc<Self>> {
        AGENT_MANAGER
            .get_or_try_init(|| async {
                let config = Config::global();
                let max_sessions = config
                    .get_goose_max_active_agents()
                    .unwrap_or(DEFAULT_MAX_SESSION);
                let session_manager = Arc::new(SessionManager::instance());
                let agent_config = StateMachineServicesConfig::new(
                    session_manager,
                    PermissionManager::instance(),
                    None,
                    config.get_goose_disable_session_naming().unwrap_or(false),
                    GoosePlatform::GooseDesktop,
                );
                Ok(Arc::new(Self::new(agent_config, Some(max_sessions))))
            })
            .await
            .cloned()
    }

    pub fn agent(&self) -> &Arc<StateMachineServices> {
        &self.agent
    }

    pub fn scheduler(&self) -> Option<Arc<dyn SchedulerTrait>> {
        self.agent.config.scheduler_service.clone()
    }

    pub fn session_manager(&self) -> &Arc<SessionManager> {
        &self.agent.config.session_manager
    }

    pub async fn agent_for_session(&self, session_id: &str) -> Result<Arc<StateMachineServices>> {
        if self.loaded_sessions.lock().await.get(session_id).is_none() {
            let load_lock = Arc::clone(
                self.load_locks
                    .lock()
                    .await
                    .entry(session_id.to_string())
                    .or_default(),
            );
            let load_guard = load_lock.lock().await;
            let result = self.load(session_id).await;
            drop(load_guard);
            drop(load_lock);
            self.prune_load_lock(session_id).await;
            result?;
        }
        Ok(Arc::clone(&self.agent))
    }

    async fn load(&self, session_id: &str) -> Result<()> {
        if self.loaded_sessions.lock().await.get(session_id).is_some() {
            return Ok(());
        }
        if let Ok(session) = self.session_manager().get_session(session_id, false).await {
            if session.provider_name.is_some() {
                if let Err(error) = self.agent.restore_provider_from_session(&session).await {
                    if crate::acp::is_auth_required(&error) {
                        return Err(error);
                    }
                    warn!(session_id, %error, "Failed to restore provider");
                }
            }
        }

        let evicted = {
            let mut loaded = self.loaded_sessions.lock().await;
            loaded.put(session_id.to_string(), ());
            let excess = loaded.len().saturating_sub(self.max_sessions);
            let evicted: Vec<String> = loaded
                .iter()
                .rev()
                .map(|(id, ())| id.clone())
                .filter(|id| !self.agent.has_active_turn(id))
                .take(excess)
                .collect();
            for id in &evicted {
                loaded.pop(id);
            }
            evicted
        };
        for id in evicted {
            self.agent.release_session(&id).await;
        }
        Ok(())
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

    pub async fn release_session(&self, session_id: &str) {
        self.loaded_sessions.lock().await.pop(session_id);
        self.agent.release_session(session_id).await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tempfile::TempDir;

    use goose_test_support::mcp::McpFixtureServer;
    use rmcp::ServiceExt;
    use tokio::sync::Barrier;

    use crate::agents::extension::ExtensionConfig;
    use crate::agents::{GoosePlatform, StateMachineServicesConfig};
    use crate::config::permission::PermissionManager;
    use crate::config::GooseMode;
    use crate::session::{EnabledExtensionsState, ExtensionState, SessionManager, SessionType};

    use super::AgentManager;

    fn test_manager(temp_dir: &TempDir, max_sessions: usize) -> AgentManager {
        let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let agent_config = StateMachineServicesConfig::new(
            session_manager,
            PermissionManager::instance(),
            None,
            true,
            GoosePlatform::GooseDesktop,
        );
        AgentManager::new(agent_config, Some(max_sessions))
    }

    fn serve_fixture(read: tokio::io::DuplexStream, write: tokio::io::DuplexStream) {
        tokio::spawn(async move {
            let running = McpFixtureServer::new().serve((read, write)).await.unwrap();
            let _ = running.waiting().await;
        });
    }

    static CONCURRENT_STARTS: AtomicUsize = AtomicUsize::new(0);

    fn serve_concurrent_fixture(read: tokio::io::DuplexStream, write: tokio::io::DuplexStream) {
        CONCURRENT_STARTS.fetch_add(1, Ordering::SeqCst);
        serve_fixture(read, write);
    }

    static EVICTION_STARTS: AtomicUsize = AtomicUsize::new(0);

    fn serve_eviction_fixture(read: tokio::io::DuplexStream, write: tokio::io::DuplexStream) {
        EVICTION_STARTS.fetch_add(1, Ordering::SeqCst);
        serve_fixture(read, write);
    }

    async fn session_with_fixture(
        manager: &AgentManager,
        temp_dir: &TempDir,
        fixture: &'static str,
        serve: crate::builtin_extension::SpawnServerFn,
    ) -> String {
        crate::builtin_extension::register_builtin_extension(fixture, serve);
        let session = manager
            .session_manager()
            .create_session(
                temp_dir.path().to_path_buf(),
                "fixture".to_string(),
                SessionType::User,
                GooseMode::default(),
            )
            .await
            .unwrap();
        let extension = ExtensionConfig::Builtin {
            name: fixture.to_string(),
            display_name: None,
            description: String::new(),
            timeout: None,
            bundled: None,
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

    async fn start_extensions(manager: &AgentManager, session_id: &str) {
        manager
            .agent_for_session(session_id)
            .await
            .unwrap()
            .extension_manager
            .current_lease(session_id)
            .await
            .unwrap()
            .start()
            .await;
    }

    #[tokio::test]
    async fn concurrent_first_requests_start_extensions_once() {
        let temp_dir = TempDir::new().unwrap();
        let manager = Arc::new(test_manager(&temp_dir, 100));
        let session_id = session_with_fixture(
            &manager,
            &temp_dir,
            "concurrent_fixture",
            serve_concurrent_fixture,
        )
        .await;

        let callers = 20;
        let barrier = Arc::new(Barrier::new(callers));
        let handles = (0..callers).map(|_| {
            let manager = Arc::clone(&manager);
            let session_id = session_id.clone();
            let barrier = Arc::clone(&barrier);
            tokio::spawn(async move {
                barrier.wait().await;
                start_extensions(&manager, &session_id).await;
            })
        });
        for handle in futures::future::join_all(handles).await {
            handle.unwrap();
        }

        assert_eq!(CONCURRENT_STARTS.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn eviction_releases_the_least_recently_used_idle_session() {
        let temp_dir = TempDir::new().unwrap();
        let manager = test_manager(&temp_dir, 2);
        let busy = session_with_fixture(
            &manager,
            &temp_dir,
            "eviction_fixture",
            serve_eviction_fixture,
        )
        .await;
        let idle = session_with_fixture(
            &manager,
            &temp_dir,
            "eviction_fixture",
            serve_eviction_fixture,
        )
        .await;
        let recent = session_with_fixture(
            &manager,
            &temp_dir,
            "eviction_fixture",
            serve_eviction_fixture,
        )
        .await;
        for session_id in [&busy, &idle] {
            start_extensions(&manager, session_id).await;
        }
        let _turn = manager.agent().hold_turn_for_test(&busy);
        let starts_before = EVICTION_STARTS.load(Ordering::SeqCst);

        start_extensions(&manager, &recent).await;
        start_extensions(&manager, &busy).await;
        assert_eq!(
            EVICTION_STARTS.load(Ordering::SeqCst) - starts_before,
            1,
            "only the new session starts; the busy one keeps its extensions"
        );

        start_extensions(&manager, &idle).await;
        assert_eq!(
            EVICTION_STARTS.load(Ordering::SeqCst) - starts_before,
            2,
            "the evicted idle session starts its extensions again"
        );
    }
}
