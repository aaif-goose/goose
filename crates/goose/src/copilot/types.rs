use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::CopilotPrefs;

#[derive(Debug, Deserialize, Clone, ToSchema)]
pub struct CopilotReviewRequest {
    pub github_token: String,
    pub repo: String,
    pub pr_number: u64,
    pub head_sha: String,
    pub pr_url: String,
    #[serde(default)]
    pub check_run_id: Option<u64>,
    #[serde(default)]
    pub comment_id: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CopilotReviewResponse {
    pub accepted: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CopilotSetupResponse {
    pub installation_id: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CopilotStatusResponse {
    pub installation_id: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CopilotDisconnectResponse {
    pub disconnected: bool,
}

#[derive(Debug, Deserialize, Clone, ToSchema)]
pub struct CopilotCommentRequest {
    pub github_token: String,
    pub repo: String,
    pub pr_number: u64,
    pub pr_url: String,
    pub comment_body: String,
    pub commenter: String,
    #[serde(default)]
    pub head_ref: String,
    #[serde(default)]
    pub comment_id: Option<u64>,
    /// Omitted in older switchboard payloads (treated as PR); current switchboard always sends this.
    #[serde(default = "is_pr_absent_legacy_default")]
    pub is_pr: bool,
}

fn is_pr_absent_legacy_default() -> bool {
    true
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CopilotPrefsResponse {
    pub prefs: CopilotPrefs,
    pub switchboard_synced: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switchboard_error: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CopilotPrefsRequest {
    pub prefs: CopilotPrefs,
}
