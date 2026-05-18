//! Goose Copilot preferences schema and helpers.
//!
//! Single source of truth for everything the user can configure about the
//! PR-review bot. Defined here so:
//!   - goosed (`/copilot/prefs`) serialises and validates them
//!   - Desktop receives generated TypeScript types via OpenAPI
//!   - the switchboard's TypeScript schema can be kept in lockstep by hand
//!
//! The schema is versioned: bump `SCHEMA_VERSION` whenever the shape changes,
//! and add a serde-compatible default for any new field so older payloads
//! still deserialize.

pub mod prefs;
pub mod repos;

pub use prefs::{
    CopilotPrefs, ReviewModelChoice, ReviewOutputStyle, RoutingPrefs, TriggerPermission,
    TriggerPreference, SCHEMA_VERSION,
};
pub use repos::{CopilotRepo, CopilotReposResponse, RepoVisibility};
