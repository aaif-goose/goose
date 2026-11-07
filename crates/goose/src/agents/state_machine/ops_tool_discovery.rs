//! Offers `search_tools` and `load_tools` to sessions that defer tools and
//! records what `load_tools` loads.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use goose_agent::post_tool_call_hook::PostToolCallHook;
use rmcp::model::{CallToolResult, ContentBlock, JsonObject, Tool, ToolAnnotations};
use schemars::{schema_for, JsonSchema};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;
use tracing_futures::Instrument;

use crate::agents::extension_manager::ExtensionLease;
use crate::agents::reply_parts::is_tool_visible_to_model;
use crate::agents::state_machine::ops_toolcalling::{
    advertised_tool_count, pending_advertised_tool_requests, tool_span, ToolDisposition,
    EXPIRED_APPROVAL_RESPONSE,
};
use crate::agents::state_machine::post_tool_hook::run_post_tool_call_hook;
use crate::agents::state_machine::{
    applied, messages_since_kickoff, not_applicable, Emitter, GooseEffect, Operation,
    OperationResult,
};
use crate::agents::tool_discovery::search::{search_tools, DEFAULT_SEARCH_LIMIT, MAX_SEARCH_LIMIT};
use crate::agents::tool_discovery::{
    defers_tools, resolve_load, LOAD_TOOLS_TOOL_NAME, SEARCH_TOOLS_TOOL_NAME,
};
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

#[derive(Deserialize, JsonSchema)]
struct SearchToolsParams {
    /// A few words describing the tool you need, or its exact name.
    query: String,
    /// Maximum number of results, from 1 to 50.
    #[serde(default = "default_search_limit")]
    limit: usize,
}

fn default_search_limit() -> usize {
    DEFAULT_SEARCH_LIMIT
}

fn discovery_tool(
    name: &'static str,
    title: &str,
    description: &'static str,
    schema: schemars::Schema,
) -> Result<Tool> {
    let schema = serde_json::to_value(schema)?
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("{name} schema is not an object"))?;
    Ok(
        Tool::new(name, description, schema).annotate(ToolAnnotations::from_raw(
            Some(title.to_string()),
            Some(true),
            Some(false),
            Some(true),
            Some(false),
        )),
    )
}

fn discovery_tools() -> Result<Vec<Tool>> {
    Ok(vec![
        discovery_tool(
            SEARCH_TOOLS_TOOL_NAME,
            "Search tools",
            "Search the deferred tools, which are omitted from tools[] until loaded, by name \
             and description. Returns the best matches with a short description and whether \
             each is loaded. Load the ones you need with load_tools.",
            schema_for!(SearchToolsParams),
        )?,
        discovery_tool(
            LOAD_TOOLS_TOOL_NAME,
            "Load tools",
            "Load deferred tools found with search_tools. A loaded tool's schema appears in \
             tools[] on your next request, so don't call it in the same response that loads it.",
            schema_for!(LoadToolsParams),
        )?,
    ])
}

fn parse_arguments<T: DeserializeOwned>(arguments: Option<&JsonObject>) -> Result<T, String> {
    let arguments = arguments.ok_or_else(|| "Missing arguments".to_string())?;
    serde_json::from_value(Value::Object(arguments.clone()))
        .map_err(|error| format!("Invalid arguments: {error}"))
}

/// Slots for the tools operations add to a request (`load_skill`,
/// `search_tools`, `load_tools`, a recipe's `final_output`) when the
/// request's own count wasn't recorded.
const RESERVED_OPERATION_TOOLS: usize = 4;

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

    /// `advertised` is how many tools the inference that called `load_tools`
    /// sent, when it recorded them.
    async fn load(
        lease: &ExtensionLease,
        state: &mut ToolDiscoveryState,
        arguments: Option<&JsonObject>,
        advertised: Option<usize>,
    ) -> CallToolResult {
        let requested = match parse_arguments::<LoadToolsParams>(arguments) {
            Ok(params) => params.tool_names,
            Err(error) => return CallToolResult::error(vec![ContentBlock::text(error)]),
        };
        let mut available = lease.tools_excluding(crate::skills::EXTENSION_NAME).await;
        available.retain(is_tool_visible_to_model);
        let deferrable = lease.deferrable_tools().await;
        let advertised = advertised.unwrap_or_else(|| {
            let still_deferred = deferrable
                .iter()
                .filter(|tool| !state.loaded_tools.iter().any(|name| name == &*tool.name))
                .count();
            available.len() - still_deferred + RESERVED_OPERATION_TOOLS
        });
        let (outcome, newly_loaded) = resolve_load(
            &requested,
            &available,
            &deferrable,
            &state.loaded_tools,
            advertised,
        );
        state.loaded_tools.extend(newly_loaded);
        let text = serde_json::to_string(&outcome).expect("load outcome serializes");
        if outcome.loaded.is_empty() {
            CallToolResult::error(vec![ContentBlock::text(text)])
        } else {
            CallToolResult::success(vec![ContentBlock::text(text)])
        }
    }

    async fn search(
        lease: &ExtensionLease,
        state: &ToolDiscoveryState,
        arguments: Option<&JsonObject>,
    ) -> CallToolResult {
        let params = match parse_arguments::<SearchToolsParams>(arguments) {
            Ok(params) => params,
            Err(error) => return CallToolResult::error(vec![ContentBlock::text(error)]),
        };
        if params.query.trim().is_empty() {
            return CallToolResult::error(vec![ContentBlock::text("query must not be empty")]);
        }
        let limit = params.limit.clamp(1, MAX_SEARCH_LIMIT);
        let deferrable = lease.deferrable_tools().await;
        let summaries: HashMap<String, String> = lease
            .instructions()
            .await
            .into_iter()
            .map(|extension| (extension.name, extension.instructions))
            .collect();
        let hits = search_tools(
            &params.query,
            limit,
            &deferrable,
            &summaries,
            &state.loaded_tools,
        );
        let text = if hits.is_empty() {
            format!(
                "No tools matched \"{}\". Try other words, or load all of an extension's tools \
                 with `<extension>__*`.",
                params.query
            )
        } else {
            serde_json::to_string(&hits).expect("search hits serialize")
        };
        CallToolResult::success(vec![ContentBlock::text(text)])
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
        if defers {
            discovery_tools()
        } else {
            Ok(Vec::new())
        }
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
                request.tool_call.as_ref().is_ok_and(|tool_call| {
                    matches!(
                        tool_call.name.as_ref(),
                        LOAD_TOOLS_TOOL_NAME | SEARCH_TOOLS_TOOL_NAME
                    )
                })
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
                        anyhow!("tool discovery call could not be parsed: {error}")
                    })?;
                    let span = tool_span(&tool_call.name, &request.id, &session.id);
                    let arguments = tool_call.arguments.as_ref();
                    let result = if tool_call.name == SEARCH_TOOLS_TOOL_NAME {
                        Self::search(lease, &state, arguments)
                            .instrument(span.clone())
                            .await
                    } else {
                        let loaded_this_batch =
                            state.loaded_tools.len() - original_state.loaded_tools.len();
                        let advertised = advertised_tool_count(messages, &request)
                            .map(|count| count + loaded_this_batch);
                        Self::load(lease, &mut state, arguments, advertised)
                            .instrument(span.clone())
                            .await
                    };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fallback_budget_reserves_a_slot_for_every_operation_tool() {
        let load_skill_and_final_output = 2;
        assert_eq!(
            RESERVED_OPERATION_TOOLS,
            discovery_tools().unwrap().len() + load_skill_and_final_output
        );
    }
}
