use anyhow::Result;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::acp::{
    extension_configs_to_mcp_servers, AcpProvider, AcpProviderConfig, ACP_CURRENT_MODEL,
};
use crate::config::search_path::SearchPaths;
use crate::config::{Config, GooseMode};
use crate::providers::base::{
    current_working_dir, ProviderDef, ProviderDescriptor, ProviderMetadata,
};
use crate::providers::catalog::ProviderSetupMetadata;

pub(crate) const CLAUDE_ACP_PROVIDER_NAME: &str = "claude-acp";
const CLAUDE_ACP_DOC_URL: &str = "https://github.com/agentclientprotocol/claude-agent-acp";
pub(crate) const CLAUDE_ACP_BINARY: &str = "claude-agent-acp";

pub struct ClaudeAcpProvider;

impl goose_providers::base::ProviderDescriptor for ClaudeAcpProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            CLAUDE_ACP_PROVIDER_NAME,
            "Claude Code ACP",
            "Use goose with your Claude Code subscription via the claude-agent-acp adapter.",
            ACP_CURRENT_MODEL,
            vec![],
            CLAUDE_ACP_DOC_URL,
            vec![],
        )
        .with_setup_steps(vec![
            "Install the ACP adapter: `npm install -g @agentclientprotocol/claude-agent-acp`",
            "Ensure your Claude CLI is authenticated (run `claude` to verify)",
        ])
        .with_setup(
            ProviderSetupMetadata::cli_agent(
                CLAUDE_ACP_BINARY,
                &["claude-acp", "claude_code", "claude"],
            )
            .with_acp()
            .with_docs_url("https://docs.anthropic.com/en/docs/claude-code")
            .with_capabilities(true, true, true),
        )
    }
}

impl ProviderDef for ClaudeAcpProvider {
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
        Box::pin(async move {
            let config = Config::global();
            // with_npm() includes npm global bin dir (desktop app PATH may not)
            let resolved_command = SearchPaths::builder()
                .with_npm()
                .resolve(CLAUDE_ACP_BINARY)?;
            let goose_mode = config.get_goose_mode().unwrap_or(GooseMode::Auto);

            let mode_mapping = HashMap::from([
                // Closest to "autonomous": bypassPermissions skips confirmations.
                (GooseMode::Auto, vec!["bypassPermissions".to_string()]),
                // Claude Code's default matches "ask before risky actions".
                (GooseMode::Approve, vec!["default".to_string()]),
                // acceptEdits auto-accepts file edits but still prompts for risky ops.
                (GooseMode::SmartApprove, vec!["acceptEdits".to_string()]),
                // Plan mode disables tool execution, aligning with chat-only intent.
                (GooseMode::Chat, vec!["plan".to_string()]),
            ]);

            let provider_config = AcpProviderConfig {
                command: resolved_command,
                args: vec![],
                env: vec![],
                // Prevent nested-session detection in claude-agent-acp (wraps Claude Code)
                env_remove: vec!["CLAUDECODE".to_string()],
                work_dir: working_dir,
                mcp_servers: extension_configs_to_mcp_servers(&extensions),
                session_mode_id: mode_mapping[&goose_mode].first().cloned(),
                session_config_options: vec![],
                // claude-agent-acp advertises the model as a "model" select
                // config option and applies session/set_config_option for it
                // via query.setModel, so forward the picker's selection.
                model_config_option_id: Some("model".to_string()),
                mode_mapping,
                notification_callback: None,
            };

            let metadata = Self::metadata();
            AcpProvider::connect(metadata.name, goose_mode, provider_config).await
        })
    }
}
