use super::api_client::{ApiClient, AuthMethod};
use super::base::{ConfigKey, ProviderDef, ProviderMetadata};
use super::openai_compatible::OpenAiCompatibleProvider;
use crate::model::ModelConfig;
use anyhow::Result;
use futures::future::BoxFuture;

const NVIDIA_PROVIDER_NAME: &str = "nvidia";
pub const NVIDIA_API_HOST: &str = "https://integrate.api.nvidia.com/v1";
pub const NVIDIA_DEFAULT_MODEL: &str = "nvidia/llama-3.3-nemotron-super-49b-v1.5";
pub const NVIDIA_KNOWN_MODELS: &[&str] = &[
    "nvidia/llama-3.3-nemotron-super-49b-v1.5",
    "meta/llama-3.1-70b-instruct",
    "meta/llama-3.3-70b-instruct",
    "deepseek-ai/deepseek-r1",
    "moonshotai/kimi-k2-instruct",
    "microsoft/phi-4-mini-instruct",
    "google/gemma-3-27b-it",
    "qwen/qwen3-235b-a22b-instruct-2507",
];

pub const NVIDIA_DOC_URL: &str = "https://docs.api.nvidia.com/nim/reference/llm-apis";

pub struct NvidiaProvider;

impl ProviderDef for NvidiaProvider {
    type Provider = OpenAiCompatibleProvider;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            NVIDIA_PROVIDER_NAME,
            "NVIDIA NIM",
            "NVIDIA NIM hosted models (Llama, DeepSeek, Nemotron, Kimi, and more) via an OpenAI-compatible API",
            NVIDIA_DEFAULT_MODEL,
            NVIDIA_KNOWN_MODELS.to_vec(),
            NVIDIA_DOC_URL,
            vec![
                ConfigKey::new("NVIDIA_API_KEY", true, true, None, true),
                ConfigKey::new("NVIDIA_BASE_URL", false, false, Some(NVIDIA_API_HOST), false),
            ],
        )
    }

    fn from_env(
        model: ModelConfig,
        _extensions: Vec<crate::config::ExtensionConfig>,
    ) -> BoxFuture<'static, Result<OpenAiCompatibleProvider>> {
        Box::pin(async move {
            let config = crate::config::Config::global();
            let api_key: String = config.get_secret("NVIDIA_API_KEY")?;
            let host: String = config
                .get_param("NVIDIA_BASE_URL")
                .unwrap_or_else(|_| NVIDIA_API_HOST.to_string());

            let api_client = ApiClient::new(host, AuthMethod::BearerToken(api_key))?;

            Ok(OpenAiCompatibleProvider::new(
                NVIDIA_PROVIDER_NAME.to_string(),
                api_client,
                model,
                String::new(),
            ))
        })
    }
}
