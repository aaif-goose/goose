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

pub trait ProviderDef: ProviderDescriptor + Send + Sync {
    type Provider: Provider + 'static;

    /// Instances hold a remote session or are built from one session's
    /// extensions and working directory, so they cannot be shared.
    const SESSION_BOUND: bool = false;

    fn from_env(
        extensions: Vec<ExtensionConfig>,
        tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>>
    where
        Self: Sized;

    fn from_env_with_working_dir(
        extensions: Vec<ExtensionConfig>,
        _working_dir: PathBuf,
        tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>>
    where
        Self: Sized,
    {
        Self::from_env(extensions, tls_config)
    }

    fn from_env_with_default_model(
        extensions: Vec<ExtensionConfig>,
        tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>>
    where
        Self: Sized,
    {
        Self::from_env(extensions, tls_config)
    }
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
