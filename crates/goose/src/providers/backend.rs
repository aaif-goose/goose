use std::sync::Arc;

use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;

use crate::acp::AcpProvider;
use crate::config::GooseMode;

use super::base::Provider;

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
            Self::Acp(provider) => provider.get_name(),
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
            Self::Acp(provider) => provider.update_mode(session_id, mode).await,
        }
    }
}
