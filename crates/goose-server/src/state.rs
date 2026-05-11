use axum::http::StatusCode;
use goose::builtin_extension::register_builtin_extensions;
use goose::execution::manager::AgentManager;
use goose::scheduler_trait::SchedulerTrait;
use goose::session::SessionManager;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::session_event_bus::SessionEventBus;
use crate::tunnel::TunnelManager;
use goose::agents::ExtensionLoadResult;
use goose::gateway::manager::GatewayManager;
#[cfg(feature = "local-inference")]
use goose::providers::local_inference::InferenceRuntime;

type ExtensionLoadingTasks =
    Arc<Mutex<HashMap<String, Arc<Mutex<Option<JoinHandle<Vec<ExtensionLoadResult>>>>>>>>;

#[derive(Clone)]
pub struct AppState {
    pub(crate) agent_manager: Arc<AgentManager>,
    pub recipe_file_hash_map: Arc<Mutex<HashMap<String, PathBuf>>>,
    recipe_session_tracker: Arc<Mutex<HashSet<String>>>,
    pub tunnel_manager: Arc<TunnelManager>,
    pub gateway_manager: Arc<GatewayManager>,
    pub extension_loading_tasks: ExtensionLoadingTasks,
    #[cfg(feature = "local-inference")]
    inference_runtime: Arc<OnceLock<Arc<InferenceRuntime>>>,
    session_buses: Arc<Mutex<HashMap<String, Arc<SessionEventBus>>>>,
}

impl AppState {
    pub async fn new(tls: bool) -> anyhow::Result<Arc<AppState>> {
        register_builtin_extensions(goose_mcp::BUILTIN_EXTENSIONS.clone());

        let agent_manager = AgentManager::instance().await?;
        let tunnel_manager = Arc::new(TunnelManager::new(tls));
        let gateway_manager = Arc::new(GatewayManager::new(agent_manager.clone())?);

        Ok(Arc::new(Self {
            agent_manager,
            recipe_file_hash_map: Arc::new(Mutex::new(HashMap::new())),
            recipe_session_tracker: Arc::new(Mutex::new(HashSet::new())),
            tunnel_manager,
            gateway_manager,
            extension_loading_tasks: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(feature = "local-inference")]
            inference_runtime: Arc::new(OnceLock::new()),
            session_buses: Arc::new(Mutex::new(HashMap::new())),
        }))
    }

    #[cfg(feature = "local-inference")]
    pub fn get_inference_runtime(&self) -> anyhow::Result<Arc<InferenceRuntime>> {
        if let Some(runtime) = self.inference_runtime.get() {
            return Ok(runtime.clone());
        }

        let runtime = InferenceRuntime::get_or_init()?;

        // Another thread may win the race to cache the runtime in AppState.
        // In that case, return the already-initialized cached runtime.
        match self.inference_runtime.set(runtime.clone()) {
            Ok(()) => Ok(runtime),
            Err(_) => Ok(self
                .inference_runtime
                .get()
                .expect("inference runtime initialized by another thread")
                .clone()),
        }
    }

    pub async fn set_extension_loading_task(
        &self,
        session_id: String,
        task: JoinHandle<Vec<ExtensionLoadResult>>,
    ) {
        let mut tasks = self.extension_loading_tasks.lock().await;
        tasks.insert(session_id, Arc::new(Mutex::new(Some(task))));
    }

    pub async fn take_extension_loading_task(
        &self,
        session_id: &str,
    ) -> Option<Vec<ExtensionLoadResult>> {
        let task_holder = {
            let tasks = self.extension_loading_tasks.lock().await;
            tasks.get(session_id).cloned()
        };

        if let Some(holder) = task_holder {
            let task = holder.lock().await.take();
            if let Some(handle) = task {
                match handle.await {
                    Ok(results) => return Some(results),
                    Err(e) => {
                        tracing::warn!("Background extension loading task failed: {}", e);
                    }
                }
            }
        }
        None
    }

    pub async fn remove_extension_loading_task(&self, session_id: &str) {
        let mut tasks = self.extension_loading_tasks.lock().await;
        tasks.remove(session_id);
    }

    pub fn scheduler(&self) -> Arc<dyn SchedulerTrait> {
        self.agent_manager.scheduler()
    }

    pub fn session_manager(&self) -> &SessionManager {
        self.agent_manager.session_manager()
    }

    pub async fn set_recipe_file_hash_map(&self, hash_map: HashMap<String, PathBuf>) {
        let mut map = self.recipe_file_hash_map.lock().await;
        *map = hash_map;
    }

    pub async fn mark_recipe_run_if_absent(&self, session_id: &str) -> bool {
        let mut sessions = self.recipe_session_tracker.lock().await;
        if sessions.contains(session_id) {
            false
        } else {
            sessions.insert(session_id.to_string());
            true
        }
    }

    pub async fn get_or_create_event_bus(&self, session_id: &str) -> Arc<SessionEventBus> {
        let mut buses = self.session_buses.lock().await;
        buses
            .entry(session_id.to_string())
            .or_insert_with(|| Arc::new(SessionEventBus::new()))
            .clone()
    }

    /// Get an existing event bus for a session without creating one.
    pub async fn get_event_bus(&self, session_id: &str) -> Option<Arc<SessionEventBus>> {
        let buses = self.session_buses.lock().await;
        buses.get(session_id).cloned()
    }

    /// Remove the event bus for a session, freeing its replay buffer.
    pub async fn remove_event_bus(&self, session_id: &str) {
        let mut buses = self.session_buses.lock().await;
        buses.remove(session_id);
    }

    pub async fn get_agent(&self, session_id: String) -> anyhow::Result<Arc<goose::agents::Agent>> {
        self.agent_manager.get_or_create_agent(session_id).await
    }

    pub async fn get_agent_for_route(
        &self,
        session_id: String,
    ) -> Result<Arc<goose::agents::Agent>, StatusCode> {
        self.get_agent(session_id).await.map_err(|e| {
            tracing::error!("Failed to get agent: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
    }
}
