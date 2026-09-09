use serde::{Deserialize, Serialize};

use crate::conversation::token_usage::Usage;

/// Modality types for model input/output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    Text,
    Image,
    Audio,
    Video,
    Pdf,
}

fn deserialize_modalities<'de, D>(deserializer: D) -> Result<Vec<Modality>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let strings: Vec<String> = Vec::deserialize(deserializer)?;
    Ok(strings
        .into_iter()
        .filter_map(|s| serde_json::from_value(serde_json::Value::String(s)).ok())
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Modalities {
    /// Input modalities (e.g., [Text, Image, Pdf])
    #[serde(default, deserialize_with = "deserialize_modalities")]
    pub input: Vec<Modality>,

    /// Output modalities (e.g., [Text])
    #[serde(default, deserialize_with = "deserialize_modalities")]
    pub output: Vec<Modality>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Pricing {
    /// Cost in USD per million input tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<f64>,

    /// Cost in USD per million output tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<f64>,

    /// Cost per million cached read tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<f64>,

    /// Cost per million cached write tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<f64>,
}

impl Pricing {
    /// True when the entry carries no usable rate signal: `estimate_cost` needs both
    /// an input and an output price, so an unset or literal-zero value in either field
    /// makes the whole estimate wrong-low rather than merely incomplete. Mirrors the
    /// `is_price_gap` predicate in `goose::providers::canonical_cost`.
    pub fn has_no_usable_rate(&self) -> bool {
        fn is_gap(price: Option<f64>) -> bool {
            matches!(price, None | Some(0.0))
        }
        is_gap(self.input) || is_gap(self.output)
    }

    pub fn estimate_cost(&self, usage: &Usage) -> Option<f64> {
        let input_price = self.input?;
        let output_price = self.output?;
        let cache_read_price = self.cache_read.unwrap_or(input_price);
        let cache_write_price = self.cache_write.unwrap_or(input_price);

        let input_tokens = usage.input_tokens.unwrap_or(0).max(0) as f64;
        let output_tokens = usage.output_tokens.unwrap_or(0).max(0) as f64;
        let cache_read_tokens = usage.cache_read_input_tokens.unwrap_or(0).max(0) as f64;
        let cache_write_tokens = usage.cache_write_input_tokens.unwrap_or(0).max(0) as f64;
        let uncached_input_tokens =
            (input_tokens - cache_read_tokens - cache_write_tokens).max(0.0);

        Some(
            (uncached_input_tokens * input_price
                + cache_read_tokens * cache_read_price
                + cache_write_tokens * cache_write_price
                + output_tokens * output_price)
                / 1_000_000.0,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Limit {
    /// Maximum context window size in tokens
    pub context: usize,

    /// Maximum output/completion tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingMode {
    Enabled,
    Adaptive,
    AlwaysOnAdaptive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalModel {
    /// Model identifier (e.g., "anthropic/claude-3-5-sonnet")
    pub id: String,

    /// Human-readable name (e.g., "Claude Sonnet 3.5 v2")
    pub name: String,

    /// Model family (e.g., "claude-sonnet", "gpt")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,

    /// Whether the model supports attachments
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<bool>,

    /// Whether the model supports reasoning/thinking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,

    /// Request shape to use when enabling thinking/reasoning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_mode: Option<ThinkingMode>,

    /// Whether the model supports tool calling
    #[serde(default)]
    pub tool_call: bool,

    /// Whether the model supports temperature parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<bool>,

    /// Knowledge cutoff date (e.g., "2024-04-30")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub knowledge: Option<String>,

    /// Release date (e.g., "2024-10-22")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,

    /// Last updated date (e.g., "2024-10-22")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,

    /// Input and output modalities
    #[serde(default)]
    pub modalities: Modalities,

    /// Whether the model has open weights
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_weights: Option<bool>,

    /// Pricing information
    #[serde(default)]
    pub cost: Pricing,

    /// Token limits
    #[serde(default)]
    pub limit: Limit,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pricing(
        input: Option<f64>,
        output: Option<f64>,
        cache_read: Option<f64>,
        cache_write: Option<f64>,
    ) -> Pricing {
        Pricing {
            input,
            output,
            cache_read,
            cache_write,
        }
    }

    #[test]
    fn estimate_cost_prices_cache_tokens_at_cache_rates() {
        let pricing = pricing(Some(5.0), Some(25.0), Some(0.5), Some(6.25));
        let usage =
            Usage::new(Some(10_000), Some(1_000), None).with_cache_tokens(Some(8_000), Some(1_000));

        let cost = pricing.estimate_cost(&usage).unwrap();
        let expected =
            (1_000.0 * 5.0 + 8_000.0 * 0.5 + 1_000.0 * 6.25 + 1_000.0 * 25.0) / 1_000_000.0;
        assert!((cost - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn estimate_cost_handles_missing_prices() {
        let usage =
            Usage::new(Some(1_000), Some(100), None).with_cache_tokens(Some(600), Some(200));

        // Unpriced cache tokens fall back to the input rate.
        let cost = pricing(Some(2.0), Some(10.0), None, None)
            .estimate_cost(&usage)
            .unwrap();
        assert_eq!(cost, (1_000.0 * 2.0 + 100.0 * 10.0) / 1_000_000.0);

        // Missing input or output pricing means no estimate at all.
        assert!(pricing(None, Some(10.0), None, None)
            .estimate_cost(&usage)
            .is_none());
        assert!(pricing(Some(2.0), None, None, None)
            .estimate_cost(&usage)
            .is_none());
    }

    #[test]
    fn has_no_usable_rate_covers_absent_zero_and_asymmetric_prices() {
        assert!(pricing(None, None, None, None).has_no_usable_rate());
        assert!(pricing(Some(0.0), Some(0.0), None, None).has_no_usable_rate());
        // A half-zero rate would bill one side at nothing, so it is not usable either.
        assert!(pricing(Some(0.0), Some(5.0), None, None).has_no_usable_rate());
        assert!(pricing(Some(5.0), Some(0.0), None, None).has_no_usable_rate());
        assert!(pricing(None, Some(5.0), None, None).has_no_usable_rate());

        assert!(!pricing(Some(1.25), Some(10.0), None, None).has_no_usable_rate());
        // Zero cache rates are legitimate: free cache reads on a paid model.
        assert!(!pricing(Some(1.25), Some(10.0), Some(0.0), Some(0.0)).has_no_usable_rate());
    }

    #[test]
    fn estimate_cost_clamps_cache_tokens_exceeding_input() {
        let pricing = pricing(Some(5.0), Some(25.0), Some(0.5), Some(6.25));
        let usage = Usage::new(Some(100), Some(10), None).with_cache_tokens(Some(150), Some(50));

        let cost = pricing.estimate_cost(&usage).unwrap();
        let expected = (150.0 * 0.5 + 50.0 * 6.25 + 10.0 * 25.0) / 1_000_000.0;
        assert!((cost - expected).abs() < f64::EPSILON);
    }
}
