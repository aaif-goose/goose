//! Exposes extension capabilities and executes requests that belong to them.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::{FutureExt, StreamExt};
use goose_agent::tool_call_state::request_executable;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ErrorData, Role, Tool, ToolAnnotations,
};

use crate::agents::extension::ExtensionInfo;
use crate::agents::extension_manager::{CallRequest, ExtensionLease, ExtensionManager};
use crate::agents::reply_parts::is_tool_visible_to_model;
use crate::agents::state_machine::ops_llm::{ADVERTISED_TOOLS_NOTE, LLM_OPERATION_NAME};
use crate::agents::state_machine::ops_skills::LOAD_SKILL_TOOL_NAME;
use crate::agents::state_machine::post_tool_hook::GoosePostToolHook;
use crate::agents::state_machine::{
    applied, messages_since_kickoff, not_applicable, yielded_with, ConversationEffect, Emitter,
    GooseEffect, Operation, OperationResult, SlashCommand,
};
use crate::agents::tool_execution::{
    tool_stream, ToolCallResult, ToolStreamItem, CHAT_MODE_TOOL_SKIPPED_RESPONSE,
};
use crate::agents::AgentEvent;
use crate::config::GooseMode;
use crate::conversation::message::{
    ActionRequiredData, Message, MessageContent, ProviderMetadata, ToolRequest, ToolResponse,
};
use crate::conversation::Conversation;
use crate::hints::load_hints::SubdirectoryHintTracker;
use crate::hooks::{HookContext, HookEvent, HookManager};
use crate::session::Session;

pub(super) const EXPIRED_APPROVAL_RESPONSE: &str =
    "Tool approval expired because its extension lease is no longer available. Request the tool again.";
use std::sync::{Arc, Mutex as StdMutex};
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

#[derive(Clone, Copy)]
pub(super) enum ToolCategory {
    Shell,
    Read,
    Write,
    Other,
}

pub(super) fn categorize_tool(tool_name: &str) -> ToolCategory {
    match tool_name.rsplit("__").next().unwrap_or(tool_name) {
        "shell" | "bash" | "exec" | "run" => ToolCategory::Shell,
        "read" | "view" | "cat" | "read_file" => ToolCategory::Read,
        "write" | "edit" | "patch" | "write_file" | "edit_file" => ToolCategory::Write,
        _ => ToolCategory::Other,
    }
}

pub(super) fn string_argument(input: &serde_json::Value, keys: &[&str]) -> Option<String> {
    let arguments = input.as_object()?;
    keys.iter().find_map(|key| {
        arguments
            .get(*key)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
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

pub(super) async fn emit_with_matcher(
    hook_manager: &HookManager,
    event: HookEvent,
    session_id: &str,
    working_dir: &Path,
    matcher: String,
    tool_name: &str,
    tool_input: Option<serde_json::Value>,
) {
    if !hook_manager.has_hooks(event) {
        return;
    }
    let mut context = HookContext::new(event, session_id)
        .with_tool(tool_name.to_string(), tool_input)
        .with_working_dir(working_dir.to_string_lossy().to_string());
    context.matcher_context = Some(matcher);
    hook_manager.emit(event, context).await;
}

pub(super) async fn emit_extended_pre_hooks(
    hook_manager: &HookManager,
    tool_name: &str,
    tool_input: Option<&serde_json::Value>,
    session: &Session,
) {
    let (event, matcher) = match categorize_tool(tool_name) {
        ToolCategory::Shell => (
            HookEvent::BeforeShellExecution,
            tool_input.and_then(|input| string_argument(input, &["command"])),
        ),
        ToolCategory::Read => (
            HookEvent::BeforeReadFile,
            tool_input.and_then(|input| string_argument(input, &["path", "file", "file_path"])),
        ),
        ToolCategory::Write | ToolCategory::Other => return,
    };
    if let Some(matcher) = matcher {
        emit_with_matcher(
            hook_manager,
            event,
            &session.id,
            &session.working_dir,
            matcher,
            tool_name,
            tool_input.cloned(),
        )
        .await;
    }
}

#[derive(Default)]
struct ToolBatch {
    actions: Vec<Message>,
    response: Option<Message>,
}

impl ToolBatch {
    fn record(
        &mut self,
        request_id: &str,
        result: std::result::Result<CallToolResult, ErrorData>,
        metadata: Option<&ProviderMetadata>,
    ) {
        let response = self
            .response
            .get_or_insert_with(|| Message::user().with_generated_id_if_missing());
        if !response.get_tool_response_ids().contains(&request_id) {
            response.add_tool_response_with_metadata(request_id, result, metadata);
        }
    }

    fn take(&mut self) -> (Vec<Message>, Option<Message>) {
        (std::mem::take(&mut self.actions), self.response.take())
    }
}

fn recording_result(
    batch: &Arc<StdMutex<ToolBatch>>,
    result: ToolCallResult,
    span: tracing::Span,
    request: &ToolRequest,
    session: Arc<Session>,
    post_tool_call_hook: Arc<GoosePostToolHook>,
) -> ToolCallResult {
    let batch = Arc::clone(batch);
    let request = request.clone();
    let tool_call_id = request.id.clone();
    let metadata = request.metadata.clone();
    let future = async move {
        let result =
            crate::agents::large_response_handler::process_tool_response(result.result.await);
        batch
            .lock()
            .unwrap()
            .record(&tool_call_id, result.clone(), metadata.as_ref());
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
        post_tool_call_hook
            .after_call(session.as_ref(), &request, &response)
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

pub(super) fn leased_working_dir(
    extension_lease: &StdMutex<Option<Arc<ExtensionLease>>>,
    session: &Session,
) -> PathBuf {
    extension_lease
        .lock()
        .expect("extension lease unavailable")
        .as_ref()
        .filter(|lease| lease.scope_id() == session.id)
        .and_then(|lease| lease.working_dir())
        .map_or_else(|| session.working_dir.clone(), Path::to_path_buf)
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
    post_tool_call_hook: Arc<GoosePostToolHook>,
    lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
    batch: Arc<StdMutex<ToolBatch>>,
    defer_tools: bool,
    always_load_tools: HashSet<String>,
}

impl ToolExecutionOperation {
    pub fn new(
        extension_manager: Arc<ExtensionManager>,
        hook_manager: HookManager,
        post_tool_call_hook: Arc<GoosePostToolHook>,
        lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
        defer_tools: bool,
        always_load_tools: Vec<String>,
    ) -> Self {
        Self {
            extension_manager,
            hook_manager,
            post_tool_call_hook,
            lease,
            batch: Arc::default(),
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
            .tools_where(|extension| !extension.is_platform())
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
        self.batch.lock().unwrap().take()
    }

    async fn lease(&self, session: &Session) -> Result<Arc<ExtensionLease>> {
        let lease = self
            .lease
            .lock()
            .expect("extension lease unavailable")
            .clone();
        if let Some(lease) = lease {
            if lease.scope_id() == session.id {
                return Ok(lease);
            }
        }
        self.resolve_lease(session).await
    }

    async fn resolve_lease(&self, session: &Session) -> Result<Arc<ExtensionLease>> {
        let lease = Arc::new(self.extension_manager.current_lease(&session.id).await?);
        *self.lease.lock().expect("extension lease unavailable") = Some(Arc::clone(&lease));
        Ok(lease)
    }

    async fn dispatch_tool_call(
        &self,
        lease: Arc<ExtensionLease>,
        tool_call: CallToolRequestParams,
        request: &ToolRequest,
        cancellation_token: CancellationToken,
        call_session: &Arc<Session>,
    ) -> ToolCallResult {
        let session = call_session.as_ref();
        let request_id = request.id.clone();
        let span = tool_span(&tool_call.name, &request_id, &session.id);
        crate::agents::gen_ai_telemetry::record_tool_arguments(&span, &tool_call);
        let result_span = span.clone();

        async {
            let tool_input = tool_call
                .arguments
                .as_ref()
                .map(|arguments| serde_json::Value::Object(arguments.clone()));
            emit_extended_pre_hooks(
                &self.hook_manager,
                &tool_call.name,
                tool_input.as_ref(),
                session,
            )
            .await;

            let result = if self.defers_tools(&lease) && tool_call.name == ENABLE_TOOLS {
                ToolCallResult::from(Ok(self.enable_tools(&lease, &tool_call).await))
            } else {
                let result = lease
                    .call(
                        tool_call.clone(),
                        CallRequest::new(request_id.clone())
                            .with_container(session.container.clone()),
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
            recording_result(
                &self.batch,
                result,
                result_span,
                request,
                Arc::clone(call_session),
                Arc::clone(&self.post_tool_call_hook),
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
    let answered: HashSet<&str> = messages
        .iter()
        .flat_map(Message::get_tool_response_ids)
        .collect();

    messages
        .iter()
        .filter(|message| message.role == Role::Assistant)
        .flat_map(|message| {
            message.content.iter().filter_map(|c| match c {
                MessageContent::ToolRequest(req) if req.was_executed_externally() => None,
                MessageContent::ToolRequest(req) if !answered.contains(req.id.as_str()) => {
                    if let Err(parse_error) = &req.tool_call {
                        return Some((
                            req.clone(),
                            ToolDisposition::ParseError(parse_error.to_string()),
                        ));
                    }
                    (request_executable(req) == Some(true))
                        .then(|| (req.clone(), ToolDisposition::Execute))
                }
                _ => None,
            })
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

pub(super) fn approval_expired(
    extension_lease: &StdMutex<Option<Arc<ExtensionLease>>>,
    messages: &[Message],
    request: &ToolRequest,
) -> bool {
    if extension_lease
        .lock()
        .expect("extension lease unavailable")
        .is_some()
    {
        return false;
    }
    match request
        .tool_call
        .as_ref()
        .map(|tool_call| tool_call.name.as_ref())
    {
        Ok(LOAD_SKILL_TOOL_NAME) => true,
        _ => {
            let pending = pending_advertised_tool_requests(messages);
            !requests_can_resolve_a_lease(
                messages,
                pending
                    .iter()
                    .filter(|(request, disposition)| {
                        *disposition == ToolDisposition::Execute
                            && !request
                                .tool_call
                                .as_ref()
                                .is_ok_and(|tool_call| tool_call.name == LOAD_SKILL_TOOL_NAME)
                    })
                    .map(|(request, _)| request),
            )
        }
    }
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

        let lease = self
            .lease
            .lock()
            .expect("extension lease unavailable")
            .clone();
        let lease = match lease {
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

        let call_session = Arc::new(session_in_lease(&lease, session));
        let mut tool_streams = Vec::new();
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

            let req_id = request.id.clone();
            let stream = tool_stream(
                result
                    .notification_stream
                    .unwrap_or_else(|| Box::new(futures::stream::empty())),
                result
                    .action_required_stream
                    .unwrap_or_else(|| Box::new(futures::stream::empty())),
                result.result,
            )
            .map(move |item| (req_id.clone(), item));
            tool_streams.push(stream);
        }

        let mut combined = futures::stream::select_all(tool_streams);
        for (request, disposition) in &pending {
            let mut batch = self.batch.lock().unwrap();
            match disposition {
                ToolDisposition::Execute => {}
                ToolDisposition::ParseError(parse_error) => {
                    batch.record(
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

        while let Some((request_id, item)) = combined.next().await {
            match item {
                ToolStreamItem::Result(Ok(result)) => {
                    if let Some(notification) = platform_notification(&result) {
                        emit.emit(AgentEvent::McpNotification((request_id, notification)));
                    }
                }
                ToolStreamItem::Result(Err(_)) => {}
                ToolStreamItem::Message(msg) => {
                    emit.emit(AgentEvent::McpNotification((request_id, msg)));
                }
                ToolStreamItem::ActionRequired(msg) => {
                    let msg = msg.with_generated_id_if_missing();
                    self.batch.lock().unwrap().actions.push(msg.clone());
                    emit.message(msg);
                }
            }
        }

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
