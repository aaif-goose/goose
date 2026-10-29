use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, Weak};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::extension_manager::host::{
    ExtensionAuth, ExtensionHost, ExtensionSession, PlatformExtension, SpawnServerFn,
    StaticOAuthClientConfig,
};
use goose_agent::extension_manager::ExtensionManager;
use rmcp::transport::auth::CredentialStore;
use rmcp::transport::AuthorizationManager;

use super::{
    name_to_key, EnabledExtensionsState, PlatformExtensionContext, Session, PLATFORM_EXTENSIONS,
};
use crate::agents::provider_manager::provider_name_for;
use crate::config::search_path::SearchPaths;
use crate::config::Config;
use goose_agent::action_required_manager::ActionRequiredManager;
use goose_agent::extension::{ExtensionConfig, ExtensionResult};
use goose_agent::mcp_client::{McpClientTrait, TraceContextInjector};

pub(super) struct GooseExtensionHost {
    pub context: PlatformExtensionContext,
}

impl GooseExtensionHost {
    pub(super) async fn session_selection(&self, session: &Session) -> ExtensionSession {
        let provider_runs_tool_loop = match self.context.providers.pinned(session).await {
            Some(provider) => provider.manages_own_context(),
            None => match provider_name_for(session) {
                Ok(name) => crate::providers::get_from_registry(&name)
                    .await
                    .is_ok_and(|entry| entry.runs_own_tool_loop()),
                Err(_) => false,
            },
        };
        ExtensionSession {
            id: session.id.clone(),
            working_dir: session.working_dir.clone(),
            container: session.container.clone(),
            extensions: EnabledExtensionsState::from_extension_data(&session.extension_data)
                .map(|state| state.extensions)
                .unwrap_or_default(),
            provider_runs_tool_loop,
        }
    }
}

#[async_trait]
impl ExtensionHost for GooseExtensionHost {
    async fn session(&self, session_id: &str) -> Result<ExtensionSession> {
        let session = self
            .context
            .session_manager
            .get_session(session_id, false)
            .await?;
        Ok(self.session_selection(&session).await)
    }

    async fn select(&self, session_id: &str, config: ExtensionConfig) -> ExtensionResult<()> {
        self.context
            .session_manager
            .update_enabled_extensions(session_id, |extensions| {
                let key = config.key();
                match extensions.iter_mut().find(|selected| selected.key() == key) {
                    Some(selected) => *selected = config,
                    None => extensions.push(config),
                }
            })
            .await
            .map_err(|error| goose_agent::extension::ExtensionError::SetupError(error.to_string()))
    }

    async fn disable(&self, session_id: &str, key: &str) -> ExtensionResult<bool> {
        self.context
            .session_manager
            .update_enabled_extensions(session_id, |extensions| {
                let selected = extensions.len();
                extensions.retain(|config| config.key() != key);
                extensions.len() != selected
            })
            .await
            .map_err(|error| goose_agent::extension::ExtensionError::SetupError(error.to_string()))
    }

    async fn resolve_config(&self, config: ExtensionConfig) -> ExtensionResult<ExtensionConfig> {
        config.resolve(Config::global()).await
    }

    fn extension_by_name(&self, name: &str) -> Option<ExtensionConfig> {
        crate::config::get_extension_by_name(name)
    }

    fn action_required(&self) -> Arc<ActionRequiredManager> {
        self.context.session_manager.action_required()
    }

    fn auth(&self) -> Arc<dyn ExtensionAuth> {
        Arc::new(GooseExtensionAuth)
    }

    fn config_generation(&self) -> u64 {
        Config::global().generation()
    }

    fn client_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn default_timeout(&self) -> u64 {
        Config::global()
            .get_goose_default_extension_timeout()
            .unwrap_or(goose_agent::extension::DEFAULT_EXTENSION_TIMEOUT)
    }

    fn platform_extension(&self, name: &str) -> Option<PlatformExtension> {
        PLATFORM_EXTENSIONS
            .get(name_to_key(name).as_str())
            .map(|def| PlatformExtension {
                unprefixed_tools: def.unprefixed_tools,
            })
    }

    fn connect_platform(
        &self,
        name: &str,
        manager: Weak<ExtensionManager>,
    ) -> ExtensionResult<Option<Box<dyn McpClientTrait>>> {
        let def = &PLATFORM_EXTENSIONS[name_to_key(name).as_str()];
        let mut context = self.context.clone();
        context.extension_manager = Some(manager);
        Ok((def.client_factory)(context))
    }

    fn builtin_extension(&self, name: &str) -> Option<SpawnServerFn> {
        crate::builtin_extension::get_builtin_extension(name)
    }

    fn builtin_command(&self, name: &str) -> ExtensionResult<(String, Vec<String>)> {
        Ok((
            "goose".to_string(),
            vec!["mcp".to_string(), name.to_string()],
        ))
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

    fn trace_context(&self) -> Option<TraceContextInjector> {
        #[cfg(feature = "otel")]
        {
            Some(Arc::new(crate::otel::trace_context::inject_current))
        }
        #[cfg(not(feature = "otel"))]
        {
            None
        }
    }
}

struct GooseExtensionAuth;

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
