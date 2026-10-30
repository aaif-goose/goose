use super::{
    canonical_name, CanonicalModel, CanonicalModelRegistry, Limit, Modalities, Modality, Pricing,
    PricingTier, ThinkingMode,
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;

const DEFAULT_CONTEXT_LIMIT: usize = 128_000;

fn normalize_provider_name(provider: &str) -> &str {
    match provider {
        "llama" => "meta-llama",
        "xai" => "x-ai",
        "mistral" => "mistralai",
        _ => provider,
    }
}

fn get_string(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(|v| v.as_str()).map(String::from)
}

fn get_thinking_mode(canonical_id: &str, value: &Value) -> Option<ThinkingMode> {
    value
        .get("thinking_mode")
        .and_then(|v| v.as_str())
        .and_then(|mode| serde_json::from_value(Value::String(mode.to_string())).ok())
        .or_else(|| inferred_thinking_mode(canonical_id))
}

fn inferred_thinking_mode(canonical_id: &str) -> Option<ThinkingMode> {
    match canonical_id {
        "anthropic/claude-fable-5" => Some(ThinkingMode::AlwaysOnAdaptive),
        "anthropic/claude-fable-5.1" => Some(ThinkingMode::AlwaysOnAdaptive),
        "anthropic/claude-opus-5" => Some(ThinkingMode::Adaptive),
        "anthropic/claude-opus-5.5" => Some(ThinkingMode::AlwaysOnAdaptive),
        "anthropic/claude-opus-4.6" => Some(ThinkingMode::Adaptive),
        "anthropic/claude-opus-4.7" => Some(ThinkingMode::Adaptive),
        "anthropic/claude-opus-4.8" => Some(ThinkingMode::Adaptive),
        "anthropic/claude-sonnet-4.6" => Some(ThinkingMode::Adaptive),
        "anthropic/claude-sonnet-5" => Some(ThinkingMode::Adaptive),
        "anthropic/claude-sonnet-5.5" => Some(ThinkingMode::AlwaysOnAdaptive),
        "anthropic/claude-haiku-5.5" => Some(ThinkingMode::AlwaysOnAdaptive),
        _ => None,
    }
}

fn parse_modalities(model_data: &Value, field: &str) -> Vec<Modality> {
    model_data
        .get("modalities")
        .and_then(|m| m.get(field))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .filter_map(|s| {
                    serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
                })
                .collect()
        })
        .unwrap_or_else(|| vec![Modality::Text])
}

fn parse_context_tiers(cost: &Value) -> Vec<PricingTier> {
    let Some(tiers) = cost.get("tiers").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    tiers
        .iter()
        .filter(|t| t["tier"]["type"] == "context")
        .filter_map(|t| {
            Some(PricingTier {
                above_input_tokens: t["tier"]["size"].as_u64()?,
                input: t.get("input").and_then(|v| v.as_f64()),
                output: t.get("output").and_then(|v| v.as_f64()),
                cache_read: t.get("cache_read").and_then(|v| v.as_f64()),
                cache_write: t.get("cache_write").and_then(|v| v.as_f64()),
            })
        })
        .collect()
}

fn process_model(
    model_id: &str,
    model_data: &Value,
    normalized_provider: &str,
) -> Result<(String, CanonicalModel)> {
    let name = model_data["name"]
        .as_str()
        .with_context(|| format!("Model {} missing name", model_id))?;

    let canonical_id = canonical_name(normalized_provider, model_id);

    let modalities = Modalities {
        input: parse_modalities(model_data, "input"),
        output: parse_modalities(model_data, "output"),
    };

    let cost = match model_data.get("cost") {
        Some(c) if !c.is_null() => Pricing {
            input: c.get("input").and_then(|v| v.as_f64()),
            output: c.get("output").and_then(|v| v.as_f64()),
            cache_read: c.get("cache_read").and_then(|v| v.as_f64()),
            cache_write: c.get("cache_write").and_then(|v| v.as_f64()),
            tiers: parse_context_tiers(c),
        },
        _ => Pricing::default(),
    };

    let limit = Limit {
        context: model_data
            .get("limit")
            .and_then(|l| l.get("context"))
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_CONTEXT_LIMIT as u64) as usize,
        output: model_data
            .get("limit")
            .and_then(|l| l.get("output"))
            .and_then(|v| v.as_u64())
            .map(|v| v as usize),
    };

    let canonical_model = CanonicalModel {
        id: canonical_id.clone(),
        name: name.to_string(),
        family: get_string(model_data, "family"),
        attachment: model_data.get("attachment").and_then(|v| v.as_bool()),
        reasoning: model_data.get("reasoning").and_then(|v| v.as_bool()),
        reasoning_efforts: model_data
            .get("reasoning_options")
            .and_then(|v| v.as_array())
            .and_then(|options| {
                options
                    .iter()
                    .find(|option| option.get("type").and_then(Value::as_str) == Some("effort"))
            })
            .and_then(|option| option.get("values"))
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            }),
        thinking_mode: get_thinking_mode(&canonical_id, model_data),
        tool_call: model_data
            .get("tool_call")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        temperature: model_data.get("temperature").and_then(|v| v.as_bool()),
        knowledge: get_string(model_data, "knowledge"),
        release_date: get_string(model_data, "release_date"),
        last_updated: get_string(model_data, "last_updated"),
        modalities,
        open_weights: model_data.get("open_weights").and_then(|v| v.as_bool()),
        cost,
        limit,
    };

    let model_name = canonical_id
        .strip_prefix(&format!("{}/", normalized_provider))
        .unwrap_or(model_id)
        .to_string();

    Ok((model_name, canonical_model))
}

fn pick_winning_variant(variants: &[(String, CanonicalModel)]) -> usize {
    variants
        .iter()
        .enumerate()
        .min_by(|(_, (id_a, a)), (_, (id_b, b))| {
            id_a.len()
                .cmp(&id_b.len())
                .then_with(|| b.last_updated.cmp(&a.last_updated))
                .then_with(|| b.release_date.cmp(&a.release_date))
                .then_with(|| id_a.cmp(id_b))
        })
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

pub fn from_models_dev(content: &str) -> Result<CanonicalModelRegistry> {
    let json: Value = serde_json::from_str(content)?;
    let providers = json
        .as_object()
        .context("Expected object in models.dev response")?;
    let mut registry = CanonicalModelRegistry::new();
    for (provider_key, provider_data) in providers {
        let models = match provider_data.get("models").and_then(Value::as_object) {
            Some(models) => models,
            None => continue,
        };
        let provider = normalize_provider_name(provider_key);
        let mut candidates: BTreeMap<String, Vec<(String, CanonicalModel)>> = BTreeMap::new();
        for (model_id, model_data) in models {
            let (name, model) = process_model(model_id, model_data, provider)?;
            candidates
                .entry(name)
                .or_default()
                .push((model_id.clone(), model));
        }
        for (name, variants) in candidates {
            let winner = pick_winning_variant(&variants);
            registry.register(provider, &name, variants[winner].1.clone());
        }
    }
    if registry.count() == 0 {
        anyhow::bail!("models.dev catalog is empty");
    }
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variant(id: &str, release: Option<&str>, updated: Option<&str>) -> (String, CanonicalModel) {
        (
            id.to_string(),
            CanonicalModel {
                id: format!("openai/{id}"),
                name: id.to_string(),
                family: None,
                attachment: None,
                reasoning: None,
                reasoning_efforts: None,
                thinking_mode: None,
                tool_call: false,
                temperature: None,
                knowledge: None,
                release_date: release.map(String::from),
                last_updated: updated.map(String::from),
                modalities: Modalities::default(),
                open_weights: None,
                cost: Pricing::default(),
                limit: Limit::default(),
            },
        )
    }

    #[test]
    fn shortest_variant_wins() {
        let variants = vec![
            variant("gpt-4o-2024-08-06", Some("2024-08-06"), Some("2024-08-06")),
            variant("gpt-4o", Some("2024-05-13"), Some("2024-08-06")),
            variant("gpt-4o-2024-11-20", Some("2024-11-20"), Some("2024-11-20")),
            variant("gpt-4o-2024-05-13", Some("2024-05-13"), Some("2024-05-13")),
        ];
        assert_eq!(variants[pick_winning_variant(&variants)].0, "gpt-4o");

        let variants = vec![
            variant(
                "claude-haiku-4-5-20251001",
                Some("2025-10-16"),
                Some("2025-10-16"),
            ),
            variant("claude-haiku-4-5", Some("2025-10-16"), Some("2025-10-16")),
        ];
        assert_eq!(
            variants[pick_winning_variant(&variants)].0,
            "claude-haiku-4-5"
        );
    }

    #[test]
    fn parses_effort_options_without_confusing_other_reasoning_options() {
        let json = r#"{"openai":{"models":{"future":{"name":"Future","reasoning":true,"reasoning_options":[{"type":"budget","values":[100]},{"type":"effort","values":["low","max"]}]}}}}"#;
        let registry = from_models_dev(json).unwrap();
        assert_eq!(
            registry.get("openai", "future").unwrap().reasoning_efforts,
            Some(vec!["low".to_string(), "max".to_string()])
        );
    }

    #[test]
    fn converts_provider_models_and_rejects_empty_catalog() {
        let json = r#"{"openai":{"models":{"gpt-4o":{"name":"GPT-4o","tool_call":true,"limit":{"context":128000,"output":4096},"cost":{"input":2.5}}}}}"#;
        let registry = from_models_dev(json).unwrap();
        let model = registry.get("openai", "gpt-4o").unwrap();
        assert_eq!(model.id, "openai/gpt-4o");
        assert_eq!(model.limit.context, 128000);
        assert_eq!(model.cost.input, Some(2.5));
        assert!(from_models_dev("{}").is_err());
    }
}
