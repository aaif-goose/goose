//! Python and Kotlin bindings for the Open Intelligence Kit (oi-kit).
//!
//! With default features this crate re-exports the shared oi-kit wire types from
//! `goose-acp-types` so you can build an Agent Client Protocol (ACP) client
//! that talks to `goose acp` over stdio.
//!
//! With `--features uniffi` the crate additionally compiles as a
//! `cdylib`/`staticlib` and exposes an in-process API to Python and Kotlin via
//! [uniffi-rs](https://github.com/mozilla/uniffi-rs). The current uniffi surface
//! lets callers construct declarative providers from JSON and stream provider
//! completions.

pub use goose_acp_types::{custom_notifications, custom_requests};

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!("oi_kit");

#[cfg(feature = "uniffi")]
pub mod bindings;

#[cfg(feature = "uniffi")]
pub mod observability;
