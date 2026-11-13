pub use goose_agent::extension_manager::{
    get_parameter_names, get_tool_owner, get_tool_resource_uri, is_tool_owned_by_extension,
    recover_mangled_tool_name, CallRequest, ClientContext, ExtensionLoadResult, ResourceItem,
};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use crate::agents::extension_config::{
    name_to_key, ExtensionConfig, ExtensionError, ExtensionResult,
};
use crate::session::EnabledExtensionsState;

pub use goose_agent::extension_manager::ExtensionLease as McpLease;

mod apps;
mod builtin;
mod goose_host;
mod registration;
pub use apps::TRUSTED_TOOL_UPDATE_META_KEY;
mod mutation;
use super::GoosePlatform;
use goose_agent::mcp_client::{GooseMcpClientCapabilities, GooseMcpHostInfo};
pub use mutation::ExtensionMutation;

/// Wires the extension runtime to Goose's application services.
#[derive(Clone)]
pub struct ExtensionManager {
    manager: Arc<goose_agent::extension_manager::ExtensionManager>,
    host: Arc<goose_host::GooseExtensionHost>,
}

impl ExtensionManager {
    pub fn new(
        session_manager: Arc<crate::session::SessionManager>,
        client: ClientContext,
    ) -> Self {
        let host = Arc::new(goose_host::GooseExtensionHost { session_manager });
        let manager = Arc::new(
            goose_agent::extension_manager::ExtensionManager::with_services(
                host.services(),
                client,
            ),
        );
        Self { manager, host }
    }

    pub fn extension_set(
        &self,
        scope: &str,
        cwd: Option<PathBuf>,
        configs: Vec<crate::agents::extension_config::ExtensionConfig>,
        container: Option<crate::agents::container::Container>,
    ) -> Result<ExtensionSet> {
        crate::agents::extension_config::check_unique_keys(&configs)?;
        let runtime = goose_agent::extension_manager::ExtensionSet::new(
            scope,
            cwd,
            configs
                .iter()
                .filter(|config| !crate::agents::platform_extensions::is_platform(config))
                .cloned()
                .map(|config| {
                    registration::registration(config, container.clone(), Arc::clone(&self.host))
                })
                .collect(),
        )?;
        Ok(ExtensionSet { runtime, configs })
    }

    pub async fn resolve(&self, set: &ExtensionSet) -> McpLease {
        self.manager.resolve(&set.runtime).await
    }

    pub fn attach_client(&self, scope: &str, client: ClientContext) {
        self.manager.attach_client(scope, client);
    }

    pub async fn release(&self, scope: &str) {
        self.manager.release(scope).await;
    }

    #[cfg(test)]
    pub(crate) fn client(&self, scope: &str) -> Arc<ClientContext> {
        self.manager.client(scope)
    }

    #[cfg(test)]
    pub(crate) async fn scope_lease(&self, scope: &str) -> McpLease {
        self.manager.scope_lease(scope).await
    }

    pub async fn enable(
        &self,
        scope: &str,
        config: crate::agents::extension_config::ExtensionConfig,
    ) -> crate::agents::extension_config::ExtensionResult<()> {
        let session = self
            .host
            .session_manager
            .get_session(scope, false)
            .await
            .map_err(|e| ExtensionError::SetupError(e.to_string()))?;
        self.manager
            .enable(
                scope,
                registration::registration(
                    config.clone(),
                    session.container,
                    Arc::clone(&self.host),
                ),
                &session.working_dir,
                self.select(scope, config),
            )
            .await
    }

    pub(super) async fn select(&self, scope: &str, config: ExtensionConfig) -> ExtensionResult<()> {
        self.host
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
        self.host
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

    pub async fn get_extension_configs(
        &self,
        scope: &str,
    ) -> Result<Vec<crate::agents::extension_config::ExtensionConfig>> {
        let session = self.host.session_manager.get_session(scope, false).await?;
        Ok(
            EnabledExtensionsState::from_extension_data(&session.extension_data)
                .map(|state| state.extensions)
                .unwrap_or_default(),
        )
    }
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

pub struct ExtensionSet {
    pub(crate) runtime: goose_agent::extension_manager::ExtensionSet,
    pub(crate) configs: Vec<ExtensionConfig>,
}
