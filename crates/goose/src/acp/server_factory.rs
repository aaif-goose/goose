use crate::acp::server::{AcpProviderFactory, GooseAcpAgent};
use crate::agents::GoosePlatform;
use anyhow::Result;
use std::sync::Arc;
use tracing::info;

pub struct AcpServerFactoryConfig {
    pub builtins: Vec<String>,
    pub data_dir: std::path::PathBuf,
    pub config_dir: std::path::PathBuf,
    pub goose_platform: GoosePlatform,
}

pub struct AcpServer {
    config: AcpServerFactoryConfig,
}

impl AcpServer {
    pub fn new(config: AcpServerFactoryConfig) -> Self {
        Self { config }
    }

    pub async fn create_agent(&self) -> Result<Arc<GooseAcpAgent>> {
        let config_path = self
            .config
            .config_dir
            .join(crate::config::base::CONFIG_YAML_NAME);
        let config = crate::config::Config::new(&config_path, "goose")?;

        let goose_mode = config
            .get_goose_mode()
            .unwrap_or(crate::config::GooseMode::Auto);
        let disable_session_naming = config.get_goose_disable_session_naming().unwrap_or(false);

        let provider_factory: AcpProviderFactory =
            Arc::new(move |provider_name, model_config, extensions| {
                Box::pin(async move {
                    crate::providers::create(&provider_name, model_config, extensions).await
                })
            });

        let agent = GooseAcpAgent::new(
            provider_factory,
            self.config.builtins.clone(),
            self.config.data_dir.clone(),
            self.config.config_dir.clone(),
            goose_mode,
            disable_session_naming,
            self.config.goose_platform.clone(),
        )
        .await?;
        info!("Created new ACP agent");

        Ok(Arc::new(agent))
    }
}
