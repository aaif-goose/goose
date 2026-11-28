use std::sync::Arc;

use async_trait::async_trait;
use oi_kit_provider_types::base::Provider;
use oi_kit_provider_types::conversation::message::Message;
use oi_kit_provider_types::conversation::token_usage::ProviderUsage;
use oi_kit_provider_types::errors::ProviderError;
use oi_kit_provider_types::model::ModelConfig;
use oi_kit_provider_types::pricing::{CatalogPrices, Prices};

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
    prices: Arc<dyn Prices>,
}

impl ProviderModel {
    pub fn new(provider: Arc<dyn Provider>, model_config: ModelConfig) -> Self {
        Self {
            provider,
            model_config,
            prices: Arc::new(CatalogPrices),
        }
    }

    pub fn with_prices(mut self, prices: Arc<dyn Prices>) -> Self {
        self.prices = prices;
        self
    }
}

#[async_trait]
impl CompactionModel for ProviderModel {
    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        let (message, usage) = self
            .provider
            .complete(&self.model_config, system, messages, &[])
            .await?;
        let usage = usage.priced(self.provider.get_name(), self.prices.as_ref());
        Ok((message, usage))
    }
}
