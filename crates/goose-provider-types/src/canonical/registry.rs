use super::CanonicalModel;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::Path;

pub const MODEL_CATALOG_PATH_ENV: &str = "GOOSE_MODEL_CATALOG_PATH";

static MODEL_REGISTRY: Lazy<Result<CanonicalModelRegistry>> = Lazy::new(|| {
    if let Some(path) = std::env::var_os(MODEL_CATALOG_PATH_ENV) {
        return CanonicalModelRegistry::from_file(path);
    }

    #[cfg(feature = "bundled-model-catalog")]
    {
        const CANONICAL_MODELS_JSON: &str = include_str!("data/canonical_models.json");
        return CanonicalModelRegistry::from_json(CANONICAL_MODELS_JSON)
            .context("Failed to parse bundled canonical models JSON");
    }

    #[cfg(not(feature = "bundled-model-catalog"))]
    anyhow::bail!(
        "Set {MODEL_CATALOG_PATH_ENV} to a canonical models JSON file; this build has no bundled model catalog"
    )
});

#[derive(Debug, Clone)]
pub struct CanonicalModelRegistry {
    models: HashMap<(String, String), CanonicalModel>,
}

impl CanonicalModelRegistry {
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
        }
    }

    pub fn bundled() -> Result<&'static Self> {
        MODEL_REGISTRY
            .as_ref()
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .context("Failed to read canonical models file")?;
        Self::from_json(&content).context("Failed to parse canonical models JSON")
    }

    fn from_json(content: &str) -> Result<Self> {
        let models: Vec<CanonicalModel> = serde_json::from_str(content)?;
        let mut registry = Self::new();
        for model in models {
            if let Some((provider, model_name)) = model.id.split_once('/') {
                let provider = provider.to_string();
                let model_name = model_name.to_string();
                registry.register(&provider, &model_name, model);
            }
        }
        Ok(registry)
    }

    pub fn to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let mut models: Vec<&CanonicalModel> = self.models.values().collect();
        models.sort_by(|a, b| a.id.cmp(&b.id));

        let json = serde_json::to_string_pretty(&models)
            .context("Failed to serialize canonical models")?;

        std::fs::write(path.as_ref(), json).context("Failed to write canonical models file")?;

        Ok(())
    }

    pub fn register(&mut self, provider: &str, model: &str, canonical_model: CanonicalModel) {
        self.models
            .insert((provider.to_string(), model.to_string()), canonical_model);
    }

    pub fn get(&self, provider: &str, model: &str) -> Option<&CanonicalModel> {
        self.models.get(&(provider.to_string(), model.to_string()))
    }

    pub fn get_all_models_for_provider(&self, provider: &str) -> Vec<CanonicalModel> {
        self.models
            .iter()
            .filter(|((p, _), _)| p == provider)
            .map(|(_, model)| model.clone())
            .collect()
    }

    pub fn all_models(&self) -> Vec<&CanonicalModel> {
        self.models.values().collect()
    }

    pub fn count(&self) -> usize {
        self.models.len()
    }
}

impl Default for CanonicalModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}
