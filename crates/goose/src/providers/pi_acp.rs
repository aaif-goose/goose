use anyhow::Result;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::acp::{
    extension_configs_to_mcp_servers, AcpProvider, AcpProviderConfig, ACP_CURRENT_MODEL,
};
use crate::config::search_path::SearchPaths;
use crate::config::{Config, GooseMode};
use crate::model::ModelConfig;
use crate::providers::base::{ProviderDef, ProviderMetadata};

const PI_ACP_PROVIDER_NAME: &str = "pi-acp";
const PI_ACP_DOC_URL: &str = "https://github.com/anthropics/pi";
const PI_ACP_BINARY: &str = "pi-acp";

pub struct PiAcpProvider;

impl ProviderDef for PiAcpProvider {
    type Provider = AcpProvider;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            PI_ACP_PROVIDER_NAME,
            "Pi",
            "Use goose with Pi via the pi-acp adapter.",
            ACP_CURRENT_MODEL,
            vec![],
            PI_ACP_DOC_URL,
            vec![],
        )
        .with_setup_steps(vec![
            "Install the Pi CLI and the pi-acp adapter",
            "Ensure your Pi CLI is authenticated (run `pi` to verify)",
            "Set in your goose config file (`~/.config/goose/config.yaml` on macOS/Linux):\n  GOOSE_PROVIDER: pi-acp\n  GOOSE_MODEL: current",
            "Restart goose for changes to take effect",
        ])
    }

    fn from_env(
        model: ModelConfig,
        extensions: Vec<crate::config::ExtensionConfig>,
    ) -> BoxFuture<'static, Result<AcpProvider>> {
        Box::pin(async move {
            let config = Config::global();
            let resolved_command = SearchPaths::builder().with_npm().resolve(PI_ACP_BINARY)?;
            let goose_mode = config.get_goose_mode().unwrap_or(GooseMode::Auto);

            let mode_mapping = HashMap::from([
                (GooseMode::Auto, "auto".to_string()),
                (GooseMode::Approve, "approve".to_string()),
                (GooseMode::SmartApprove, "smart-approve".to_string()),
                (GooseMode::Chat, "chat".to_string()),
            ]);

            let provider_config = AcpProviderConfig {
                command: resolved_command,
                args: vec![],
                env: vec![],
                env_remove: vec![],
                work_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                mcp_servers: extension_configs_to_mcp_servers(&extensions),
                session_mode_id: Some(mode_mapping[&goose_mode].clone()),
                mode_mapping,
                notification_callback: None,
            };

            let metadata = Self::metadata();
            AcpProvider::connect(metadata.name, model, goose_mode, provider_config).await
        })
    }
}
