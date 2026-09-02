//! Cost estimation for model usage.
//!
//! Price resolution precedence (highest first):
//! 1. provider-reported costs (handled by callers before this module)
//! 2. exact provider/model prices in the automatically loaded pricing.yaml
//! 3. prices the user declared in a custom provider config file — users of
//!    custom endpoints (negotiated rates, gateways, self-hosting) know their
//!    real prices better than a name-matched catalog entry
//! 4. the bundled canonical registry
//! 5. prices declared in bundled declarative provider definitions, which only
//!    fill registry gaps — they are vendored and may lag registry syncs. A
//!    registry price that is unset or zero (e.g. a name-inferred cross-provider
//!    match against a free listing) counts as a gap here.
//!
//! Note: a non-zero price the registry finds by cross-provider name inference
//! still outranks bundled provider-declared prices (a host may declare a
//! higher negotiated rate than the inferred catalog row). Demoting inferred
//! matches belongs in the canonical mapping layer, not here.
//!
//! Custom-provider declarations retain canonical cache rates for compatibility.
//! Pricing overrides are complete replacements: omitted cache rates remain unset.
//! Config files are read live (price edits take effect immediately); bundled
//! definitions are immutable and cached for the process.

use crate::config::declarative_providers::{
    custom_provider_file_path, deserialize_provider_config, fixed_provider_configs,
    DeclarativeProviderConfig,
};
use crate::config::pricing::get_pricing_overrides;
use crate::config::Config;
use crate::providers::base::ModelInfo;
use goose_providers::canonical::{maybe_get_canonical_model, Pricing};
use goose_providers::conversation::token_usage::{CostSource, Usage};
use std::sync::OnceLock;
use tracing::warn;

const DEFAULT_CURRENCY: &str = "$";

/// Estimate the USD cost of a model invocation.
pub fn estimate_model_cost(provider: &str, model: &str, usage: &Usage) -> Option<f64> {
    estimate_model_cost_with_source(provider, model, usage).map(|(cost, _)| cost)
}

pub fn estimate_model_cost_with_source(
    provider: &str,
    model: &str,
    usage: &Usage,
) -> Option<(f64, CostSource)> {
    let (pricing, source) = resolve_pricing_with_source(provider, model);
    pricing.and_then(|pricing| pricing.estimate_cost(usage).map(|cost| (cost, source)))
}

/// Resolve a provider usage cost using provider-reported cost before configured estimates.
pub fn resolve_usage_cost(
    provider: Option<&str>,
    usage: &goose_providers::conversation::token_usage::ProviderUsage,
) -> (Option<f64>, Option<CostSource>) {
    resolve_usage_cost_with_config(Config::global(), provider, usage)
}

fn resolve_usage_cost_with_config(
    config: &Config,
    provider: Option<&str>,
    usage: &goose_providers::conversation::token_usage::ProviderUsage,
) -> (Option<f64>, Option<CostSource>) {
    if let Some(cost) = usage.cost {
        return (Some(cost), Some(CostSource::ProviderReported));
    }
    let Some(provider) = provider else {
        return (None, None);
    };
    let (pricing, source) = resolve_pricing_with_config(config, provider, &usage.model);
    match pricing.and_then(|pricing| pricing.estimate_cost(&usage.usage)) {
        Some(cost) => (Some(cost), Some(source)),
        None => (None, None),
    }
}

/// Resolve the pricing for a provider/model honoring the precedence described
/// in this module's documentation.
pub(crate) fn resolve_pricing(provider: &str, model: &str) -> Option<Pricing> {
    resolve_pricing_with_config(Config::global(), provider, model).0
}

fn resolve_pricing_with_source(provider: &str, model: &str) -> (Option<Pricing>, CostSource) {
    resolve_pricing_with_config(Config::global(), provider, model)
}

fn resolve_pricing_with_config(
    config: &Config,
    provider: &str,
    model: &str,
) -> (Option<Pricing>, CostSource) {
    match get_pricing_overrides(config) {
        Ok(overrides) => {
            if let Some(entry) = overrides
                .iter()
                .find(|entry| entry.provider == provider && entry.model == model)
            {
                return (Some(entry.pricing()), CostSource::UserConfigured);
            }
        }
        Err(error) => {
            warn!(error = %error, "pricing overrides are invalid; cost estimation disabled");
            return (None, CostSource::Estimated);
        }
    }

    let canonical = maybe_get_canonical_model(provider, model).map(|c| c.cost);
    let bundled =
        bundled_model_info(provider, model).and_then(|info| pricing_from_model_info(&info));
    let base = match (canonical, bundled) {
        (Some(mut canonical), Some(bundled)) => {
            if is_price_gap(canonical.input) {
                canonical.input = bundled.input;
            }
            if is_price_gap(canonical.output) {
                canonical.output = bundled.output;
            }
            Some(canonical)
        }
        (Some(canonical), None) => Some(canonical),
        (None, bundled) => bundled,
    };

    let declared =
        custom_file_model_info(provider, model).and_then(|info| pricing_from_model_info(&info));
    let pricing = match (declared, base) {
        (Some(declared), Some(base)) => Some(merge_pricing(declared, &base)),
        (Some(declared), None) => Some(declared),
        (None, base) => base,
    };
    (pricing, CostSource::Estimated)
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

/// Merge user-declared pricing with registry/bundled pricing: declared
/// input/output prices win; canonical cache rates fill the gaps so cached
/// tokens are not overestimated at the full input rate.
fn merge_pricing(mut declared: Pricing, base: &Pricing) -> Pricing {
    // A declared zero price means "free here": registry cache rates must not
    // reintroduce cost for cached tokens.
    if matches!(declared.input, Some(0.0)) && matches!(declared.output, Some(0.0)) {
        return declared;
    }
    if declared.cache_read.is_none() {
        declared.cache_read = base.cache_read;
    }
    if declared.cache_write.is_none() {
        declared.cache_write = base.cache_write;
    }
    declared
}

/// Convert a declarative [`ModelInfo`]'s per-token USD costs into canonical
/// [`Pricing`] (per-million-token USD).
fn pricing_from_model_info(info: &ModelInfo) -> Option<Pricing> {
    Some(Pricing {
        input: Some(info.input_token_cost.map(|c| c * 1_000_000.0)?),
        output: Some(info.output_token_cost.map(|c| c * 1_000_000.0)?),
        cache_read: None,
        cache_write: None,
    })
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
    full_pricing_model(&config, model)
}

/// Bundled declarative provider definitions are embedded and immutable, so
/// they are cached for the lifetime of the process.
fn bundled_model_info(provider: &str, model: &str) -> Option<ModelInfo> {
    static CONFIGS: OnceLock<Vec<DeclarativeProviderConfig>> = OnceLock::new();
    let configs = CONFIGS.get_or_init(|| fixed_provider_configs().unwrap_or_default());
    configs
        .iter()
        .find(|config| config.name == provider)
        .and_then(|config| full_pricing_model(config, model))
}

/// A registry price that is unset or zero provides no usable rate signal
/// (e.g. a name-inferred cross-provider match against a free listing).
fn is_price_gap(price: Option<f64>) -> bool {
    matches!(price, None | Some(0.0))
}

fn full_pricing_model(config: &DeclarativeProviderConfig, model: &str) -> Option<ModelInfo> {
    config
        .models
        .iter()
        .find(|m| m.name == model)
        .filter(|info| info.input_token_cost.is_some() && info.output_token_cost.is_some())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::declarative_providers::custom_providers_dir;

    fn usage(input: Option<i32>, output: Option<i32>, cache_read: Option<i32>) -> Usage {
        Usage {
            input_tokens: input,
            output_tokens: output,
            total_tokens: None,
            cache_read_input_tokens: cache_read,
            cache_write_input_tokens: None,
        }
    }

    /// Both agent loops price a chunk through `estimate_model_cost` and tag the result
    /// `CostSource::Estimated` when it returns `Some` — `reply_parts::resolve_chunk_cost` for
    /// the legacy path, `state_machine::usage::enrich` for the new one. Pinning the shared
    /// helper covers the behaviour both paths inherit.
    #[test]
    fn azure_foundry_estimates_from_the_azure_catalog_rate() {
        let used = usage(Some(1_000_000), Some(1_000_000), None);

        let gpt5 = estimate_model_cost("azure_foundry", "gpt-5", &used)
            .expect("gpt-5 prices through the Azure catalog");
        assert!(gpt5 > 0.0);

        // Priced from azure/llama-3.3-70b-instruct ($0.71/M in and out), not from the
        // meta-llama publisher row that lists the open weights at 0.0/0.0.
        let llama = estimate_model_cost("azure_foundry", "llama-3.3-70b-instruct", &used)
            .expect("llama-3.3-70b-instruct prices through the Azure catalog");
        assert!((llama - 1.42).abs() < 1e-9, "got {llama}");
    }

    #[test]
    fn pricing_from_model_info_converts_per_token_to_per_million_usd() {
        let info = ModelInfo::with_cost("m", 262_144, 0.000002, 0.000006);
        let pricing = pricing_from_model_info(&info).unwrap();
        assert!((pricing.input.unwrap() - 2.0).abs() < 1e-9);
        assert!((pricing.output.unwrap() - 6.0).abs() < 1e-9);
    }

    #[test]
    fn pricing_from_model_info_returns_none_without_prices() {
        let info = ModelInfo::new("m").with_context_limit(1_000);
        assert!(pricing_from_model_info(&info).is_none());
    }

    #[test]
    fn partial_pricing_is_rejected() {
        let mut info = ModelInfo::new("m").with_context_limit(1_000);
        info.input_token_cost = Some(0.000002);
        assert!(pricing_from_model_info(&info).is_none());
    }

    #[test]
    fn declared_input_output_win_and_canonical_cache_rates_fill_the_gap() {
        let merged = merge_pricing(
            Pricing {
                input: Some(2.0),
                output: Some(6.0),
                cache_read: None,
                cache_write: None,
            },
            &Pricing {
                input: Some(3.0),
                output: Some(15.0),
                cache_read: Some(0.3),
                cache_write: Some(3.75),
            },
        );
        assert_eq!(merged.input, Some(2.0));
        assert_eq!(merged.output, Some(6.0));
        assert_eq!(merged.cache_read, Some(0.3));
        assert_eq!(merged.cache_write, Some(3.75));
    }

    #[test]
    fn declared_zero_price_suppresses_registry_cache_rates() {
        // A model explicitly declared free in a custom config must not pick up
        // catalog cache pricing from a canonical name-match.
        let merged = merge_pricing(
            Pricing {
                input: Some(0.0),
                output: Some(0.0),
                cache_read: None,
                cache_write: None,
            },
            &Pricing {
                input: Some(3.0),
                output: Some(15.0),
                cache_read: Some(0.3),
                cache_write: Some(3.75),
            },
        );
        assert_eq!(merged.cache_read, None);
        assert_eq!(merged.cache_write, None);
        let usage = usage(Some(1_000_000), Some(0), Some(1_000_000));
        assert_eq!(merged.estimate_cost(&usage), Some(0.0));
    }

    #[test]
    fn cached_tokens_price_at_cache_rate_not_full_input_rate() {
        // A canonical model proxied through a gateway with declared
        // input/output prices but no cache prices.
        let merged = merge_pricing(
            Pricing {
                input: Some(3.0),
                output: Some(15.0),
                cache_read: None,
                cache_write: None,
            },
            &Pricing {
                input: Some(3.0),
                output: Some(15.0),
                cache_read: Some(0.3),
                cache_write: None,
            },
        );
        let usage = usage(Some(1_000_000), Some(0), Some(1_000_000));
        assert!((merged.estimate_cost(&usage).unwrap() - 0.3).abs() < 1e-9);
    }

    #[test]
    fn bundled_provider_prices_are_respected() {
        let info = configured_model_info("ovhcloud", "Qwen3-32B").unwrap();
        assert!((info.input_token_cost.unwrap() - 9e-8).abs() < 1e-12);
        let pricing = resolve_pricing("ovhcloud", "Qwen3-32B").unwrap();
        assert!((pricing.input.unwrap() - 0.09).abs() < 1e-9);
        assert!((pricing.output.unwrap() - 0.25).abs() < 1e-9);
    }

    #[test]
    fn custom_file_prices_drive_estimation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_root = temp_dir.path().display().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(temp_root.as_str()))]);

        let dir = custom_providers_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("priced_gateway.json"),
            r#"{
                "name": "priced_gateway",
                "engine": "openai",
                "display_name": "Gateway",
                "description": null,
                "api_key_env": "",
                "base_url": "https://example.invalid/v1",
                "models": [
                    {
                        "name": "claude-sonnet-4-5",
                        "context_limit": 200000,
                        "input_token_cost": 0.000001,
                        "output_token_cost": 0.000004
                    }
                ],
                "requires_auth": false
            }"#,
        )
        .unwrap();

        let used = usage(Some(10_000), Some(100), None);
        let cost = estimate_model_cost("priced_gateway", "claude-sonnet-4-5", &used).unwrap();
        let expected = (10_000.0 * 1.0 + 100.0 * 4.0) / 1_000_000.0;
        assert!((cost - expected).abs() < 1e-12);
    }

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

    #[test]
    fn provider_reported_cost_wins_over_user_override() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(
            &path,
            r#"GOOSE_PRICING_OVERRIDES:
  - { provider: openai, model: gpt-5, input_usd_per_million_tokens: 1, output_usd_per_million_tokens: 2 }
"#,
        )
        .unwrap();
        let config = Config::new_with_file_secrets(&path, dir.path().join("secrets.yaml")).unwrap();
        let mut provider_usage = goose_providers::conversation::token_usage::ProviderUsage::new(
            "gpt-5".to_string(),
            usage(Some(1_000_000), Some(1_000_000), None),
        );
        provider_usage.cost = Some(0.42);
        assert_eq!(
            resolve_usage_cost_with_config(&config, Some("openai"), &provider_usage),
            (Some(0.42), Some(CostSource::ProviderReported))
        );
    }

    #[test]
    fn exact_override_wins_without_inheriting_catalog_cache_prices() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(
            &path,
            r#"GOOSE_PRICING_OVERRIDES:
  - provider: openai
    model: gpt-5
    input_usd_per_million_tokens: 1
    output_usd_per_million_tokens: 2
"#,
        )
        .unwrap();
        let config = Config::new_with_file_secrets(&path, dir.path().join("secrets.yaml")).unwrap();

        let (pricing, source) = resolve_pricing_with_config(&config, "openai", "gpt-5");
        let pricing = pricing.unwrap();
        assert_eq!(source, CostSource::UserConfigured);
        assert_eq!(pricing.input, Some(1.0));
        assert_eq!(pricing.output, Some(2.0));
        assert_eq!(pricing.cache_read, None);
        assert_eq!(pricing.cache_write, None);
    }

    #[test]
    fn override_matching_is_exact_for_azure_deployments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(
            &path,
            r#"GOOSE_PRICING_OVERRIDES:
  - provider: azure_foundry
    model: deployment-a
    input_usd_per_million_tokens: 1
    output_usd_per_million_tokens: 2
"#,
        )
        .unwrap();
        let config = Config::new_with_file_secrets(&path, dir.path().join("secrets.yaml")).unwrap();

        assert_eq!(
            resolve_pricing_with_config(&config, "azure_foundry", "deployment-a").1,
            CostSource::UserConfigured
        );
        assert_ne!(
            resolve_pricing_with_config(&config, "azure_foundry", "deployment-b").1,
            CostSource::UserConfigured
        );
        assert_ne!(
            resolve_pricing_with_config(&config, "openai", "deployment-a").1,
            CostSource::UserConfigured
        );
    }

    #[test]
    fn override_cache_rates_are_used_in_cost_estimation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(
            &path,
            r#"GOOSE_PRICING_OVERRIDES:
  - provider: openai
    model: negotiated
    input_usd_per_million_tokens: 1
    output_usd_per_million_tokens: 2
    cache_read_usd_per_million_tokens: 0.1
    cache_write_usd_per_million_tokens: 0.5
"#,
        )
        .unwrap();
        let config = Config::new_with_file_secrets(&path, dir.path().join("secrets.yaml")).unwrap();
        let provider_usage = goose_providers::conversation::token_usage::ProviderUsage::new(
            "negotiated".to_string(),
            Usage::new(Some(1_000_000), Some(100_000), None)
                .with_cache_tokens(Some(200_000), Some(300_000)),
        );

        let (cost, source) =
            resolve_usage_cost_with_config(&config, Some("openai"), &provider_usage);
        assert!((cost.unwrap() - 0.87).abs() < 1e-12);
        assert_eq!(source, Some(CostSource::UserConfigured));
    }

    #[test]
    fn invalid_override_config_disables_catalog_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(
            &path,
            r#"GOOSE_PRICING_OVERRIDES:
  - provider: openai
    model: gpt-5
    input_usd_per_million_tokens: 1
"#,
        )
        .unwrap();
        let config = Config::new_with_file_secrets(&path, dir.path().join("secrets.yaml")).unwrap();
        assert!(resolve_pricing_with_config(&config, "openai", "gpt-5")
            .0
            .is_none());
    }

    #[test]
    fn unpriced_everywhere_yields_none() {
        assert!(estimate_model_cost(
            "definitely-not-a-registry-provider",
            "nope-xqzj-model",
            &usage(Some(1), Some(1), None),
        )
        .is_none());
    }
}
