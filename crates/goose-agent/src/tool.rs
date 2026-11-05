#[cfg(not(feature = "std"))]
use crate::prelude::*;
use crate::runtime::{Cell as Mutex, Shared as Arc};
use alloc::collections::BTreeSet as HashSet;
use goose_provider_types::trace as tracing;

use anyhow::Result;
use async_trait::async_trait;
use goose_provider_types::conversation::{
    message::{Message, MessageContent, ToolRequest},
    Conversation,
};
use goose_provider_types::data::{
    CallToolRequestParams, CallToolResult, ErrorData, JsonObject, Tool,
};
#[cfg(feature = "mcp-interop")]
use rmcp::handler::server::router::tool::{AsyncTool, SyncTool, ToolBase};
#[cfg(feature = "mcp-interop")]
use serde_json::{json, Value};

use crate::operation::{
    applied, messages_since_kickoff, not_applicable, Emitter, Operation, OperationFuture,
    OperationResult,
};
use crate::post_tool_call_hook::PostToolCallHook;
use crate::tool_call_state::{tool_calls, ToolCallState};
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};

#[cfg(feature = "mcp-interop")]
fn definition<T: ToolBase>() -> Tool {
    let schema = T::input_schema().unwrap_or_else(|| {
        std::sync::Arc::new(
            serde_json::from_value(
                json!({"type":"object", "properties":{}, "additionalProperties":false}),
            )
            .unwrap(),
        )
    });
    let mut tool = rmcp::model::Tool::new_with_raw(T::name(), T::description(), schema);
    if let Some(title) = T::title() {
        tool = tool.with_title(title);
    }
    if let Some(schema) = T::output_schema() {
        tool = tool.with_raw_output_schema(schema);
    }
    if let Some(annotations) = T::annotations() {
        tool = tool.with_annotations(annotations);
    }
    if let Some(icons) = T::icons() {
        tool = tool.with_icons(icons);
    }
    if let Some(meta) = T::meta() {
        tool = tool.with_meta(meta);
    }
    tool.try_into()
        .expect("MCP tool definition matches GDK data")
}
fn pending_requests(requests: Vec<ToolRequest>, tool_names: &HashSet<&str>) -> Vec<ToolRequest> {
    requests
        .into_iter()
        .filter(|request| {
            request
                .tool_call
                .as_ref()
                .map_or(true, |call| tool_names.contains(call.name.as_ref()))
        })
        .collect()
}

#[cfg(feature = "mcp-interop")]
fn parameters<T: ToolBase>(arguments: Option<JsonObject>) -> Result<T::Parameter, ErrorData> {
    if T::input_schema().is_none() {
        return Ok(T::Parameter::default());
    }

    serde_json::from_value(Value::Object(arguments.unwrap_or_default())).map_err(|error| {
        ErrorData::invalid_params(format!("failed to deserialize parameters: {error}"), None)
    })
}

#[cfg(feature = "mcp-interop")]
fn result<T: ToolBase>(output: Result<T::Output, T::Error>) -> Result<CallToolResult, ErrorData> {
    let output = output.map_err(|error| {
        let error: rmcp::ErrorData = error.into();
        ErrorData::from(error)
    })?;
    let value = serde_json::to_value(output).map_err(|error| {
        ErrorData::internal_error(format!("failed to serialize tool output: {error}"), None)
    })?;
    Ok(CallToolResult::structured(value))
}

#[cfg(all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")))]
#[cfg(feature = "mcp-interop")]
async fn invoke_sync<S, T>(
    session: S,
    parameters: T::Parameter,
) -> Result<CallToolResult, ErrorData>
where
    S: Send + Sync + 'static,
    T: SyncTool<S> + 'static,
{
    tokio::task::spawn_blocking(move || result::<T>(T::invoke(&session, parameters)))
        .await
        .map_err(|error| {
            ErrorData::internal_error(format!("synchronous tool task failed: {error}"), None)
        })?
}

// wasm32 has no threads to move blocking work onto, so the tool runs inline
// and cancellation cannot interrupt it.
#[cfg(any(not(feature = "std"), feature = "local", target_arch = "wasm32"))]
#[cfg(feature = "mcp-interop")]
async fn invoke_sync<S, T>(
    session: S,
    parameters: T::Parameter,
) -> Result<CallToolResult, ErrorData>
where
    S: MaybeSend + MaybeSync + rmcp::service::MaybeSend + 'static,
    T: SyncTool<S>,
{
    result::<T>(T::invoke(&session, parameters))
}

/// A local asynchronous tool. Implementations own their JSON schema and use the embedder's peripherals directly.
#[cfg_attr(
    all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")),
    async_trait
)]
#[cfg_attr(any(not(feature = "std"), feature = "local", target_arch = "wasm32"), async_trait(?Send))]
pub trait LocalTool<S>: MaybeSend + MaybeSync {
    fn definition(&self) -> Tool;
    async fn call(&self, session: &S, arguments: JsonObject) -> Result<CallToolResult, ErrorData>;
}

/// Supplies tools whose definitions and implementations may vary by session.
///
/// A provider's tool names and their handlers must remain stable from an inference
/// advertisement until every tool call produced by that inference has been handled.
#[cfg_attr(
    all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")),
    async_trait
)]
#[cfg_attr(any(not(feature = "std"), feature = "local", target_arch = "wasm32"), async_trait(?Send))]
pub trait ToolProvider<S>: MaybeSend + MaybeSync {
    async fn tools(&self, session: &S) -> Result<Vec<Tool>>;

    async fn call(
        &self,
        session: &S,
        request_id: &str,
        call: CallToolRequestParams,
        emit: &Emitter,
    ) -> Result<CallToolResult, ErrorData>;
}

#[cfg(all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")))]
type ToolHandler<S> = dyn for<'a> Fn(&'a S, Option<JsonObject>) -> OperationFuture<'a, Result<CallToolResult, ErrorData>>
    + Send
    + Sync;

#[cfg(any(not(feature = "std"), feature = "local", target_arch = "wasm32"))]
type ToolHandler<S> = dyn for<'a> Fn(
    &'a S,
    Option<JsonObject>,
) -> OperationFuture<'a, Result<CallToolResult, ErrorData>>;

struct RegisteredTool<S> {
    definition: Tool,
    handler: Arc<ToolHandler<S>>,
}

struct RegisteredToolProvider<S> {
    tools: Vec<RegisteredTool<S>>,
}

#[cfg_attr(
    all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")),
    async_trait
)]
#[cfg_attr(any(not(feature = "std"), feature = "local", target_arch = "wasm32"), async_trait(?Send))]
impl<S> ToolProvider<S> for RegisteredToolProvider<S>
where
    S: MaybeSend + MaybeSync + 'static,
{
    async fn tools(&self, _session: &S) -> Result<Vec<Tool>> {
        Ok(self
            .tools
            .iter()
            .map(|tool| tool.definition.clone())
            .collect())
    }

    async fn call(
        &self,
        session: &S,
        _request_id: &str,
        call: CallToolRequestParams,
        _emit: &Emitter,
    ) -> Result<CallToolResult, ErrorData> {
        let tool = self
            .tools
            .iter()
            .find(|tool| tool.definition.name == call.name)
            .ok_or_else(|| {
                ErrorData::invalid_params(format!("unknown tool {}", call.name), None)
            })?;
        (tool.handler)(session, call.arguments).await
    }
}

/// An agent operation that advertises and dispatches tools.
///
/// Tools can be registered from rmcp's typed tool traits, or supplied at runtime
/// by a [`ToolProvider`] whose definitions may vary by session.
pub struct ToolOperation<S> {
    registered: RegisteredToolProvider<S>,
    providers: Vec<Arc<dyn ToolProvider<S>>>,
    post_tool_call_hook: Option<Arc<dyn PostToolCallHook<S>>>,
    tool_responses: Mutex<Option<Message>>,
}

impl<S> ToolOperation<S>
where
    S: MaybeSend + MaybeSync + 'static,
{
    pub fn new() -> Self {
        Self {
            registered: RegisteredToolProvider { tools: Vec::new() },
            providers: Vec::new(),
            post_tool_call_hook: None,
            tool_responses: Mutex::new(None),
        }
    }

    fn register(&mut self, tool: RegisteredTool<S>) {
        if let Some(existing) = self
            .registered
            .tools
            .iter_mut()
            .find(|existing| existing.definition.name == tool.definition.name)
        {
            *existing = tool;
        } else {
            self.registered.tools.push(tool);
        }
    }

    pub fn with_local_tool<T: LocalTool<S> + 'static>(mut self, tool: T) -> Self {
        let definition = tool.definition();
        let tool = Arc::new(tool);
        self.register(RegisteredTool {
            definition,
            handler: Arc::new(move |session, arguments| {
                let tool = tool.clone();
                Box::pin(async move { tool.call(session, arguments.unwrap_or_default()).await })
            }),
        });
        self
    }

    pub fn with_provider(mut self, provider: Arc<dyn ToolProvider<S>>) -> Self {
        self.providers.push(provider);
        self
    }

    pub fn with_post_tool_call_hook(mut self, hook: Arc<dyn PostToolCallHook<S>>) -> Self {
        self.post_tool_call_hook = Some(hook);
        self
    }

    #[cfg(feature = "mcp-interop")]
    pub fn with_sync_tool<T>(mut self) -> Self
    where
        S: Clone + rmcp::service::MaybeSend,
        T: SyncTool<S> + MaybeSend + MaybeSync + 'static,
    {
        self.register(RegisteredTool {
            definition: definition::<T>(),
            handler: Arc::new(|session, arguments| {
                let session = session.clone();
                Box::pin(async move {
                    let parameters = parameters::<T>(arguments)?;
                    invoke_sync::<S, T>(session, parameters).await
                })
            }),
        });
        self
    }

    #[cfg(feature = "mcp-interop")]
    pub fn with_async_tool<T>(mut self) -> Self
    where
        S: rmcp::service::MaybeSend,
        T: AsyncTool<S> + MaybeSend + MaybeSync + 'static,
    {
        self.register(RegisteredTool {
            definition: definition::<T>(),
            handler: Arc::new(|session, arguments| {
                Box::pin(async move {
                    let parameters = parameters::<T>(arguments)?;
                    result::<T>(T::invoke(session, parameters).await)
                })
            }),
        });
        self
    }

    async fn available_tools(&self, session: &S) -> Result<Vec<(Tool, &dyn ToolProvider<S>)>> {
        let mut available = self
            .registered
            .tools(session)
            .await?
            .into_iter()
            .map(|tool| (tool, &self.registered as &dyn ToolProvider<S>))
            .collect::<Vec<_>>();
        for provider in &self.providers {
            for tool in provider.tools(session).await? {
                if available
                    .iter()
                    .any(|(available, _)| available.name == tool.name)
                {
                    anyhow::bail!("multiple tool providers registered '{}'", tool.name);
                }
                available.push((tool, provider.as_ref()));
            }
        }
        Ok(available)
    }
}

impl<S> Default for ToolOperation<S>
where
    S: MaybeSend + MaybeSync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(
    all(feature = "std", not(feature = "local"), not(target_arch = "wasm32")),
    async_trait
)]
#[cfg_attr(any(not(feature = "std"), feature = "local", target_arch = "wasm32"), async_trait(?Send))]
impl<S, E> Operation<S, E> for ToolOperation<S>
where
    S: MaybeSend + MaybeSync + 'static,
    E: From<Message> + MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        "tools"
    }

    async fn finalize_cancellation(
        &self,
        _session: &S,
        _conversation: &Conversation,
        emit: &Emitter,
    ) -> Vec<E> {
        let tool_responses = self.tool_responses.borrow_mut().take();
        match tool_responses {
            Some(tool_responses) => vec![E::from(emit.message(tool_responses))],
            None => Vec::new(),
        }
    }

    async fn inference_tools(&self, session: &S) -> Result<Vec<Tool>> {
        Ok(self
            .available_tools(session)
            .await?
            .into_iter()
            .map(|(tool, _)| tool)
            .collect())
    }

    async fn run(
        &self,
        session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let turn = messages_since_kickoff(conversation)?;
        let pending = tool_calls(turn)
            .into_iter()
            .filter(|(request, state)| {
                !request.was_executed_externally()
                    && (*state == ToolCallState::Executable
                        || (*state == ToolCallState::Unchecked && request.tool_call.is_err()))
            })
            .map(|(request, _)| request.clone())
            .collect::<Vec<_>>();
        if pending.is_empty() {
            return not_applicable();
        }

        let available = self.available_tools(session).await?;
        let available_names = available
            .iter()
            .map(|(tool, _)| tool.name.as_ref())
            .collect();
        let pending = pending_requests(pending, &available_names);
        if pending.is_empty() {
            return not_applicable();
        }

        for request in pending {
            let provider = match request.tool_call.as_ref() {
                Ok(call) => available
                    .iter()
                    .find(|(tool, _)| tool.name == call.name)
                    .map(|(_, provider)| *provider)
                    .expect("pending requests were filtered by available tools"),
                Err(_) => &self.registered as &dyn ToolProvider<S>,
            };
            let tool_result = match request.tool_call.as_ref() {
                Err(error) => Err(error.clone()),
                Ok(call) => {
                    provider
                        .call(session, &request.id, call.clone(), emit)
                        .await
                }
            };
            let tool_response = {
                let mut tool_responses = self.tool_responses.borrow_mut();
                let tool_responses = tool_responses.get_or_insert_with(|| {
                    Message::user_with_host(emit.host())
                        .with_generated_id_if_missing_with_host(emit.host())
                });
                tool_responses.add_tool_response_with_metadata(
                    request.id.clone(),
                    tool_result,
                    request.metadata.as_ref(),
                );
                tool_responses
                    .content
                    .last()
                    .and_then(MessageContent::as_tool_response)
                    .cloned()
                    .expect("tool response just added")
            };
            if let (Ok(_), Some(hook)) = (&request.tool_call, &self.post_tool_call_hook) {
                if let Err(error) = hook
                    .after_call(session, conversation, &request, &tool_response)
                    .await
                {
                    tracing::warn!(%error, tool_call_id = %request.id, "post tool call hook failed");
                }
            }
        }
        let tool_responses = self
            .tool_responses
            .borrow_mut()
            .take()
            .expect("tool responses retained");
        applied([E::from(emit.message(tool_responses))])
    }
}
