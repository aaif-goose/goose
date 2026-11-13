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

use super::{EnabledExtensionsState, PlatformExtensionContext, Session};
use crate::agents::extension_config::ExtensionResult;
use crate::agents::provider_manager::provider_name_for;
use crate::config::search_path::SearchPaths;
use crate::config::Config;

pub(super) struct GooseExtensionHost {
    pub context: PlatformExtensionContext,
}

impl GooseExtensionHost {
    pub(super) fn services(self: &Arc<Self>) -> ExtensionServices {
        let mut services = ExtensionServices::new(self.context.session_manager.action_required());
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

    pub(super) async fn session_selection(
        self: &Arc<Self>,
        session: &Session,
    ) -> Result<super::ExtensionSet> {
        let provider_runs_tool_loop = match self.context.providers.pinned(session).await {
            Some(provider) => provider.manages_own_context(),
            None => match provider_name_for(session) {
                Ok(name) => crate::providers::get_from_registry(&name)
                    .await
                    .is_ok_and(|entry| entry.runs_own_tool_loop()),
                Err(_) => false,
            },
        };
        let configs = EnabledExtensionsState::from_extension_data(&session.extension_data)
            .map(|state| state.extensions)
            .unwrap_or_default();
        let extensions = configs
            .into_iter()
            .filter(|config| !provider_runs_tool_loop || config.external().is_none())
            .map(|config| {
                super::registration::registration(
                    config,
                    session.container.clone(),
                    Arc::clone(self),
                )
            })
            .collect();
        Ok(super::ExtensionSet::new(
            &session.id,
            Some(session.working_dir.clone()),
            extensions,
        )?)
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

    fn exposes_tool(&self, extension: &str, tool: &str, catalog_has_resources: bool) -> bool {
        use crate::agents::platform_extensions::ext_manager::{
            LIST_RESOURCES_TOOL_NAME, READ_RESOURCE_TOOL_NAME,
        };
        extension != "extensionmanager"
            || catalog_has_resources
            || !matches!(tool, LIST_RESOURCES_TOOL_NAME | READ_RESOURCE_TOOL_NAME)
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
