use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use goose_providers::model::ModelConfig;
use tracing::warn;

use crate::config::Config;
use crate::providers::base::Provider;
use crate::providers::provider_registry::ProviderEntry;
use crate::session::extension_data::EnabledExtensionsState;
use crate::session::Session;

type SessionSlot = Arc<tokio::sync::Mutex<Option<Arc<dyn Provider>>>>;
type SharedProviders = HashMap<String, (u64, Arc<dyn Provider>)>;

/// Hands out the provider a session should talk to. Providers that can serve
/// any session are built once and shared; session-bound providers are built
/// per session and kept until the session switches provider or is released.
#[derive(Default)]
pub struct ProviderManager {
    shared: tokio::sync::Mutex<SharedProviders>,
    sessions: Mutex<HashMap<String, SessionSlot>>,
}

impl ProviderManager {
    pub async fn provider_for(&self, session: &Session) -> Result<Arc<dyn Provider>> {
        let name = provider_name_for(session)?;
        let slot = self.slot(&session.id);
        let mut slot = slot.lock().await;
        if let Some(provider) = slot.as_ref().filter(|p| p.get_name() == name) {
            return Ok(provider.clone());
        }

        let entry = crate::providers::get_from_registry(&name).await?;
        if !entry.session_bound() {
            let provider = self.shared(&name, &entry).await?;
            *slot = None;
            return Ok(provider);
        }

        let extensions = EnabledExtensionsState::extensions_or_default(
            Some(&session.extension_data),
            Config::global(),
        );
        let provider = entry
            .create_with_working_dir(extensions, session.working_dir.clone())
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

    /// Pins `provider` to the session until the session switches to another
    /// provider name or is released.
    pub async fn set_provider(&self, session_id: &str, provider: Arc<dyn Provider>) {
        *self.slot(session_id).lock().await = Some(provider);
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
        let mut shared = self.shared.lock().await;
        if let Some((generation, provider)) = shared.get(name) {
            if *generation == Config::global().generation() {
                return Ok(provider.clone());
            }
        }
        let provider = entry.create(Vec::new()).await?;
        // Read after creating: building a provider can itself write config.
        let generation = Config::global().generation();
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
    use serial_test::serial;

    fn session(id: &str, provider_name: &str) -> Session {
        Session {
            id: id.to_string(),
            provider_name: Some(provider_name.to_string()),
            model_config: Some(ModelConfig::new("test-model")),
            ..Default::default()
        }
    }

    #[tokio::test]
    #[serial]
    async fn shares_stateless_providers_and_binds_the_rest_to_one_session() {
        std::env::set_var("GEMINI_CLI_COMMAND", "/bin/sh");
        let manager = ProviderManager::default();

        let openai_a = manager.provider_for(&session("a", "openai")).await.unwrap();
        let openai_b = manager.provider_for(&session("b", "openai")).await.unwrap();
        assert!(Arc::ptr_eq(&openai_a, &openai_b));

        let gemini_a = Arc::downgrade(
            &manager
                .provider_for(&session("a", "gemini-cli"))
                .await
                .unwrap(),
        );
        let gemini_b = Arc::downgrade(
            &manager
                .provider_for(&session("b", "gemini-cli"))
                .await
                .unwrap(),
        );
        assert!(!gemini_a.ptr_eq(&gemini_b));
        let gemini_a_again = manager
            .provider_for(&session("a", "gemini-cli"))
            .await
            .unwrap();
        assert!(Arc::ptr_eq(&gemini_a.upgrade().unwrap(), &gemini_a_again));
        drop(gemini_a_again);

        manager.provider_for(&session("a", "openai")).await.unwrap();
        assert!(gemini_a.upgrade().is_none());

        manager.release("b");
        assert!(gemini_b.upgrade().is_none());
        std::env::remove_var("GEMINI_CLI_COMMAND");
    }
}
