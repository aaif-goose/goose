//! Unified execution management for Goose agents
//!
//! This module provides centralized agent lifecycle management with session isolation,
//! enabling multiple concurrent sessions with independent agents, extensions, and providers.

mod active_run;
pub mod manager;

pub use active_run::ActiveRunRegistry;
pub(crate) use active_run::StartRunError;
