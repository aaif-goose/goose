use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct CopilotAnalytics {
    #[serde(default)]
    pub prs_reviewed: u64,
    #[serde(default)]
    pub issues_handled: u64,
    #[serde(default)]
    pub commits_pushed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AnalyticsEvent {
    PrReviewed,
    IssueHandled,
    CommitPushed,
}
