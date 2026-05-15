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
use crate::providers::acp_tooling::{acp_adapter_installed, acp_inventory_identity};
use crate::providers::base::{ProviderDef, ProviderMetadata};
use crate::providers::inventory::InventoryIdentityInput;

const CURSOR_ACP_PROVIDER_NAME: &str = "cursor-acp";
const CURSOR_ACP_DOC_URL: &str = "https://cursor.com/docs/cli/acp";
const CURSOR_ACP_BINARY: &str = "cursor-agent";

pub struct CursorAcpProvider;

impl ProviderDef for CursorAcpProvider {
    type Provider = AcpProvider;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            CURSOR_ACP_PROVIDER_NAME,
            "Cursor (ACP)",
            "Use goose with your Cursor subscription via the cursor-agent ACP adapter.",
            ACP_CURRENT_MODEL,
            vec![],
            CURSOR_ACP_DOC_URL,
            vec![],
        )
        .with_setup_steps(vec![
            "Install the Cursor agent CLI (download from https://cursor.com)",
            "Ensure your Cursor CLI is authenticated (run `cursor-agent login` to verify)",
            "Set in your goose config file (`~/.config/goose/config.yaml` on macOS/Linux):\n  GOOSE_PROVIDER: cursor-acp\n  GOOSE_MODEL: current",
            "Restart goose for changes to take effect",
        ])
    }

    fn from_env(
        model: ModelConfig,
        extensions: Vec<crate::config::ExtensionConfig>,
    ) -> BoxFuture<'static, Result<AcpProvider>> {
        Box::pin(async move {
            let config = Config::global();
            let resolved_command = SearchPaths::builder()
                .with_npm()
                .resolve(CURSOR_ACP_BINARY)?;
            let goose_mode = config.get_goose_mode().unwrap_or(GooseMode::Auto);

            let mode_mapping = HashMap::from([
                (GooseMode::Auto, "full-auto".to_string()),
                (GooseMode::Approve, "default".to_string()),
                (GooseMode::SmartApprove, "auto-accept-edits".to_string()),
                (GooseMode::Chat, "plan".to_string()),
            ]);

            let provider_config = AcpProviderConfig {
                command: resolved_command,
                args: vec!["acp".to_string()],
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

    fn supports_inventory_refresh() -> bool {
        true
    }

    fn inventory_identity() -> Result<InventoryIdentityInput> {
        acp_inventory_identity(CURSOR_ACP_PROVIDER_NAME, CURSOR_ACP_BINARY)
    }

    fn inventory_configured() -> bool {
        acp_adapter_installed(CURSOR_ACP_BINARY)
    }
}
