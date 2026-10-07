use super::api_client::TlsConfig;
use anyhow::Result;
use futures::future::BoxFuture;
pub use goose_providers::conversation::token_usage::{
    CostSource, DraftStats, ProviderStats, ProviderUsage, Usage,
};
use serde::{Deserialize, Serialize};

pub use goose_providers::api_client::{
    DEFAULT_CONNECT_TIMEOUT_SECS, DEFAULT_PROVIDER_TIMEOUT_SECS,
};

use crate::config::ExtensionConfig;

use std::path::PathBuf;

pub use goose_providers::base::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderType {
    Preferred,
    Builtin,
    Declarative,
    Custom,
}

pub(crate) fn current_working_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Defines providers that can be shared across sessions.
pub trait ProviderDef: ProviderDescriptor + Send + Sync {
    type Provider: Provider + 'static;

    fn from_env(tls_config: Option<TlsConfig>) -> BoxFuture<'static, Result<Self::Provider>>
    where
        Self: Sized;
}

/// Defines legacy providers that retain session state or extension-derived MCP configuration.
///
/// These definitions remain separate from ACP without splitting the runtime Provider interface.
pub trait SessionBoundProviderDef: ProviderDescriptor + Send + Sync {
    type Provider: Provider + 'static;

    fn from_env(
        extensions: Vec<ExtensionConfig>,
        tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>>
    where
        Self: Sized;
}

/// Defines ACP construction independently of standard provider construction.
///
/// The registry still bridges ACP instances to `Arc<dyn Provider>` temporarily;
/// this trait separates definitions, not the runtime provider interfaces.
pub trait AcpProviderDef: ProviderDescriptor + Send + Sync {
    fn from_env(
        extensions: Vec<ExtensionConfig>,
        tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<crate::acp::AcpProvider>>
    where
        Self: Sized;

    fn from_env_with_working_dir(
        extensions: Vec<ExtensionConfig>,
        _working_dir: PathBuf,
        tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<crate::acp::AcpProvider>>
    where
        Self: Sized,
    {
        Self::from_env(extensions, tls_config)
    }

    fn from_env_with_default_model(
        extensions: Vec<ExtensionConfig>,
        tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<crate::acp::AcpProvider>>
    where
        Self: Sized,
    {
        Self::from_env(extensions, tls_config)
    }
}
