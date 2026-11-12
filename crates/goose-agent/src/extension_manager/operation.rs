//! Running a turn's tool calls against extension leases.
//!
//! [`TurnLease`] holds the lease the latest inference advertised, so tool requests run
//! against the extensions that offered them. [`ToolCallBatch`] runs calls concurrently and
//! collects one response message. [`ExtensionOperation`] composes them into an
//! [`Operation`] using the shared tool-approval state.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use futures::{FutureExt, StreamExt};
use goose_provider_types::conversation::message::{
    Message, ProviderMetadata, ToolRequest, ToolResponse,
};
use goose_provider_types::conversation::Conversation;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData, Tool};
use tracing_futures::Instrument;

use super::{get_tool_owner, recover_mangled_tool_name, CallRequest, ExtensionLease};
use crate::events::AgentEvent;
use crate::operation::{
    applied, messages_since_kickoff, not_applicable, Emitter, Operation, OperationResult,
};
use crate::tool_execution::ToolCallResult;
use crate::tool_visibility::is_tool_visible_to_model;

/// The answer to a tool request whose lease is gone, so its tool may now belong to
/// different extensions.
pub const EXPIRED_TOOL_REQUEST: &str =
    "This tool request's extension lease is no longer available. Request the tool again.";

/// The lease the turn's latest inference advertised. Clones share it.
#[derive(Clone, Default)]
pub struct TurnLease(Arc<Mutex<Option<Arc<ExtensionLease>>>>);

impl TurnLease {
    pub fn get(&self) -> Option<Arc<ExtensionLease>> {
        self.0.lock().expect("turn lease poisoned").clone()
    }

    /// The lease, if it is for `scope_id`.
    pub fn for_scope(&self, scope_id: &str) -> Option<Arc<ExtensionLease>> {
        self.get().filter(|lease| lease.scope_id() == scope_id)
    }

    pub fn set(&self, lease: impl Into<Arc<ExtensionLease>>) -> Arc<ExtensionLease> {
        let lease = lease.into();
        *self.0.lock().expect("turn lease poisoned") = Some(Arc::clone(&lease));
        lease
    }

    /// Requests made under the cleared lease are expired from then on.
    pub fn clear(&self) {
        *self.0.lock().expect("turn lease poisoned") = None;
    }
}

/// A started tool call and where its response goes.
pub struct PendingCall {
    pub request_id: String,
    pub metadata: Option<ProviderMetadata>,
    pub call: ToolCallResult,
}

#[derive(Default)]
struct BatchState {
    actions: Vec<Message>,
    response: Option<Message>,
}

/// Collects one response message for a batch of tool calls, plus the action-required
/// messages they raised. Results are recorded as each call finishes, so a cancelled turn
/// can still answer the calls that completed. Clones share the batch.
#[derive(Clone, Default)]
pub struct ToolCallBatch(Arc<Mutex<BatchState>>);

impl ToolCallBatch {
    /// Record a response; the first one recorded for a request wins.
    pub fn record(
        &self,
        request_id: &str,
        result: Result<CallToolResult, ErrorData>,
        metadata: Option<&ProviderMetadata>,
    ) {
        let mut state = self.0.lock().expect("tool call batch poisoned");
        let response = state
            .response
            .get_or_insert_with(|| Message::user().with_generated_id_if_missing());
        if !response.get_tool_response_ids().contains(&request_id) {
            response.add_tool_response_with_metadata(request_id, result, metadata);
        }
    }

    /// Drive the calls concurrently. MCP notifications are emitted as
    /// [`AgentEvent::McpNotification`], including any that arrive with the result;
    /// action-required messages are emitted and kept for [`Self::take`]. `on_result` sees
    /// each result before it is recorded.
    pub async fn run(
        &self,
        calls: Vec<PendingCall>,
        emit: &Emitter,
        on_result: &(dyn Fn(&str, &Result<CallToolResult, ErrorData>) + Send + Sync),
    ) {
        futures::future::join_all(
            calls
                .into_iter()
                .map(|call| self.drive(call, emit, on_result)),
        )
        .await;
    }

    async fn drive(
        &self,
        pending: PendingCall,
        emit: &Emitter,
        on_result: &(dyn Fn(&str, &Result<CallToolResult, ErrorData>) + Send + Sync),
    ) {
        let PendingCall {
            request_id,
            metadata,
            call,
        } = pending;
        let mut notifications = call
            .notification_stream
            .unwrap_or_else(|| Box::new(futures::stream::empty()));
        let mut actions = call
            .action_required_stream
            .unwrap_or_else(|| Box::new(futures::stream::empty()));
        let mut result = call.result;
        let notify = |notification| {
            emit.emit(AgentEvent::McpNotification((
                request_id.clone(),
                notification,
            )))
        };
        let act = |action: Message| {
            let action = emit.message(action);
            self.0
                .lock()
                .expect("tool call batch poisoned")
                .actions
                .push(action);
        };
        let output = loop {
            tokio::select! {
                Some(action) = actions.next() => act(action),
                Some(notification) = notifications.next() => notify(notification),
                output = &mut result => break output,
            }
        };
        // Servers may send a notification just before the result, but clients handle
        // notifications on their own tasks, so it can be delivered after.
        while let Some(Some(notification)) = notifications.next().now_or_never() {
            notify(notification);
        }
        while let Some(Some(action)) = actions.next().now_or_never() {
            act(action);
        }
        on_result(&request_id, &output);
        self.record(&request_id, output, metadata.as_ref());
    }

    /// The action-required messages and the response, leaving the batch empty.
    pub fn take(&self) -> (Vec<Message>, Option<Message>) {
        let mut state = self.0.lock().expect("tool call batch poisoned");
        (std::mem::take(&mut state.actions), state.response.take())
    }
}

/// How an [`ExtensionOperation`] finds a session's extensions.
#[async_trait]
pub trait LeaseResolver<S>: Send + Sync {
    async fn resolve(&self, session: &S) -> Result<ExtensionLease>;
}

/// Application policy around the shared extension execution lifecycle.
///
/// The operation owns dispatch, response recording and cancellation. Hosts can
/// customize advertisement and decorate calls without implementing another executor.
#[async_trait]
pub trait ExtensionOperationHost<S: Sync>: LeaseResolver<S> {
    fn name(&self) -> &'static str {
        "extensions"
    }

    async fn tools(&self, _session: &S, lease: &ExtensionLease) -> Result<Vec<Tool>> {
        Ok(lease
            .tools()
            .await
            .into_iter()
            .filter(is_tool_visible_to_model)
            .collect())
    }

    /// Tools routed to this operation, which may be broader than the advertised
    /// subset when a host defers schemas. Hosts must still validate advertisement.
    async fn execution_tools(&self, _session: &S, lease: &ExtensionLease) -> Result<Vec<Tool>> {
        Ok(lease.tools().await)
    }

    /// Claim expired requests without starting replacement extensions.
    fn claims_expired_request(&self, _messages: &[Message], _request: &ToolRequest) -> bool {
        false
    }

    fn call_span(&self, _session: &S, _request: &ToolRequest) -> tracing::Span {
        tracing::Span::none()
    }

    fn dispatch_error(&self, _request: &ToolRequest, _error: &ErrorData) {}

    async fn prompt_parts(
        &self,
        _session: &S,
        _conversation: &Conversation,
        lease: &ExtensionLease,
    ) -> Result<Vec<(String, String)>> {
        let prompt = instructions_prompt(&lease.instructions().await);
        Ok(if prompt.is_empty() {
            Vec::new()
        } else {
            vec![("extensions".to_string(), prompt)]
        })
    }

    /// Reject requests before dispatch, hooks or fresh lease resolution.
    fn accepts_request(&self, _messages: &[Message], _request: &ToolRequest) -> bool {
        true
    }

    /// Explicitly allow generated requests to acquire a lease without inference.
    /// This must never authorize model requests whose advertised lease has expired.
    fn can_resolve_for_requests(&self, _messages: &[Message], _requests: &[&ToolRequest]) -> bool {
        false
    }

    fn expired_result(&self) -> CallToolResult {
        CallToolResult::error(vec![ContentBlock::text(EXPIRED_TOOL_REQUEST)])
    }

    fn skipped_result(&self, _session: &S) -> Option<CallToolResult> {
        None
    }

    /// Return a denial to answer the request without dispatch or post-call hooks.
    async fn before_call(
        &self,
        _session: &S,
        _lease: &ExtensionLease,
        _request: &ToolRequest,
    ) -> Option<CallToolResult> {
        None
    }

    /// Return a result for a host-provided tool, or dispatch through the lease.
    async fn call_override(
        &self,
        _session: &S,
        _lease: &ExtensionLease,
        _request: &ToolRequest,
    ) -> Option<ToolCallResult> {
        None
    }

    /// Transform the result before it is recorded or passed to post-call hooks.
    fn decorate_call(
        &self,
        _session: &S,
        _lease: &ExtensionLease,
        _request: &ToolRequest,
        call: ToolCallResult,
    ) -> ToolCallResult {
        call
    }

    /// Runs after the transformed response is recorded for cancellation recovery.
    async fn after_call(
        &self,
        _session: &S,
        _conversation: &Conversation,
        _lease: &ExtensionLease,
        _request: &ToolRequest,
        _response: &ToolResponse,
    ) {
    }

    fn on_result(
        &self,
        _request_id: &str,
        _result: &Result<CallToolResult, ErrorData>,
        _emit: &Emitter,
    ) {
    }

    fn emit_response(&self, response: Message, emit: &Emitter) -> Message {
        emit.message(response)
    }
}

struct DefaultHost<S>(Arc<dyn LeaseResolver<S>>);

#[async_trait]
impl<S: Send + Sync> LeaseResolver<S> for DefaultHost<S> {
    async fn resolve(&self, session: &S) -> Result<ExtensionLease> {
        self.0.resolve(session).await
    }
}

#[async_trait]
impl<S: Send + Sync> ExtensionOperationHost<S> for DefaultHost<S> {}

/// Advertises extension capabilities and executes approved requests against the
/// lease that advertised them. Build one per turn, after `ToolApprovalOperation`.
pub struct ExtensionOperation<S> {
    host: Arc<dyn ExtensionOperationHost<S>>,
    lease: TurnLease,
    batch: ToolCallBatch,
    shared_snapshot: bool,
}

impl<S: Send + Sync + 'static> ExtensionOperation<S> {
    pub fn new(resolver: Arc<dyn LeaseResolver<S>>) -> Self {
        Self::with_host(Arc::new(DefaultHost(resolver)))
    }

    pub fn with_host(host: Arc<dyn ExtensionOperationHost<S>>) -> Self {
        Self {
            host,
            lease: TurnLease::default(),
            batch: ToolCallBatch::default(),
            shared_snapshot: false,
        }
    }

    /// Use the lease prepared together with the host's inference session snapshot.
    /// The host must replace this lease before collecting each inference's input.
    pub fn with_inference_lease(mut self, lease: TurnLease) -> Self {
        self.lease = lease;
        self.shared_snapshot = true;
        self
    }

    fn finish<E: From<Message>>(&self, emit: &Emitter) -> Result<OperationResult<E>> {
        let (actions, response) = self.batch.take();
        let response = response.expect("each call records a response");
        applied(
            actions
                .into_iter()
                .chain([self.host.emit_response(response, emit)])
                .map(E::from),
        )
    }

    pub fn lease(&self) -> &TurnLease {
        &self.lease
    }

    async fn current(&self, session: &S) -> Result<Arc<ExtensionLease>> {
        match self.lease.get() {
            Some(lease) => Ok(lease),
            None => Ok(self.lease.set(self.host.resolve(session).await?)),
        }
    }
}

/// Format extension instructions as one prompt section.
pub fn instructions_prompt(lease_instructions: &[crate::extension::ExtensionInfo]) -> String {
    lease_instructions
        .iter()
        .filter(|info| !info.instructions.trim().is_empty())
        .map(|info| format!("## {}\n\n{}", info.name, info.instructions.trim()))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[async_trait]
impl<S, E> Operation<S, E> for ExtensionOperation<S>
where
    S: Clone + Send + Sync + 'static,
    E: From<Message> + Send + 'static,
{
    fn name(&self) -> &'static str {
        self.host.name()
    }

    async fn inference_tools(&self, session: &S) -> Result<Vec<Tool>> {
        let lease = if self.shared_snapshot {
            self.current(session).await?
        } else {
            self.lease.set(self.host.resolve(session).await?)
        };
        self.host.tools(session, &lease).await
    }

    async fn prompt_parts(
        &self,
        session: &S,
        conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        let lease = self.current(session).await?;
        self.host.prompt_parts(session, conversation, &lease).await
    }

    async fn moim_parts(&self, session: &S, _conversation: &Conversation) -> Result<Vec<String>> {
        Ok(self.current(session).await?.moim().await)
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
            .chain(response.map(|response| self.host.emit_response(response, emit)))
            .map(E::from)
            .collect()
    }

    async fn run(
        &self,
        session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let messages = messages_since_kickoff(conversation)?;
        let pending: Vec<_> = crate::tool_call_state::pending_tool_requests(messages)
            .into_iter()
            .filter(|request| self.host.accepts_request(messages, request))
            .collect();
        if pending.is_empty() {
            return not_applicable();
        }
        if self.lease.get().is_none()
            && !self.host.can_resolve_for_requests(messages, &pending)
            && pending
                .iter()
                .all(|request| self.host.claims_expired_request(messages, request))
        {
            for request in pending {
                let result = match &request.tool_call {
                    Ok(_) => self.host.expired_result(),
                    Err(error) => CallToolResult::error(vec![ContentBlock::text(format!(
                        "The tool call could not be parsed: {error}. Correct the arguments and try again."
                    ))]),
                };
                self.batch
                    .record(&request.id, Ok(result), request.metadata.as_ref());
            }
            return self.finish(emit);
        }
        let (lease, expired) = match self.lease.get() {
            Some(lease) => (lease, false),
            None if self.host.can_resolve_for_requests(messages, &pending) => {
                (self.lease.set(self.host.resolve(session).await?), false)
            }
            None => (Arc::new(self.host.resolve(session).await?), true),
        };
        let tools = self.host.execution_tools(session, &lease).await?;
        let tool_owners: Vec<_> = tools.iter().map(get_tool_owner).collect();
        let mine: Vec<_> = pending
            .into_iter()
            .filter(|request| {
                request.tool_call.as_ref().map_or(true, |call| {
                    tools.iter().any(|tool| tool.name == call.name)
                        || recover_mangled_tool_name(
                            &call.name,
                            tools
                                .iter()
                                .zip(&tool_owners)
                                .map(|(tool, owner)| (tool.name.as_ref(), owner.as_deref())),
                        )
                        .is_some()
                })
            })
            .collect();
        if mine.is_empty() {
            return not_applicable();
        }

        let mut calls = Vec::new();
        for request in mine {
            let request = request.clone();
            let call = if let Err(error) = &request.tool_call {
                ToolCallResult::from(Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "The tool call could not be parsed: {error}. Correct the arguments and try again."
                ))])))
            } else if expired {
                ToolCallResult::from(Ok(self.host.expired_result()))
            } else if let Some(result) = self.host.skipped_result(session) {
                ToolCallResult::from(Ok(result))
            } else {
                let span = self.host.call_span(session, &request);
                let denied = self
                    .host
                    .before_call(session, &lease, &request)
                    .instrument(span.clone())
                    .await;
                if let Some(denied) = denied {
                    self.batch
                        .record(&request.id, Ok(denied), request.metadata.as_ref());
                    continue;
                }
                let call = async {
                    let call = if let Some(call) =
                        self.host.call_override(session, &lease, &request).await
                    {
                        call
                    } else {
                        match lease
                            .call(
                                request.tool_call.clone().expect("filtered to parsed calls"),
                                CallRequest::new(request.id.clone()),
                                emit.cancel_token().child_token(),
                            )
                            .await
                        {
                            Ok(call) => call,
                            Err(error) => {
                                self.host.dispatch_error(&request, &error);
                                ToolCallResult::from(Err(error))
                            }
                        }
                    };
                    self.host.decorate_call(session, &lease, &request, call)
                }
                .instrument(span.clone())
                .await;
                let host = Arc::clone(&self.host);
                let batch = self.batch.clone();
                let session = session.clone();
                let conversation = conversation.clone();
                let lease = Arc::clone(&lease);
                let request = request.clone();
                let result = call.result;
                ToolCallResult {
                    result: Box::new(
                        async move {
                            let result = result.await;
                            batch.record(&request.id, result.clone(), request.metadata.as_ref());
                            let response = ToolResponse {
                                id: request.id.clone(),
                                tool_result: result.clone(),
                                metadata: request.metadata.clone(),
                            };
                            host.after_call(&session, &conversation, &lease, &request, &response)
                                .await;
                            result
                        }
                        .instrument(span)
                        .boxed(),
                    ),
                    notification_stream: call.notification_stream,
                    action_required_stream: call.action_required_stream,
                }
            };
            calls.push(PendingCall {
                request_id: request.id,
                metadata: request.metadata,
                call,
            });
        }
        self.batch
            .run(calls, emit, &|id, result| {
                self.host.on_result(id, result, emit)
            })
            .await;
        self.finish(emit)
    }
}
