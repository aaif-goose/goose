pub mod amp_acp;
pub mod anthropic;
pub mod api_client;
pub mod avian;
pub mod azure;
pub mod azureauth;
pub mod base;
#[cfg(feature = "aws-providers")]
pub mod bedrock;
pub mod canonical;
pub mod catalog;
pub mod chatgpt_codex;
pub mod claude_acp;
pub mod claude_code;
pub(crate) mod cli_common;
pub mod codex;
pub mod codex_acp;
pub mod copilot_acp;
pub mod cursor_agent;
pub mod databricks;
pub mod embedding;
pub mod errors;
pub mod formats;
mod gcpauth;
pub mod gcpvertexai;
pub mod gemini_cli;
pub mod gemini_oauth;
pub mod githubcopilot;
pub mod google;
mod init;
pub mod litellm;
#[cfg(feature = "local-inference")]
pub mod local_inference;
pub mod nanogpt;
pub mod oauth;
pub mod ollama;
pub mod openai;
pub mod openai_compatible;
pub mod openrouter;
pub mod pi_acp;
pub mod provider_registry;
pub mod provider_test;
mod retry;
#[cfg(feature = "aws-providers")]
pub mod sagemaker_tgi;
pub mod snowflake;
pub mod testprovider;
pub mod tetrate;
pub mod toolshim;
pub mod usage_estimator;
pub mod utils;
pub mod venice;
pub mod xai;

pub use init::{
    cleanup_provider, create, create_with_default_model, create_with_named_model,
    get_from_registry, providers, refresh_custom_providers,
};
pub use retry::{retry_operation, RetryConfig};
