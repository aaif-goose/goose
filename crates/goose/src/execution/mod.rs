//! Unified execution management for Goose agents
//!
//! This module provides centralized agent lifecycle management with session isolation,
//! enabling multiple concurrent sessions with independent agents, extensions, and providers.

mod active_run;
pub mod manager;

pub use active_run::ActiveRunRegistry;
pub(crate) use active_run::StartRunError;

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionExecutionMode {
    Interactive,
    Background,
    SubTask { parent_session: String },
}

impl SessionExecutionMode {
    /// Create an interactive chat mode
    pub fn chat() -> Self {
        Self::Interactive
    }

    /// Create a background/scheduled mode
    pub fn scheduled() -> Self {
        Self::Background
    }

    /// Create a sub-task mode with parent reference
    pub fn task(parent: String) -> Self {
        Self::SubTask {
            parent_session: parent,
        }
    }
}

impl fmt::Display for SessionExecutionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Interactive => write!(f, "interactive"),
            Self::Background => write!(f, "background"),
            Self::SubTask { parent_session } => write!(f, "subtask(parent: {})", parent_session),
        }
    }
}
