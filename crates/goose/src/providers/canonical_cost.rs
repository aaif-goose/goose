//! Cost estimation for model usage.
//!
//! Prices come from `oi_kit_providers::pricing`. Goose adds the prices users
//! declare in their custom provider config files, which outrank the catalog.
//! Those files are read live, so price edits take effect immediately.

use crate::config::declarative_providers::{
    custom_provider_file_path, deserialize_provider_config,
};
use crate::providers::base::ModelInfo;
use oi_kit_providers::canonical::Pricing;
use oi_kit_providers::conversation::token_usage::Usage;
use oi_kit_providers::pricing::{bundled_model_info, Prices, ProviderPrices};
use std::sync::{Arc, LazyLock};
use tracing::warn;

const DEFAULT_CURRENCY: &str = "$";

struct CustomProviderPrices;

impl Prices for CustomProviderPrices {
    fn pricing(&self, provider: &str, model: &str) -> Option<Pricing> {
        custom_file_model_info(provider, model)?.pricing()
    }
}

pub(crate) fn prices() -> Arc<dyn Prices> {
    static PRICES: LazyLock<Arc<dyn Prices>> = LazyLock::new(|| {
        Arc::new(ProviderPrices::default().with_declared(Arc::new(CustomProviderPrices)))
    });
    PRICES.clone()
}

/// Estimate the USD cost of a model invocation.
pub fn estimate_model_cost(provider: &str, model: &str, usage: &Usage) -> Option<f64> {
    resolve_pricing(provider, model).and_then(|pricing| pricing.estimate_cost(usage))
}

pub(crate) fn resolve_pricing(provider: &str, model: &str) -> Option<Pricing> {
    prices().pricing(provider, model)
}

/// [`ModelInfo`] for a pricing-declared model — custom provider config file
/// first, then bundled definitions. Only models declaring both input and
/// output prices qualify: partial pricing cannot be estimated correctly
/// without silently pricing one direction at zero.
pub(crate) fn configured_model_info(provider: &str, model: &str) -> Option<ModelInfo> {
    custom_file_model_info(provider, model).or_else(|| bundled_model_info(provider, model))
}

/// The currency clients render alongside config-declared prices. Clients print
/// this verbatim as a symbol, so the ISO codes configs commonly use (bundled
/// definitions declare `USD`) are mapped to their symbol; anything else is
/// shown exactly as declared.
pub(crate) fn display_currency(info: Option<&ModelInfo>) -> String {
    info.and_then(|info| info.currency.as_deref())
        .map(currency_symbol)
        .unwrap_or_else(|| DEFAULT_CURRENCY.to_string())
}

fn currency_symbol(declared: &str) -> String {
    let declared = declared.trim();
    match declared.to_ascii_uppercase().as_str() {
        "" | "USD" => DEFAULT_CURRENCY.to_string(),
        "EUR" => "€".to_string(),
        "GBP" => "£".to_string(),
        "JPY" => "¥".to_string(),
        _ => declared.to_string(),
    }
}

/// Read the custom provider config file for `provider` from disk. Reads are
/// live: price edits take effect without a restart.
fn custom_file_model_info(provider: &str, model: &str) -> Option<ModelInfo> {
    let path = custom_provider_file_path(provider).ok()?;
    if !path.exists() {
        return None;
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) => {
            warn!(path = %path.display(), error = %e, "custom provider config unreadable; cost fallback disabled");
            return None;
        }
    };
    let config = match deserialize_provider_config(&content) {
        Ok(config) => config,
        Err(e) => {
            warn!(provider = %provider, error = %e, "custom provider config failed to parse; cost fallback disabled");
            return None;
        }
    };
    config
        .models
        .into_iter()
        .find(|info| info.name == model && info.pricing().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_currency_is_rendered_as_a_symbol() {
        let mut info = ModelInfo::with_cost("m", 1_000, 0.000001, 0.000004);
        info.currency = Some("EUR".to_string());
        assert_eq!(display_currency(Some(&info)), "€");

        info.currency = Some("¥".to_string());
        assert_eq!(display_currency(Some(&info)), "¥");
    }

    #[test]
    fn missing_or_usd_currency_falls_back_to_the_dollar_symbol() {
        let mut info = ModelInfo::new("m").with_context_limit(1_000);
        assert_eq!(display_currency(Some(&info)), "$");
        assert_eq!(display_currency(None), "$");

        // Bundled definitions declare the ISO code, which clients would
        // otherwise print verbatim as "USD0.01".
        info.currency = Some("usd".to_string());
        assert_eq!(display_currency(Some(&info)), "$");
    }
}
