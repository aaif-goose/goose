use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;
use utoipa::ToSchema;

pub const DEFAULT_CONTEXT_LIMIT: usize = 128_000;

#[derive(Debug, Clone, Deserialize)]
struct PredefinedModel {
    name: String,
    #[serde(default)]
    context_limit: Option<usize>,
    #[serde(default)]
    request_params: Option<HashMap<String, Value>>,
}

fn get_predefined_models() -> Vec<PredefinedModel> {
    static PREDEFINED_MODELS: Lazy<Vec<PredefinedModel>> =
        Lazy::new(|| match std::env::var("GOOSE_PREDEFINED_MODELS") {
            Ok(json_str) => serde_json::from_str(&json_str).unwrap_or_else(|e| {
                tracing::warn!("Failed to parse GOOSE_PREDEFINED_MODELS: {}", e);
                Vec::new()
            }),
            Err(_) => Vec::new(),
        });
    PREDEFINED_MODELS.clone()
}

fn find_predefined_model(model_name: &str) -> Option<PredefinedModel> {
    get_predefined_models()
        .into_iter()
        .find(|m| m.name == model_name)
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Environment variable '{0}' not found")]
    EnvVarMissing(String),
    #[error("Invalid value for '{0}': '{1}' - {2}")]
    InvalidValue(String, String, String),
    #[error("Value for '{0}' is out of valid range: {1}")]
    InvalidRange(String, String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct ModelConfig {
    pub model_name: String,
    pub context_limit: Option<usize>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    pub toolshim: bool,
    pub toolshim_model: Option<String>,
    #[serde(skip)]
    pub fast_model_config: Option<Box<ModelConfig>>,
    /// Provider-specific request parameters (e.g., anthropic_beta headers)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_params: Option<HashMap<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
}

impl ModelConfig {
    pub fn new(model_name: &str) -> Result<Self, ConfigError> {
        Self::new_base(model_name.to_string(), None)
    }

    pub fn new_with_context_env(
        model_name: String,
        provider_name: &str,
        context_env_var: Option<&str>,
    ) -> Result<Self, ConfigError> {
        let config = Self::new_base(model_name, context_env_var)?;
        Ok(config.with_canonical_limits(provider_name))
    }

    fn new_base(model_name: String, context_env_var: Option<&str>) -> Result<Self, ConfigError> {
        // Check a provider-specific env var first (e.g. DATABRICKS_CONTEXT_LIMIT),
        // then fall back to GOOSE_CONTEXT_LIMIT.  Using Config::global().get_param()
        // reads from both environment variables and config.yaml, so users can set
        // `GOOSE_CONTEXT_LIMIT: 1000000` in config.yaml instead of exporting an
        // env var.  See #7839.
        let context_limit = if let Some(env_var) = context_env_var {
            if let Ok(val) = std::env::var(env_var) {
                Some(Self::validate_context_limit(&val, env_var)?)
            } else {
                None
            }
        } else {
            match crate::config::Config::global().get_param::<usize>("GOOSE_CONTEXT_LIMIT") {
                Ok(limit) => {
                    if limit == 0 {
                        return Err(ConfigError::InvalidRange(
                            "GOOSE_CONTEXT_LIMIT".to_string(),
                            "must be greater than 0".to_string(),
                        ));
                    }
                    Some(limit)
                }
                Err(crate::config::ConfigError::NotFound(_)) => None,
                Err(e) => {
                    return Err(ConfigError::InvalidValue(
                        "GOOSE_CONTEXT_LIMIT".to_string(),
                        String::new(),
                        e.to_string(),
                    ))
                }
            }
        };

        let max_tokens = Self::parse_max_tokens()?;
        let temperature = Self::parse_temperature()?;
        let toolshim = Self::parse_toolshim()?;
        let toolshim_model = Self::parse_toolshim_model()?;

        // Pick up request_params from predefined models (always applies)
        let predefined = find_predefined_model(&model_name);
        let request_params = predefined.and_then(|pm| pm.request_params);

        Ok(Self {
            model_name,
            context_limit,
            temperature,
            max_tokens,
            toolshim,
            toolshim_model,
            fast_model_config: None,
            request_params,
            reasoning: None,
        })
    }

    pub fn with_canonical_limits(mut self, provider_name: &str) -> Self {
        if let Some(pm) = find_predefined_model(&self.model_name) {
            if self.context_limit.is_none() {
                self.context_limit = pm.context_limit;
            }
        }

        // Try canonical lookup with the full model name first, then fall back
        // to the name with reasoning-effort suffixes stripped (e.g.
        // "databricks-gpt-5.4-high" → "databricks-gpt-5.4").
        let canonical =
            crate::providers::canonical::maybe_get_canonical_model(provider_name, &self.model_name)
                .or_else(|| {
                    let (base, _effort) =
                        crate::providers::utils::extract_reasoning_effort(&self.model_name);
                    if base != self.model_name {
                        crate::providers::canonical::maybe_get_canonical_model(provider_name, &base)
                    } else {
                        None
                    }
                });

        if let Some(canonical) = canonical {
            if self.context_limit.is_none() {
                self.context_limit = Some(canonical.limit.context);
            }
            if self.max_tokens.is_none() {
                self.max_tokens = canonical.limit.output.map(|o| o as i32);
            }
            if self.reasoning.is_none() {
                self.reasoning = canonical.reasoning;
            }
        }

        self
    }

    fn validate_context_limit(val: &str, env_var: &str) -> Result<usize, ConfigError> {
        let limit = val.parse::<usize>().map_err(|_| {
            ConfigError::InvalidValue(
                env_var.to_string(),
                val.to_string(),
                "must be a positive integer".to_string(),
            )
        })?;

        if limit < 4 * 1024 {
            return Err(ConfigError::InvalidRange(
                env_var.to_string(),
                "must be greater than 4K".to_string(),
            ));
        }

        Ok(limit)
    }

    fn parse_temperature() -> Result<Option<f32>, ConfigError> {
        if let Ok(val) = std::env::var("GOOSE_TEMPERATURE") {
            let temp = val.parse::<f32>().map_err(|_| {
                ConfigError::InvalidValue(
                    "GOOSE_TEMPERATURE".to_string(),
                    val.clone(),
                    "must be a valid number".to_string(),
                )
            })?;
            if temp < 0.0 {
                return Err(ConfigError::InvalidRange(
                    "GOOSE_TEMPERATURE".to_string(),
                    val,
                ));
            }
            Ok(Some(temp))
        } else {
            Ok(None)
        }
    }

    fn parse_max_tokens() -> Result<Option<i32>, ConfigError> {
        match crate::config::Config::global().get_param::<i32>("GOOSE_MAX_TOKENS") {
            Ok(tokens) => {
                if tokens <= 0 {
                    return Err(ConfigError::InvalidRange(
                        "goose_max_tokens".to_string(),
                        "must be greater than 0".to_string(),
                    ));
                }
                Ok(Some(tokens))
            }
            Err(crate::config::ConfigError::NotFound(_)) => Ok(None),
            Err(e) => Err(ConfigError::InvalidValue(
                "goose_max_tokens".to_string(),
                String::new(),
                e.to_string(),
            )),
        }
    }

    fn parse_toolshim() -> Result<bool, ConfigError> {
        if let Ok(val) = std::env::var("GOOSE_TOOLSHIM") {
            match val.to_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => Ok(true),
                "0" | "false" | "no" | "off" => Ok(false),
                _ => Err(ConfigError::InvalidValue(
                    "GOOSE_TOOLSHIM".to_string(),
                    val,
                    "must be one of: 1, true, yes, on, 0, false, no, off".to_string(),
                )),
            }
        } else {
            Ok(false)
        }
    }

    fn parse_toolshim_model() -> Result<Option<String>, ConfigError> {
        match std::env::var("GOOSE_TOOLSHIM_OLLAMA_MODEL") {
            Ok(val) if val.trim().is_empty() => Err(ConfigError::InvalidValue(
                "GOOSE_TOOLSHIM_OLLAMA_MODEL".to_string(),
                val,
                "cannot be empty if set".to_string(),
            )),
            Ok(val) => Ok(Some(val)),
            Err(_) => Ok(None),
        }
    }

    pub fn with_context_limit(mut self, limit: Option<usize>) -> Self {
        if limit.is_some() {
            self.context_limit = limit;
        }
        self
    }

    pub fn with_temperature(mut self, temp: Option<f32>) -> Self {
        self.temperature = temp;
        self
    }

    pub fn with_max_tokens(mut self, tokens: Option<i32>) -> Self {
        self.max_tokens = tokens;
        self
    }

    pub fn with_toolshim(mut self, toolshim: bool) -> Self {
        self.toolshim = toolshim;
        self
    }

    pub fn with_toolshim_model(mut self, model: Option<String>) -> Self {
        self.toolshim_model = model;
        self
    }

    pub fn with_fast(
        mut self,
        fast_model_name: &str,
        provider_name: &str,
    ) -> Result<Self, ConfigError> {
        // Create a full ModelConfig for the fast model with proper canonical lookup
        let fast_config = ModelConfig::new(fast_model_name)?.with_canonical_limits(provider_name);
        self.fast_model_config = Some(Box::new(fast_config));
        Ok(self)
    }

    pub fn with_request_params(mut self, params: Option<HashMap<String, Value>>) -> Self {
        self.request_params = params;
        self
    }

    pub fn use_fast_model(&self) -> Self {
        if let Some(fast_config) = &self.fast_model_config {
            *fast_config.clone()
        } else {
            self.clone()
        }
    }

    pub fn context_limit(&self) -> usize {
        self.context_limit.unwrap_or(DEFAULT_CONTEXT_LIMIT)
    }

    pub fn is_openai_reasoning_model(&self) -> bool {
        crate::providers::utils::is_openai_responses_model(&self.model_name)
    }

    pub fn max_output_tokens(&self) -> i32 {
        if let Some(tokens) = self.max_tokens {
            return tokens;
        }

        4_096
    }

    pub fn get_config_param<T: for<'de> serde::Deserialize<'de>>(
        &self,
        request_key: &str,
        config_key: &str,
    ) -> Option<T> {
        self.request_params
            .as_ref()
            .and_then(|params| params.get(request_key))
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .or_else(|| {
                crate::config::Config::global()
                    .get_param::<T>(config_key)
                    .ok()
            })
    }

    pub fn new_or_fail(model_name: &str) -> ModelConfig {
        ModelConfig::new(model_name)
            .unwrap_or_else(|_| panic!("Failed to create model config for {}", model_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_max_tokens_valid() {
        let _guard = env_lock::lock_env([("GOOSE_MAX_TOKENS", Some("4096"))]);
        let result = ModelConfig::parse_max_tokens().unwrap();
        assert_eq!(result, Some(4096));
    }

    #[test]
    fn test_parse_max_tokens_not_set() {
        let _guard = env_lock::lock_env([("GOOSE_MAX_TOKENS", None::<&str>)]);
        let result = ModelConfig::parse_max_tokens().unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_max_tokens_invalid_string() {
        let _guard = env_lock::lock_env([("GOOSE_MAX_TOKENS", Some("not_a_number"))]);
        let result = ModelConfig::parse_max_tokens();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::InvalidValue(..)));
    }

    #[test]
    fn test_parse_max_tokens_zero() {
        let _guard = env_lock::lock_env([("GOOSE_MAX_TOKENS", Some("0"))]);
        let result = ModelConfig::parse_max_tokens();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::InvalidRange(..)));
    }

    #[test]
    fn test_parse_max_tokens_negative() {
        let _guard = env_lock::lock_env([("GOOSE_MAX_TOKENS", Some("-100"))]);
        let result = ModelConfig::parse_max_tokens();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::InvalidRange(..)));
    }

    #[test]
    fn test_model_config_with_max_tokens_env() {
        let _guard = env_lock::lock_env([
            ("GOOSE_MAX_TOKENS", Some("8192")),
            ("GOOSE_TEMPERATURE", None::<&str>),
            ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ("GOOSE_TOOLSHIM", None::<&str>),
            ("GOOSE_TOOLSHIM_OLLAMA_MODEL", None::<&str>),
        ]);
        let config = ModelConfig::new("test-model").unwrap();
        assert_eq!(config.max_tokens, Some(8192));
    }

    #[test]
    fn test_model_config_without_max_tokens_env() {
        let _guard = env_lock::lock_env([
            ("GOOSE_MAX_TOKENS", None::<&str>),
            ("GOOSE_TEMPERATURE", None::<&str>),
            ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ("GOOSE_TOOLSHIM", None::<&str>),
            ("GOOSE_TOOLSHIM_OLLAMA_MODEL", None::<&str>),
        ]);
        let config = ModelConfig::new("test-model").unwrap();
        assert_eq!(config.max_tokens, None);
    }

    #[test]
    fn test_get_config_param() {
        let _guard = env_lock::lock_env([
            ("CLAUDE_THINKING_EFFORT", Some("high")),
            ("CLAUDE_THINKING_TYPE", None::<&str>),
        ]);

        let mut params = HashMap::new();
        params.insert("effort".to_string(), serde_json::json!("low"));

        let config_with_params = ModelConfig {
            model_name: "test".to_string(),
            request_params: Some(params),
            ..Default::default()
        };

        let config_without_params = ModelConfig {
            request_params: None,
            ..config_with_params.clone()
        };

        assert_eq!(
            config_with_params.get_config_param::<String>("effort", "CLAUDE_THINKING_EFFORT"),
            Some("low".to_string())
        );
        assert_eq!(
            config_without_params.get_config_param::<String>("effort", "CLAUDE_THINKING_EFFORT"),
            Some("high".to_string())
        );
        assert_eq!(
            config_without_params
                .get_config_param::<String>("nonexistent", "NONEXISTENT_CONFIG_KEY"),
            None
        );
    }

    mod with_canonical_limits {
        use super::*;

        #[test]
        fn sets_limits_from_canonical_model() {
            let _guard = env_lock::lock_env([
                ("GOOSE_MAX_TOKENS", None::<&str>),
                ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ]);
            let config = ModelConfig::new_or_fail("gpt-4o").with_canonical_limits("openai");

            assert_eq!(config.context_limit, Some(128_000));
            assert_eq!(config.max_tokens, Some(16_384));
            assert_eq!(config.reasoning, Some(false));
        }

        #[test]
        fn does_not_override_existing_context_limit() {
            let _guard = env_lock::lock_env([
                ("GOOSE_MAX_TOKENS", None::<&str>),
                ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ]);
            let mut config = ModelConfig::new_or_fail("gpt-4o");
            config.context_limit = Some(64_000);
            let config = config.with_canonical_limits("openai");

            assert_eq!(config.context_limit, Some(64_000));
        }

        #[test]
        fn does_not_override_existing_max_tokens() {
            let _guard = env_lock::lock_env([
                ("GOOSE_MAX_TOKENS", None::<&str>),
                ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ]);
            let mut config = ModelConfig::new_or_fail("gpt-4o");
            config.max_tokens = Some(1_000);
            let config = config.with_canonical_limits("openai");

            assert_eq!(config.max_tokens, Some(1_000));
        }

        #[test]
        fn unknown_model_leaves_fields_none() {
            let _guard = env_lock::lock_env([
                ("GOOSE_MAX_TOKENS", None::<&str>),
                ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ]);
            let config =
                ModelConfig::new_or_fail("totally-unknown-model").with_canonical_limits("openai");

            assert_eq!(config.context_limit, None);
            assert_eq!(config.max_tokens, None);
            assert_eq!(config.reasoning, None);
        }

        #[test]
        fn resolves_after_stripping_reasoning_effort_suffix() {
            let _guard = env_lock::lock_env([
                ("GOOSE_MAX_TOKENS", None::<&str>),
                ("GOOSE_CONTEXT_LIMIT", None::<&str>),
            ]);

            // "databricks-gpt-5.4-high" should resolve via "databricks-gpt-5.4"
            let config = ModelConfig::new_or_fail("databricks-gpt-5.4-high")
                .with_canonical_limits("databricks");
            assert_eq!(config.context_limit, Some(1_050_000));

            // "gpt-5.4-xhigh" should resolve via "gpt-5.4"
            let config = ModelConfig::new_or_fail("gpt-5.4-xhigh").with_canonical_limits("openai");
            assert_eq!(config.context_limit, Some(1_050_000));

            // "gpt-5.4-nano-low" should resolve via "gpt-5.4-nano"
            let config =
                ModelConfig::new_or_fail("gpt-5.4-nano-low").with_canonical_limits("openai");
            assert_eq!(config.context_limit, Some(400_000));
        }
    }

    mod is_openai_reasoning_model {
        use super::*;

        const ENV_LOCK_KEYS: [(&str, Option<&str>); 5] = [
            ("GOOSE_MAX_TOKENS", None),
            ("GOOSE_TEMPERATURE", None),
            ("GOOSE_CONTEXT_LIMIT", None),
            ("GOOSE_TOOLSHIM", None),
            ("GOOSE_TOOLSHIM_OLLAMA_MODEL", None),
        ];

        #[test]
        fn bare_reasoning_models() {
            let _guard = env_lock::lock_env(ENV_LOCK_KEYS);
            assert!(ModelConfig::new_or_fail("o1").is_openai_reasoning_model());
            assert!(ModelConfig::new_or_fail("o1-preview").is_openai_reasoning_model());
            assert!(ModelConfig::new_or_fail("o3").is_openai_reasoning_model());
            assert!(ModelConfig::new_or_fail("o3-mini").is_openai_reasoning_model());
            assert!(ModelConfig::new_or_fail("o4-mini").is_openai_reasoning_model());
            assert!(ModelConfig::new_or_fail("gpt-5").is_openai_reasoning_model());
            assert!(ModelConfig::new_or_fail("gpt-5-3-codex").is_openai_reasoning_model());
        }

        #[test]
        fn goose_prefixed_reasoning_models() {
            let _guard = env_lock::lock_env(ENV_LOCK_KEYS);
            assert!(ModelConfig::new_or_fail("goose-o3-mini").is_openai_reasoning_model());
            assert!(ModelConfig::new_or_fail("goose-o4-mini").is_openai_reasoning_model());
            assert!(ModelConfig::new_or_fail("goose-gpt-5").is_openai_reasoning_model());
        }

        #[test]
        fn databricks_prefixed_reasoning_models() {
            let _guard = env_lock::lock_env(ENV_LOCK_KEYS);
            assert!(ModelConfig::new_or_fail("databricks-o3-mini").is_openai_reasoning_model());
            assert!(ModelConfig::new_or_fail("databricks-o4-mini").is_openai_reasoning_model());
            assert!(ModelConfig::new_or_fail("databricks-gpt-5").is_openai_reasoning_model());
        }

        #[test]
        fn non_reasoning_models() {
            let _guard = env_lock::lock_env(ENV_LOCK_KEYS);
            assert!(!ModelConfig::new_or_fail("claude-sonnet-4").is_openai_reasoning_model());
            assert!(!ModelConfig::new_or_fail("gpt-4o").is_openai_reasoning_model());
            assert!(
                !ModelConfig::new_or_fail("databricks-claude-sonnet-4").is_openai_reasoning_model()
            );
            assert!(!ModelConfig::new_or_fail("goose-claude-sonnet-4").is_openai_reasoning_model());
            assert!(!ModelConfig::new_or_fail("llama-3-70b").is_openai_reasoning_model());
        }
    }
}
