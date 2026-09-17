use crate::config::{Config, ConfigError};
use crate::conversation::message::Message;
use crate::providers::base::Provider;
use anyhow::{anyhow, Result};
use goose_providers::conversation::token_usage::ProviderUsage;
use goose_providers::errors::ProviderError;
use goose_providers::thinking::ThinkingEffort;
use goose_providers::{
    canonical::Modality,
    model::{normalize_modalities, strip_reasoning_effort_suffix, ModelConfig},
};
use rmcp::model::Tool;
use serde_json::Value;
use std::collections::HashMap;

pub fn model_config_from_user_config(
    provider_name: &str,
    model_name: impl AsRef<str>,
) -> Result<ModelConfig> {
    let model = base_model_config_from_user_config(provider_name, model_name.as_ref())?;
    materialize_model_config(provider_name, model)
}

pub fn model_config_from_user_config_with_session_settings(
    provider_name: &str,
    model_name: impl AsRef<str>,
    previous: Option<&ModelConfig>,
    request_params: Option<HashMap<String, Value>>,
    _context_limit: Option<usize>,
) -> Result<ModelConfig> {
    let config = Config::global();
    let model = base_model_config_from_user_config(provider_name, model_name.as_ref())?;
    let model = materialize_model_config_inner(model, provider_name, false)?
        .with_inherited_session_settings_from(previous, request_params)
        .with_default_thinking_effort(config.get_goose_thinking_effort());

    Ok(apply_canonical_limits(provider_name, model))
}

pub fn materialize_model_config(provider_name: &str, model: ModelConfig) -> Result<ModelConfig> {
    let model = materialize_model_config_inner(model, provider_name, true)?;
    Ok(apply_canonical_limits(provider_name, model))
}

fn custom_model_modalities(
    provider_name: &str,
    model_name: &str,
) -> Option<goose_providers::base::ModelModalities> {
    let provider = crate::config::declarative_providers::load_provider(provider_name)
        .ok()
        .filter(|provider| provider.is_editable)?;

    provider
        .config
        .models
        .iter()
        .find(|candidate| candidate.name == model_name)
        .or_else(|| {
            provider.config.models.iter().find(|candidate| {
                strip_reasoning_effort_suffix(&candidate.name)
                    .is_some_and(|(base, _)| base == model_name)
            })
        })
        .and_then(|model| model.modalities.clone())
}

fn resolve_modalities(
    provider_name: &str,
    model_name: &str,
    legacy_input_modalities: Vec<Modality>,
) -> (Vec<Modality>, Vec<Modality>) {
    let canonical_provider = crate::config::declarative_providers::load_provider(provider_name)
        .ok()
        .filter(|provider| provider.is_editable)
        .and_then(|provider| provider.config.catalog_provider_id)
        .unwrap_or_else(|| provider_name.to_string());
    let canonical =
        goose_providers::canonical::maybe_get_canonical_model(&canonical_provider, model_name)
            .or_else(|| {
                let (base, _effort) = strip_reasoning_effort_suffix(model_name)?;
                goose_providers::canonical::maybe_get_canonical_model(&canonical_provider, &base)
            });
    let custom = custom_model_modalities(provider_name, model_name);

    (
        normalize_modalities(
            custom
                .as_ref()
                .and_then(|modalities| modalities.input.clone())
                .unwrap_or_else(|| {
                    canonical
                        .as_ref()
                        .map(|model| model.modalities.input.clone())
                        .unwrap_or(legacy_input_modalities)
                }),
        ),
        normalize_modalities(
            custom
                .and_then(|modalities| modalities.output)
                .unwrap_or_else(|| {
                    canonical
                        .map(|model| model.modalities.output)
                        .unwrap_or_default()
                }),
        ),
    )
}

fn apply_canonical_limits(provider_name: &str, model: ModelConfig) -> ModelConfig {
    let model = if provider_name == goose_providers::azure_foundry::AZURE_FOUNDRY_PROVIDER_NAME {
        model
    } else {
        model.with_canonical_limits(provider_name)
    };
    let legacy_input_modalities = model.input_modalities.clone();
    let (input_modalities, output_modalities) =
        resolve_modalities(provider_name, &model.model_name, legacy_input_modalities);
    model.with_modalities(input_modalities, output_modalities)
}

fn materialize_model_config_inner(
    mut model: ModelConfig,
    provider_name: &str,
    include_default_thinking_effort: bool,
) -> Result<ModelConfig> {
    let config = Config::global();

    if model.temperature.is_none() {
        model = model.with_temperature(get_goose_temperature(config)?);
    }

    if model.toolshim && model.toolshim_model.is_none() {
        model = model.with_toolshim_model(get_goose_toolshim_model(config)?);
    }

    model = model.with_default_max_tokens(config.get_goose_max_tokens()?);

    if include_default_thinking_effort {
        model = model.with_default_thinking_effort(config.get_goose_thinking_effort());
    }

    if model.cache_ttl().is_none() {
        if let Some(ttl) = get_goose_cache_ttl(config)? {
            model = model.with_cache_ttl(&ttl);
        }
    }

    if provider_name == goose_providers::openai::OPEN_AI_PROVIDER_NAME {
        model = apply_openai_request_params(model);
    }

    Ok(model)
}

fn one_shot_model_config(model_config: ModelConfig) -> ModelConfig {
    model_config
        .with_thinking_effort(ThinkingEffort::Off)
        .with_prompt_cache_disabled()
}

/// Run a completion for a one-shot auxiliary task on the main session model.
/// Thinking is disabled and prompt-cache writes are skipped because this prompt
/// will not recur.
pub async fn complete_one_shot(
    provider: &dyn Provider,
    model_config: &ModelConfig,
    session_id: &str,
    system: &str,
    messages: &[Message],
    tools: &[Tool],
) -> Result<(Message, ProviderUsage), ProviderError> {
    let one_shot_model_config = one_shot_model_config(model_config.clone());

    crate::session_context::with_session_id(
        Some(session_id.to_string()),
        provider.complete(&one_shot_model_config, system, messages, tools),
    )
    .await
}

fn apply_openai_request_params(mut model: ModelConfig) -> ModelConfig {
    let config = Config::global();
    if let Some(store) = config.get_openai_store() {
        model = model.with_merged_request_params(HashMap::from([(
            "store".to_string(),
            serde_json::json!(store),
        )]));
    }
    model
}

fn base_model_config_from_user_config(
    provider_name: &str,
    model_name: &str,
) -> Result<ModelConfig> {
    let config = Config::global();
    let mut model = ModelConfig {
        model_name: model_name.to_string(),
        context_limit: None,
        temperature: get_goose_temperature(config)?,
        max_tokens: None,
        toolshim: get_goose_toolshim(config)?.unwrap_or(false),
        toolshim_model: get_goose_toolshim_model(config)?,
        request_params: None,
        reasoning: None,
        input_modalities: vec![Modality::Text],
        output_modalities: vec![Modality::Text],
        request_headers: None,
    };
    if provider_name != goose_providers::azure_foundry::AZURE_FOUNDRY_PROVIDER_NAME {
        model.normalize_effort_suffix();
    }
    Ok(model)
}

/// Re-derive the prompt-cache TTL from the current configuration, discarding
/// any value stored on the model config. The TTL is configuration state, not
/// session state: a resumed session must reflect the user's current opt-in,
/// not a value persisted by an earlier (possibly clamped) run.
pub fn with_rederived_cache_ttl(model: ModelConfig) -> Result<ModelConfig> {
    let mut model = model.without_cache_ttl();
    if let Some(ttl) = get_goose_cache_ttl(Config::global())? {
        model = model.with_cache_ttl(&ttl);
    }
    Ok(model)
}

fn get_goose_cache_ttl(config: &Config) -> Result<Option<String>> {
    match config.get_param::<String>("GOOSE_CACHE_TTL") {
        Ok(ttl) => {
            let ttl = ttl.trim().to_lowercase();
            match ttl.as_str() {
                "5m" | "1h" => Ok(Some(ttl)),
                other => Err(anyhow!(
                    "GOOSE_CACHE_TTL must be '5m' or '1h', got '{other}'"
                )),
            }
        }
        Err(ConfigError::NotFound(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn get_goose_temperature(config: &Config) -> Result<Option<f32>> {
    match config.get_param::<f32>("GOOSE_TEMPERATURE") {
        Ok(temp) if temp < 0.0 => Err(anyhow!(
            "Value for 'GOOSE_TEMPERATURE' is out of valid range: {temp}"
        )),
        Ok(temp) => Ok(Some(temp)),
        Err(ConfigError::NotFound(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn get_goose_toolshim(config: &Config) -> Result<Option<bool>> {
    match config.get_param::<serde_yaml::Value>("GOOSE_TOOLSHIM") {
        Ok(value) => parse_yaml_bool_config("GOOSE_TOOLSHIM", value).map(Some),
        Err(ConfigError::NotFound(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Resolve the global toolshim setting, defaulting to false when unset.
pub fn global_toolshim() -> bool {
    get_goose_toolshim(Config::global())
        .ok()
        .flatten()
        .unwrap_or(false)
}

fn get_goose_toolshim_model(config: &Config) -> Result<Option<String>> {
    match config.get_param::<String>("GOOSE_TOOLSHIM_OLLAMA_MODEL") {
        Ok(value) if value.trim().is_empty() => Err(anyhow!(
            "Invalid value for 'GOOSE_TOOLSHIM_OLLAMA_MODEL': '{value}' - cannot be empty if set"
        )),
        Ok(value) => Ok(Some(value)),
        Err(ConfigError::NotFound(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn parse_bool_config(key: &str, value: &str) -> Result<bool> {
    match value.to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(anyhow!(
            "Invalid value for '{key}': '{value}' - must be one of: 1, true, yes, on, 0, false, no, off"
        )),
    }
}

fn parse_yaml_bool_config(key: &str, value: serde_yaml::Value) -> Result<bool> {
    match value {
        serde_yaml::Value::Bool(value) => Ok(value),
        serde_yaml::Value::Number(value) => parse_bool_config(key, &value.to_string()),
        serde_yaml::Value::String(value) => parse_bool_config(key, &value),
        other => {
            Err(anyhow!(
            "Invalid value for '{key}': '{}' - must be one of: 1, true, yes, on, 0, false, no, off",
            serde_yaml::to_string(&other).unwrap_or_else(|_| "<unprintable>".to_string()).trim()
        ))
        }
    }
}

#[cfg(test)]
mod modality_tests {
    use super::*;
    use crate::config::declarative_providers::{
        create_custom_provider, CreateCustomProviderParams,
    };
    use goose_providers::base::{ModelInfo, ModelModalities};

    fn create_provider(model: ModelInfo) -> String {
        create_custom_provider(CreateCustomProviderParams {
            engine: "openai".to_string(),
            display_name: "Modalities".to_string(),
            api_url: "https://example.invalid/v1".to_string(),
            api_key: None,
            models: vec![model],
            supports_streaming: Some(true),
            headers: None,
            requires_auth: false,
            catalog_provider_id: None,
            base_path: None,
            toolshim: false,
            preserves_thinking: None,
            auth: None,
        })
        .unwrap()
        .name
    }

    #[test]
    fn custom_modalities_override_canonical_metadata_per_direction() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().display().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root.as_str()))]);
        let mut model = ModelInfo::new("gpt-4o");
        model.modalities = Some(ModelModalities {
            input: Some(vec![Modality::Text]),
            output: Some(vec![Modality::Audio]),
        });
        let provider = create_provider(model);

        let config = materialize_model_config(&provider, ModelConfig::new("gpt-4o")).unwrap();
        assert_eq!(config.input_modalities, vec![Modality::Text]);
        assert_eq!(config.output_modalities, vec![Modality::Audio]);
    }

    #[test]
    fn custom_modalities_fall_back_independently_and_normalize_empty_lists() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().display().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root.as_str()))]);
        let mut model = ModelInfo::new("unknown-model");
        model.modalities = Some(ModelModalities {
            input: Some(Vec::new()),
            output: None,
        });
        let provider = create_provider(model);

        let config =
            materialize_model_config(&provider, ModelConfig::new("unknown-model")).unwrap();
        assert_eq!(config.input_modalities, vec![Modality::Text]);
        assert_eq!(config.output_modalities, vec![Modality::Text]);
    }

    #[test]
    fn suffixed_custom_model_matches_normalized_runtime_model() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().display().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root.as_str()))]);
        let mut model = ModelInfo::new("gpt-5-high");
        model.modalities = Some(ModelModalities {
            input: Some(vec![Modality::Text, Modality::Image]),
            output: Some(vec![Modality::Text]),
        });
        let provider = create_provider(model);

        let config = materialize_model_config(&provider, ModelConfig::new("gpt-5")).unwrap();
        assert!(config.supports_input_modality(Modality::Image));
    }

    #[test]
    fn xai_suffixed_custom_model_matches_normalized_runtime_model() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().display().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root.as_str()))]);
        let mut model = ModelInfo::new("grok-3-mini-high");
        model.modalities = Some(ModelModalities {
            input: Some(vec![Modality::Text, Modality::Image]),
            output: Some(vec![Modality::Text]),
        });
        let provider = create_provider(model);

        let config = materialize_model_config(&provider, ModelConfig::new("grok-3-mini")).unwrap();
        assert!(config.supports_input_modality(Modality::Image));
    }

    #[test]
    fn custom_input_modalities_control_openai_image_formatting() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().display().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root.as_str()))]);
        let mut model = ModelInfo::new("gpt-4o");
        model.modalities = Some(ModelModalities {
            input: Some(vec![Modality::Text, Modality::Image]),
            output: None,
        });
        let provider = create_provider(model);
        let messages = vec![Message::user().with_image("aW1hZ2U=", "image/png")];

        let config = materialize_model_config(&provider, ModelConfig::new("gpt-4o")).unwrap();
        let request = goose_providers::formats::openai::create_request(
            &config,
            "",
            &messages,
            &[],
            &goose_providers::images::ImageFormat::OpenAi,
            false,
        )
        .unwrap();
        assert!(request.to_string().contains("image_url"));

        let mut model = ModelInfo::new("gpt-4o");
        model.modalities = Some(ModelModalities {
            input: Some(vec![Modality::Text]),
            output: None,
        });
        let provider = create_provider(model);
        let config = materialize_model_config(&provider, ModelConfig::new("gpt-4o")).unwrap();
        let request = goose_providers::formats::openai::create_request(
            &config,
            "",
            &messages,
            &[],
            &goose_providers::images::ImageFormat::OpenAi,
            false,
        )
        .unwrap();
        assert!(!request.to_string().contains("image_url"));
    }
}

#[cfg(test)]
mod one_shot_tests {
    use super::*;

    #[test]
    fn thinking_and_prompt_cache_are_disabled() {
        let config = one_shot_model_config(
            ModelConfig::new("claude-haiku-4-5").with_thinking_effort(ThinkingEffort::High),
        );

        assert_eq!(config.thinking_effort(), Some(ThinkingEffort::Off));
        assert!(config.prompt_cache_disabled());
    }
}

#[cfg(test)]
mod cache_ttl_tests {
    use super::*;

    #[test]
    fn env_var_populates_cache_ttl() {
        let _guard = env_lock::lock_env([("GOOSE_CACHE_TTL", Some("1h"))]);
        let model = materialize_model_config_inner(
            ModelConfig::new("claude-sonnet-4-5"),
            "anthropic",
            false,
        )
        .unwrap();
        assert_eq!(model.cache_ttl().as_deref(), Some("1h"));
    }

    #[test]
    fn absent_env_var_leaves_cache_ttl_unset() {
        let _guard = env_lock::lock_env([("GOOSE_CACHE_TTL", None::<&str>)]);
        let model = materialize_model_config_inner(
            ModelConfig::new("claude-sonnet-4-5"),
            "anthropic",
            false,
        )
        .unwrap();
        assert!(model.cache_ttl().is_none());
    }

    #[test]
    fn invalid_env_var_is_rejected() {
        let _guard = env_lock::lock_env([("GOOSE_CACHE_TTL", Some("2h"))]);
        let result = materialize_model_config_inner(
            ModelConfig::new("claude-sonnet-4-5"),
            "anthropic",
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn rederive_replaces_stored_ttl_with_configured_value() {
        let _guard = env_lock::lock_env([("GOOSE_CACHE_TTL", Some("1h"))]);
        let model =
            with_rederived_cache_ttl(ModelConfig::new("claude-sonnet-4-5").with_cache_ttl("5m"))
                .unwrap();
        assert_eq!(model.cache_ttl().as_deref(), Some("1h"));
    }

    #[test]
    fn rederive_drops_stored_ttl_when_config_absent() {
        let _guard = env_lock::lock_env([("GOOSE_CACHE_TTL", None::<&str>)]);
        let model =
            with_rederived_cache_ttl(ModelConfig::new("claude-sonnet-4-5").with_cache_ttl("1h"))
                .unwrap();
        assert!(model.cache_ttl().is_none());
    }

    #[test]
    fn explicit_model_ttl_wins_over_env_var() {
        let _guard = env_lock::lock_env([("GOOSE_CACHE_TTL", Some("1h"))]);
        let model = materialize_model_config_inner(
            ModelConfig::new("claude-sonnet-4-5").with_cache_ttl("5m"),
            "anthropic",
            false,
        )
        .unwrap();
        assert_eq!(model.cache_ttl().as_deref(), Some("5m"));
    }
}

#[cfg(test)]
mod azure_foundry_tests {
    use super::*;

    #[test]
    fn deployment_name_survives_thinking_effort_changes() {
        let config = base_model_config_from_user_config("azure_foundry", "gpt-5-high")
            .unwrap()
            .with_thinking_effort(ThinkingEffort::Off);

        assert_eq!(config.model_name, "gpt-5-high");
        assert_eq!(config.context_limit, None);
        assert_eq!(config.thinking_effort(), Some(ThinkingEffort::Off));
    }

    #[test]
    fn none_suffixed_deployment_name_is_preserved() {
        let config = base_model_config_from_user_config("azure_foundry", "gpt-5-none").unwrap();

        assert_eq!(config.model_name, "gpt-5-none");
        assert_eq!(config.thinking_effort(), None);
    }
}
