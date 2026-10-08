use std::sync::Arc;

use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;

use crate::acp::AcpProvider;
use crate::config::GooseMode;
use crate::permission::PermissionConfirmation;

use super::base::{PermissionRouting, Provider};

/// Application-level backend identity, retained independently of registry names.
#[derive(Clone)]
pub enum ProviderBackend {
    Standard(Arc<dyn Provider>),
    Acp(Arc<AcpProvider>),
}

impl ProviderBackend {
    /// Temporary bridge for consumers not yet migrated to typed backend dispatch.
    /// ACP remains typed in storage; only the returned handle loses that identity.
    pub fn into_legacy_provider(self) -> Arc<dyn Provider> {
        match self {
            Self::Standard(provider) => provider,
            Self::Acp(provider) => provider,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Standard(provider) => provider.get_name(),
            Self::Acp(provider) => provider.name(),
        }
    }

    pub async fn resume(&self, session_id: &str) -> Result<(), ProviderError> {
        match self {
            Self::Standard(provider) => provider.resume(session_id).await,
            Self::Acp(provider) => provider.resume(session_id).await,
        }
    }

    pub fn session_id(&self) -> Option<String> {
        match self {
            Self::Standard(provider) => provider.provider_session_id(),
            Self::Acp(provider) => Some(provider.session_id()),
        }
    }

    pub fn permission_routing(&self) -> PermissionRouting {
        match self {
            Self::Standard(provider) => provider.permission_routing(),
            Self::Acp(_) => PermissionRouting::ActionRequired,
        }
    }

    pub async fn handle_permission_confirmation(
        &self,
        request_id: &str,
        confirmation: &PermissionConfirmation,
    ) -> bool {
        match self {
            Self::Standard(provider) => {
                provider
                    .handle_permission_confirmation(request_id, confirmation)
                    .await
            }
            Self::Acp(provider) => {
                provider
                    .handle_permission_confirmation(request_id, confirmation)
                    .await
            }
        }
    }

    pub async fn apply_model_selection(&self, model: &ModelConfig) -> Result<(), ProviderError> {
        match self {
            Self::Standard(provider) => provider.apply_model_selection(model).await,
            Self::Acp(provider) => provider.apply_model_selection(model).await,
        }
    }

    pub async fn update_mode(
        &self,
        session_id: &str,
        mode: GooseMode,
    ) -> Result<(), ProviderError> {
        match self {
            Self::Standard(provider) => provider.update_mode(session_id, mode).await,
            Self::Acp(provider) => provider.update_mode(mode).await,
        }
    }

    pub async fn set_thinking_effort(
        &self,
        session_id: &str,
        value: &str,
    ) -> Result<bool, ProviderError> {
        match self {
            Self::Standard(provider) => provider.set_thinking_effort(session_id, value).await,
            Self::Acp(provider) => provider.set_thinking_effort(value).await.map(|()| true),
        }
    }
}
