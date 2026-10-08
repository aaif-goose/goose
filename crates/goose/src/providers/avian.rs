use super::api_client::{ApiClient, AuthMethod};
use super::base::{ConfigKey, ProviderDef, ProviderMetadata};
use super::openai_compatible::OpenAiCompatibleProvider;
use anyhow::Result;
use futures::future::BoxFuture;

const AVIAN_PROVIDER_NAME: &str = "avian";
pub const AVIAN_API_HOST: &str = "https://api.avian.io/v1";
pub const AVIAN_DEFAULT_MODEL: &str = "deepseek/deepseek-v3.2";
pub const AVIAN_KNOWN_MODELS: &[&str] = &[
    "deepseek/deepseek-v3.2",
    "moonshotai/kimi-k2.5",
    "z-ai/glm-5",
    "minimax/minimax-m2.5",
];
pub const AVIAN_DOC_URL: &str = "https://avian.io/docs";

pub struct AvianProvider;

impl goose_providers::base::ProviderDescriptor for AvianProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            AVIAN_PROVIDER_NAME,
            "Avian",
            "Cost-effective inference API with DeepSeek, Kimi, GLM, and MiniMax models",
            AVIAN_DEFAULT_MODEL,
            AVIAN_KNOWN_MODELS.to_vec(),
            AVIAN_DOC_URL,
            vec![
                ConfigKey::new("AVIAN_API_KEY", true, true, None, true),
                ConfigKey::new("AVIAN_HOST", false, false, Some(AVIAN_API_HOST), false),
            ],
        )
    }
}

impl ProviderDef for AvianProvider {
    type Provider = OpenAiCompatibleProvider;

    fn from_env(
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<OpenAiCompatibleProvider>> {
        Box::pin(async move {
            let config = crate::config::Config::global();
            let api_key: String = config.get_secret("AVIAN_API_KEY")?;
            let host: String = config
                .get_param("AVIAN_HOST")
                .unwrap_or_else(|_| AVIAN_API_HOST.to_string());

            let api_client =
                ApiClient::new_with_tls(host, AuthMethod::BearerToken(api_key), tls_config)?
                    .with_request_builder(crate::session_context::session_id_request_builder());

            Ok(OpenAiCompatibleProvider::new(
                AVIAN_PROVIDER_NAME.to_string(),
                api_client,
                String::new(),
            ))
        })
    }
}
