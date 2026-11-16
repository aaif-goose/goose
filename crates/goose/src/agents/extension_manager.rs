pub use crate::agents::extension_runtime::{
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

pub use crate::agents::extension_runtime::ExtensionLease as McpLease;

mod apps;
mod builtin;
mod goose_host;
mod registration;
pub use apps::TRUSTED_TOOL_UPDATE_META_KEY;
mod mutation;
use super::GoosePlatform;
use crate::agents::mcp_client::{GooseMcpClientCapabilities, GooseMcpHostInfo};
pub use mutation::ExtensionMutation;

/// Wires the extension runtime to Goose's application services.
#[derive(Clone)]
pub struct ExtensionManager {
    manager: Arc<crate::agents::extension_runtime::ExtensionManager>,
    session_manager: Arc<crate::session::SessionManager>,
}

impl std::ops::Deref for ExtensionManager {
    type Target = Arc<crate::agents::extension_runtime::ExtensionManager>;

    fn deref(&self) -> &Self::Target {
        &self.manager
    }
}

impl ExtensionManager {
    pub fn new(
        session_manager: Arc<crate::session::SessionManager>,
        client: ClientContext,
    ) -> Self {
        let host = Arc::new(goose_host::GooseExtensionHost {
            session_manager: Arc::clone(&session_manager),
        });
        let manager = Arc::new(
            crate::agents::extension_runtime::ExtensionManager::with_services(
                host.services(),
                client,
            ),
        );
        Self {
            manager,
            session_manager,
        }
    }

    fn host(&self) -> Arc<goose_host::GooseExtensionHost> {
        Arc::new(goose_host::GooseExtensionHost {
            session_manager: Arc::clone(&self.session_manager),
        })
    }

    pub fn extension_set(
        &self,
        scope: &str,
        cwd: Option<PathBuf>,
        configs: Vec<crate::agents::extension_config::ExtensionConfig>,
        container: Option<crate::agents::container::Container>,
    ) -> Result<ExtensionSet> {
        crate::agents::extension_config::check_unique_keys(&configs)?;
        let host = self.host();
        let runtime = crate::agents::extension_runtime::ExtensionSet::new(
            scope,
            cwd.clone(),
            configs
                .iter()
                .cloned()
                .map(|config| {
                    registration::registration(config, container.clone(), Arc::clone(&host))
                })
                .collect(),
        )?;
        Ok(ExtensionSet { runtime, configs })
    }

    pub async fn resolve(&self, set: &ExtensionSet) -> McpLease {
        self.manager.resolve(&set.runtime).await
    }

    pub async fn enable(
        &self,
        scope: &str,
        config: crate::agents::extension_config::ExtensionConfig,
    ) -> crate::agents::extension_config::ExtensionResult<()> {
        let session = self
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

    pub(super) async fn select(&self, scope: &str, config: ExtensionConfig) -> ExtensionResult<()> {
        self.session_manager
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
        self.session_manager
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
        let session = self.session_manager.get_session(scope, false).await?;
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
    pub(crate) runtime: crate::agents::extension_runtime::ExtensionSet,
    pub(crate) configs: Vec<ExtensionConfig>,
}

impl ExtensionSet {
    pub fn new(
        scope: impl Into<String>,
        directory: Option<PathBuf>,
        registrations: Vec<crate::agents::extension_runtime::Registration>,
    ) -> ExtensionResult<Self> {
        Ok(Self {
            runtime: crate::agents::extension_runtime::ExtensionSet::new(
                scope,
                directory,
                registrations,
            )?,
            configs: Vec::new(),
        })
    }
}
