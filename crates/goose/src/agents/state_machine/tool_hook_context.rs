use crate::session::Session;
use goose_agent::extension_manager::TurnLease;
use std::path::{Path, PathBuf};

pub(super) fn tool_span(tool_name: &str, tool_call_id: &str, session_id: &str) -> tracing::Span {
    tracing::info_span!(
        target: "goose::state_machine",
        "execute_tool",
        "gen_ai.operation.name" = "execute_tool",
        "gen_ai.tool.name" = %tool_name,
        "gen_ai.tool.call.id" = %tool_call_id,
        "gen_ai.conversation.id" = %session_id,
        "gen_ai.tool.call.arguments" = tracing::field::Empty,
        "gen_ai.tool.call.result" = tracing::field::Empty,
        "error.type" = tracing::field::Empty,
        session.id = %session_id,
    )
}

pub(super) fn leased_working_dir(extension_lease: &TurnLease, session: &Session) -> PathBuf {
    extension_lease
        .for_scope(&session.id)
        .and_then(|lease| lease.working_dir().map(Path::to_path_buf))
        .unwrap_or_else(|| session.working_dir.clone())
}
