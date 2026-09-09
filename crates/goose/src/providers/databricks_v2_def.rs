use anyhow::Result;
use futures::future::BoxFuture;
use goose_providers::api_client::TlsConfig;
use goose_providers::base::ProviderDescriptor;
use goose_providers::databricks_auth::{
    DatabricksAuth, DatabricksOauthTokenProvider, DatabricksRefreshHook, DatabricksTokenResolver,
};
use goose_providers::databricks_v2::DatabricksV2Provider;
use std::sync::Arc;

use crate::config::{Config, ConfigError, ExtensionConfig};
use crate::providers::base::ProviderDef;

pub struct DatabricksV2ProviderDef;

impl ProviderDescriptor for DatabricksV2ProviderDef {
    fn metadata() -> goose_providers::base::ProviderMetadata {
        DatabricksV2Provider::metadata().with_setup(
            crate::providers::catalog::ProviderSetupMetadata::new(
                crate::providers::catalog::ProviderSetupCategory::Model,
                crate::providers::catalog::ProviderSetupMethod::HostWithOauthFallback,
                crate::providers::catalog::ProviderSetupGroup::Additional,
            )
            .with_docs_url("https://docs.databricks.com/en/generative-ai/ai-gateway/")
            .with_aliases(&["databricks_ai_gateway"])
            .with_capabilities(false, true, false)
            .with_field(
                "DATABRICKS_HOST",
                "Host URL",
                Some("https://dbc-...cloud.databricks.com"),
                None,
            )
            .with_field(
                "DATABRICKS_TOKEN",
                "Access Token",
                Some("Paste your access token"),
                None,
            ),
        )
    }
}

impl ProviderDef for DatabricksV2ProviderDef {
    type Provider = DatabricksV2Provider;

    fn from_env(
        _extensions: Vec<ExtensionConfig>,
        tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(from_env(tls_config))
    }
}

pub async fn from_env(tls_config: Option<TlsConfig>) -> Result<DatabricksV2Provider> {
    let config = Config::global();
    let host = load_host(config)?;
    let retry_config = DatabricksV2Provider::load_retry_config(|key| config.get_param(key).ok());
    let auth = if let Ok(api_key) = config.get_secret("DATABRICKS_TOKEN") {
        DatabricksAuth::token(api_key)
    } else {
        DatabricksAuth::oauth(host.clone())
    };

    DatabricksV2Provider::new(
        host,
        auth,
        retry_config,
        tls_config,
        Some(oauth_token_provider()),
        Some(token_resolver()),
        Some(crate::session_context::session_id_request_builder()),
        Some(refresh_hook()),
    )
}

pub async fn cleanup() -> Result<()> {
    crate::providers::oauth::cleanup_oauth_cache()
}

fn load_host(config: &Config) -> Result<String> {
    let mut host: Result<String, ConfigError> = config.get_param("DATABRICKS_HOST");
    if host.is_err() {
        host = config.get_secret("DATABRICKS_HOST")
    }

    host.map_err(|_| {
        ConfigError::NotFound(
            "Did not find DATABRICKS_HOST in either config file or keyring".to_string(),
        )
        .into()
    })
}

fn token_resolver() -> DatabricksTokenResolver {
    Arc::new(|| {
        Config::global()
            .get_secret::<String>("DATABRICKS_TOKEN")
            .ok()
    })
}

fn refresh_hook() -> DatabricksRefreshHook {
    Arc::new(|| Config::global().invalidate_secrets_cache())
}

fn oauth_token_provider() -> DatabricksOauthTokenProvider {
    Arc::new(|host, client_id, redirect_url, scopes| {
        Box::pin(async move {
            crate::providers::oauth::get_oauth_token_async(
                &host,
                &client_id,
                &redirect_url,
                &scopes,
            )
            .await
        })
    })
}
