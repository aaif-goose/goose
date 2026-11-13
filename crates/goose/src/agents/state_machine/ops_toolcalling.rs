//! Exposes extension capabilities and executes requests that belong to them.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::FutureExt;
use goose_agent::tool_call_state::pending_tool_requests as executable_tool_requests;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, Role, Tool, ToolAnnotations,
};

use crate::agents::extension::ExtensionInfo;
use crate::agents::extension_manager::{CallRequest, ExtensionLease, ExtensionManager};
use crate::agents::reply_parts::is_tool_visible_to_model;
use crate::agents::state_machine::ops_llm::{ADVERTISED_TOOLS_NOTE, LLM_OPERATION_NAME};
use crate::agents::state_machine::tool_hooks::{check_pre_tool_hooks, emit_post_tool_hooks};
use crate::agents::state_machine::{
    applied, messages_since_kickoff, not_applicable, yielded_with, ConversationEffect, Emitter,
    GooseEffect, Operation, OperationResult, SlashCommand,
};
use crate::agents::tool_execution::ToolCallResult;
use crate::agents::tool_execution::CHAT_MODE_TOOL_SKIPPED_RESPONSE;
use crate::agents::AgentEvent;
use crate::config::GooseMode;
use crate::conversation::message::{
    ActionRequiredData, Message, MessageContent, ToolRequest, ToolResponse,
};
use crate::conversation::Conversation;
use crate::hints::load_hints::SubdirectoryHintTracker;
use crate::hooks::HookManager;
use crate::session::Session;
use goose_agent::extension_manager::{PendingCall, ToolCallBatch, TurnLease};

pub(super) const EXPIRED_APPROVAL_RESPONSE: &str =
    "Tool approval expired because its extension lease is no longer available. Request the tool again.";
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
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

fn with_hooks_and_recording(
    batch: &ToolCallBatch,
    result: ToolCallResult,
    span: tracing::Span,
    request: &ToolRequest,
    hook_manager: &HookManager,
    session: &Session,
) -> ToolCallResult {
    let batch = batch.clone();
    let hook_manager = hook_manager.clone();
    let session_id = session.id.clone();
    let working_dir = session.working_dir.clone();
    let request = request.clone();
    let tool_call_id = request.id.clone();
    let metadata = request.metadata.clone();
    let future = async move {
        if let Some(denied) =
            check_pre_tool_hooks(&hook_manager, &session_id, &working_dir, &request).await
        {
            let denied = Ok(denied);
            batch.record(&tool_call_id, denied.clone(), metadata.as_ref());
            return denied;
        }
        let result =
            crate::agents::large_response_handler::process_tool_response(result.result.await);
        batch.record(&tool_call_id, result.clone(), metadata.as_ref());
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
        let response = ToolResponse {
            id: tool_call_id,
            tool_result: result.clone(),
            metadata,
        };
        emit_post_tool_hooks(
            &hook_manager,
            &session_id,
            &working_dir,
            &request,
            &response,
        )
        .await;
        result
    }
    .instrument(span);
    ToolCallResult {
        notification_stream: result.notification_stream,
        action_required_stream: result.action_required_stream,
        result: Box::new(future.boxed()),
    }
}

pub(super) fn leased_working_dir(extension_lease: &TurnLease, session: &Session) -> PathBuf {
    extension_lease
        .for_scope(&session.id)
        .and_then(|lease| lease.working_dir().map(Path::to_path_buf))
        .unwrap_or_else(|| session.working_dir.clone())
}

fn session_in_lease(lease: &ExtensionLease, session: &Session) -> Session {
    let mut session = session.clone();
    if let Some(working_dir) = lease.working_dir() {
        session.working_dir = working_dir.to_path_buf();
    }
    session
}

pub struct ToolExecutionOperation {
    extension_manager: Arc<ExtensionManager>,
    hook_manager: HookManager,
    lease: TurnLease,
    batch: ToolCallBatch,
    defer_tools: bool,
    always_load_tools: HashSet<String>,
}

impl ToolExecutionOperation {
    pub fn new(
        extension_manager: Arc<ExtensionManager>,
        hook_manager: HookManager,
        lease: TurnLease,
        defer_tools: bool,
        always_load_tools: Vec<String>,
    ) -> Self {
        Self {
            extension_manager,
            hook_manager,
            lease,
            batch: ToolCallBatch::default(),
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

    async fn mcp_tools(&self, lease: &ExtensionLease) -> Vec<Tool> {
        lease
            .untrusted_tools()
            .await
            .into_iter()
            .filter(is_tool_visible_to_model)
            .collect()
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
        let tools = self.mcp_tools(lease).await;
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

    fn take_batch(&self) -> (Vec<Message>, Option<Message>) {
        self.batch.take()
    }

    async fn lease(&self, session: &Session) -> Result<Arc<ExtensionLease>> {
        match self.lease.for_scope(&session.id) {
            Some(lease) => Ok(lease),
            None => self.resolve_lease(session).await,
        }
    }

    async fn resolve_lease(&self, session: &Session) -> Result<Arc<ExtensionLease>> {
        Ok(self
            .lease
            .set(self.extension_manager.current_lease(&session.id).await?))
    }

    async fn dispatch_tool_call(
        &self,
        lease: Arc<ExtensionLease>,
        tool_call: CallToolRequestParams,
        request: &ToolRequest,
        cancellation_token: CancellationToken,
        session: &Session,
    ) -> ToolCallResult {
        let request_id = request.id.clone();
        let span = tool_span(&tool_call.name, &request_id, &session.id);
        crate::agents::gen_ai_telemetry::record_tool_arguments(&span, &tool_call);
        let result_span = span.clone();

        async {
            let result = if self.defers_tools(&lease) && tool_call.name == ENABLE_TOOLS {
                ToolCallResult::from(Ok(self.enable_tools(&lease, &tool_call).await))
            } else {
                let result = lease
                    .call(
                        tool_call.clone(),
                        CallRequest::new(request_id.clone()),
                        cancellation_token,
                    )
                    .await;
                result.unwrap_or_else(|error| {
                    #[cfg(feature = "telemetry")]
                    crate::posthog::emit_error(
                        "tool_execution_failed",
                        &format!("{}: {}", tool_call.name, error),
                    );
                    ToolCallResult::from(Err(error))
                })
            };
            let result = self
                .extension_manager
                .applying_mutation(result, &session.id);
            with_hooks_and_recording(
                &self.batch,
                result,
                result_span,
                request,
                &self.hook_manager,
                session,
            )
        }
        .instrument(span)
        .await
    }

    fn command_response(
        conversation: &Conversation,
        message: String,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let command = messages_since_kickoff(conversation)?
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("prompt command conversation has no kickoff message"))?;
        let message_id = command
            .id
            .clone()
            .ok_or_else(|| anyhow!("Persisted slash command message has no id"))?;
        let command = command.with_visibility(true, false);
        let response = Message::assistant()
            .with_text(message)
            .with_visibility(true, false);
        emit.message(command);
        let response = emit.message(response);
        yielded_with([
            ConversationEffect::SetMessageVisibility {
                message_id,
                user_visible: true,
                agent_visible: false,
            }
            .into(),
            response.into(),
        ])
    }

    async fn list_prompts(
        &self,
        command: &SlashCommand<'_>,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let prompts = self
            .lease(session)
            .await?
            .list_prompts(emit.cancel_token().clone())
            .await;
        let extension_filter = command.params_str.split_whitespace().next();
        if let Some(filter) = extension_filter {
            if !prompts.contains_key(filter) {
                return Self::command_response(
                    conversation,
                    format!("Extension '{filter}' not found"),
                    emit,
                );
            }
        }

        let filtered: HashMap<_, _> = prompts
            .into_iter()
            .filter(|(extension, _)| extension_filter.is_none_or(|filter| extension == filter))
            .collect();
        let mut output = String::new();
        if filtered.is_empty() {
            output.push_str("No prompts available.\n");
        } else {
            output.push_str("Available prompts:\n\n");
            for (extension, prompts) in filtered {
                output.push_str(&format!("**{extension}**:\n"));
                for prompt in prompts {
                    output.push_str(&format!("  - {}\n", prompt.name));
                }
                output.push('\n');
            }
        }
        Self::command_response(conversation, output, emit)
    }

    async fn run_prompt(
        &self,
        command: &SlashCommand<'_>,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let params: Vec<_> = command.params_str.split_whitespace().collect();
        let Some(prompt_name) = params.first() else {
            return Self::command_response(
                conversation,
                "Prompt name argument is required".to_string(),
                emit,
            );
        };
        let prompts = self
            .lease(session)
            .await?
            .list_prompts(emit.cancel_token().clone())
            .await;
        let found = prompts.iter().find_map(|(extension, prompts)| {
            prompts
                .iter()
                .find(|prompt| prompt.name == *prompt_name)
                .map(|prompt| (extension.clone(), prompt.clone()))
        });
        let Some((extension, prompt)) = found else {
            return Self::command_response(
                conversation,
                format!("Prompt '{prompt_name}' not found"),
                emit,
            );
        };

        if params.get(1) == Some(&"--info") {
            let mut output = format!("**Prompt: {}**\n\n", prompt.name);
            if let Some(description) = &prompt.description {
                output.push_str(&format!("Description: {description}\n\n"));
            }
            output.push_str(&format!("Extension: {extension}\n\n"));
            if let Some(arguments) = &prompt.arguments {
                output.push_str("Arguments:\n");
                for argument in arguments {
                    output.push_str(&format!("  - {}", argument.name));
                    if let Some(description) = &argument.description {
                        output.push_str(&format!(": {description}"));
                    }
                    output.push('\n');
                }
            }
            return Self::command_response(conversation, output, emit);
        }

        let arguments: HashMap<_, _> = params
            .iter()
            .skip(1)
            .filter_map(|param| param.split_once('='))
            .map(|(key, value)| (key.to_string(), value.trim_matches('"').to_string()))
            .collect();
        let result = match self
            .lease(session)
            .await?
            .get_prompt(
                &extension,
                prompt_name,
                serde_json::to_value(arguments)?,
                emit.cancel_token().clone(),
            )
            .await
        {
            Ok(result) => result,
            Err(error) => {
                return Self::command_response(conversation, error.to_string(), emit);
            }
        };

        let command_message = messages_since_kickoff(conversation)?
            .first()
            .ok_or_else(|| anyhow!("prompt command conversation has no kickoff message"))?;
        let message_id = command_message
            .id
            .clone()
            .ok_or_else(|| anyhow!("Persisted slash command message has no id"))?;
        let mut effects = vec![ConversationEffect::SetMessageVisibility {
            message_id,
            user_visible: true,
            agent_visible: false,
        }
        .into()];
        for (index, prompt_message) in result.messages.into_iter().enumerate() {
            let message = Message::from(prompt_message);
            let expected_role = if index % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            };
            if message.role != expected_role {
                return Self::command_response(
                    conversation,
                    format!(
                        "Expected {expected_role:?} message at position {index}, but found {:?}",
                        message.role
                    ),
                    emit,
                );
            }
            effects.push(message.with_visibility(false, true).into());
        }
        if effects.len() == 1 {
            return Self::command_response(
                conversation,
                format!("Prompt '{prompt_name}' returned no messages"),
                emit,
            );
        }
        applied(effects)
    }
}

pub(super) fn pending_tool_requests(messages: &[Message]) -> Vec<(ToolRequest, ToolDisposition)> {
    executable_tool_requests(messages)
        .into_iter()
        .map(|request| {
            let disposition = match &request.tool_call {
                Ok(_) => ToolDisposition::Execute,
                Err(error) => ToolDisposition::ParseError(error.to_string()),
            };
            (request.clone(), disposition)
        })
        .collect()
}

pub(super) fn pending_advertised_tool_requests(
    messages: &[Message],
) -> Vec<(ToolRequest, ToolDisposition)> {
    pending_tool_requests(messages)
        .into_iter()
        .filter(|(request, _)| request_was_advertised(messages, request))
        .collect()
}

pub(super) fn call_will_not_run(
    session: &Session,
    conversation: &Conversation,
    request: &ToolRequest,
) -> Result<bool> {
    let messages = messages_since_kickoff(conversation)?;
    Ok(session.goose_mode == GooseMode::Chat || !request_was_advertised(messages, request))
}

pub(super) fn request_was_advertised(messages: &[Message], request: &ToolRequest) -> bool {
    let Some(tool_call) = request.tool_call.as_ref().ok() else {
        return true;
    };
    let Some(message) = messages.iter().find(|message| {
        message.role == Role::Assistant
            && message.content.iter().any(|content| {
                content
                    .as_tool_request()
                    .is_some_and(|item| item.id == request.id)
            })
    }) else {
        return false;
    };
    if message.metadata.inference.is_none() {
        return true;
    }
    message
        .metadata
        .operation_note(LLM_OPERATION_NAME, ADVERTISED_TOOLS_NOTE)
        .and_then(serde_json::Value::as_array)
        .is_some_and(|tools| {
            tools
                .iter()
                .any(|tool| tool.as_str() == Some(tool_call.name.as_ref()))
        })
}

fn requests_can_resolve_a_lease<'a>(
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
impl Operation<Session, GooseEffect> for ToolExecutionOperation {
    fn name(&self) -> &'static str {
        "tool_execution"
    }

    async fn finalize_cancellation(
        &self,
        _session: &Session,
        _conversation: &Conversation,
        emit: &Emitter,
    ) -> Vec<GooseEffect> {
        let (actions, response) = self.take_batch();
        if let Some(response) = &response {
            emit.emit(AgentEvent::Message(response.user_visible_content()));
        }
        actions
            .into_iter()
            .chain(response)
            .map(GooseEffect::from)
            .collect()
    }

    async fn run_command(
        &self,
        command: &SlashCommand<'_>,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        match command.command {
            "prompts" => {
                self.list_prompts(command, session, conversation, emit)
                    .await
            }
            "prompt" => self.run_prompt(command, session, conversation, emit).await,
            _ => not_applicable(),
        }
    }

    async fn inference_tools(&self, session: &Session) -> Result<Vec<Tool>> {
        let lease = self.lease(session).await?;
        let mut tools = lease.tools_excluding(crate::skills::EXTENSION_NAME).await;
        if self.defers_tools(&lease) {
            let deferred: HashSet<_> = self
                .mcp_tools(&lease)
                .await
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

    async fn moim_parts(
        &self,
        session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<String>> {
        Ok(self.lease(session).await?.moim().await)
    }

    async fn prompt_parts(
        &self,
        session: &Session,
        conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        let mut hints = SubdirectoryHintTracker::new();
        for message in conversation
            .messages()
            .iter()
            .filter(|message| message.is_agent_visible())
        {
            for content in &message.content {
                if let MessageContent::ToolRequest(request) = content {
                    if let Ok(tool_call) = &request.tool_call {
                        hints.record_tool_arguments(&tool_call.arguments, &session.working_dir);
                    }
                }
            }
        }
        let mut prompt_parts = hints.load_new_hints(&session.working_dir);

        let lease = self.lease(session).await?;
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
            self.defers_tools(&lease),
        ));
        if self.defers_tools(&lease) {
            let mut names: Vec<_> = self
                .mcp_tools(&lease)
                .await
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

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        let mut pending = pending_advertised_tool_requests(messages);
        if pending.is_empty() {
            return not_applicable();
        }

        let lease = match self.lease.get() {
            Some(lease) => Some(lease),
            None if requests_can_resolve_a_lease(
                messages,
                pending
                    .iter()
                    .filter(|(_, disposition)| *disposition == ToolDisposition::Execute)
                    .map(|(request, _)| request),
            ) =>
            {
                Some(self.resolve_lease(session).await?)
            }
            None => None,
        };
        let Some(lease) = lease else {
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
            return applied([response.into()]);
        };

        let mut known_tools: HashSet<_> = lease
            .tools_excluding(crate::skills::EXTENSION_NAME)
            .await
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        if self.defers_tools(&lease) {
            known_tools.insert(ENABLE_TOOLS.to_string());
        }
        pending.retain(|(request, _)| {
            request
                .tool_call
                .as_ref()
                .is_ok_and(|tool_call| known_tools.contains(tool_call.name.as_ref()))
        });
        if pending.is_empty() {
            return not_applicable();
        }

        if session.goose_mode == GooseMode::Chat {
            let mut response = Message::user();
            for (request, disposition) in &pending {
                let result = match disposition {
                    ToolDisposition::ParseError(parse_error) => {
                        CallToolResult::error(vec![ContentBlock::text(format!(
                            "The tool call could not be parsed: {parse_error}."
                        ))])
                    }
                    _ => CallToolResult::success(vec![ContentBlock::text(
                        CHAT_MODE_TOOL_SKIPPED_RESPONSE,
                    )]),
                };
                response.add_tool_response_with_metadata(
                    request.id.clone(),
                    Ok(result),
                    request.metadata.as_ref(),
                );
            }
            let response = emit.message(response);
            return applied([response.into()]);
        }

        let call_session = session_in_lease(&lease, session);
        for (request, disposition) in &pending {
            match disposition {
                ToolDisposition::Execute => {}
                ToolDisposition::ParseError(parse_error) => {
                    self.batch.record(
                        &request.id,
                        Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                            "The tool call could not be parsed: {parse_error}. \
                             Correct the arguments and try again."
                        ))])),
                        request.metadata.as_ref(),
                    );
                }
            }
        }

        let mut calls = Vec::new();
        for (request, disposition) in &pending {
            if *disposition != ToolDisposition::Execute {
                continue;
            }
            let tool_call = request
                .tool_call
                .clone()
                .map_err(|e| anyhow!("tool call could not be parsed: {e}"))?;
            let result = self
                .dispatch_tool_call(
                    Arc::clone(&lease),
                    tool_call,
                    request,
                    emit.cancel_token().clone(),
                    &call_session,
                )
                .await;
            calls.push(PendingCall {
                request_id: request.id.clone(),
                metadata: request.metadata.clone(),
                call: result,
            });
        }
        self.batch
            .run(calls, emit, &|request_id, output| {
                if let Some(notification) = output.as_ref().ok().and_then(platform_notification) {
                    emit.emit(AgentEvent::McpNotification((
                        request_id.to_string(),
                        notification,
                    )));
                }
            })
            .await;

        let (actions, response) = self.take_batch();
        let response = response.ok_or_else(|| anyhow!("tool batch produced no responses"))?;
        emit.emit(AgentEvent::Message(response.user_visible_content()));
        applied(actions.into_iter().chain([response]).map(GooseEffect::from))
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
