use super::api_client::{ApiClient, AuthMethod};
use super::base::{ConfigKey, ProviderDef, ProviderMetadata};
use super::openai_compatible::OpenAiCompatibleProvider;
use crate::model::ModelConfig;
use anyhow::Result;
use futures::future::BoxFuture;

const PERPLEXITY_PROVIDER_NAME: &str = "perplexity";
pub const PERPLEXITY_API_HOST: &str = "https://api.perplexity.ai";
pub const PERPLEXITY_DEFAULT_MODEL: &str = "sonar-pro";

/// Models exposed via Perplexity's OpenAI-compatible chat completions endpoint.
///
/// Perplexity ships new and renames existing models on its own cadence; this list
/// is a curated default for setup wizards. Users can override
/// `GOOSE_MODEL` to point at any other model the API accepts.
pub const PERPLEXITY_KNOWN_MODELS: &[&str] = &[
    "sonar",
    "sonar-pro",
    "sonar-reasoning",
    "sonar-reasoning-pro",
];

pub const PERPLEXITY_DOC_URL: &str = "https://docs.perplexity.ai/docs/getting-started";

pub struct PerplexityProvider;

impl PerplexityProvider {
    /// Resolves the API key, accepting either `PERPLEXITY_API_KEY` (the canonical
    /// name) or `PPLX_API_KEY` (the abbreviated alias used by Perplexity's SDKs).
    fn resolve_api_key() -> Result<String, crate::config::ConfigError> {
        let config = crate::config::Config::global();
        match config.get_secret::<String>("PERPLEXITY_API_KEY") {
            Ok(key) => Ok(key),
            Err(primary_err) => match config.get_secret::<String>("PPLX_API_KEY") {
                Ok(key) => Ok(key),
                Err(_) => Err(primary_err),
            },
        }
    }
}

impl ProviderDef for PerplexityProvider {
    type Provider = OpenAiCompatibleProvider;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            PERPLEXITY_PROVIDER_NAME,
            "Perplexity",
            "Perplexity chat models with built-in real-time web search grounding",
            PERPLEXITY_DEFAULT_MODEL,
            PERPLEXITY_KNOWN_MODELS.to_vec(),
            PERPLEXITY_DOC_URL,
            vec![
                ConfigKey::new("PERPLEXITY_API_KEY", true, true, None, true),
                ConfigKey::new(
                    "PERPLEXITY_HOST",
                    false,
                    false,
                    Some(PERPLEXITY_API_HOST),
                    false,
                ),
            ],
        )
        .with_setup_steps(vec![
            "Go to https://www.perplexity.ai/account/api/keys",
            "Create or copy an existing API key",
            "Paste the key above as PERPLEXITY_API_KEY",
        ])
    }

    fn from_env(
        model: ModelConfig,
        _extensions: Vec<crate::config::ExtensionConfig>,
    ) -> BoxFuture<'static, Result<OpenAiCompatibleProvider>> {
        Box::pin(async move {
            let api_key = Self::resolve_api_key()?;
            let host: String = crate::config::Config::global()
                .get_param("PERPLEXITY_HOST")
                .unwrap_or_else(|_| PERPLEXITY_API_HOST.to_string());

            let api_client = ApiClient::new(host, AuthMethod::BearerToken(api_key))?;

            Ok(OpenAiCompatibleProvider::new(
                PERPLEXITY_PROVIDER_NAME.to_string(),
                api_client,
                model,
                String::new(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_structure() {
        let metadata = PerplexityProvider::metadata();

        assert_eq!(metadata.name, PERPLEXITY_PROVIDER_NAME);
        assert_eq!(metadata.display_name, "Perplexity");
        assert_eq!(metadata.default_model, PERPLEXITY_DEFAULT_MODEL);
        assert_eq!(metadata.model_doc_link, PERPLEXITY_DOC_URL);

        assert_eq!(metadata.config_keys.len(), 2);

        let api_key = &metadata.config_keys[0];
        assert_eq!(api_key.name, "PERPLEXITY_API_KEY");
        assert!(api_key.required);
        assert!(api_key.secret);
        assert!(api_key.primary);

        let host = &metadata.config_keys[1];
        assert_eq!(host.name, "PERPLEXITY_HOST");
        assert!(!host.required);
        assert!(!host.secret);
        assert_eq!(host.default.as_deref(), Some(PERPLEXITY_API_HOST));
    }

    #[test]
    fn test_known_models_non_empty() {
        let metadata = PerplexityProvider::metadata();
        assert!(!metadata.known_models.is_empty());
        assert!(metadata
            .known_models
            .iter()
            .any(|m| m.name == PERPLEXITY_DEFAULT_MODEL));
    }

    #[test]
    fn test_setup_steps_present() {
        let metadata = PerplexityProvider::metadata();
        assert!(!metadata.setup_steps.is_empty());
        assert!(metadata
            .setup_steps
            .iter()
            .any(|step| step.contains("PERPLEXITY_API_KEY")));
    }

    #[test]
    fn test_default_model_is_known() {
        assert!(PERPLEXITY_KNOWN_MODELS.contains(&PERPLEXITY_DEFAULT_MODEL));
    }

    #[test]
    fn test_doc_url_points_to_perplexity_docs() {
        assert!(PERPLEXITY_DOC_URL.starts_with("https://docs.perplexity.ai"));
    }
}
