//! Offers `load_tools` to sessions that defer tools and records what it loads.

use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use goose_agent::post_tool_call_hook::PostToolCallHook;
use rmcp::model::{CallToolResult, ContentBlock, JsonObject, Tool, ToolAnnotations};
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use serde_json::Value;
use tracing_futures::Instrument;

use crate::agents::extension_manager::ExtensionLease;
use crate::agents::reply_parts::is_tool_visible_to_model;
use crate::agents::state_machine::ops_toolcalling::{
    pending_advertised_tool_requests, tool_span, ToolDisposition, EXPIRED_APPROVAL_RESPONSE,
};
use crate::agents::state_machine::post_tool_hook::run_post_tool_call_hook;
use crate::agents::state_machine::{
    applied, messages_since_kickoff, not_applicable, Emitter, GooseEffect, Operation,
    OperationResult,
};
use crate::agents::tool_discovery::{defers_tools, resolve_load, LOAD_TOOLS_TOOL_NAME};
use crate::agents::tool_execution::CHAT_MODE_TOOL_SKIPPED_RESPONSE;
use crate::config::GooseMode;
use crate::conversation::message::{Message, ToolResponse};
use crate::conversation::Conversation;
use crate::session::{Session, ToolDiscoveryState};

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
        "Load extension tools that are listed as not loaded in your system instructions. \
         A loaded tool becomes callable in your next response, so don't call it in the \
         same response that loads it.",
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

fn parse_tool_names(arguments: Option<&JsonObject>) -> Result<Vec<String>, String> {
    let arguments = arguments.ok_or_else(|| "Missing arguments".to_string())?;
    serde_json::from_value::<LoadToolsParams>(Value::Object(arguments.clone()))
        .map(|params| params.tool_names)
        .map_err(|error| format!("Invalid arguments: {error}"))
}

pub struct ToolDiscoveryOperation {
    extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
    post_tool_call_hook: Arc<dyn PostToolCallHook<Session>>,
}

impl ToolDiscoveryOperation {
    pub fn new(
        extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
        post_tool_call_hook: Arc<dyn PostToolCallHook<Session>>,
    ) -> Self {
        Self {
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
        state: &mut ToolDiscoveryState,
        arguments: Option<&JsonObject>,
    ) -> CallToolResult {
        let requested = match parse_tool_names(arguments) {
            Ok(requested) => requested,
            Err(error) => return CallToolResult::error(vec![ContentBlock::text(error)]),
        };
        let mut available = lease.tools_excluding(crate::skills::EXTENSION_NAME).await;
        available.retain(is_tool_visible_to_model);
        let deferrable = lease.deferrable_tools().await;
        let (outcome, newly_loaded) =
            resolve_load(&requested, &available, &deferrable, &state.loaded_tools);
        state.loaded_tools.extend(newly_loaded);
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
        "tool_discovery"
    }

    async fn inference_tools(&self, session: &Session) -> Result<Vec<Tool>> {
        let defers = self
            .session_lease(session)
            .is_some_and(|lease| defers_tools(session, &lease));
        Ok(if defers {
            vec![load_tools_tool()?]
        } else {
            Vec::new()
        })
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        let pending: Vec<_> = pending_advertised_tool_requests(messages)
            .into_iter()
            .filter(|(request, _)| {
                request
                    .tool_call
                    .as_ref()
                    .is_ok_and(|tool_call| tool_call.name.as_ref() == LOAD_TOOLS_TOOL_NAME)
            })
            .collect();
        if pending.is_empty() {
            return not_applicable();
        }

        let lease = self.session_lease(session);
        let original_state = ToolDiscoveryState::of(session);
        let mut state = original_state.clone();
        let mut response = Message::user();
        for (request, disposition) in pending {
            let result = match (disposition, lease.as_deref()) {
                (ToolDisposition::ParseError(error), _) => {
                    CallToolResult::error(vec![ContentBlock::text(format!(
                        "The tool call could not be parsed: {error}. Correct the arguments and try again."
                    ))])
                }
                (ToolDisposition::Execute, _) if session.goose_mode == GooseMode::Chat => {
                    CallToolResult::success(vec![ContentBlock::text(
                        CHAT_MODE_TOOL_SKIPPED_RESPONSE,
                    )])
                }
                (ToolDisposition::Execute, None) => {
                    CallToolResult::error(vec![ContentBlock::text(EXPIRED_APPROVAL_RESPONSE)])
                }
                (ToolDisposition::Execute, Some(lease)) => {
                    let tool_call = request.tool_call.as_ref().map_err(|error| {
                        anyhow!("load_tools tool call could not be parsed: {error}")
                    })?;
                    let span = tool_span(&tool_call.name, &request.id, &session.id);
                    let result = Self::load(lease, &mut state, tool_call.arguments.as_ref())
                        .instrument(span.clone())
                        .await;
                    if result.is_error == Some(true) {
                        span.record("error.type", "tool_error");
                    }
                    let tool_response = ToolResponse {
                        id: request.id.clone(),
                        tool_result: Ok(result.clone()),
                        metadata: request.metadata.clone(),
                    };
                    run_post_tool_call_hook(
                        self.post_tool_call_hook.as_ref(),
                        session,
                        conversation,
                        &request,
                        &tool_response,
                    )
                    .instrument(span)
                    .await;
                    result
                }
            };
            response.add_tool_response_with_metadata(
                request.id,
                Ok(result),
                request.metadata.as_ref(),
            );
        }

        let mut effects = Vec::new();
        if state != original_state {
            effects.push(GooseEffect::extension_state(&state)?);
        }
        effects.push(emit.message(response).into());
        applied(effects)
    }
}
