use anyhow::Result;
use async_trait::async_trait;
use oi_kit_provider_types::conversation::message::{Message, ToolRequest};
use oi_kit_provider_types::conversation::Conversation;
use rmcp::model::{CallToolRequestParams, Tool};

use super::{
    get_tool_owner, recover_mangled_tool_name, CallRequest, ExtensionLease, ExtensionSpec,
    ToolCallBatch, ToolCallResult,
};
use crate::operation::{
    applied, messages_since_kickoff, not_applicable, Emitter, Operation, OperationResult,
};
use crate::tool_call_state::pending_tool_requests;
use crate::tool_visibility::is_tool_visible_to_model;

/// Supplies the extension lease a session runs with, and shapes how its tools
/// are offered and called.
#[async_trait]
pub trait ExtensionSource<S: Sync, C: ExtensionSpec>: Send + Sync {
    /// The lease to advertise to the next inference request, typically
    /// [`ExtensionManager::resolve`](super::ExtensionManager::resolve) of the
    /// session's selection.
    async fn advertise(&self, session: &S) -> Result<ExtensionLease<C>>;

    /// The lease the pending calls were advertised from. `None` leaves the
    /// calls to later operations, for example when that lease is gone and an
    /// approval should not run against a newer selection.
    async fn advertised(
        &self,
        session: &S,
        _conversation: &Conversation,
    ) -> Result<Option<ExtensionLease<C>>> {
        self.advertise(session).await.map(Some)
    }

    /// The lease tools offered to the model and accepted from it. Tools left
    /// out here are neither advertised nor run.
    async fn tools(&self, _session: &S, lease: &ExtensionLease<C>) -> Result<Vec<Tool>> {
        Ok(lease.tools().await)
    }

    /// Whether a pending call may run. `messages` are the turn's messages.
    fn accepts(&self, _messages: &[Message], _request: &ToolRequest) -> bool {
        true
    }

    /// Wraps a started call, for example to trace it or post-process its result.
    fn decorate(
        &self,
        _session: &S,
        _request: &ToolRequest,
        result: ToolCallResult,
    ) -> ToolCallResult {
        result
    }

    /// System prompt parts describing the lease's extensions.
    async fn prompt_parts(
        &self,
        _session: &S,
        lease: &ExtensionLease<C>,
    ) -> Result<Vec<(String, String)>> {
        let mut extensions = lease.instructions().await;
        extensions.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(extensions
            .into_iter()
            .filter(|extension| !extension.instructions.is_empty())
            .map(|extension| {
                (
                    format!("extension:{}", extension.name),
                    format!("## {}\n\n{}", extension.name, extension.instructions),
                )
            })
            .collect())
    }
}

/// Advertises the tools of a session's MCP extensions and runs the calls the
/// model makes to them.
///
/// If the machine uses an approval policy, place it before this operation.
/// Calls already answered or belonging to another operation are skipped.
pub struct ExtensionOperation<S: Sync, C: ExtensionSpec> {
    source: std::sync::Arc<dyn ExtensionSource<S, C>>,
    batch: ToolCallBatch,
}

impl<S: Sync, C: ExtensionSpec> ExtensionOperation<S, C> {
    pub fn new(source: std::sync::Arc<dyn ExtensionSource<S, C>>) -> Self {
        Self {
            source,
            batch: ToolCallBatch::new(),
        }
    }
}

fn owned_call(request: &ToolRequest, tools: &[Tool]) -> Option<CallToolRequestParams> {
    let mut call = request.tool_call.as_ref().ok()?.clone();
    if tools.iter().any(|tool| tool.name == call.name) {
        return Some(call);
    }
    let owners: Vec<_> = tools.iter().map(get_tool_owner).collect();
    let recovered = recover_mangled_tool_name(
        &call.name,
        tools
            .iter()
            .zip(&owners)
            .map(|(tool, owner)| (tool.name.as_ref(), owner.as_deref())),
    )?;
    call.name = recovered.into();
    Some(call)
}

fn emit_response(emit: &Emitter, response: Message) -> Message {
    let response = response.with_generated_id_if_missing();
    emit.emit(crate::events::AgentEvent::Message(
        response.user_visible_content(),
    ));
    response
}

#[async_trait]
impl<S, C, E> Operation<S, E> for ExtensionOperation<S, C>
where
    S: Send + Sync + 'static,
    C: ExtensionSpec,
    E: From<Message> + Send + 'static,
{
    fn name(&self) -> &'static str {
        "extensions"
    }

    async fn finalize_cancellation(
        &self,
        _session: &S,
        _conversation: &Conversation,
        emit: &Emitter,
    ) -> Vec<E> {
        let (actions, response) = self.batch.take();
        actions
            .into_iter()
            .chain(response.map(|response| emit_response(emit, response)))
            .map(E::from)
            .collect()
    }

    async fn inference_tools(&self, session: &S) -> Result<Vec<Tool>> {
        let lease = self.source.advertise(session).await?;
        let tools = self.source.tools(session, &lease).await?;
        Ok(tools.into_iter().filter(is_tool_visible_to_model).collect())
    }

    async fn prompt_parts(
        &self,
        session: &S,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        let lease = self.source.advertise(session).await?;
        self.source.prompt_parts(session, &lease).await
    }

    async fn run(
        &self,
        session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let messages = messages_since_kickoff(conversation)?;
        let requests: Vec<_> = pending_tool_requests(messages)
            .filter(|request| request.tool_call.is_ok() && self.source.accepts(messages, request))
            .cloned()
            .collect();
        if requests.is_empty() {
            return not_applicable();
        }
        let Some(lease) = self.source.advertised(session, conversation).await? else {
            return not_applicable();
        };
        let tools = self.source.tools(session, &lease).await?;
        let owned: Vec<_> = requests
            .into_iter()
            .filter_map(|request| owned_call(&request, &tools).map(|call| (request, call)))
            .collect();
        if owned.is_empty() {
            return not_applicable();
        }

        let mut calls = Vec::with_capacity(owned.len());
        for (request, call) in owned {
            let result = lease
                .call(
                    call,
                    CallRequest::new(request.id.clone()),
                    emit.cancel_token().clone(),
                )
                .await
                .unwrap_or_else(|error| ToolCallResult::from(Err(error)));
            let result = self.source.decorate(session, &request, result);
            calls.push((request, result));
        }
        let (actions, response) = self.batch.run(calls, emit).await?;
        applied(
            actions
                .into_iter()
                .chain([emit_response(emit, response)])
                .map(E::from),
        )
    }
}
