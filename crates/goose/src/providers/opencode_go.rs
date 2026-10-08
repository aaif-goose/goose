use super::base::{ProviderDef, ProviderDescriptor, ProviderMetadata};
use crate::config::declarative_providers::DeclarativeProviderConfig;
use crate::conversation::message::Message;
use crate::providers::base::{MessageStream, Provider};
use futures::future::BoxFuture;
use goose_providers::anthropic::AnthropicProvider;
use goose_providers::api_client::TlsConfig;
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;
use goose_providers::openai::OpenAiProvider;
use rmcp::model::Tool;

#[derive(Debug, PartialEq)]
enum OpenCodeGoRoute {
    Responses,
    Messages,
    ChatCompletions,
}

const OPEN_CODE_GO_PROVIDER_NAME: &str = "opencode_go";

const RESPONSES_MODELS: &[&str] = &["grok-4.6"];
const RESPONSES_PREFIXES: &[&str] = &["gpt-", "muse-spark-"];
const MESSAGES_PREFIXES: &[&str] = &["minimax-", "qwen"];
const CHAT_COMPLETIONS_MODELS: &[&str] = &["grok-4.5", "omen-alpha"];
const CHAT_COMPLETIONS_PREFIXES: &[&str] =
    &["glm-", "kimi-", "longcat-", "deepseek-", "mimo-", "hy"];

fn has_prefix(model: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| model.starts_with(prefix))
}

fn route_for_model(model_name: &str) -> Result<OpenCodeGoRoute, ProviderError> {
    if RESPONSES_MODELS.contains(&model_name) || has_prefix(model_name, RESPONSES_PREFIXES) {
        return Ok(OpenCodeGoRoute::Responses);
    } else if has_prefix(model_name, MESSAGES_PREFIXES) {
        return Ok(OpenCodeGoRoute::Messages);
    } else if CHAT_COMPLETIONS_MODELS.contains(&model_name)
        || has_prefix(model_name, CHAT_COMPLETIONS_PREFIXES)
    {
        return Ok(OpenCodeGoRoute::ChatCompletions);
    }
    Err(ProviderError::InvalidValue(format!(
        "Model {} is not supported",
        model_name
    )))
}

pub struct OpenCodeGoProvider {
    openai: OpenAiProvider,
    anthropic: AnthropicProvider,
}

impl OpenCodeGoProvider {
    pub fn from_custom_config(
        config: DeclarativeProviderConfig,
        tls_config: Option<TlsConfig>,
    ) -> anyhow::Result<Self> {
        let openai =
            crate::providers::openai_def::from_custom_config(config.clone(), tls_config.clone())?;
        let anthropic = crate::providers::anthropic_def::from_custom_config(config, tls_config)?;
        Ok(Self { openai, anthropic })
    }

    pub fn matches_declarative_config(config: &DeclarativeProviderConfig) -> bool {
        config.name == OPEN_CODE_GO_PROVIDER_NAME
    }
}

#[async_trait::async_trait]
impl Provider for OpenCodeGoProvider {
    fn get_name(&self) -> &str {
        self.openai.get_name()
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        match route_for_model(&model_config.model_name)? {
            OpenCodeGoRoute::ChatCompletions => {
                self.openai
                    .stream(model_config, system, messages, tools)
                    .await
            }
            OpenCodeGoRoute::Messages => {
                self.anthropic
                    .stream_for_model(
                        model_config,
                        &model_config.model_name,
                        system,
                        messages,
                        tools,
                    )
                    .await
            }
            OpenCodeGoRoute::Responses => {
                let (wire_model, _) = goose_providers::formats::openai::extract_reasoning_effort(
                    &model_config.model_name,
                );
                self.openai
                    .stream_for_model(
                        model_config,
                        &wire_model,
                        &model_config.model_name,
                        system,
                        messages,
                        tools,
                    )
                    .await
            }
        }
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        self.openai.fetch_supported_models().await
    }

    async fn get_context_limit(&self, model: &str, override_limit: Option<usize>) -> usize {
        self.openai.get_context_limit(model, override_limit).await
    }

    fn skip_canonical_filtering(&self) -> bool {
        self.openai.skip_canonical_filtering()
    }
}

impl ProviderDescriptor for OpenCodeGoProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            OPEN_CODE_GO_PROVIDER_NAME,
            "OpenCode Go",
            "Access OpenCode Go models via the OpenAI and Anthropic APIs.",
            "kimi-k2.6",
            vec![],
            "https://opencode.ai/docs/go",
            vec![],
        )
    }
}

impl ProviderDef for OpenCodeGoProvider {
    type Provider = Self;

    fn from_env(
        _extensions: Vec<crate::config::ExtensionConfig>,
        _tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, anyhow::Result<Self::Provider>> {
        Box::pin(async {
            anyhow::bail!("OpenCode Go provider cannot be created from environment variables. Please use a declarative configuration.");
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_for_model() {
        assert_eq!(
            route_for_model("gpt-5.6-luna").unwrap(),
            OpenCodeGoRoute::Responses
        );
        assert_eq!(
            route_for_model("grok-4.6").unwrap(),
            OpenCodeGoRoute::Responses
        );
        assert_eq!(
            route_for_model("muse-spark-abc").unwrap(),
            OpenCodeGoRoute::Responses
        );
        assert_eq!(
            route_for_model("qwen3.6-plus").unwrap(),
            OpenCodeGoRoute::Messages
        );
        assert_eq!(
            route_for_model("minimax-abc").unwrap(),
            OpenCodeGoRoute::Messages
        );
        assert_eq!(
            route_for_model("longcat-2.0").unwrap(),
            OpenCodeGoRoute::ChatCompletions
        );
        assert_eq!(
            route_for_model("glm-xyz").unwrap(),
            OpenCodeGoRoute::ChatCompletions
        );
        assert_eq!(
            route_for_model("hy3").unwrap(),
            OpenCodeGoRoute::ChatCompletions
        );
        assert_eq!(
            route_for_model("grok-4.5").unwrap(),
            OpenCodeGoRoute::ChatCompletions
        );
        assert_eq!(
            route_for_model("omen-alpha").unwrap(),
            OpenCodeGoRoute::ChatCompletions
        );
        for model in ["grok-123", "grok-4.50", "grok-4.60", "omen-alpha-next"] {
            assert!(matches!(
                route_for_model(model),
                Err(ProviderError::InvalidValue(_))
            ));
        }
        assert!(matches!(
            route_for_model("unknown-model"),
            Err(ProviderError::InvalidValue(_))
        ));
    }
}
