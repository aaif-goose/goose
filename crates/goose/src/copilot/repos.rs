//! Repo-list payload returned by `GET /copilot/repos`. The data is transient
//! (we never persist it locally) so this lives separately from `prefs.rs`,
//! but uses the same crate so Desktop's generated TS types stay coherent.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RepoVisibility {
    Public,
    Private,
    Internal,
    /// Falls through unknown values from GitHub instead of failing the call.
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct CopilotRepo {
    pub id: u64,
    pub full_name: String,
    pub name: String,
    pub owner: String,
    #[serde(default)]
    pub visibility: RepoVisibility,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub default_branch: String,
    #[serde(default)]
    pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct CopilotReposResponse {
    /// Total repos accessible to the installation. May be larger than
    /// `repos.len()` if pagination cut the list off.
    pub total_count: u64,
    pub repos: Vec<CopilotRepo>,
    /// `true` when GitHub had more pages than we fetched. Desktop can show a
    /// "showing first N" notice.
    #[serde(default)]
    pub truncated: bool,
}
