#[cfg(not(any(feature = "rustls-tls", feature = "native-tls")))]
compile_error!("At least one of `rustls-tls` or `native-tls` features must be enabled");

#[cfg(all(feature = "rustls-tls", feature = "native-tls"))]
compile_error!("Features `rustls-tls` and `native-tls` are mutually exclusive");

pub mod acp;
pub use goose_sdk::custom_requests;
pub mod action_required_manager;
pub mod agents;
pub mod builtin_extension;
pub mod config;
pub mod context_mgmt;
pub mod conversation;
pub mod dictation;
pub mod doctor;
pub mod download_manager;
pub mod execution;
pub mod gateway;
pub mod goose_apps;
pub mod hints;
pub mod instance_id;
pub mod logging;
pub mod mcp_utils;
pub mod model;
pub mod oauth;
#[cfg(feature = "otel")]
pub mod otel;
pub mod permission;
#[cfg(feature = "telemetry")]
pub mod posthog;
pub mod prompt_template;
pub mod providers;
pub mod recipe;
pub mod recipe_deeplink;
pub mod scheduler;
pub mod scheduler_trait;
pub mod security;
pub mod session;
pub mod session_context;
pub mod slash_commands;
pub mod sources;
pub mod subprocess;
pub mod token_counter;
pub mod tool_inspection;
pub mod tool_monitor;
pub mod tracing;
pub mod utils;
