use super::api_client::{ApiClient, AuthMethod};
use super::base::{ConfigKey, ProviderDef, ProviderMetadata};
use super::openai_compatible::OpenAiCompatibleProvider;
use anyhow::Result;
use futures::future::BoxFuture;

const GONDOLA_PROVIDER_NAME: &str = "gondola";
pub const GONDOLA_API_HOST: &str = "https://api.gondola-ai.com/v1";
pub const GONDOLA_DEFAULT_MODEL: &str = "deepseek-v4-flash";
pub const GONDOLA_KNOWN_MODELS: &[&str] = &[
    "claude-opus-5",
    "claude-sonnet-5",
    "deepseek-v3.2",
    "deepseek-v4-flash",
    "deepseek-v4-pro",
    "e2ee-deepseek-v4-flash",
    "e2ee-qwen3-6-35b-a3b",
    "kimi-k2-5",
    "kimi-k3",
    "llama-3.3-70b",
    "qwen3-235b-a22b-instruct-2507",
];
pub const GONDOLA_DOC_URL: &str = "https://gondola-ai.com/guides";

pub struct GondolaProvider;

impl goose_providers::base::ProviderDescriptor for GondolaProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            GONDOLA_PROVIDER_NAME,
            "Gondola",
            "Pay-per-request inference via Venice AI, settled in USDC. Privacy-preserving with TEE model options.",
            GONDOLA_DEFAULT_MODEL,
            GONDOLA_KNOWN_MODELS.to_vec(),
            GONDOLA_DOC_URL,
            vec![
                ConfigKey::new("GONDOLA_API_KEY", true, true, None, true),
                ConfigKey::new("GONDOLA_HOST", false, false, Some(GONDOLA_API_HOST), false),
            ],
        )
        .with_setup(
            crate::providers::catalog::ProviderSetupMetadata::api_key(
                crate::providers::catalog::ProviderSetupGroup::Default,
            )
            .with_docs_url(GONDOLA_DOC_URL),
        )
    }
}

impl ProviderDef for GondolaProvider {
    type Provider = OpenAiCompatibleProvider;

    fn from_env(
        _extensions: Vec<crate::config::ExtensionConfig>,
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<OpenAiCompatibleProvider>> {
        Box::pin(async move {
            let config = crate::config::Config::global();
            let api_key: String = config.get_secret("GONDOLA_API_KEY")?;
            let host: String = config
                .get_param("GONDOLA_HOST")
                .unwrap_or_else(|_| GONDOLA_API_HOST.to_string());

            let api_client =
                ApiClient::new_with_tls(host, AuthMethod::BearerToken(api_key), tls_config)?
                    .with_request_builder(crate::session_context::session_id_request_builder());

            Ok(OpenAiCompatibleProvider::new(
                GONDOLA_PROVIDER_NAME.to_string(),
                api_client,
                String::new(),
            ))
        })
    }
}
