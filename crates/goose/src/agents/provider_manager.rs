use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use goose_providers::model::ModelConfig;
use tracing::warn;

use crate::config::Config;
use crate::providers::base::Provider;
use crate::providers::provider_registry::ProviderEntry;
use crate::providers::ProviderBackend;
use crate::session::extension_data::EnabledExtensionsState;
use crate::session::Session;

type SessionSlot = Arc<tokio::sync::Mutex<Option<ProviderBackend>>>;
type SharedProviders = HashMap<String, (u64, Arc<dyn Provider>)>;

#[derive(Default)]
pub struct ProviderManager {
    shared: tokio::sync::Mutex<SharedProviders>,
    sessions: Mutex<HashMap<String, SessionSlot>>,
}

impl ProviderManager {
    /// Temporary bridge for agent-loop consumers awaiting typed backend dispatch.
    pub async fn provider_for(&self, session: &Session) -> Result<Arc<dyn Provider>> {
        Ok(self.backend_for(session).await?.into_legacy_provider())
    }

    pub async fn backend_for(&self, session: &Session) -> Result<ProviderBackend> {
        let name = provider_name_for(session)?;
        let slot = self.slot(&session.id);
        let mut slot = slot.lock().await;
        if let Some(provider) = slot.as_ref().filter(|p| p.name() == name) {
            return Ok(provider.clone());
        }

        let entry = crate::providers::get_from_registry(&name).await?;
        if !entry.session_bound() {
            let provider = self.shared(&name, &entry).await?;
            *slot = None;
            return Ok(ProviderBackend::Standard(provider));
        }

        let extensions = EnabledExtensionsState::extensions_or_default(
            Some(&session.extension_data),
            Config::global(),
        );
        let provider = entry
            .create_backend_with_working_dir(extensions, session.working_dir.clone())
            .await?;
        let model_config = entry.normalize_model_config(model_config_for(session)?)?;
        if let Err(e) = provider.apply_model_selection(&model_config).await {
            warn!("Failed to apply model selection to provider: {e}");
        }
        provider
            .update_mode(&session.id, session.goose_mode)
            .await
            .map_err(|e| anyhow!("Failed to propagate mode to provider: {e}"))?;
        *slot = Some(provider.clone());
        Ok(provider)
    }

    pub async fn set_provider(&self, session_id: &str, provider: Arc<dyn Provider>) {
        self.set_backend(session_id, ProviderBackend::Standard(provider))
            .await;
    }

    pub async fn set_backend(&self, session_id: &str, backend: ProviderBackend) {
        *self.slot(session_id).lock().await = Some(backend);
    }

    pub fn release(&self, session_id: &str) {
        self.sessions.lock().unwrap().remove(session_id);
    }

    fn slot(&self, session_id: &str) -> SessionSlot {
        self.sessions
            .lock()
            .unwrap()
            .entry(session_id.to_string())
            .or_default()
            .clone()
    }

    async fn shared(&self, name: &str, entry: &ProviderEntry) -> Result<Arc<dyn Provider>> {
        let generation = Config::global().generation();
        let mut shared = self.shared.lock().await;
        if let Some((cached_generation, provider)) = shared.get(name) {
            if *cached_generation == generation {
                return Ok(provider.clone());
            }
        }
        let provider = match entry.create_backend(Vec::new()).await? {
            ProviderBackend::Standard(provider) => provider,
            ProviderBackend::Acp(_) => {
                anyhow::bail!("ACP backends cannot enter the shared provider cache")
            }
        };
        shared.insert(name.to_string(), (generation, provider.clone()));
        Ok(provider)
    }
}

pub fn provider_name_for(session: &Session) -> Result<String> {
    match &session.provider_name {
        Some(name) => Ok(name.clone()),
        None => Config::global()
            .get_goose_provider()
            .map_err(|_| anyhow!("Could not configure agent: missing provider")),
    }
}

pub fn model_config_for(session: &Session) -> Result<ModelConfig> {
    if let Some(model_config) = &session.model_config {
        return Ok(model_config.clone());
    }
    let config = Config::global();
    let provider_name = provider_name_for(session)?;
    let model_name = config
        .get_goose_model()
        .map_err(|_| anyhow!("Could not resolve model config: missing model"))?;
    crate::model_config::model_config_from_user_config(&provider_name, &model_name)
        .map_err(|e| anyhow!("Could not resolve model config: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn typed_injection_retains_acp_and_legacy_injection_is_standard() {
        let manager = ProviderManager::default();
        let acp = Arc::new(crate::acp::AcpProvider::new_test_stub());
        manager
            .set_backend("session", ProviderBackend::Acp(acp.clone()))
            .await;
        let slot = manager.slot("session");
        assert!(
            matches!(slot.lock().await.as_ref(), Some(ProviderBackend::Acp(provider)) if Arc::ptr_eq(provider, &acp))
        );
        let standard: Arc<dyn Provider> =
            Arc::new(crate::providers::testprovider::TestProvider::new_recording(
                acp,
                "unused-recording.json",
            ));
        manager.set_provider("session", standard.clone()).await;
        assert!(
            matches!(slot.lock().await.as_ref(), Some(ProviderBackend::Standard(provider)) if Arc::ptr_eq(provider, &standard))
        );
        manager.release("session");
        assert!(manager.slot("session").lock().await.is_none());
    }
}
