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

const CODEX_ACP_PROVIDER_NAME: &str = "codex-acp";
const CODEX_ACP_DOC_URL: &str = "https://github.com/zed-industries/codex-acp";

pub struct CodexAcpProvider;

impl ProviderDef for CodexAcpProvider {
    type Provider = AcpProvider;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            CODEX_ACP_PROVIDER_NAME,
            "Codex CLI",
            "Use goose with your ChatGPT Plus/Pro subscription via the codex-acp adapter.",
            ACP_CURRENT_MODEL,
            vec![],
            CODEX_ACP_DOC_URL,
            vec![],
        )
        .with_setup_steps(vec![
            "Install the ACP adapter: `npm install -g @zed-industries/codex-acp`",
            "Run `codex` once to authenticate with your OpenAI account",
            "Set in your goose config file (`~/.config/goose/config.yaml` on macOS/Linux):\n  GOOSE_PROVIDER: codex-acp\n  GOOSE_MODEL: current",
            "Restart goose for changes to take effect",
        ])
    }

    fn from_env(
        model: ModelConfig,
        extensions: Vec<crate::config::ExtensionConfig>,
    ) -> BoxFuture<'static, Result<AcpProvider>> {
        Box::pin(async move {
            let config = Config::global();
            // with_npm() includes npm global bin dir (desktop app PATH may not)
            let resolved_command = SearchPaths::builder()
                .with_npm()
                .resolve(CODEX_ACP_PROVIDER_NAME)?;
            let work_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let env = vec![];
            let goose_mode = config.get_goose_mode().unwrap_or(GooseMode::Auto);
            let mcp_servers = extension_configs_to_mcp_servers(&extensions);

            // fixed goose mode via -c overrides until session/set-mode works
            let (approval_policy, sandbox_mode) = map_goose_mode(goose_mode);
            let mut args = vec![
                "-c".to_string(),
                format!("approval_policy={approval_policy}"),
                "-c".to_string(),
                format!("sandbox_mode={sandbox_mode}"),
            ];

            // Codex sandbox blocks network by default. Enable it when HTTP MCP
            // servers are configured so codex-acp can connect to them.
            let has_http_mcp = mcp_servers
                .iter()
                .any(|s| matches!(s, sacp::schema::McpServer::Http(_)));
            if has_http_mcp {
                args.extend([
                    "-c".to_string(),
                    "sandbox_workspace_write.network_access=true".to_string(),
                ]);
            }

            // Chat and Approve both map to "read-only".
            let mode_mapping = HashMap::from([
                (GooseMode::Auto, "full-access".to_string()),
                (GooseMode::Approve, "read-only".to_string()),
                (GooseMode::SmartApprove, "auto".to_string()),
                (GooseMode::Chat, "read-only".to_string()),
            ]);

            let provider_config = AcpProviderConfig {
                command: resolved_command,
                args,
                env,
                env_remove: vec![],
                work_dir,
                mcp_servers,
                // Disabled until https://github.com/zed-industries/codex-acp/issues/179 is fixed.
                session_mode_id: None,
                mode_mapping,
                notification_callback: None,
            };

            let metadata = Self::metadata();
            AcpProvider::connect(metadata.name, model, goose_mode, provider_config).await
        })
    }
}

// Codex sandbox scope determines what needs approval: operations within the
// sandbox are auto-approved, operations outside it trigger on-request prompts.
// So Approve uses read-only sandbox to force write approvals through goose.
fn map_goose_mode(goose_mode: GooseMode) -> (&'static str, &'static str) {
    match goose_mode {
        GooseMode::Auto => ("never", "danger-full-access"),
        GooseMode::SmartApprove => ("on-request", "workspace-write"),
        GooseMode::Approve => ("on-request", "read-only"),
        GooseMode::Chat => ("never", "read-only"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case(GooseMode::Auto, "never", "danger-full-access")]
    #[test_case(GooseMode::SmartApprove, "on-request", "workspace-write")]
    #[test_case(GooseMode::Approve, "on-request", "read-only")]
    #[test_case(GooseMode::Chat, "never", "read-only")]
    fn test_map_goose_mode(mode: GooseMode, expected_approval: &str, expected_sandbox: &str) {
        let (approval, sandbox) = map_goose_mode(mode);
        assert_eq!(approval, expected_approval);
        assert_eq!(sandbox, expected_sandbox);
    }
}
