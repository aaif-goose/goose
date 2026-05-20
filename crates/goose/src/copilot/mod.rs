//! Copilot preferences schema for goosed, Desktop (OpenAPI), and the switchboard routing cache.

pub mod analytics;
pub mod comment;
pub mod github;
pub mod prefs;
pub mod repos;
pub mod review;
pub mod review_run;
pub mod runner;
pub mod store;
pub mod switchboard;
pub mod types;

pub use analytics::{AnalyticsEvent, CopilotAnalytics};
pub use comment::run_comment_reply;
pub use github::replace_comment_reaction;
pub use prefs::{
    CopilotPrefs, ReviewModelChoice, ReviewOutputStyle, ReviewSeverity, RoutingPrefs,
    TriggerPermission, TriggerPreference, SCHEMA_VERSION,
};
pub use repos::{CopilotRepo, CopilotReposResponse, RepoVisibility};
pub use review::{
    build_goose_review_args, build_review_payload, extract_final_assistant_text, parse_findings,
    Finding, ReviewPublishContext,
};
pub use review_run::run_review;
pub use store::{
    cached_installation_id, clear_install, load_prefs, load_prefs_from, save_prefs, save_prefs_to,
    PREFS_CONFIG_KEY,
};
pub use switchboard::{
    disconnect_install, extract_agent_id, fetch_analytics, fetch_oauth_client_id, fetch_repos,
    forward_routing_prefs, register_installation, report_analytics_event,
    resolve_install_credentials, unregister_installation, RegisterInstallRequest, TunnelSnapshot,
    INSTALLATION_ID_CONFIG_KEY, SWITCHBOARD_URL_ENV,
};
pub use types::{
    CopilotCommentRequest, CopilotDisconnectResponse, CopilotPrefsRequest, CopilotPrefsResponse,
    CopilotReviewRequest, CopilotReviewResponse, CopilotSetupResponse, CopilotStatusResponse,
};
