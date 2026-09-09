use anyhow::Result;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::acp::{
    configured_model_for_provider, extension_configs_to_mcp_servers, AcpProvider,
    AcpProviderConfig, ACP_CURRENT_MODEL,
};
use crate::config::search_path::SearchPaths;
use crate::config::{Config, GooseMode};
use crate::providers::base::{
    current_working_dir, ProviderDef, ProviderDescriptor, ProviderMetadata,
};
use crate::providers::catalog::ProviderSetupMetadata;

pub(crate) const COPILOT_ACP_PROVIDER_NAME: &str = "copilot-acp";
const COPILOT_ACP_DOC_URL: &str = "https://github.com/github/copilot-cli";
pub(crate) const COPILOT_ACP_BINARY: &str = "copilot";

const MODE_AGENT: &str = "https://agentclientprotocol.com/protocol/session-modes#agent";
const MODE_PLAN: &str = "https://agentclientprotocol.com/protocol/session-modes#plan";

pub struct CopilotAcpProvider;

impl goose_providers::base::ProviderDescriptor for CopilotAcpProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            COPILOT_ACP_PROVIDER_NAME,
            "GitHub Copilot CLI (ACP)",
            "Use goose with your GitHub Copilot subscription via the Copilot CLI.",
            ACP_CURRENT_MODEL,
            vec![],
            COPILOT_ACP_DOC_URL,
            vec![],
        )
        .with_setup_steps(vec![
            "Install the Copilot CLI: `npm install -g @github/copilot`",
            "Run `copilot login` to authenticate with your GitHub account",
        ])
        .with_setup(
            ProviderSetupMetadata::cli_agent(
                COPILOT_ACP_BINARY,
                &["copilot-acp", "github_copilot", "github_copilot_cli"],
            )
            .with_acp()
            .with_docs_url("https://docs.github.com/en/copilot/github-copilot-in-the-cli")
            .with_capabilities(true, true, false),
        )
    }
}

impl CopilotAcpProvider {
    fn create(
        extensions: Vec<crate::config::ExtensionConfig>,
        working_dir: PathBuf,
        use_default_model: bool,
    ) -> BoxFuture<'static, Result<AcpProvider>> {
        Box::pin(async move {
            let config = Config::global();
            // with_npm() includes npm global bin dir (desktop app PATH may not)
            let resolved_command = SearchPaths::builder()
                .with_npm()
                .resolve(COPILOT_ACP_BINARY)?;
            let goose_mode = config.get_goose_mode().unwrap_or(GooseMode::Auto);
            let model = if use_default_model {
                ACP_CURRENT_MODEL.to_string()
            } else {
                configured_model_for_provider(config, COPILOT_ACP_PROVIDER_NAME)
            };

            let args = vec!["--acp".to_string()];
            let session_config_options = if model == ACP_CURRENT_MODEL {
                vec![]
            } else {
                vec![("model".to_string(), model)]
            };

            let mode_mapping = HashMap::from([
                (GooseMode::Auto, vec![MODE_AGENT.to_string()]),
                (GooseMode::Approve, vec![MODE_AGENT.to_string()]),
                (GooseMode::SmartApprove, vec![MODE_AGENT.to_string()]),
                (GooseMode::Chat, vec![MODE_PLAN.to_string()]),
            ]);

            let provider_config = AcpProviderConfig {
                command: resolved_command,
                args,
                env: vec![],
                env_remove: vec![],
                work_dir: working_dir,
                mcp_servers: extension_configs_to_mcp_servers(&extensions),
                session_mode_id: mode_mapping[&goose_mode].first().cloned(),
                session_config_options,
                model_config_option_id: Some("model".to_string()),
                mode_mapping,
                notification_callback: None,
            };

            let metadata = Self::metadata();
            AcpProvider::connect(metadata.name, goose_mode, provider_config).await
        })
    }
}

impl ProviderDef for CopilotAcpProvider {
    type Provider = AcpProvider;

    fn from_env(
        extensions: Vec<crate::config::ExtensionConfig>,
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<AcpProvider>> {
        Self::from_env_with_working_dir(extensions, current_working_dir(), tls_config)
    }

    fn from_env_with_working_dir(
        extensions: Vec<crate::config::ExtensionConfig>,
        working_dir: PathBuf,
        _tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<AcpProvider>> {
        Self::create(extensions, working_dir, false)
    }

    fn from_env_with_default_model(
        extensions: Vec<crate::config::ExtensionConfig>,
        _tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<AcpProvider>> {
        Self::create(extensions, current_working_dir(), true)
    }
}
