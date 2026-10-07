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
        let generation = Config::global().generation();
        let mut shared = self.shared.lock().await;
        if let Some((cached_generation, provider)) = shared.get(name) {
            if *cached_generation == generation {
                return Ok(provider.clone());
            }
        }
        let provider = entry.create(Vec::new()).await?;
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
