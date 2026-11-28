//! What a model call costs.
//!
//! [`ProviderUsage::priced`](crate::conversation::token_usage::ProviderUsage::priced)
//! keeps a cost the provider reported and otherwise estimates one from a
//! [`Prices`] source. [`CatalogPrices`] is the default; a host that knows its
//! prices better supplies its own.

use crate::canonical::{maybe_get_canonical_model, Pricing};

pub trait Prices: Send + Sync {
    fn pricing(&self, provider: &str, model: &str) -> Option<Pricing>;
}

/// Public list prices from the bundled model catalog.
pub struct CatalogPrices;

impl Prices for CatalogPrices {
    fn pricing(&self, provider: &str, model: &str) -> Option<Pricing> {
        maybe_get_canonical_model(provider, model).map(|model| model.cost)
    }
}
