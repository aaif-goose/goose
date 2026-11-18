use crate::acp::server::{
    acp_services_config, AcpBuiltinSelection, AcpConnection, AcpConnectionOptions,
    AcpProviderFactory, LiveVoiceService,
};
use crate::agents::GoosePlatform;
use crate::agents::StateMachineServices;
use crate::config::paths::Paths;
use crate::session::SessionManager;
use crate::source_roots::SourceRoot;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::OnceCell;
use tracing::info;

pub struct AcpServerFactoryConfig {
    pub builtins: AcpBuiltinSelection,
    pub config_dir: std::path::PathBuf,
    pub goose_platform: GoosePlatform,
    pub additional_source_roots: Vec<SourceRoot>,
    /// When set, new sessions use this host-controlled working directory
    /// instead of the `cwd` the connecting client sends. Used by roaming, where
    /// the connector's absolute path is meaningless on the host machine.
    pub session_cwd: Option<std::path::PathBuf>,
    pub enable_scheduler: bool,
}

pub struct AcpServer {
    config: AcpServerFactoryConfig,
    data_dir: std::path::PathBuf,
    services: OnceCell<Arc<StateMachineServices>>,
    live_voice: std::sync::OnceLock<Arc<LiveVoiceService>>,
}

impl AcpServer {
    pub fn new(config: AcpServerFactoryConfig) -> Self {
        let data_dir = Paths::data_dir();
        Self {
            config,
            data_dir,
            services: OnceCell::new(),
            live_voice: std::sync::OnceLock::new(),
        }
    }

    /// Builds the services, and with them the scheduler, before any client connects, so
    /// a headless `goose serve` runs scheduled jobs; on failure the first connection retries.
    pub async fn start_scheduler(&self) -> Result<()> {
        self.services().await.map(|_| ())
    }

    async fn services(&self) -> Result<Arc<StateMachineServices>> {
        self.services
            .get_or_try_init(|| async {
                let session_manager = Arc::new(SessionManager::new(self.data_dir.clone()));
                #[cfg(feature = "scheduler")]
                let scheduler: Option<
                    Arc<dyn crate::scheduler_trait::SchedulerTrait>,
                > = if self.config.enable_scheduler {
                    Some(
                        crate::scheduler::Scheduler::new(
                            self.data_dir.join("schedule.json"),
                            session_manager.clone(),
                        )
                        .await?,
                    )
                } else {
                    None
                };
                #[cfg(not(feature = "scheduler"))]
                let scheduler = None;
                let config = acp_services_config(
                    session_manager,
                    self.config.config_dir.clone(),
                    crate::config::Config::global()
                        .get_goose_disable_session_naming()
                        .unwrap_or(false),
                    self.config.goose_platform.clone(),
                );
                anyhow::Ok(StateMachineServices::with_config(
                    crate::agents::StateMachineServicesConfig {
                        scheduler_service: scheduler,
                        ..config
                    },
                ))
            })
            .await
            .cloned()
    }

    pub async fn create_connection(&self) -> Result<Arc<AcpConnection>> {
        self.create_connection_with_session_cwd(self.config.session_cwd.clone())
            .await
    }

    /// Create an agent whose sessions use `session_cwd` instead of this
    /// server's configured default. Used by the roaming bridge on `goose
    /// serve --roam`: the serve-wide server keeps `session_cwd: None` for
    /// local ACP clients whose paths are real on this machine, while each
    /// roaming connection gets a host-controlled working directory (the
    /// connector's absolute path is meaningless here).
    pub async fn create_connection_with_session_cwd(
        &self,
        session_cwd: Option<std::path::PathBuf>,
    ) -> Result<Arc<AcpConnection>> {
        let services = self.services().await?;

        let provider_factory: AcpProviderFactory = Arc::new(|provider_name| {
            Box::pin(async move {
                crate::providers::create_with_default_model(&provider_name, Vec::new()).await
            })
        });

        let connection = AcpConnection::new(AcpConnectionOptions {
            provider_factory,
            builtin_selection: self.config.builtins.clone(),
            config_dir: self.config.config_dir.clone(),
            additional_source_roots: self.config.additional_source_roots.clone(),
            session_cwd,
            live_voice: Arc::clone(self.live_voice.get_or_init(|| {
                Arc::new(LiveVoiceService::from_config(services.runtime.runs.clone()))
            })),
            services,
        })
        .await?;
        info!("Created new ACP agent");

        Ok(Arc::new(connection))
    }

    #[cfg(all(test, feature = "scheduler"))]
    pub(crate) fn new_for_test(
        config: AcpServerFactoryConfig,
        data_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            config,
            data_dir,
            services: OnceCell::new(),
            live_voice: std::sync::OnceLock::new(),
        }
    }
}

#[cfg(all(test, feature = "scheduler"))]
mod tests {
    use super::*;

    fn server(data_dir: std::path::PathBuf, enable_scheduler: bool) -> AcpServer {
        AcpServer::new_for_test(
            AcpServerFactoryConfig {
                builtins: AcpBuiltinSelection::default(),
                config_dir: data_dir.clone(),
                goose_platform: GoosePlatform::GooseCli,
                additional_source_roots: Vec::new(),
                session_cwd: None,
                enable_scheduler,
            },
            data_dir,
        )
    }

    #[tokio::test]
    async fn disabled_server_does_not_construct_scheduler() {
        let root = tempfile::tempdir().unwrap();
        let server = server(root.path().to_path_buf(), false);

        let services = server.services().await.unwrap();
        assert!(services.scheduler.is_none());
        assert!(!root.path().join("schedule.json").exists());
    }

    #[tokio::test]
    async fn automatic_server_constructs_scheduler() {
        let root = tempfile::tempdir().unwrap();
        let server = server(root.path().to_path_buf(), true);

        let services = server.services().await.unwrap();
        assert!(services.scheduler.is_some());
    }

    #[tokio::test]
    async fn connections_to_one_server_share_its_services() {
        let root = tempfile::tempdir().unwrap();
        let server = server(root.path().to_path_buf(), false);

        let a = server.create_connection().await.unwrap();
        let b = server.create_connection().await.unwrap();

        assert!(Arc::ptr_eq(a.services(), b.services()));
    }

    #[tokio::test]
    async fn a_steer_on_another_connection_finds_the_run() {
        let root = tempfile::tempdir().unwrap();
        let server = server(root.path().to_path_buf(), false);

        let running = server.create_connection().await.unwrap();
        let steering = server.create_connection().await.unwrap();

        running
            .test_start_active_run("session-1", "run-1".to_string())
            .await
            .unwrap();

        let run_id = steering
            .test_require_active_run("session-1", "run-1")
            .await
            .unwrap();

        assert_eq!(run_id, "run-1");
    }

    #[tokio::test]
    async fn dropping_a_prompt_future_releases_the_shared_run() {
        let root = tempfile::tempdir().unwrap();
        let server = server(root.path().to_path_buf(), false);

        let running = server.create_connection().await.unwrap();
        running
            .test_start_active_run("session-1", "run-1".to_string())
            .await
            .unwrap();

        running.test_drop_active_run_guard("session-1", "run-1");
        tokio::task::yield_now().await;

        let second = server.create_connection().await.unwrap();
        assert!(
            second
                .test_require_active_run("session-1", "run-1")
                .await
                .is_err(),
            "a dropped prompt future must release its run so later \
             connections are not permanently locked out of the session"
        );
    }

    #[tokio::test]
    async fn start_scheduler_initializes_before_any_client_connects() {
        let root = tempfile::tempdir().unwrap();
        let server = server(root.path().to_path_buf(), true);

        assert!(!server.services.initialized());
        server.start_scheduler().await.unwrap();
        assert!(server.services.initialized());
    }

    #[tokio::test]
    async fn start_scheduler_is_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let server = server(root.path().to_path_buf(), true);

        server.start_scheduler().await.unwrap();
        server.start_scheduler().await.unwrap();
    }
}
