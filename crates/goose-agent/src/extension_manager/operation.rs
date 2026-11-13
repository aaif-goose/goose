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
use goose_provider_types::conversation::message::{Message, ProviderMetadata};
use goose_provider_types::conversation::Conversation;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData, Tool};

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

/// Advertises a session's extension tools and runs the calls the model makes to them,
/// after approval. Put `ToolApprovalOperation` before this operation, including for
/// an always-allow policy. Each inference resolves a fresh lease and calls run against
/// the one that advertised them; requests from a turn whose lease is gone are expired.
/// Build one per turn.
pub struct ExtensionOperation<S> {
    resolver: Arc<dyn LeaseResolver<S>>,
    lease: TurnLease,
    batch: ToolCallBatch,
}

impl<S> ExtensionOperation<S> {
    pub fn new(resolver: Arc<dyn LeaseResolver<S>>) -> Self {
        Self {
            resolver,
            lease: TurnLease::default(),
            batch: ToolCallBatch::default(),
        }
    }

    pub fn lease(&self) -> &TurnLease {
        &self.lease
    }

    async fn current(&self, session: &S) -> Result<Arc<ExtensionLease>> {
        match self.lease.get() {
            Some(lease) => Ok(lease),
            None => Ok(self.lease.set(self.resolver.resolve(session).await?)),
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
    S: Send + Sync + 'static,
    E: From<Message> + Send + 'static,
{
    fn name(&self) -> &'static str {
        "extensions"
    }

    async fn inference_tools(&self, session: &S) -> Result<Vec<Tool>> {
        Ok(self
            .lease
            .set(self.resolver.resolve(session).await?)
            .tools()
            .await
            .into_iter()
            .filter(is_tool_visible_to_model)
            .collect())
    }

    async fn prompt_parts(
        &self,
        session: &S,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        let prompt = instructions_prompt(&self.current(session).await?.instructions().await);
        Ok(if prompt.is_empty() {
            Vec::new()
        } else {
            vec![("extensions".to_string(), prompt)]
        })
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
            .chain(response.map(|response| emit.message(response)))
            .map(E::from)
            .collect()
    }

    async fn run(
        &self,
        session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let pending =
            crate::tool_call_state::pending_tool_requests(messages_since_kickoff(conversation)?);
        if pending.is_empty() {
            return not_applicable();
        }
        let (lease, expired) = match self.lease.get() {
            Some(lease) => (lease, false),
            // No inference advertised these this turn: answer the ones a fresh lease
            // would claim, rather than run them against extensions they did not see.
            None => (Arc::new(self.resolver.resolve(session).await?), true),
        };
        let tools = lease.tools().await;
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
                ToolCallResult::from(Ok(CallToolResult::error(vec![ContentBlock::text(
                    EXPIRED_TOOL_REQUEST,
                )])))
            } else {
                lease
                    .call(
                        request.tool_call.clone().expect("parsed call"),
                        CallRequest::new(request.id.clone()),
                        emit.cancel_token().child_token(),
                    )
                    .await
                    .unwrap_or_else(|error| ToolCallResult::from(Err(error)))
            };
            calls.push(PendingCall {
                request_id: request.id,
                metadata: request.metadata,
                call,
            });
        }
        self.batch.run(calls, emit, &|_, _| {}).await;

        let (actions, response) = self.batch.take();
        let response = response.expect("each call records a response");
        applied(
            actions
                .into_iter()
                .chain([emit.message(response)])
                .map(E::from),
        )
    }
}
