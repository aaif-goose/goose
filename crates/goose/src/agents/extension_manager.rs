use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use super::provider_manager::ProviderManager;
use crate::agents::platform_extensions::{PlatformExtensionContext, PLATFORM_EXTENSIONS};
use crate::session::{EnabledExtensionsState, Session};
use goose_agent::extension::name_to_key;

mod goose_host;
use super::GoosePlatform;
use goose_agent::extension_manager::{ClientContext, ExtensionLease};
use goose_agent::mcp_client::{GooseMcpClientCapabilities, GooseMcpHostInfo};

/// Wires the extension runtime to Goose's application services.
pub struct ExtensionManager {
    manager: Arc<goose_agent::extension_manager::ExtensionManager>,
    context: PlatformExtensionContext,
}

impl std::ops::Deref for ExtensionManager {
    type Target = Arc<goose_agent::extension_manager::ExtensionManager>;

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
        let manager = Arc::new(goose_agent::extension_manager::ExtensionManager::with_host(
            host, client,
        ));
        Self { manager, context }
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
    pub(crate) fn runtime(&self) -> &Arc<goose_agent::extension_manager::ExtensionManager> {
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
        let selection = goose_host::GooseExtensionHost {
            context: self.context.clone(),
        }
        .session_selection(&session)
        .await;
        let lease = self.manager.session_lease(&selection).await?;
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
            protocol_version: None,
        },
    }
}
