//! Wraps extension execution with conversation-derived progressive tool discovery.

use std::collections::HashSet;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use goose_agent::post_tool_call_hook::PostToolCallHook;
use rmcp::model::{CallToolResult, ContentBlock, JsonObject, Tool, ToolAnnotations};
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing_futures::Instrument;

use super::ops_toolcalling::{
    advertised_tool_names, pending_tool_requests, request_was_advertised, requesting_message,
    tool_span, ToolDisposition, ToolExecutionOperation, EXPIRED_APPROVAL_RESPONSE,
};
use super::post_tool_hook::run_post_tool_call_hook;
use super::tool_discovery::{
    always_loads, deferred_tool_named, names_by_extension, not_loaded_message, resolve_load,
    select_tools, NotSent, LOAD_TOOLS_TOOL_NAME,
};
use super::{
    applied, messages_since_kickoff, Emitter, GooseEffect, InferenceInput, Operation,
    OperationResult, SlashCommand,
};
use crate::agents::extension_manager::{get_tool_owner, ExtensionLease};
use crate::agents::reply_parts::is_tool_visible_to_model;
use crate::agents::tool_execution::CHAT_MODE_TOOL_SKIPPED_RESPONSE;
use crate::config::GooseMode;
use crate::conversation::message::{Message, ToolResponse};
use crate::conversation::Conversation;
use crate::session::Session;

const OPERATION_NAME: &str = "tool_discovery";
const LOADED_TOOLS_NOTE: &str = "loaded_tools";
const TOOL_DISCOVERY_NOTE: &str = "Some tools are deferred and omitted from tools[]. \
Each extension below lists its deferred tools under \"Deferred tools\". \
Use load_tools to load the ones you need, by exact name or `<extension>__*` for all of an extension's tools. \
After loading, their full schemas appear in tools[] on your next request.";

#[derive(Deserialize, JsonSchema)]
struct LoadToolsParams {
    /// Exact tool names, or `<extension>__*` for all of an extension's tools.
    tool_names: Vec<String>,
}

fn load_tools_tool() -> Result<Tool> {
    let schema = serde_json::to_value(schema_for!(LoadToolsParams))?;
    let schema = schema
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("load_tools schema is not an object"))?;
    Ok(Tool::new(
        LOAD_TOOLS_TOOL_NAME,
        "Load extension tools listed in your system instructions. Their full schemas become \
         callable in your next response; do not call a tool in the same response that loads it.",
        schema,
    )
    .annotate(ToolAnnotations::from_raw(
        Some("Load tools".to_string()),
        Some(true),
        Some(false),
        Some(true),
        Some(false),
    )))
}

pub(super) fn loaded_tools(messages: &[Message]) -> Vec<String> {
    let mut loaded = Vec::new();
    for message in messages {
        if let Some(names) = message
            .metadata
            .operation_note(OPERATION_NAME, LOADED_TOOLS_NOTE)
            .and_then(Value::as_array)
        {
            for name in names.iter().filter_map(Value::as_str) {
                if !loaded.iter().any(|loaded| loaded == name) {
                    loaded.push(name.to_string());
                }
            }
        }
    }
    loaded
}

fn enabled(lease: &ExtensionLease) -> bool {
    #[cfg(feature = "code-mode")]
    {
        !lease.is_enabled(crate::agents::platform_extensions::code_execution::EXTENSION_NAME)
    }
    #[cfg(not(feature = "code-mode"))]
    {
        let _ = lease;
        true
    }
}

async fn deferred_tools(lease: &ExtensionLease) -> Vec<Tool> {
    let extensions: HashSet<String> = lease
        .configs()
        .into_iter()
        .filter(|config| !config.is_platform())
        .map(|config| config.key())
        .collect();
    lease
        .tools_excluding(crate::skills::EXTENSION_NAME)
        .await
        .into_iter()
        .filter(|tool| {
            is_tool_visible_to_model(tool)
                && !always_loads(tool)
                && get_tool_owner(tool).is_some_and(|owner| extensions.contains(&owner))
        })
        .collect()
}

pub struct ToolDiscoveryOperation {
    execution: ToolExecutionOperation,
    extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
    post_tool_call_hook: Arc<dyn PostToolCallHook<Session>>,
}

impl ToolDiscoveryOperation {
    pub fn new(
        execution: ToolExecutionOperation,
        extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
        post_tool_call_hook: Arc<dyn PostToolCallHook<Session>>,
    ) -> Self {
        Self {
            execution,
            extension_lease,
            post_tool_call_hook,
        }
    }

    fn session_lease(&self, session: &Session) -> Option<Arc<ExtensionLease>> {
        self.extension_lease
            .lock()
            .expect("extension lease unavailable")
            .clone()
            .filter(|lease| lease.scope_id() == session.id.as_str())
    }

    async fn load(
        lease: &ExtensionLease,
        loaded: &mut Vec<String>,
        arguments: Option<&JsonObject>,
        sent: &[String],
        loaded_at_request: &[String],
    ) -> CallToolResult {
        let requested = arguments
            .ok_or_else(|| "Missing arguments".to_string())
            .and_then(|arguments| {
                serde_json::from_value::<LoadToolsParams>(Value::Object(arguments.clone()))
                    .map(|params| params.tool_names)
                    .map_err(|error| format!("Invalid arguments: {error}"))
            });
        let requested = match requested {
            Ok(requested) => requested,
            Err(error) => return CallToolResult::error(vec![ContentBlock::text(error)]),
        };
        let mut available = lease.tools_excluding(crate::skills::EXTENSION_NAME).await;
        available.retain(is_tool_visible_to_model);
        let deferred = deferred_tools(lease).await;
        // Include loads completed since this inference, even if an approval pause
        // split its tool calls across separate runs of the operation.
        let admitted_since_request = loaded
            .iter()
            .filter(|name| !loaded_at_request.contains(name) && !sent.contains(name))
            .count();
        let left_out: Vec<String> = loaded_at_request
            .iter()
            .filter(|name| !sent.contains(name))
            .cloned()
            .collect();
        let (outcome, newly_loaded) = resolve_load(
            &requested,
            &available,
            &deferred,
            loaded,
            &left_out,
            sent.len() + admitted_since_request,
        );
        loaded.extend(newly_loaded);
        let text = serde_json::to_string(&outcome).expect("load outcome serializes");
        if outcome.loaded.is_empty() {
            CallToolResult::error(vec![ContentBlock::text(text)])
        } else {
            CallToolResult::success(vec![ContentBlock::text(text)])
        }
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for ToolDiscoveryOperation {
    fn name(&self) -> &'static str {
        OPERATION_NAME
    }

    async fn finalize_cancellation(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Vec<GooseEffect> {
        self.execution
            .finalize_cancellation(session, conversation, emit)
            .await
    }

    async fn run_command(
        &self,
        command: &SlashCommand<'_>,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        self.execution
            .run_command(command, session, conversation, emit)
            .await
    }

    async fn inference_tools(&self, session: &Session) -> Result<Vec<Tool>> {
        let mut tools = self.execution.inference_tools(session).await?;
        if enabled(self.execution.lease(session).await?.as_ref()) {
            tools.push(load_tools_tool()?);
        }
        Ok(tools)
    }

    async fn prompt_parts(
        &self,
        session: &Session,
        conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        let mut parts = self.execution.prompt_parts(session, conversation).await?;
        let lease = self.execution.lease(session).await?;
        if !enabled(&lease) {
            return Ok(parts);
        }
        let deferred = deferred_tools(&lease).await;
        let grouped = names_by_extension(&deferred);
        for (key, part) in &mut parts {
            if key == "extensions" {
                part.push_str("\n\n");
                part.push_str(TOOL_DISCOVERY_NOTE);
            } else if let Some(names) = key
                .strip_prefix("extension:")
                .and_then(|name| grouped.get(name))
            {
                part.push_str(&format!("\n\nDeferred tools: {}", names.join(", ")));
            }
        }
        Ok(parts)
    }

    async fn moim_parts(
        &self,
        session: &Session,
        conversation: &Conversation,
    ) -> Result<Vec<String>> {
        self.execution.moim_parts(session, conversation).await
    }

    async fn finalize_inference_input(
        &self,
        session: &Session,
        conversation: &Conversation,
        input: &mut InferenceInput,
    ) -> Result<()> {
        let lease = self.execution.lease(session).await?;
        if !enabled(&lease) {
            return Ok(());
        }
        let deferred = deferred_tools(&lease).await;
        let loaded = loaded_tools(conversation.messages());
        select_tools(&mut input.tools, &deferred, &loaded);
        Ok(())
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        let lease = self.session_lease(session);
        let deferred = match lease.as_deref().filter(|lease| enabled(lease)) {
            Some(lease) => deferred_tools(lease).await,
            None => Vec::new(),
        };
        let original_loaded = loaded_tools(conversation.messages());
        let mut loaded = original_loaded.clone();
        let mut response = Message::user();
        let mut pending = pending_tool_requests(messages);
        pending.sort_by_key(|(request, _)| {
            !request
                .tool_call
                .as_ref()
                .is_ok_and(|call| call.name == LOAD_TOOLS_TOOL_NAME)
        });
        for (request, disposition) in pending {
            let Ok(call) = &request.tool_call else {
                continue;
            };
            let advertised = request_was_advertised(messages, &request);
            let is_load = advertised && call.name == LOAD_TOOLS_TOOL_NAME;
            let not_sent = (!advertised)
                .then(|| deferred_tool_named(&call.name, &deferred))
                .flatten();
            if !is_load && not_sent.is_none() {
                continue;
            }
            let requesting = requesting_message(conversation.messages(), &request);
            let request_index = conversation
                .messages()
                .iter()
                .position(|message| {
                    requesting.is_some_and(|requesting| std::ptr::eq(message, requesting))
                })
                .unwrap_or(conversation.messages().len());
            let loaded_at_request = loaded_tools(&conversation.messages()[..request_index]);
            let span = tool_span(&call.name, &request.id, &session.id);
            let result = match disposition {
                ToolDisposition::ParseError(error) => {
                    CallToolResult::error(vec![ContentBlock::text(format!(
                        "Invalid arguments: {error}"
                    ))])
                }
                ToolDisposition::Execute if not_sent.is_some() => {
                    let name = not_sent.as_ref().unwrap();
                    let why = if loaded_at_request.contains(name) {
                        NotSent::LeftOutForLimit
                    } else if loaded.contains(name) {
                        NotSent::LoadedInSameResponse
                    } else {
                        NotSent::NotLoaded
                    };
                    CallToolResult::error(vec![ContentBlock::text(not_loaded_message(name, why))])
                }
                ToolDisposition::Execute if session.goose_mode == GooseMode::Chat => {
                    CallToolResult::success(vec![ContentBlock::text(
                        CHAT_MODE_TOOL_SKIPPED_RESPONSE,
                    )])
                }
                ToolDisposition::Execute => {
                    match (
                        lease.as_deref().filter(|lease| enabled(lease)),
                        advertised_tool_names(messages, &request),
                    ) {
                        (Some(lease), Some(sent)) => {
                            let result = Self::load(
                                lease,
                                &mut loaded,
                                call.arguments.as_ref(),
                                &sent,
                                &loaded_at_request,
                            )
                            .instrument(span.clone())
                            .await;
                            run_post_tool_call_hook(
                                self.post_tool_call_hook.as_ref(),
                                session,
                                conversation,
                                &request,
                                &ToolResponse {
                                    id: request.id.clone(),
                                    tool_result: Ok(result.clone()),
                                    metadata: request.metadata.clone(),
                                },
                            )
                            .instrument(span.clone())
                            .await;
                            result
                        }
                        (None, _) => CallToolResult::error(vec![ContentBlock::text(
                            EXPIRED_APPROVAL_RESPONSE,
                        )]),
                        (_, None) => CallToolResult::error(vec![ContentBlock::text(
                            "Load tools from a model response with an advertised tool catalog.",
                        )]),
                    }
                }
            };
            if result.is_error == Some(true) {
                span.record("error.type", "tool_error");
            }
            response.add_tool_response_with_metadata(
                request.id,
                Ok(result),
                request.metadata.as_ref(),
            );
        }
        if response.content.is_empty() {
            return self.execution.run(session, conversation, emit).await;
        }
        if loaded.len() > original_loaded.len() {
            self.set_message_meta(
                &mut response,
                LOADED_TOOLS_NOTE,
                json!(&loaded[original_loaded.len()..]),
            );
        }
        applied([emit.message(response).into()])
    }
}
