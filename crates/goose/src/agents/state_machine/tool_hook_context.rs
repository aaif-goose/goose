use crate::agents::tool_snapshot::ToolSnapshot;
use crate::session::Session;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

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

pub(super) fn advertised_working_dir(
    tool_snapshot: &StdMutex<Option<Arc<ToolSnapshot>>>,
    session: &Session,
) -> PathBuf {
    tool_snapshot
        .lock()
        .expect("tool dispatcher unavailable")
        .as_ref()
        .filter(|tools| tools.scope_id() == session.id)
        .and_then(|tools| tools.working_dir())
        .map_or_else(|| session.working_dir.clone(), Path::to_path_buf)
}
