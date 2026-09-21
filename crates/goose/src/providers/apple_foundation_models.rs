pub use goose_providers::apple_foundation_models::AppleFoundationModelsProvider;

use crate::{
    config::ExtensionConfig,
    providers::{api_client::TlsConfig, base::ProviderDef},
};
use anyhow::Result;
use futures::future::BoxFuture;

impl ProviderDef for AppleFoundationModelsProvider {
    type Provider = Self;
    fn from_env(
        _extensions: Vec<ExtensionConfig>,
        _tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<Self>> {
        Box::pin(async { Ok(Self::new()?) })
    }
}
