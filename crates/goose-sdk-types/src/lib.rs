//! Shared types for the Goose SDK.

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

pub mod custom_requests;

pub use custom_requests::{AgentEvent, ExtensionSpec, ProviderSpec};
