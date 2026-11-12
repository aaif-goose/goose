//! Python and Kotlin bindings for the Open Intelligence Kit (oi-kit).
//!
//! With `--features uniffi` the crate compiles as a `cdylib`/`staticlib` and
//! exposes an in-process API to Python and Kotlin via
//! [uniffi-rs](https://github.com/mozilla/uniffi-rs). The current uniffi surface
//! lets callers construct declarative providers from JSON and stream provider
//! completions.

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!("oi_kit");

#[cfg(feature = "uniffi")]
pub mod bindings;

#[cfg(feature = "uniffi")]
pub mod observability;
