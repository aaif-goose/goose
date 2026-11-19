//! Exposes extension capabilities and executes requests that belong to them.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use async_trait::async_trait;
use futures::FutureExt;
use oi_kit_agent::tool_call_state::{open_tool_calls, ToolCallState};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, Role, Tool, ToolAnnotations,
};

use crate::agents::extension::{ExtensionConfig, ExtensionInfo};
use crate::agents::extension_manager::McpLease;
use crate::agents::extension_runtime::{ExtensionSource, ToolCallBatch};
use crate::agents::reply_parts::is_tool_visible_to_model;
use crate::agents::state_machine::{
    applied, messages_since_kickoff, not_applicable, Emitter, GooseEffect, Operation,
    OperationResult,
};
use crate::agents::tool_execution::ToolCallResult;
use crate::agents::tool_snapshot::ToolSnapshot;
use crate::agents::AgentEvent;
use crate::agents::StateMachineServices;
use crate::conversation::message::{ActionRequiredData, Message, MessageContent, ToolRequest};
use crate::conversation::Conversation;
use crate::session::Session;

pub(super) const EXPIRED_APPROVAL_RESPONSE: &str =
    "Tool approval expired because the advertised tools are no longer available. Request the tool again.";
use std::sync::{Arc, Mutex as StdMutex};
use tracing_futures::Instrument;

const ENABLE_TOOLS: &str = "enable_tools";

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct EnableToolsParams {
    /// Exact MCP tool names to make available on the next response.
    tool_names: Vec<String>,
}

fn enable_tools_tool() -> Result<Tool> {
    let schema: rmcp::model::JsonObject = serde_json::from_value(serde_json::to_value(
        schemars::schema_for!(EnableToolsParams),
    )?)?;
    Ok(Tool::new(
        ENABLE_TOOLS,
        "Enable MCP tools listed in the system instructions. Call them in your next response.",
        schema,
    )
    .annotate(ToolAnnotations::new().read_only(true)))
}

fn enabled_tools(conversation: &Conversation) -> HashSet<String> {
    let mut requests = HashSet::new();
    let mut enabled = HashSet::new();
    for content in conversation
        .messages()
        .iter()
        .flat_map(|message| &message.content)
    {
        match content {
            MessageContent::ToolRequest(request)
                if request
                    .tool_call
                    .as_ref()
                    .is_ok_and(|call| call.name == ENABLE_TOOLS) =>
            {
                requests.insert(&request.id);
            }
            MessageContent::ToolResponse(response) if requests.remove(&response.id) => {
                let Ok(result) = &response.tool_result else {
                    continue;
                };
                if result.is_error == Some(true) {
                    continue;
                }
                let Some(names) = result
                    .structured_content
                    .as_ref()
                    .and_then(|value| value["enabled"].as_array())
                else {
                    continue;
                };
                enabled.extend(
                    names
                        .iter()
                        .filter_map(|name| name.as_str().map(str::to_string)),
                );
            }
            _ => {}
        }
    }
    enabled
}

fn platform_notification(result: &CallToolResult) -> Option<rmcp::model::ServerNotification> {
    let notification = result.meta.as_ref()?.0.get("platform_notification")?;
    let method = notification.get("method")?.as_str()?;
    Some(rmcp::model::ServerNotification::CustomNotification(
        rmcp::model::CustomNotification::new(
            method.to_string(),
            notification.get("params").cloned(),
        ),
    ))
}

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

fn with_telemetry(result: ToolCallResult, span: tracing::Span) -> ToolCallResult {
    let future = async move {
        let result =
            crate::agents::large_response_handler::process_tool_response(result.result.await);
        crate::agents::gen_ai_telemetry::record_tool_result(&tracing::Span::current(), &result);
        match &result {
            Ok(result) if result.is_error == Some(true) => {
                tracing::Span::current().record("error.type", "tool_error");
            }
            Err(_) => {
                tracing::Span::current().record("error.type", "tool_execution_error");
            }
            _ => {}
        }
        result
    }
    .instrument(span);
    ToolCallResult {
        notification_stream: result.notification_stream,
        action_required_stream: result.action_required_stream,
        result: Box::new(future.boxed()),
    }
}

pub(super) fn advertised_working_dir(
    tool_snapshot: &StdMutex<Option<Arc<ToolSnapshot>>>,
    session: &Session,
) -> PathBuf {
    tool_snapshot
        .lock()
        .expect("tool snapshot unavailable")
        .as_ref()
        .filter(|tools| tools.scope_id() == session.id)
        .and_then(|tools| tools.working_dir())
        .map_or_else(|| session.working_dir.clone(), Path::to_path_buf)
}

/// Goose's view of the session's MCP extensions: the lease shared through the
/// [`ToolSnapshot`], minus tools a platform extension already provides and
/// deferred tools the model has not enabled yet.
pub struct GooseExtensions {
    services: Arc<StateMachineServices>,
    tools: Arc<StdMutex<Option<Arc<ToolSnapshot>>>>,
    deferral: Arc<ToolDeferral>,
}

impl GooseExtensions {
    pub fn new(
        services: Arc<StateMachineServices>,
        tools: Arc<StdMutex<Option<Arc<ToolSnapshot>>>>,
        deferral: Arc<ToolDeferral>,
    ) -> Self {
        Self {
            services,
            tools,
            deferral,
        }
    }

    async fn snapshot(&self, session: &Session) -> Result<Arc<ToolSnapshot>> {
        ToolSnapshot::for_session(&self.tools, &self.services, &session.id).await
    }
}

#[async_trait]
impl ExtensionSource<Session, ExtensionConfig> for GooseExtensions {
    async fn advertise(&self, session: &Session) -> Result<McpLease> {
        Ok(self.snapshot(session).await?.mcp.clone())
    }

    async fn advertised(
        &self,
        session: &Session,
        conversation: &Conversation,
    ) -> Result<Option<McpLease>> {
        let tools = self
            .tools
            .lock()
            .expect("tool snapshot unavailable")
            .clone();
        if let Some(tools) = tools {
            return Ok(Some(tools.mcp.clone()));
        }
        let messages = messages_since_kickoff(conversation)?;
        let pending = pending_tool_requests(messages);
        if !requests_can_resolve_tools(messages, executed(&pending)) {
            return Ok(None);
        }
        self.advertise(session).await.map(Some)
    }

    async fn tools(&self, session: &Session, lease: &McpLease) -> Result<Vec<Tool>> {
        let snapshot = self.snapshot(session).await?;
        let platform_names: HashSet<_> = snapshot
            .platform_tools()
            .await
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        let mut tools: Vec<_> = lease
            .tools()
            .await
            .into_iter()
            .filter(|tool| !platform_names.contains(tool.name.as_ref()))
            .collect();
        if self.deferral.applies(&snapshot) {
            let enabled = session
                .conversation
                .as_ref()
                .map(enabled_tools)
                .unwrap_or_default();
            tools
                .retain(|tool| !self.deferral.defers(tool) || enabled.contains(tool.name.as_ref()));
        }
        Ok(tools)
    }

    fn decorate(
        &self,
        session: &Session,
        request: &ToolRequest,
        result: ToolCallResult,
    ) -> ToolCallResult {
        traced(session, request, result)
    }

    async fn prompt_parts(
        &self,
        session: &Session,
        lease: &McpLease,
    ) -> Result<Vec<(String, String)>> {
        let snapshot = self.snapshot(session).await?;
        #[cfg(feature = "code-mode")]
        if snapshot.is_enabled(crate::agents::platform_extensions::code_execution::EXTENSION_NAME) {
            return Ok(Vec::new());
        }
        let mut extensions = lease.instructions().await;
        if extensions.is_empty() {
            return Ok(Vec::new());
        }
        // HashMap order shuffles across restarts and would bust the prompt cache.
        extensions.sort_by(|a, b| a.name.cmp(&b.name));
        let defers = self.deferral.applies(&snapshot);
        let mut parts = extension_prompt_parts(&extensions, defers);
        if defers {
            let mut names: Vec<_> = self
                .deferral
                .deferred(lease)
                .await
                .into_iter()
                .map(|tool| tool.name.to_string())
                .collect();
            names.sort();
            parts.push(("mcp_tools".to_string(), format!(
                "MCP tools available to enable: {}. Call enable_tools with exact tool_names before using them. \
                 Their schemas become available on your next response. Enabling does not execute a tool.",
                names.join(", ")
            )));
        }
        Ok(parts)
    }
}

fn executed(pending: &[(ToolRequest, ToolDisposition)]) -> impl Iterator<Item = &ToolRequest> {
    pending
        .iter()
        .filter(|(_, disposition)| *disposition == ToolDisposition::Execute)
        .map(|(request, _)| request)
}

pub(super) fn traced(
    session: &Session,
    request: &ToolRequest,
    result: ToolCallResult,
) -> ToolCallResult {
    let tool_call = request
        .tool_call
        .as_ref()
        .expect("selected calls are parsed");
    let span = tool_span(&tool_call.name, &request.id, &session.id);
    crate::agents::gen_ai_telemetry::record_tool_arguments(&span, tool_call);
    with_telemetry(result, span)
}

/// Whether MCP tools are listed in the prompt instead of advertised, until
/// the model enables them with `enable_tools`.
pub struct ToolDeferral {
    enabled: bool,
    always_load_tools: HashSet<String>,
}

impl ToolDeferral {
    pub fn new(enabled: bool, always_load_tools: Vec<String>) -> Self {
        Self {
            enabled,
            always_load_tools: always_load_tools.into_iter().collect(),
        }
    }

    fn applies(&self, tools: &ToolSnapshot) -> bool {
        #[cfg(feature = "code-mode")]
        if tools.is_enabled(crate::agents::platform_extensions::code_execution::EXTENSION_NAME) {
            return false;
        }
        let _ = tools;
        self.enabled
    }

    fn defers(&self, tool: &Tool) -> bool {
        is_tool_visible_to_model(tool) && !self.always_load_tools.contains(tool.name.as_ref())
    }

    async fn deferred(&self, lease: &McpLease) -> Vec<Tool> {
        lease
            .tools()
            .await
            .into_iter()
            .filter(|tool| self.defers(tool))
            .collect()
    }
}

/// Advertises and answers `enable_tools` while [`ToolDeferral`] applies.
pub struct ToolDeferralOperation {
    services: Arc<StateMachineServices>,
    tools: Arc<StdMutex<Option<Arc<ToolSnapshot>>>>,
    deferral: Arc<ToolDeferral>,
}

impl ToolDeferralOperation {
    pub fn new(
        services: Arc<StateMachineServices>,
        tools: Arc<StdMutex<Option<Arc<ToolSnapshot>>>>,
        deferral: Arc<ToolDeferral>,
    ) -> Self {
        Self {
            services,
            tools,
            deferral,
        }
    }

    async fn enable_tools(&self, lease: &McpLease, call: &CallToolRequestParams) -> CallToolResult {
        let params = serde_json::from_value::<EnableToolsParams>(serde_json::Value::Object(
            call.arguments.clone().unwrap_or_default(),
        ));
        let params = match params {
            Ok(params) => params,
            Err(error) => {
                return CallToolResult::error(vec![ContentBlock::text(error.to_string())])
            }
        };
        let tools: Vec<_> = lease
            .tools()
            .await
            .into_iter()
            .filter(is_tool_visible_to_model)
            .collect();
        if let Some(name) = params
            .tool_names
            .iter()
            .find(|name| !tools.iter().any(|tool| tool.name == name.as_str()))
        {
            return CallToolResult::error(vec![ContentBlock::text(format!(
                "Unknown MCP tool: {name}"
            ))]);
        }
        CallToolResult::structured(serde_json::json!({"enabled": params.tool_names}))
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for ToolDeferralOperation {
    fn name(&self) -> &'static str {
        "tool_deferral"
    }

    async fn inference_tools(&self, session: &Session) -> Result<Vec<Tool>> {
        let snapshot = ToolSnapshot::for_session(&self.tools, &self.services, &session.id).await?;
        Ok(if self.deferral.applies(&snapshot) {
            vec![enable_tools_tool()?]
        } else {
            Vec::new()
        })
    }

    async fn run(
        &self,
        _session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        let requests: Vec<_> = pending_tool_requests(messages)
            .into_iter()
            .filter(|(request, disposition)| {
                *disposition == ToolDisposition::Execute
                    && request
                        .tool_call
                        .as_ref()
                        .is_ok_and(|call| call.name == ENABLE_TOOLS)
            })
            .collect();
        if requests.is_empty() {
            return not_applicable();
        }
        let tools = self
            .tools
            .lock()
            .expect("tool snapshot unavailable")
            .clone();
        let Some(tools) = tools.filter(|tools| self.deferral.applies(tools)) else {
            return not_applicable();
        };
        let mut response = Message::user();
        for (request, _) in requests {
            let call = request
                .tool_call
                .as_ref()
                .expect("selected calls are parsed");
            let result = self.enable_tools(&tools.mcp, call).await;
            response.add_tool_response_with_metadata(
                request.id,
                Ok(result),
                request.metadata.as_ref(),
            );
        }
        let response = emit.message(response);
        applied([response.into()])
    }
}

/// Answers calls whose advertised tools are gone, rather than running them
/// against tools the user never approved them for.
pub struct ExpiredToolsOperation {
    tools: Arc<StdMutex<Option<Arc<ToolSnapshot>>>>,
}

impl ExpiredToolsOperation {
    pub fn new(tools: Arc<StdMutex<Option<Arc<ToolSnapshot>>>>) -> Self {
        Self { tools }
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for ExpiredToolsOperation {
    fn name(&self) -> &'static str {
        "expired_tools"
    }

    async fn run(
        &self,
        _session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        let pending = pending_tool_requests(messages);
        if pending.is_empty()
            || self
                .tools
                .lock()
                .expect("tool snapshot unavailable")
                .is_some()
            || requests_can_resolve_tools(messages, executed(&pending))
        {
            return not_applicable();
        }
        let mut response = Message::user();
        for (request, disposition) in pending {
            let result = match disposition {
                ToolDisposition::Execute => {
                    CallToolResult::error(vec![ContentBlock::text(EXPIRED_APPROVAL_RESPONSE)])
                }
                ToolDisposition::ParseError(parse_error) => {
                    CallToolResult::error(vec![ContentBlock::text(format!(
                        "The tool call could not be parsed: {parse_error}. Correct the arguments and try again."
                    ))])
                }
            };
            response.add_tool_response_with_metadata(
                request.id,
                Ok(result),
                request.metadata.as_ref(),
            );
        }
        let response = emit.message(response);
        applied([response.into()])
    }
}

pub(super) fn pending_tool_requests(messages: &[Message]) -> Vec<(ToolRequest, ToolDisposition)> {
    open_tool_calls(messages)
        .filter_map(|(request, state)| match (&request.tool_call, state) {
            (Err(parse_error), _) => Some((
                request.clone(),
                ToolDisposition::ParseError(parse_error.to_string()),
            )),
            (Ok(_), ToolCallState::Executable) => Some((request.clone(), ToolDisposition::Execute)),
            _ => None,
        })
        .collect()
}

pub(super) fn requests_can_resolve_tools<'a>(
    messages: &[Message],
    requests: impl IntoIterator<Item = &'a ToolRequest>,
) -> bool {
    let mut requests = requests.into_iter().peekable();
    requests.peek().is_some()
        && requests.all(|request| {
            request_was_generated_by_operation(messages, request)
                && !request_has_approval_history(messages, request)
        })
}

fn request_was_generated_by_operation(messages: &[Message], request: &ToolRequest) -> bool {
    messages.iter().any(|message| {
        message.role == Role::Assistant
            && message.metadata.inference.is_none()
            && message.content.iter().any(|content| {
                content
                    .as_tool_request()
                    .is_some_and(|item| item.id == request.id)
            })
    })
}

fn request_has_approval_history(messages: &[Message], request: &ToolRequest) -> bool {
    messages.iter().any(|message| {
        message.content.iter().any(|content| match content {
            MessageContent::ActionRequired(action) => matches!(
                &action.data,
                ActionRequiredData::ToolConfirmation { id, .. }
                    | ActionRequiredData::ToolConfirmationResponse { id, .. }
                    if id == &request.id
            ),
            _ => false,
        })
    })
}

#[derive(Clone, Eq, PartialEq)]
pub(super) enum ToolDisposition {
    Execute,
    ParseError(String),
}

/// The extension block, as a header part plus one part per extension. Prompt
/// extras are joined with a blank line, so this renders the same text as one
/// combined part while keeping each extension's tokens attributable.
pub(super) fn extension_prompt_parts(
    extensions: &[ExtensionInfo],
    defer_tools: bool,
) -> Vec<(String, String)> {
    let mut parts = vec![(
        "extensions".to_string(),
        "# Extensions\n\n\
         Extensions provide additional tools and context from different data sources and applications.\n\
         You can dynamically enable or disable extensions as needed to help complete tasks.\n\n\
         Because you dynamically load extensions, your conversation history may refer to interactions with extensions that are not currently active. The currently active extensions are below."
            .to_string(),
    )];
    if !defer_tools {
        parts[0].1.push_str(
            " Each of these extensions provides tools that are in your tool specification.",
        );
    }
    for extension in extensions {
        let mut lines = vec![format!("## {}", extension.name)];
        if extension.has_resources {
            lines.push(format!("{} supports resources.", extension.name));
        }
        if !extension.instructions.is_empty() {
            lines.push(format!("### Instructions\n{}", extension.instructions));
        }
        parts.push((format!("extension:{}", extension.name), lines.join("\n\n")));
    }
    parts
}

pub(super) struct ToolExecution {
    batch: ToolCallBatch,
}

impl ToolExecution {
    pub(super) fn new() -> Self {
        Self {
            batch: ToolCallBatch::new().with_result_notification(platform_notification),
        }
    }

    pub(super) fn finalize_cancellation(&self, emit: &Emitter) -> Vec<GooseEffect> {
        let (actions, response) = self.batch.take();
        if let Some(response) = &response {
            emit.emit(AgentEvent::Message(response.user_visible_content()));
        }
        actions
            .into_iter()
            .chain(response)
            .map(GooseEffect::from)
            .collect()
    }

    pub(super) async fn run(
        &self,
        calls: Vec<(ToolRequest, ToolCallResult)>,
        session: &Session,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let calls = calls
            .into_iter()
            .map(|(request, result)| {
                let result = traced(session, &request, result);
                (request, result)
            })
            .collect();
        let (actions, response) = self.batch.run(calls, emit).await?;
        emit.emit(AgentEvent::Message(response.user_visible_content()));
        applied(actions.into_iter().chain([response]).map(GooseEffect::from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oi_kit_agent::tool_call_state::EXECUTABLE_KEY;

    #[test]
    fn extension_prompt_parts_render_one_block_per_extension() {
        let extensions = vec![
            ExtensionInfo::new("developer", "Shell things.", false),
            ExtensionInfo::new("memory", "", true),
        ];

        let parts = extension_prompt_parts(&extensions, false);

        let keys: Vec<&str> = parts.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(
            keys,
            ["extensions", "extension:developer", "extension:memory"]
        );
        let rendered = parts
            .iter()
            .map(|(_, value)| value.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        assert!(rendered.ends_with(
            "## developer\n\n### Instructions\nShell things.\n\n## memory\n\nmemory supports resources."
        ));
    }

    #[test]
    fn externally_dispatched_observations_are_not_pending_execution() {
        use crate::conversation::message::TOOL_META_EXTERNAL_DISPATCH_KEY;

        let message = Message::assistant().with_tool_request_with_metadata(
            "external",
            Ok(CallToolRequestParams::new("registered__tool")),
            None,
            Some(serde_json::json!({ TOOL_META_EXTERNAL_DISPATCH_KEY: true })),
        );

        assert!(pending_tool_requests(&[message]).is_empty());
    }

    #[test]
    fn executable_requests_are_pending() {
        let message = Message::assistant().with_tool_request_with_metadata(
            "operation-generated",
            Ok(CallToolRequestParams::new("registered__tool")),
            None,
            Some(serde_json::json!({ EXECUTABLE_KEY: true })),
        );

        let pending = pending_tool_requests(&[message]);

        assert_eq!(pending.len(), 1);
        assert!(matches!(pending[0].1, ToolDisposition::Execute));
    }

    #[test]
    fn reads_platform_notification_from_tool_result() {
        let meta = serde_json::json!({
            "platform_notification": {
                "method": "platform_event",
                "params": { "event_type": "app_updated" }
            }
        });
        let result = CallToolResult::success(Vec::new()).with_meta(Some(rmcp::model::MetaObject(
            meta.as_object().unwrap().clone(),
        )));

        let Some(rmcp::model::ServerNotification::CustomNotification(notification)) =
            platform_notification(&result)
        else {
            panic!("expected a custom notification");
        };
        assert_eq!(notification.method, "platform_event");
        assert_eq!(
            notification.params,
            Some(serde_json::json!({ "event_type": "app_updated" }))
        );
    }
}
