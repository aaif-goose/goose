//! Goose SDK.

pub use goose_sdk_types::custom_requests;
pub use goose_sdk_types::{AgentEvent, ExtensionSpec, ProviderSpec};

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

#[cfg(feature = "uniffi")]
pub mod bindings;
