use super::{Config, ConfigError};
use goose_providers::canonical::Pricing;
use serde::Deserialize;
use std::collections::HashSet;

pub const PRICING_OVERRIDES_CONFIG_KEY: &str = "GOOSE_PRICING_OVERRIDES";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PricingOverride {
    pub provider: String,
    pub model: String,
    pub input_usd_per_million_tokens: f64,
    pub output_usd_per_million_tokens: f64,
    #[serde(default)]
    pub cache_read_usd_per_million_tokens: Option<f64>,
    #[serde(default)]
    pub cache_write_usd_per_million_tokens: Option<f64>,
    #[serde(default = "default_currency")]
    pub currency: String,
}

fn default_currency() -> String {
    "USD".to_string()
}

impl PricingOverride {
    pub fn pricing(&self) -> Pricing {
        Pricing {
            input: Some(self.input_usd_per_million_tokens),
            output: Some(self.output_usd_per_million_tokens),
            cache_read: self.cache_read_usd_per_million_tokens,
            cache_write: self.cache_write_usd_per_million_tokens,
        }
    }

    fn validate(&self, index: usize) -> Result<(), String> {
        if self.provider.is_empty() {
            return Err(format!("entry {index} has an empty provider"));
        }
        if self.model.is_empty() {
            return Err(format!("entry {index} has an empty model"));
        }
        if !self.currency.eq_ignore_ascii_case("USD") {
            return Err(format!(
                "entry {index} uses unsupported currency {:?}; only USD is supported",
                self.currency
            ));
        }
        for (name, rate) in [
            (
                "input_usd_per_million_tokens",
                Some(self.input_usd_per_million_tokens),
            ),
            (
                "output_usd_per_million_tokens",
                Some(self.output_usd_per_million_tokens),
            ),
            (
                "cache_read_usd_per_million_tokens",
                self.cache_read_usd_per_million_tokens,
            ),
            (
                "cache_write_usd_per_million_tokens",
                self.cache_write_usd_per_million_tokens,
            ),
        ] {
            if let Some(rate) = rate {
                if !rate.is_finite() || rate < 0.0 {
                    return Err(format!(
                        "entry {index} has invalid {name} rate {rate}; rates must be finite and non-negative"
                    ));
                }
            }
        }
        Ok(())
    }
}

pub fn get_pricing_overrides(config: &Config) -> Result<Vec<PricingOverride>, ConfigError> {
    let overrides: Vec<PricingOverride> =
        match config.get_pricing_param(PRICING_OVERRIDES_CONFIG_KEY) {
            Ok(overrides) => overrides,
            Err(ConfigError::NotFound(_)) => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };

    let mut keys = HashSet::new();
    for (index, entry) in overrides.iter().enumerate() {
        entry
            .validate(index)
            .map_err(ConfigError::DeserializeError)?;
        if !keys.insert((&entry.provider, &entry.model)) {
            return Err(ConfigError::DeserializeError(format!(
                "duplicate GOOSE_PRICING_OVERRIDES entry for provider {:?} and model {:?}",
                entry.provider, entry.model
            )));
        }
    }
    Ok(overrides)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(contents: &str) -> (tempfile::TempDir, Config) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, contents).unwrap();
        let config = Config::new_with_file_secrets(&path, dir.path().join("secrets.yaml")).unwrap();
        (dir, config)
    }

    #[test]
    fn loads_typed_per_million_usd_rates() {
        let (_dir, config) = config_with(
            r#"GOOSE_PRICING_OVERRIDES:
  - provider: openai
    model: gpt-negotiated
    input_usd_per_million_tokens: 1.25
    output_usd_per_million_tokens: 4.5
    cache_read_usd_per_million_tokens: 0.125
"#,
        );
        let entries = get_pricing_overrides(&config).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].currency, "USD");
        assert_eq!(entries[0].pricing().cache_write, None);
    }

    #[test]
    fn rejects_duplicates() {
        let (_dir, config) = config_with(
            r#"GOOSE_PRICING_OVERRIDES:
  - { provider: azure_foundry, model: deployment-a, input_usd_per_million_tokens: 1, output_usd_per_million_tokens: 2 }
  - { provider: azure_foundry, model: deployment-a, input_usd_per_million_tokens: 3, output_usd_per_million_tokens: 4 }
"#,
        );
        assert!(get_pricing_overrides(&config)
            .unwrap_err()
            .to_string()
            .contains("duplicate"));
    }

    #[test]
    fn rejects_partial_non_usd_and_invalid_rates() {
        for entry in [
            "{ provider: openai, model: m, input_usd_per_million_tokens: 1 }",
            "{ provider: openai, model: m, input_usd_per_million_tokens: 1, output_usd_per_million_tokens: 2, currency: EUR }",
            "{ provider: openai, model: m, input_usd_per_million_tokens: -1, output_usd_per_million_tokens: 2 }",
            "{ provider: openai, model: m, input_usd_per_million_tokens: .inf, output_usd_per_million_tokens: 2 }",
        ] {
            let (_dir, config) = config_with(&format!(
                "GOOSE_PRICING_OVERRIDES:
  - {entry}
"
            ));
            assert!(get_pricing_overrides(&config).is_err(), "accepted {entry}");
        }
    }
}
