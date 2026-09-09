use std::sync::Arc;

use async_trait::async_trait;
use goose_providers::base::Provider;
use goose_providers::conversation::message::Message;
use goose_providers::conversation::token_usage::ProviderUsage;
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;

/// The single completion call compaction needs. Implementations decide model
/// selection, fallbacks and session plumbing.
#[async_trait]
pub trait CompactionModel: Send + Sync {
    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
    ) -> Result<(Message, ProviderUsage), ProviderError>;
}

/// Counts tokens for usage estimation and retained-context reporting.
#[async_trait]
pub trait TokenEstimator: Send + Sync {
    async fn count_chat_tokens(&self, system: &str, messages: &[Message]) -> usize;
    async fn count_text_tokens(&self, text: &str) -> usize;
}

pub struct ProviderModel {
    provider: Arc<dyn Provider>,
    model_config: ModelConfig,
}

impl ProviderModel {
    pub fn new(provider: Arc<dyn Provider>, model_config: ModelConfig) -> Self {
        Self {
            provider,
            model_config,
        }
    }
}

#[async_trait]
impl CompactionModel for ProviderModel {
    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        self.provider
            .complete(&self.model_config, system, messages, &[])
            .await
    }
}
