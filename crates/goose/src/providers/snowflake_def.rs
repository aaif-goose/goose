use anyhow::Result;
use futures::future::BoxFuture;
use goose_providers::base::ProviderDescriptor;
use goose_providers::snowflake::SnowflakeProvider;

use crate::config::{Config, ConfigError, ExtensionConfig};
use crate::providers::api_client::TlsConfig;
use crate::providers::base::{ProviderDef, ProviderMetadata};

pub struct SnowflakeProviderDef;

impl ProviderDescriptor for SnowflakeProviderDef {
    fn metadata() -> ProviderMetadata {
        SnowflakeProvider::metadata().with_setup(
            crate::providers::catalog::ProviderSetupMetadata::new(
                crate::providers::catalog::ProviderSetupCategory::Model,
                crate::providers::catalog::ProviderSetupMethod::ConfigFields,
                crate::providers::catalog::ProviderSetupGroup::Additional,
            )
            .with_field(
                "SNOWFLAKE_HOST",
                "Host URL",
                Some("https://your-account.snowflakecomputing.com"),
                None,
            )
            .with_field(
                "SNOWFLAKE_TOKEN",
                "Access Token",
                Some("Paste your access token"),
                None,
            ),
        )
    }
}

impl ProviderDef for SnowflakeProviderDef {
    type Provider = SnowflakeProvider;

    fn from_env(
        _extensions: Vec<ExtensionConfig>,
        tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(from_env(tls_config))
    }
}

pub async fn from_env(tls_config: Option<TlsConfig>) -> Result<SnowflakeProvider> {
    let config = Config::global();
    let host = get_config_or_secret(config, "SNOWFLAKE_HOST")?;
    let token = get_config_or_secret(config, "SNOWFLAKE_TOKEN")?;

    SnowflakeProvider::new(
        host,
        token,
        tls_config,
        Some(crate::session_context::session_id_request_builder()),
    )
}

fn get_config_or_secret(config: &Config, key: &str) -> Result<String> {
    config
        .get_param(key)
        .or_else(|_| config.get_secret(key))
        .map_err(|_| {
            ConfigError::NotFound(format!(
                "Did not find {} in either config file or keyring",
                key
            ))
            .into()
        })
}
