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

pub(crate) const AGY_ACP_PROVIDER_NAME: &str = "agy-acp";
const AGY_ACP_DOC_URL: &str = "https://antigravity.google";
pub(crate) const AGY_ACP_BINARY: &str = "agy";

const MODE_DEFAULT: &str = "default";
const MODE_ACCEPT_EDITS: &str = "accept-edits";
const MODE_AUTO: &str = "auto";

/// Provider wrapping the Google Antigravity CLI (`agy acp --stdio`) over ACP.
pub struct AgyAcpProvider;

impl goose_providers::base::ProviderDescriptor for AgyAcpProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            AGY_ACP_PROVIDER_NAME,
            "Antigravity CLI (ACP)",
            "Use goose with Google Antigravity CLI via its native ACP server.",
            ACP_CURRENT_MODEL,
            vec![],
            AGY_ACP_DOC_URL,
            vec![],
        )
        .with_setup_steps(vec![
            "Install the Antigravity CLI (`agy`) and ensure it is on your PATH",
            "Run `agy acp --login` to authenticate with your Google account",
        ])
        .with_setup(
            ProviderSetupMetadata::cli_agent(AGY_ACP_BINARY, &["agy-acp", "agy", "antigravity"])
                .with_acp()
                .with_docs_url(AGY_ACP_DOC_URL)
                .with_capabilities(true, true, false),
        )
    }
}

impl AgyAcpProvider {
    fn create(
        extensions: Vec<crate::config::ExtensionConfig>,
        working_dir: PathBuf,
        use_default_model: bool,
    ) -> BoxFuture<'static, Result<AcpProvider>> {
        Box::pin(async move {
            let config = Config::global();
            let resolved_command = SearchPaths::builder().with_npm().resolve(AGY_ACP_BINARY)?;
            let goose_mode = config.resolve_goose_mode()?;
            let model = if use_default_model {
                ACP_CURRENT_MODEL.to_string()
            } else {
                configured_model_for_provider(config, AGY_ACP_PROVIDER_NAME)
            };

            let args = vec!["acp".to_string(), "--stdio".to_string()];
            let session_config_options = if model == ACP_CURRENT_MODEL {
                vec![]
            } else {
                vec![("model".to_string(), model)]
            };

            let mode_mapping = HashMap::from([
                (GooseMode::Auto, vec![MODE_AUTO.to_string()]),
                (GooseMode::Approve, vec![MODE_DEFAULT.to_string()]),
                (GooseMode::SmartApprove, vec![MODE_ACCEPT_EDITS.to_string()]),
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

impl ProviderDef for AgyAcpProvider {
    type Provider = AcpProvider;
    const SESSION_BOUND: bool = true;
    const RUNS_OWN_TOOL_LOOP: bool = true;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_uses_acp_defaults() {
        let meta = AgyAcpProvider::metadata();
        assert_eq!(meta.name, AGY_ACP_PROVIDER_NAME);
        assert_eq!(meta.default_model, ACP_CURRENT_MODEL);
    }
}
