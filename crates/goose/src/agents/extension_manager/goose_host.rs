use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

use crate::agents::extension_runtime::host::{
    ExtensionAuth, ExtensionEnvironment, ExtensionServices, ExtensionToolPolicy,
    StaticOAuthClientConfig,
};
use anyhow::Result;
use async_trait::async_trait;
use rmcp::transport::auth::CredentialStore;
use rmcp::transport::AuthorizationManager;

use crate::agents::extension_config::ExtensionResult;
use crate::config::search_path::SearchPaths;
use crate::config::Config;

pub(super) struct GooseExtensionHost {
    pub session_manager: Arc<crate::session::SessionManager>,
}

impl GooseExtensionHost {
    pub(super) fn services(self: &Arc<Self>) -> ExtensionServices {
        let mut services = ExtensionServices::new(self.session_manager.action_required());
        services.environment = self.clone();
        services.tool_policy = self.clone();
        services.auth = Arc::new(GooseExtensionAuth);
        #[cfg(feature = "otel")]
        {
            services.settings.trace_context =
                Some(Arc::new(crate::otel::trace_context::inject_current));
        }
        services
    }

    pub(super) fn stdio_command(
        &self,
        program: &str,
        args: &[String],
        env: std::collections::HashMap<String, String>,
        container: Option<&crate::agents::container::Container>,
    ) -> ExtensionResult<tokio::process::Command> {
        let command = match container {
            Some(container) => {
                let mut command = tokio::process::Command::new("docker");
                command.arg("exec").arg("-i");
                for (key, value) in env {
                    command.arg("-e").arg(format!("{key}={value}"));
                }
                command.arg(container.id()).arg(program).args(args);
                command
            }
            None => {
                let mut command = tokio::process::Command::new(self.resolve_command(program));
                command.args(args).envs(env);
                command
            }
        };
        Ok(command)
    }
}

#[async_trait]
impl ExtensionEnvironment for GooseExtensionHost {
    async fn resolve_config(
        &self,
        config: goose_agent::extension::ExtensionConfig,
    ) -> ExtensionResult<goose_agent::extension::ExtensionConfig> {
        config.resolve(Config::global()).await
    }

    fn config_generation(&self) -> u64 {
        Config::global().generation()
    }

    fn default_timeout(&self) -> u64 {
        Config::global()
            .get_goose_default_extension_timeout()
            .unwrap_or(crate::agents::extension_config::DEFAULT_EXTENSION_TIMEOUT)
    }

    fn connection_working_dir(&self, directory: &std::path::Path) -> PathBuf {
        if directory.as_os_str().is_empty() {
            std::env::var("GOOSE_WORKING_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default())
        } else {
            directory.to_path_buf()
        }
    }

    fn resolve_command(&self, cmd: &str) -> PathBuf {
        SearchPaths::builder()
            .with_npm()
            .resolve(cmd)
            .unwrap_or_else(|_| PathBuf::from(cmd))
    }

    fn search_path(&self) -> Result<OsString> {
        SearchPaths::builder().path()
    }
}

#[async_trait]
impl ExtensionToolPolicy for GooseExtensionHost {
    fn reserved_result_meta_keys(&self) -> &'static [&'static str] {
        &[
            super::mutation::EXTENSION_MUTATION_META_KEY,
            "subagent_session_id",
        ]
    }

    async fn process_result(
        &self,
        context: crate::agents::extension_runtime::host::ResultContext<'_>,
        result: &mut rmcp::model::CallToolResult,
    ) {
        super::apps::process_result(context, result).await;
    }
}

pub(super) struct GooseExtensionAuth;

#[async_trait]
impl ExtensionAuth for GooseExtensionAuth {
    fn credential_store(&self, name: &str) -> Box<dyn CredentialStore> {
        Box::new(crate::oauth::GooseCredentialStore::new(name.to_string()))
    }

    async fn authorize(
        &self,
        uri: &str,
        name: &str,
        static_client: Option<&StaticOAuthClientConfig>,
        challenge: Option<String>,
    ) -> Result<AuthorizationManager> {
        crate::oauth::oauth_flow_with_challenge(
            &uri.to_string(),
            &name.to_string(),
            static_client,
            challenge,
        )
        .await
    }
}
