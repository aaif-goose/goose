pub use crate::agents::extension_runtime::{
    get_parameter_names, get_tool_owner, get_tool_resource_uri, is_tool_owned_by_extension,
    recover_mangled_tool_name, CallRequest, ClientContext, ExtensionLease, ExtensionLoadResult,
    ExtensionSet, ResourceItem,
};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use super::provider_manager::ProviderManager;
use crate::agents::extension_config::{
    name_to_key, ExtensionConfig, ExtensionError, ExtensionResult,
};
use crate::agents::platform_extensions::{PlatformExtensionContext, PLATFORM_EXTENSIONS};
use crate::session::{EnabledExtensionsState, Session};

mod apps;
mod builtin;
mod goose_host;
mod registration;
pub use apps::TRUSTED_TOOL_UPDATE_META_KEY;
mod mutation;
use super::GoosePlatform;
use crate::agents::mcp_client::{GooseMcpClientCapabilities, GooseMcpHostInfo};
#[cfg(feature = "code-mode")]
pub(crate) use mutation::applying_mutation;
pub use mutation::ExtensionMutation;

/// Wires the extension runtime to Goose's application services.
pub struct ExtensionManager {
    manager: Arc<crate::agents::extension_runtime::ExtensionManager>,
    context: PlatformExtensionContext,
}

impl std::ops::Deref for ExtensionManager {
    type Target = Arc<crate::agents::extension_runtime::ExtensionManager>;

    fn deref(&self) -> &Self::Target {
        &self.manager
    }
}

impl ExtensionManager {
    pub fn new(
        providers: Arc<ProviderManager>,
        session_manager: Arc<crate::session::SessionManager>,
        scheduler: Option<Arc<dyn crate::scheduler_trait::SchedulerTrait>>,
        client: ClientContext,
    ) -> Self {
        let context = PlatformExtensionContext {
            extension_manager: None,
            providers,
            session_manager,
            scheduler,
        };
        let host = Arc::new(goose_host::GooseExtensionHost {
            context: context.clone(),
        });
        let manager = Arc::new(
            crate::agents::extension_runtime::ExtensionManager::with_services(
                host.services(),
                client,
            ),
        );
        Self { manager, context }
    }

    fn host(&self) -> Arc<goose_host::GooseExtensionHost> {
        Arc::new(goose_host::GooseExtensionHost {
            context: self.context.clone(),
        })
    }

    pub fn extension_set(
        &self,
        scope: &str,
        cwd: Option<PathBuf>,
        configs: Vec<crate::agents::extension_config::ExtensionConfig>,
        container: Option<crate::agents::container::Container>,
    ) -> Result<crate::agents::extension_runtime::ExtensionSet> {
        crate::agents::extension_config::check_unique_keys(&configs)?;
        Ok(crate::agents::extension_runtime::ExtensionSet::new(
            scope,
            cwd,
            configs
                .into_iter()
                .map(|config| registration::registration(config, container.clone(), self.host()))
                .collect(),
        )?)
    }

    pub async fn enable(
        &self,
        scope: &str,
        config: crate::agents::extension_config::ExtensionConfig,
    ) -> crate::agents::extension_config::ExtensionResult<()> {
        let session = self
            .context
            .session_manager
            .get_session(scope, false)
            .await
            .map_err(|e| ExtensionError::SetupError(e.to_string()))?;
        self.manager
            .enable(
                scope,
                registration::registration(config.clone(), session.container, self.host()),
                &session.working_dir,
                self.select(scope, config),
            )
            .await
    }

    async fn select(&self, scope: &str, config: ExtensionConfig) -> ExtensionResult<()> {
        self.context
            .session_manager
            .update_enabled_extensions(scope, |extensions| {
                let key = config.key();
                match extensions.iter_mut().find(|selected| selected.key() == key) {
                    Some(selected) => *selected = config,
                    None => extensions.push(config),
                }
            })
            .await
            .map_err(|e| ExtensionError::SetupError(e.to_string()))
    }

    pub async fn disable(&self, scope: &str, key: &str) -> ExtensionResult<bool> {
        self.context
            .session_manager
            .update_enabled_extensions(scope, |extensions| {
                let selected = extensions.len();
                extensions.retain(|config| config.key() != key);
                extensions.len() != selected
            })
            .await
            .map_err(|e| ExtensionError::SetupError(e.to_string()))
    }

    pub async fn list_extensions(&self, scope: &str) -> Result<Vec<String>> {
        Ok(self
            .get_extension_configs(scope)
            .await?
            .iter()
            .map(ExtensionConfig::key)
            .collect())
    }

    pub async fn is_extension_enabled(&self, scope: &str, name: &str) -> Result<bool> {
        Ok(self
            .list_extensions(scope)
            .await?
            .contains(&name_to_key(name)))
    }

    pub async fn current_lease(&self, scope: &str) -> Result<ExtensionLease> {
        let session = self
            .context
            .session_manager
            .get_session(scope, false)
            .await?;
        self.session_lease(&session).await
    }

    async fn session_lease(&self, session: &Session) -> Result<ExtensionLease> {
        Ok(self
            .manager
            .resolve(&self.host().session_selection(session).await?)
            .await)
    }

    pub async fn get_extension_configs(
        &self,
        scope: &str,
    ) -> Result<Vec<crate::agents::extension_config::ExtensionConfig>> {
        let session = self
            .context
            .session_manager
            .get_session(scope, false)
            .await?;
        Ok(
            EnabledExtensionsState::from_extension_data(&session.extension_data)
                .map(|state| state.extensions)
                .unwrap_or_default(),
        )
    }

    pub fn with_data_dir(data_dir: PathBuf) -> Self {
        Self::new(
            Default::default(),
            Arc::new(crate::session::SessionManager::new(data_dir)),
            None,
            client_context(&GoosePlatform::GooseCli, None),
        )
    }

    #[cfg(all(test, feature = "code-mode"))]
    pub(crate) fn runtime(&self) -> &Arc<crate::agents::extension_runtime::ExtensionManager> {
        &self.manager
    }

    pub fn get_context(&self) -> &PlatformExtensionContext {
        &self.context
    }

    pub async fn current_session_snapshot(
        &self,
        session: &Session,
    ) -> Result<(Session, ExtensionLease)> {
        let session = self
            .context
            .session_manager
            .get_session(&session.id, session.conversation.is_some())
            .await?;
        let lease = self.session_lease(&session).await?;
        Ok((session, lease))
    }
}

pub fn is_first_class_extension(name: &str) -> bool {
    PLATFORM_EXTENSIONS
        .get(name_to_key(name).as_str())
        .is_some_and(|def| def.unprefixed_tools)
}

pub fn is_hidden_extension(name: &str) -> bool {
    PLATFORM_EXTENSIONS
        .get(name_to_key(name).as_str())
        .is_some_and(|def| def.hidden)
}

#[cfg(test)]
mod tests;

pub fn client_context(
    platform: &GoosePlatform,
    host_info: Option<GooseMcpHostInfo>,
) -> ClientContext {
    let mcpui = host_info
        .as_ref()
        .filter(|info| info.explicit_extensions)
        .map(GooseMcpHostInfo::mcpui_enabled)
        .unwrap_or(matches!(platform, GoosePlatform::GooseDesktop));
    let name = host_info
        .as_ref()
        .and_then(|info| info.client_name.clone())
        .unwrap_or_else(|| platform.to_string());
    ClientContext {
        name,
        capabilities: GooseMcpClientCapabilities {
            mcpui,
            host_info,
            elicitation_handler: None,
            action_required_elicitation: true,
            protocol_version: None,
        },
    }
}
