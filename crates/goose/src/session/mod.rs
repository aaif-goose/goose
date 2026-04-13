mod chat_history_search;
mod diagnostics;
pub mod extension_data;
mod legacy;
pub mod session_manager;
pub mod thread_manager;

use chrono::{DateTime, NaiveDateTime, Utc};

pub use diagnostics::{
    config_path, generate_diagnostics, get_system_info, latest_llm_log_path,
    latest_server_log_path, read_capped, read_tail, SystemInfo,
};
pub use extension_data::{EnabledExtensionsState, ExtensionData, ExtensionState, TodoState};
pub use session_manager::{
    Session, SessionInsights, SessionManager, SessionType, SessionUpdateBuilder,
};
pub use thread_manager::{Thread, ThreadManager, ThreadMetadata};

/// Parse a SQLite `CURRENT_TIMESTAMP` value (`YYYY-MM-DD HH:MM:SS`, assumed UTC).
fn parse_sql_timestamp(s: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|naive| naive.and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sql_timestamp_valid() {
        let dt = parse_sql_timestamp("2026-04-13 01:38:51").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-04-13T01:38:51+00:00");
    }

    #[test]
    fn test_parse_sql_timestamp_invalid_returns_none() {
        assert!(parse_sql_timestamp("not-a-date").is_none());
        assert!(parse_sql_timestamp("2026-04-13T01:38:51Z").is_none());
    }
}
