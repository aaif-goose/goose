mod model;
mod name_builder;
mod registry;

pub use model::{CanonicalModel, Limit, Modalities, Modality, Pricing};
pub use name_builder::{
    canonical_name, map_provider_name, map_to_canonical_model, strip_version_suffix,
};
pub use registry::CanonicalModelRegistry;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelMapping {
    pub provider_model: String,
    pub canonical_model: String,
}

impl ModelMapping {
    pub fn new(provider_model: impl Into<String>, canonical_model: impl Into<String>) -> Self {
        Self {
            provider_model: provider_model.into(),
            canonical_model: canonical_model.into(),
        }
    }
}

/// Return recommended model names for a provider using only the bundled canonical registry.
///
/// This avoids network calls by looking up all known models for the provider,
/// filtering to text-input + tool-calling models, and sorting by release date.
/// The returned names are the canonical short names (e.g. "claude-3.5-sonnet").
///
/// TODO: This trades speed for correctness — the canonical registry may not perfectly
/// match what the provider API returns (new models not yet in the registry, deprecated
/// models still listed, or locally-installed models for providers like Ollama). Consider
/// whether to reconcile with a live API call in the background.
pub fn recommended_models_from_registry(provider: &str) -> Vec<String> {
    let registry = match CanonicalModelRegistry::bundled() {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let registry_provider = map_provider_name(provider);
    let all = registry.get_all_models_for_provider(registry_provider);

    let mut models_with_dates: Vec<(String, Option<String>)> = all
        .iter()
        .filter(|m| m.modalities.input.contains(&Modality::Text) && m.tool_call)
        .filter_map(|m| {
            let (_, name) = m.id.split_once('/')?;
            Some((name.to_string(), m.release_date.clone()))
        })
        .collect();

    models_with_dates.sort_by(|a, b| match (&a.1, &b.1) {
        (Some(date_a), Some(date_b)) => date_b.cmp(date_a),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.0.cmp(&b.0),
    });

    models_with_dates
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

pub fn maybe_get_canonical_model(provider: &str, model: &str) -> Option<CanonicalModel> {
    let registry = CanonicalModelRegistry::bundled().ok()?;

    // map_to_canonical_model returns the canonical ID (provider/model)
    // Parse it to get provider and model parts for registry lookup
    let canonical_id = map_to_canonical_model(provider, model, registry)?;
    if let Some((canon_provider, canon_model)) = canonical_id.split_once('/') {
        registry.get(canon_provider, canon_model).cloned()
    } else {
        None
    }
}
