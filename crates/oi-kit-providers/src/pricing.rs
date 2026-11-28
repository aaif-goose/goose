//! Prices for the providers in this crate.
//!
//! Precedence, highest first:
//! 1. a cost the provider reported (kept by `ProviderUsage::priced`)
//! 2. prices the host declares, such as negotiated rates in its own provider
//!    config, which it knows better than a name-matched catalog entry
//! 3. the model catalog
//! 4. prices in the bundled provider definitions, which only fill catalog gaps
//!    because they are vendored and may lag catalog syncs
//!
//! Catalog cache rates are kept whenever the winning source declares none, so
//! cached tokens are not overestimated at the full input rate.

use std::sync::{Arc, OnceLock};

pub use oi_kit_provider_types::pricing::{CatalogPrices, Prices};

use crate::base::ModelInfo;
use crate::canonical::Pricing;
use crate::declarative::{fixed_provider_configs, DeclarativeProviderConfig};

#[derive(Default)]
pub struct ProviderPrices {
    declared: Option<Arc<dyn Prices>>,
}

impl ProviderPrices {
    pub fn with_declared(mut self, declared: Arc<dyn Prices>) -> Self {
        self.declared = Some(declared);
        self
    }
}

impl Prices for ProviderPrices {
    fn pricing(&self, provider: &str, model: &str) -> Option<Pricing> {
        let bundled = bundled_model_info(provider, model).and_then(|info| info.pricing());
        let base = match (CatalogPrices.pricing(provider, model), bundled) {
            (Some(catalog), Some(bundled)) => Some(fill_gaps(catalog, bundled)),
            (catalog, bundled) => catalog.or(bundled),
        };
        let declared = self
            .declared
            .as_ref()
            .and_then(|declared| declared.pricing(provider, model));
        match (declared, base) {
            (Some(declared), Some(base)) => Some(keep_cache_rates(declared, &base)),
            (declared, base) => declared.or(base),
        }
    }
}

/// The bundled definition of a model that declares both an input and an output price.
pub fn bundled_model_info(provider: &str, model: &str) -> Option<ModelInfo> {
    static CONFIGS: OnceLock<Vec<DeclarativeProviderConfig>> = OnceLock::new();
    CONFIGS
        .get_or_init(|| fixed_provider_configs().unwrap_or_default())
        .iter()
        .find(|config| config.name == provider)?
        .models
        .iter()
        .find(|info| info.name == model && info.pricing().is_some())
        .cloned()
}

/// A catalog price that is unset or zero carries no rate, e.g. a name-inferred
/// match against another provider's free listing.
fn fill_gaps(mut catalog: Pricing, bundled: Pricing) -> Pricing {
    fn is_gap(price: Option<f64>) -> bool {
        matches!(price, None | Some(0.0))
    }
    if is_gap(catalog.input) {
        catalog.input = bundled.input;
    }
    if is_gap(catalog.output) {
        catalog.output = bundled.output;
    }
    catalog.cache_read = catalog.cache_read.or(bundled.cache_read);
    catalog.cache_write = catalog.cache_write.or(bundled.cache_write);
    catalog
}

fn keep_cache_rates(mut declared: Pricing, base: &Pricing) -> Pricing {
    // A declared zero price means "free here": catalog cache rates must not
    // reintroduce cost for cached tokens.
    if matches!(declared.input, Some(0.0)) && matches!(declared.output, Some(0.0)) {
        return declared;
    }
    declared.cache_read = declared.cache_read.or(base.cache_read);
    declared.cache_write = declared.cache_write.or(base.cache_write);
    declared
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::token_usage::{CostSource, ProviderUsage, Usage};

    fn estimate(provider: &str, model: &str, usage: &Usage) -> Option<f64> {
        ProviderPrices::default()
            .pricing(provider, model)?
            .estimate_cost(usage)
    }

    /// `expected` reads as the model's base input-plus-output rate per million
    /// tokens; the prompt stays below every long-context tier.
    fn assert_cost(provider: &str, model: &str, expected: f64) {
        let used = Usage::new(Some(10_000), Some(10_000), None);
        let actual = estimate(provider, model, &used)
            .unwrap_or_else(|| panic!("{provider}/{model} should have public pricing"))
            * 100.0;
        assert!((actual - expected).abs() < 1e-9, "got {actual}");
    }

    #[test]
    fn provider_reported_cost_takes_precedence_over_public_estimate() {
        let usage = ProviderUsage::new(
            "gpt-5.6-sol".to_string(),
            Usage::new(Some(1_000_000), Some(1_000_000), None),
        )
        .with_cost(7.0, CostSource::ProviderReported)
        .priced("chatgpt_codex", &ProviderPrices::default());

        assert_eq!(
            (usage.cost, usage.cost_source),
            (Some(7.0), Some(CostSource::ProviderReported))
        );
    }

    #[test]
    fn chatgpt_codex_uses_openai_public_rates() {
        assert_cost("chatgpt_codex", "gpt-5.6-sol", 24.0);
        let prices = ProviderPrices::default();
        let codex = prices.pricing("chatgpt_codex", "gpt-5.6-sol").unwrap();
        let openai = prices.pricing("openai", "gpt-5.6-sol").unwrap();
        assert_eq!(codex.input, openai.input);
        assert_eq!(codex.output, openai.output);
        assert_eq!(codex.cache_read, openai.cache_read);
        assert_eq!(codex.cache_write, openai.cache_write);
    }

    #[test]
    fn azure_foundry_estimates_representative_publisher_rates() {
        assert_cost("azure_foundry", "gpt-5", 11.25);
        assert_cost("azure_foundry", "claude-sonnet-4-6", 18.0);
        assert_cost("azure_foundry", "deepseek-v3.2", 2.26);
        assert_cost("azure_foundry", "mistral-large", 2.0);
    }

    #[test]
    fn azure_foundry_uses_host_rates_for_azure_and_open_weight_models() {
        assert_cost("azure_foundry", "Phi-4", 0.625);

        // The upstream Meta row describes free weights; Azure's host row prices inference.
        assert_cost("azure_foundry", "llama-3.3-70b-instruct", 1.42);
    }

    #[test]
    fn azure_foundry_unknown_model_has_no_estimate() {
        assert!(estimate(
            "azure_foundry",
            "private-deployment-without-metadata",
            &Usage::new(Some(1_000_000), Some(1_000_000), None),
        )
        .is_none());
    }

    #[test]
    fn model_info_pricing_converts_per_token_to_per_million_usd() {
        let pricing = ModelInfo::with_cost("m", 262_144, 0.000002, 0.000006)
            .pricing()
            .unwrap();
        assert!((pricing.input.unwrap() - 2.0).abs() < 1e-9);
        assert!((pricing.output.unwrap() - 6.0).abs() < 1e-9);
        assert!(ModelInfo::new("m").pricing().is_none());
    }

    #[test]
    fn bundled_provider_prices_are_respected() {
        let info = bundled_model_info("ovhcloud", "Qwen3-32B").unwrap();
        assert!((info.input_token_cost.unwrap() - 9e-8).abs() < 1e-12);
        let pricing = ProviderPrices::default()
            .pricing("ovhcloud", "Qwen3-32B")
            .unwrap();
        assert!((pricing.input.unwrap() - 0.09).abs() < 1e-9);
        assert!((pricing.output.unwrap() - 0.25).abs() < 1e-9);
    }

    #[test]
    fn unpriced_everywhere_yields_none() {
        assert!(estimate(
            "definitely-not-a-registry-provider",
            "nope-xqzj-model",
            &Usage::new(Some(1), Some(1), None),
        )
        .is_none());
    }
}
