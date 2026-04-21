use super::base::{ModelInfo, Provider, ProviderDef, ProviderMetadata, ProviderType};
use super::inventory::InventoryIdentityInput;
use crate::config::{DeclarativeProviderConfig, ExtensionConfig};
use crate::model::ModelConfig;
use anyhow::Result;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::Arc;

pub type ProviderConstructor = Arc<
    dyn Fn(ModelConfig, Vec<ExtensionConfig>) -> BoxFuture<'static, Result<Arc<dyn Provider>>>
        + Send
        + Sync,
>;

pub type ProviderCleanup = Arc<dyn Fn() -> BoxFuture<'static, Result<()>> + Send + Sync>;

pub type ProviderInventoryIdentityResolver =
    Arc<dyn Fn() -> Result<InventoryIdentityInput> + Send + Sync>;

pub type ProviderInventoryConfiguredResolver = Arc<dyn Fn() -> bool + Send + Sync>;

#[derive(Clone)]
pub struct ProviderEntry {
    metadata: ProviderMetadata,
    pub(crate) constructor: ProviderConstructor,
    pub(crate) inventory_identity: ProviderInventoryIdentityResolver,
    pub(crate) inventory_configured: ProviderInventoryConfiguredResolver,
    pub(crate) cleanup: Option<ProviderCleanup>,
    provider_type: ProviderType,
    supports_inventory_refresh: bool,
}

impl ProviderEntry {
    pub fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    pub fn provider_type(&self) -> ProviderType {
        self.provider_type
    }

    pub fn supports_inventory_refresh(&self) -> bool {
        self.supports_inventory_refresh
    }

    pub fn inventory_identity(&self) -> Result<InventoryIdentityInput> {
        (self.inventory_identity)()
    }

    pub fn inventory_configured(&self) -> bool {
        (self.inventory_configured)()
    }

    fn normalize_model_config(&self, mut model: ModelConfig) -> ModelConfig {
        model = model.with_canonical_limits(&self.metadata.name);

        if model.context_limit.is_none() {
            if let Some(info) = self
                .metadata
                .known_models
                .iter()
                .find(|m| m.name == model.model_name && m.context_limit > 0)
            {
                model.context_limit = Some(info.context_limit);
            }
        }

        model
    }

    pub async fn create_with_default_model(
        &self,
        extensions: Vec<ExtensionConfig>,
    ) -> Result<Arc<dyn Provider>> {
        let default_model = &self.metadata.default_model;
        let model_config = self.normalize_model_config(ModelConfig::new(default_model.as_str())?);
        (self.constructor)(model_config, extensions).await
    }

    pub async fn create(
        &self,
        model: ModelConfig,
        extensions: Vec<ExtensionConfig>,
    ) -> Result<Arc<dyn Provider>> {
        let model = self.normalize_model_config(model);
        (self.constructor)(model, extensions).await
    }
}

#[derive(Default)]
pub struct ProviderRegistry {
    pub(crate) entries: HashMap<String, ProviderEntry>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn register<F>(&mut self, preferred: bool)
    where
        F: ProviderDef + 'static,
    {
        let metadata = F::metadata();
        let name = metadata.name.clone();

        self.entries.insert(
            name,
            ProviderEntry {
                metadata,
                constructor: Arc::new(|model, extensions| {
                    Box::pin(async move {
                        let provider = F::from_env(model, extensions).await?;
                        Ok(Arc::new(provider) as Arc<dyn Provider>)
                    })
                }),
                inventory_identity: Arc::new(F::inventory_identity),
                inventory_configured: Arc::new(F::inventory_configured),
                cleanup: None,
                provider_type: if preferred {
                    ProviderType::Preferred
                } else {
                    ProviderType::Builtin
                },
                supports_inventory_refresh: F::supports_inventory_refresh(),
            },
        );
    }

    pub fn register_with_name<P, F, G>(
        &mut self,
        config: &DeclarativeProviderConfig,
        provider_type: ProviderType,
        supports_inventory_refresh: bool,
        constructor: F,
        inventory_identity: G,
    ) where
        P: ProviderDef + 'static,
        F: Fn(ModelConfig) -> Result<P::Provider> + Send + Sync + 'static,
        G: Fn() -> Result<InventoryIdentityInput> + Send + Sync + 'static,
    {
        let base_metadata = P::metadata();
        let description = config
            .description
            .clone()
            .unwrap_or_else(|| format!("Custom {} provider", config.display_name));
        let default_model = config
            .models
            .first()
            .map(|m| m.name.clone())
            .unwrap_or_default();
        let known_models: Vec<ModelInfo> = config
            .models
            .iter()
            .map(|m| ModelInfo {
                name: m.name.clone(),
                context_limit: m.context_limit,
                input_token_cost: m.input_token_cost,
                output_token_cost: m.output_token_cost,
                currency: m.currency.clone(),
                supports_cache_control: Some(m.supports_cache_control.unwrap_or(false)),
            })
            .collect();

        let mut config_keys = base_metadata.config_keys.clone();

        if let Some(api_key_index) = config_keys.iter().position(|key| key.secret) {
            if !config.requires_auth {
                config_keys.remove(api_key_index);
            } else if !config.api_key_env.is_empty() {
                let api_key_required = provider_type == ProviderType::Declarative;
                config_keys[api_key_index] = super::base::ConfigKey::new(
                    &config.api_key_env,
                    api_key_required,
                    true,
                    None,
                    true,
                );
            }
        }

        if let Some(ref env_vars) = config.env_vars {
            for ev in env_vars {
                // Default primary to `required` so required fields show prominently in the UI
                let primary = ev.primary.unwrap_or(ev.required);
                config_keys.push(super::base::ConfigKey::new(
                    &ev.name,
                    ev.required,
                    ev.secret,
                    ev.default.as_deref(),
                    primary,
                ));
            }
        }

        let custom_metadata = ProviderMetadata {
            name: config.name.clone(),
            display_name: config.display_name.clone(),
            description,
            default_model,
            known_models,
            model_doc_link: base_metadata.model_doc_link,
            config_keys,
            setup_steps: vec![],
            model_selection_hint: None,
        };
        let inventory_config_keys = custom_metadata.config_keys.clone();

        self.entries.insert(
            config.name.clone(),
            ProviderEntry {
                metadata: custom_metadata,
                constructor: Arc::new(move |model, _extensions| {
                    let result = constructor(model);
                    Box::pin(async move {
                        let provider = result?;
                        Ok(Arc::new(provider) as Arc<dyn Provider>)
                    })
                }),
                inventory_identity: Arc::new(inventory_identity),
                inventory_configured: Arc::new(move || {
                    super::inventory::default_inventory_configured(
                        &inventory_config_keys,
                        crate::config::Config::global(),
                    )
                }),
                cleanup: None,
                provider_type,
                supports_inventory_refresh,
            },
        );
    }

    pub fn set_cleanup(&mut self, name: &str, cleanup: ProviderCleanup) {
        if let Some(entry) = self.entries.get_mut(name) {
            entry.cleanup = Some(cleanup);
        }
    }

    pub fn with_providers<F>(mut self, setup: F) -> Self
    where
        F: FnOnce(&mut Self),
    {
        setup(&mut self);
        self
    }

    pub async fn create(
        &self,
        name: &str,
        model: ModelConfig,
        extensions: Vec<ExtensionConfig>,
    ) -> Result<Arc<dyn Provider>> {
        let entry = self
            .entries
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", name))?;

        entry.create(model, extensions).await
    }

    pub fn all_metadata_with_types(&self) -> Vec<(ProviderMetadata, ProviderType)> {
        self.entries
            .values()
            .map(|e| (e.metadata.clone(), e.provider_type))
            .collect()
    }

    pub fn remove_custom_providers(&mut self) {
        self.entries.retain(|name, _| !name.starts_with("custom_"));
    }
}
