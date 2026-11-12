//! Goose adapters for the GDK extension operation.

pub(super) use super::extension_operation_host::{
    call_will_not_run, pending_advertised_tool_requests, pending_tool_requests,
    request_was_advertised, ToolDisposition, EXPIRED_APPROVAL_RESPONSE,
};
pub(super) use super::tool_hook_context::{leased_working_dir, tool_span};
