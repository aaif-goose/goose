//! Goose advertisement, result processing and execution policy for the GDK runtime.

use std::collections::HashSet;

use anyhow::Result;
use async_trait::async_trait;
use futures::FutureExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, Tool, ToolAnnotations};

use super::tool_hook_context::tool_span;
use crate::agents::extension_config::ExtensionInfo;
use crate::agents::extension_manager::ExtensionManager;
use crate::agents::state_machine::ops_llm::{ADVERTISED_TOOLS_NOTE, LLM_OPERATION_NAME};
use crate::agents::state_machine::tool_hooks::{check_pre_tool_hooks, emit_post_tool_hooks};
use crate::agents::state_machine::{messages_since_kickoff, Emitter};
use crate::agents::tool_execution::CHAT_MODE_TOOL_SKIPPED_RESPONSE;
use crate::agents::AgentEvent;
use crate::config::GooseMode;
use crate::conversation::message::{Message, MessageContent, ToolRequest, ToolResponse};
use crate::conversation::Conversation;
use crate::hooks::HookManager;
use crate::session::Session;
use goose_agent::extension_manager::{requests_can_resolve_a_lease, ExtensionLease};
use goose_agent::tool_execution::ToolCallResult;
use goose_agent::tool_visibility::is_tool_visible_to_model;

pub(super) const EXPIRED_APPROVAL_RESPONSE: &str =
    "Tool approval expired because its extension lease is no longer available. Request the tool again.";
use std::sync::Arc;
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

fn session_in_lease(lease: &ExtensionLease, session: &Session) -> Session {
    let mut session = session.clone();
    if let Some(working_dir) = lease.working_dir() {
        session.working_dir = working_dir.to_path_buf();
    }
    session
}

pub struct GooseExtensionOperationHost {
    extension_manager: Arc<ExtensionManager>,
    hook_manager: HookManager,
    defer_tools: bool,
    always_load_tools: HashSet<String>,
}

impl GooseExtensionOperationHost {
    pub fn new(
        extension_manager: Arc<ExtensionManager>,
        hook_manager: HookManager,
        defer_tools: bool,
        always_load_tools: Vec<String>,
    ) -> Self {
        Self {
            extension_manager,
            hook_manager,
            defer_tools,
            always_load_tools: always_load_tools.into_iter().collect(),
        }
    }

    fn defers_tools(&self, lease: &ExtensionLease) -> bool {
        #[cfg(feature = "code-mode")]
        if lease.is_enabled(crate::agents::platform_extensions::code_execution::EXTENSION_NAME) {
            return false;
        }
        let _ = lease;
        self.defer_tools
    }

    async fn mcp_tools(&self, lease: &ExtensionLease) -> Result<Vec<Tool>> {
        Ok(lease
            .untrusted_tools()
            .await
            .into_iter()
            .filter(is_tool_visible_to_model)
            .collect())
    }

    async fn enable_tools(
        &self,
        lease: &ExtensionLease,
        call: &CallToolRequestParams,
    ) -> CallToolResult {
        let params = serde_json::from_value::<EnableToolsParams>(serde_json::Value::Object(
            call.arguments.clone().unwrap_or_default(),
        ));
        let params = match params {
            Ok(params) => params,
            Err(error) => {
                return CallToolResult::error(vec![ContentBlock::text(error.to_string())])
            }
        };
        let tools = match self.mcp_tools(lease).await {
            Ok(tools) => tools,
            Err(error) => {
                return CallToolResult::error(vec![ContentBlock::text(error.to_string())])
            }
        };
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

pub(super) use goose_agent::extension_manager::{pending_tool_requests, ToolDisposition};

pub(super) fn pending_advertised_tool_requests(
    messages: &[Message],
) -> Vec<(ToolRequest, ToolDisposition)> {
    pending_tool_requests(messages)
        .into_iter()
        .filter(|(request, _)| request_was_advertised(messages, request))
        .collect()
}

pub(super) fn request_was_advertised(messages: &[Message], request: &ToolRequest) -> bool {
    goose_agent::extension_manager::request_was_advertised(
        messages,
        request,
        LLM_OPERATION_NAME,
        ADVERTISED_TOOLS_NOTE,
    )
}

pub(super) fn call_will_not_run(
    session: &Session,
    conversation: &Conversation,
    request: &ToolRequest,
) -> Result<bool> {
    Ok(session.goose_mode == GooseMode::Chat
        || !request_was_advertised(messages_since_kickoff(conversation)?, request))
}

/// The extension block, as a header part plus one part per extension. Prompt
/// extras are joined with a blank line, so this renders the same text as one
/// combined part while keeping each extension's tokens attributable.
fn extension_prompt_parts(
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

#[async_trait]
impl goose_agent::extension_manager::LeaseResolver<Session> for GooseExtensionOperationHost {
    async fn resolve(&self, session: &Session) -> Result<ExtensionLease> {
        self.extension_manager.current_lease(&session.id).await
    }
}

#[async_trait]
impl goose_agent::extension_manager::ExtensionOperationHost<Session>
    for GooseExtensionOperationHost
{
    fn name(&self) -> &'static str {
        "tool_execution"
    }
    async fn tools(&self, session: &Session, lease: &ExtensionLease) -> Result<Vec<Tool>> {
        let mut tools = lease.tools_excluding(crate::skills::EXTENSION_NAME).await;
        if self.defers_tools(lease) {
            let deferred: HashSet<_> = self
                .mcp_tools(lease)
                .await?
                .into_iter()
                .filter(|tool| !self.always_load_tools.contains(tool.name.as_ref()))
                .map(|tool| tool.name.to_string())
                .collect();
            let enabled = session
                .conversation
                .as_ref()
                .map(enabled_tools)
                .unwrap_or_default();
            tools.retain(|tool| {
                !deferred.contains(tool.name.as_ref()) || enabled.contains(tool.name.as_ref())
            });
            tools.push(enable_tools_tool()?);
        }
        Ok(tools)
    }

    async fn prompt_parts(
        &self,
        _session: &Session,
        _conversation: &Conversation,
        lease: &ExtensionLease,
    ) -> Result<Vec<(String, String)>> {
        let mut prompt_parts = Vec::new();

        #[cfg(feature = "code-mode")]
        if lease.is_enabled(crate::agents::platform_extensions::code_execution::EXTENSION_NAME) {
            return Ok(prompt_parts);
        }

        let mut extensions = lease.instructions().await;
        extensions.retain(|extension| extension.name != crate::skills::EXTENSION_NAME);
        if extensions.is_empty() {
            return Ok(prompt_parts);
        }
        // HashMap order shuffles across restarts and would bust the prompt cache.
        extensions.sort_by(|a, b| a.name.cmp(&b.name));
        prompt_parts.extend(extension_prompt_parts(
            &extensions,
            self.defers_tools(lease),
        ));
        if self.defers_tools(lease) {
            let mut names: Vec<_> = self
                .mcp_tools(lease)
                .await?
                .into_iter()
                .filter(|tool| !self.always_load_tools.contains(tool.name.as_ref()))
                .map(|tool| tool.name.to_string())
                .collect();
            names.sort();
            prompt_parts.push(("mcp_tools".to_string(), format!(
                "MCP tools available to enable: {}. Call enable_tools with exact tool_names before using them. \
                 Their schemas become available on your next response. Enabling does not execute a tool.",
                names.join(", ")
            )));
        }
        Ok(prompt_parts)
    }

    async fn execution_tools(
        &self,
        _session: &Session,
        lease: &ExtensionLease,
    ) -> Result<Vec<Tool>> {
        let mut tools = lease.tools_excluding(crate::skills::EXTENSION_NAME).await;
        if self.defers_tools(lease) {
            tools.push(enable_tools_tool()?);
        }
        Ok(tools)
    }

    fn claims_expired_request(&self, messages: &[Message], request: &ToolRequest) -> bool {
        request_was_advertised(messages, request)
    }

    fn call_span(&self, session: &Session, request: &ToolRequest) -> tracing::Span {
        let tool = request.tool_call.as_ref().expect("parsed call");
        let span = tool_span(&tool.name, &request.id, &session.id);
        crate::agents::gen_ai_telemetry::record_tool_arguments(&span, tool);
        span
    }

    fn dispatch_error(&self, _request: &ToolRequest, _error: &rmcp::model::ErrorData) {
        #[cfg(feature = "telemetry")]
        crate::posthog::emit_error(
            "tool_execution_failed",
            &format!(
                "{}: {}",
                _request.tool_call.as_ref().expect("parsed call").name,
                _error
            ),
        );
    }

    fn accepts_request(&self, messages: &[Message], request: &ToolRequest) -> bool {
        request_was_advertised(messages, request)
    }

    fn can_resolve_for_requests(&self, messages: &[Message], requests: &[&ToolRequest]) -> bool {
        requests_can_resolve_a_lease(
            messages,
            requests
                .iter()
                .copied()
                .filter(|request| request.tool_call.is_ok()),
        )
    }

    fn expired_result(&self) -> CallToolResult {
        CallToolResult::error(vec![ContentBlock::text(EXPIRED_APPROVAL_RESPONSE)])
    }

    fn skipped_result(&self, session: &Session) -> Option<CallToolResult> {
        (session.goose_mode == GooseMode::Chat).then(|| {
            CallToolResult::success(vec![ContentBlock::text(CHAT_MODE_TOOL_SKIPPED_RESPONSE)])
        })
    }

    async fn before_call(
        &self,
        session: &Session,
        lease: &ExtensionLease,
        request: &ToolRequest,
    ) -> Option<CallToolResult> {
        let session = session_in_lease(lease, session);
        check_pre_tool_hooks(
            &self.hook_manager,
            &session.id,
            &session.working_dir,
            request,
        )
        .await
    }

    async fn call_override(
        &self,
        _session: &Session,
        lease: &ExtensionLease,
        request: &ToolRequest,
    ) -> Option<ToolCallResult> {
        let call = request.tool_call.as_ref().expect("parsed call");
        if self.defers_tools(lease) && call.name == ENABLE_TOOLS {
            Some(ToolCallResult::from(Ok(self
                .enable_tools(lease, call)
                .await)))
        } else {
            None
        }
    }

    fn decorate_call(
        &self,
        session: &Session,
        _lease: &ExtensionLease,
        _request: &ToolRequest,
        call: ToolCallResult,
    ) -> ToolCallResult {
        let span = tracing::Span::current();
        let call = self.extension_manager.applying_mutation(call, &session.id);
        let result = call.result;
        ToolCallResult {
            result: Box::new(
                async move {
                    let result =
                        crate::agents::large_response_handler::process_tool_response(result.await);
                    crate::agents::gen_ai_telemetry::record_tool_result(
                        &tracing::Span::current(),
                        &result,
                    );
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
                .instrument(span)
                .boxed(),
            ),
            notification_stream: call.notification_stream,
            action_required_stream: call.action_required_stream,
        }
    }

    async fn after_call(
        &self,
        session: &Session,
        _conversation: &Conversation,
        lease: &ExtensionLease,
        request: &ToolRequest,
        response: &ToolResponse,
    ) {
        let session = session_in_lease(lease, session);
        emit_post_tool_hooks(
            &self.hook_manager,
            &session.id,
            &session.working_dir,
            request,
            response,
        )
        .await;
    }

    fn on_result(
        &self,
        request_id: &str,
        result: &Result<CallToolResult, rmcp::model::ErrorData>,
        emit: &Emitter,
    ) {
        if let Some(notification) = result.as_ref().ok().and_then(platform_notification) {
            emit.emit(AgentEvent::McpNotification((
                request_id.to_string(),
                notification,
            )));
        }
    }

    fn emit_response(&self, response: Message, emit: &Emitter) -> Message {
        emit.emit(AgentEvent::Message(response.user_visible_content()));
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose_agent::tool_call_state::EXECUTABLE_KEY;

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
    fn operation_generated_requests_without_inference_remain_executable() {
        let message = Message::assistant().with_tool_request_with_metadata(
            "operation-generated",
            Ok(CallToolRequestParams::new("registered__tool")),
            None,
            Some(serde_json::json!({ EXECUTABLE_KEY: true })),
        );

        let pending = pending_advertised_tool_requests(&[message]);

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
