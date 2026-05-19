use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Per-install rollups the switchboard stores in KV and Desktop renders in
/// the Analytics tab. Counts only — no per-event timestamps, no PII.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct CopilotAnalytics {
    /// Pull requests goose-copilot has reviewed.
    #[serde(default)]
    pub prs_reviewed: u64,
    /// GitHub issues goose-copilot has responded to.
    #[serde(default)]
    pub issues_handled: u64,
    /// Commits goose-copilot has pushed (existing branch or fresh PR).
    #[serde(default)]
    pub commits_pushed: u64,
}

/// One event reported by goosed when a job completes. Switchboard increments
/// the matching counter in KV.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AnalyticsEvent {
    PrReviewed,
    IssueHandled,
    CommitPushed,
}
