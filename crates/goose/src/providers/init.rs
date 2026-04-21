use std::sync::{Arc, RwLock};

#[cfg(feature = "aws-providers")]
use super::bedrock::BedrockProvider;
#[cfg(feature = "local-inference")]
use super::local_inference::LocalInferenceProvider;
#[cfg(feature = "aws-providers")]
use super::sagemaker_tgi::SageMakerTgiProvider;
use super::{
    amp_acp::AmpAcpProvider,
    anthropic::AnthropicProvider,
    avian::AvianProvider,
    azure::AzureProvider,
    base::{Provider, ProviderMetadata},
    chatgpt_codex::ChatGptCodexProvider,
    claude_acp::ClaudeAcpProvider,
    claude_code::ClaudeCodeProvider,
    codex::CodexProvider,
    codex_acp::CodexAcpProvider,
    copilot_acp::CopilotAcpProvider,
    cursor_agent::CursorAgentProvider,
    databricks::DatabricksProvider,
    gcpvertexai::GcpVertexAIProvider,
    gemini_cli::GeminiCliProvider,
    gemini_oauth::GeminiOAuthProvider,
    githubcopilot::GithubCopilotProvider,
    google::GoogleProvider,
    kimicode::KimiCodeProvider,
    litellm::LiteLLMProvider,
    nanogpt::NanoGptProvider,
    ollama::OllamaProvider,
    openai::OpenAiProvider,
    openrouter::OpenRouterProvider,
    pi_acp::PiAcpProvider,
    provider_registry::ProviderRegistry,
    snowflake::SnowflakeProvider,
    tetrate::TetrateProvider,
    venice::VeniceProvider,
    xai::XaiProvider,
};
use crate::config::ExtensionConfig;
use crate::model::ModelConfig;
use crate::providers::base::ProviderType;
use crate::{
    config::declarative_providers::register_declarative_providers,
    providers::provider_registry::ProviderEntry,
};
use anyhow::Result;
use tokio::sync::OnceCell;

static REGISTRY: OnceCell<RwLock<ProviderRegistry>> = OnceCell::const_new();

async fn init_registry() -> RwLock<ProviderRegistry> {
    let mut registry = ProviderRegistry::new().with_providers(|registry| {
        registry.register::<AmpAcpProvider>(false);
        registry.register::<AnthropicProvider>(true);
        registry.register::<AvianProvider>(false);
        registry.register::<AzureProvider>(false);
        #[cfg(feature = "aws-providers")]
        registry.register::<BedrockProvider>(false);
        #[cfg(feature = "local-inference")]
        registry.register::<LocalInferenceProvider>(false);
        registry.register::<ChatGptCodexProvider>(true);
        registry.register::<ClaudeAcpProvider>(false);
        registry.register::<ClaudeCodeProvider>(true);
        registry.register::<CodexAcpProvider>(false);
        registry.register::<CopilotAcpProvider>(false);
        registry.register::<CodexProvider>(true);
        registry.register::<CursorAgentProvider>(false);
        registry.register::<DatabricksProvider>(true);
        registry.register::<GcpVertexAIProvider>(false);
        registry.register::<GeminiCliProvider>(false);
        registry.register::<GeminiOAuthProvider>(true);
        registry.register::<GithubCopilotProvider>(false);
        registry.register::<GoogleProvider>(true);
        registry.register::<KimiCodeProvider>(true);
        registry.register::<LiteLLMProvider>(false);
        registry.register::<NanoGptProvider>(true);
        registry.register::<OllamaProvider>(true);
        registry.register::<OpenAiProvider>(true);
        registry.register::<OpenRouterProvider>(true);
        registry.register::<PiAcpProvider>(false);
        #[cfg(feature = "aws-providers")]
        registry.register::<SageMakerTgiProvider>(false);
        registry.register::<SnowflakeProvider>(false);
        registry.register::<TetrateProvider>(true);
        registry.register::<VeniceProvider>(false);
        registry.register::<XaiProvider>(false);
    });
    // Register cleanup functions for providers with cached state
    registry.set_cleanup(
        "github_copilot",
        Arc::new(|| Box::pin(GithubCopilotProvider::cleanup())),
    );
    registry.set_cleanup(
        "databricks",
        Arc::new(|| Box::pin(DatabricksProvider::cleanup())),
    );
    registry.set_cleanup(
        "kimi_code",
        Arc::new(|| Box::pin(KimiCodeProvider::cleanup())),
    );
    registry.set_cleanup(
        "chatgpt_codex",
        Arc::new(|| Box::pin(ChatGptCodexProvider::cleanup())),
    );

    if let Err(e) = load_custom_providers_into_registry(&mut registry) {
        tracing::warn!("Failed to load custom providers: {}", e);
    }
    RwLock::new(registry)
}

fn load_custom_providers_into_registry(registry: &mut ProviderRegistry) -> Result<()> {
    register_declarative_providers(registry)
}

async fn get_registry() -> &'static RwLock<ProviderRegistry> {
    REGISTRY.get_or_init(init_registry).await
}

pub async fn providers() -> Vec<(ProviderMetadata, ProviderType)> {
    get_registry()
        .await
        .read()
        .unwrap()
        .all_metadata_with_types()
}

pub async fn refresh_custom_providers() -> Result<()> {
    let registry = get_registry().await;
    registry.write().unwrap().remove_custom_providers();

    if let Err(e) = load_custom_providers_into_registry(&mut registry.write().unwrap()) {
        tracing::warn!("Failed to refresh custom providers: {}", e);
        return Err(e);
    }

    tracing::info!("Custom providers refreshed");
    Ok(())
}

pub async fn get_from_registry(name: &str) -> Result<ProviderEntry> {
    let guard = get_registry().await.read().unwrap();
    guard
        .entries
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", name))
        .cloned()
}

pub async fn inventory_identity(name: &str) -> Result<super::inventory::InventoryIdentityInput> {
    get_from_registry(name).await?.inventory_identity()
}

pub async fn create(
    name: &str,
    model: ModelConfig,
    extensions: Vec<ExtensionConfig>,
) -> Result<Arc<dyn Provider>> {
    let entry = get_from_registry(name).await?;
    entry.create(model, extensions).await
}

pub async fn create_with_default_model(
    name: impl AsRef<str>,
    extensions: Vec<ExtensionConfig>,
) -> Result<Arc<dyn Provider>> {
    get_from_registry(name.as_ref())
        .await?
        .create_with_default_model(extensions)
        .await
}

pub async fn cleanup_provider(name: &str) -> Result<()> {
    let cleanup_fn = {
        let registry = get_registry().await.read().unwrap();
        registry
            .entries
            .get(name)
            .and_then(|entry| entry.cleanup.clone())
    };
    if let Some(cleanup) = cleanup_fn {
        return cleanup().await;
    }
    Ok(())
}

pub async fn create_with_named_model(
    provider_name: &str,
    model_name: &str,
    extensions: Vec<ExtensionConfig>,
) -> Result<Arc<dyn Provider>> {
    let config = ModelConfig::new(model_name)?;
    create(provider_name, config, extensions).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::paths::Paths;
    use std::fs;

    #[tokio::test]
    async fn test_tanzu_declarative_provider_registry_wiring() {
        let providers_list = providers().await;
        let tanzu = providers_list
            .iter()
            .find(|(m, _)| m.name == "tanzu_ai")
            .expect("tanzu_ai provider should be registered");
        let (meta, provider_type) = tanzu;

        // Should be a Declarative (fixed) provider
        assert_eq!(*provider_type, ProviderType::Declarative);

        assert_eq!(meta.display_name, "VMware Tanzu Platform");
        assert_eq!(meta.default_model, "openai/gpt-oss-120b");

        // First config key should be TANZU_AI_API_KEY (secret, required)
        let api_key = meta
            .config_keys
            .iter()
            .find(|k| k.name == "TANZU_AI_API_KEY")
            .expect("TANZU_AI_API_KEY config key should exist");
        assert!(
            api_key.required,
            "API key should be required for fixed declarative provider"
        );
        assert!(api_key.secret, "API key should be secret");

        // Should have TANZU_AI_ENDPOINT config key (not secret, required)
        let endpoint = meta
            .config_keys
            .iter()
            .find(|k| k.name == "TANZU_AI_ENDPOINT")
            .expect("TANZU_AI_ENDPOINT config key should exist");
        assert!(endpoint.required, "Endpoint should be required");
        assert!(!endpoint.secret, "Endpoint should not be secret");
    }

    #[tokio::test]
    async fn test_openai_compatible_providers_config_keys() {
        let providers_list = providers().await;
        let required_api_key_cases = vec![
            ("groq", "GROQ_API_KEY"),
            ("mistral", "MISTRAL_API_KEY"),
            ("custom_deepseek", "DEEPSEEK_API_KEY"),
        ];
        for (name, expected_key) in required_api_key_cases {
            if let Some((meta, _)) = providers_list.iter().find(|(m, _)| m.name == name) {
                assert!(
                    !meta.config_keys.is_empty(),
                    "{name} provider should have config keys"
                );
                assert_eq!(
                    meta.config_keys[0].name, expected_key,
                    "First config key for {name} should be {expected_key}, got {}",
                    meta.config_keys[0].name
                );
                assert!(
                    meta.config_keys[0].required,
                    "{expected_key} should be required"
                );
                assert!(
                    meta.config_keys[0].secret,
                    "{expected_key} should be secret"
                );
            } else {
                // Provider not registered; skip test for this provider
                continue;
            }
        }

        if let Some((meta, _)) = providers_list.iter().find(|(m, _)| m.name == "openai") {
            assert!(
                !meta.config_keys.is_empty(),
                "openai provider should have config keys"
            );
            assert_eq!(
                meta.config_keys[0].name, "OPENAI_API_KEY",
                "First config key for openai should be OPENAI_API_KEY"
            );
            assert!(
                !meta.config_keys[0].required,
                "OPENAI_API_KEY should be optional for local server support"
            );
            assert!(
                meta.config_keys[0].secret,
                "OPENAI_API_KEY should be secret"
            );
        }
    }

    #[tokio::test]
    async fn test_custom_provider_context_limit_is_applied_from_file() {
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", None::<&str>)]);
        let temp_dir = tempfile::tempdir().expect("tempdir should be created");
        std::env::set_var("GOOSE_PATH_ROOT", temp_dir.path());

        let custom_dir = Paths::config_dir().join("custom_providers");
        fs::create_dir_all(&custom_dir).expect("custom providers dir should be created");

        let custom_inf = r#"{
  "name": "custom_inf",
  "engine": "openai",
  "display_name": "Custom Inf",
  "description": "test provider",
  "api_key_env": "",
  "base_url": "https://example.invalid/v1/chat/completions",
  "models": [
    {"name": "kimi-k2.5", "context_limit": 256000}
  ],
  "requires_auth": false
}"#;
        fs::write(custom_dir.join("custom_inf.json"), custom_inf)
            .expect("custom_inf.json should be written");

        let custom_zero = r#"{
  "name": "custom_zero",
  "engine": "openai",
  "display_name": "Custom Zero",
  "description": "test provider",
  "api_key_env": "",
  "base_url": "https://example.invalid/v1/chat/completions",
  "models": [
    {"name": "zero-model", "context_limit": 0}
  ],
  "requires_auth": false
}"#;
        fs::write(custom_dir.join("custom_zero.json"), custom_zero)
            .expect("custom_zero.json should be written");

        refresh_custom_providers()
            .await
            .expect("custom providers should refresh");

        let provider = create_with_named_model("custom_inf", "kimi-k2.5", Vec::new())
            .await
            .expect("custom_inf provider should be creatable");
        assert_eq!(provider.get_model_config().context_limit, Some(256_000));

        let zero_provider = create_with_named_model("custom_zero", "zero-model", Vec::new())
            .await
            .expect("custom_zero provider should be creatable");
        assert_eq!(zero_provider.get_model_config().context_limit, None);

        std::env::remove_var("GOOSE_PATH_ROOT");
    }
}
