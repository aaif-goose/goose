use super::Lease;
use crate::{
    operation::{
        applied, messages_since_kickoff, not_applicable, Emitter, Operation, OperationResult,
    },
    post_tool_call_hook::PostToolCallHook,
    tool_call_state::{tool_calls, ToolCallState},
};
use anyhow::Result;
use async_trait::async_trait;
use goose_provider_types::{
    conversation::{
        message::{Message, MessageContent, ToolRequest},
        Conversation,
    },
    maybe_send::{MaybeSend, MaybeSync},
};
use rmcp::model::{CallToolRequestParams, CallToolResult, ErrorData, Tool};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// Host-specific binding and decoration, shared by inference and execution in one run.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait McpHost<S>: MaybeSend + MaybeSync {
    async fn tools(&self, session: &S) -> Result<Vec<Tool>>;
    async fn prompt_parts(
        &self,
        _session: &S,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        Ok(Vec::new())
    }
    async fn moim_parts(&self, _session: &S, _conversation: &Conversation) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
    async fn call(
        &self,
        session: &S,
        request: &ToolRequest,
        emit: &Emitter,
    ) -> Result<CallToolResult, ErrorData>;
}

/// A run-scoped operation. Keep its host binding alive while approvals are pending.
pub struct McpOperation<S> {
    host: Arc<dyn McpHost<S>>,
    post_hook: Option<Arc<dyn PostToolCallHook<S>>>,
    responses: Mutex<Option<Message>>,
}
impl<S> McpOperation<S> {
    pub fn new(host: Arc<dyn McpHost<S>>) -> Self {
        Self {
            host,
            post_hook: None,
            responses: Mutex::new(None),
        }
    }
    pub fn with_post_tool_call_hook(mut self, hook: Arc<dyn PostToolCallHook<S>>) -> Self {
        self.post_hook = Some(hook);
        self
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for McpOperation<S>
where
    S: MaybeSend + MaybeSync + 'static,
    E: From<Message> + MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        "mcp"
    }
    async fn inference_tools(&self, session: &S) -> Result<Vec<Tool>> {
        self.host.tools(session).await
    }
    async fn prompt_parts(
        &self,
        session: &S,
        conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        self.host.prompt_parts(session, conversation).await
    }
    async fn moim_parts(&self, session: &S, conversation: &Conversation) -> Result<Vec<String>> {
        self.host.moim_parts(session, conversation).await
    }
    async fn finalize_cancellation(
        &self,
        _session: &S,
        _conversation: &Conversation,
        emit: &Emitter,
    ) -> Vec<E> {
        self.responses
            .lock()
            .unwrap()
            .take()
            .map(|message| E::from(emit.message(message)))
            .into_iter()
            .collect()
    }
    async fn run(
        &self,
        session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let pending = tool_calls(messages_since_kickoff(conversation)?)
            .into_iter()
            .filter(|(request, state)| {
                !request.was_executed_externally()
                    && matches!(state, ToolCallState::Unchecked | ToolCallState::Executable)
            })
            .map(|(request, _)| request.clone())
            .collect::<Vec<_>>();
        if pending.is_empty() {
            return not_applicable();
        }
        let tools = self.host.tools(session).await?;
        let mut handled = false;
        for request in pending {
            if request
                .tool_call
                .as_ref()
                .is_ok_and(|call| !tools.iter().any(|tool| tool.name == call.name))
            {
                continue;
            }
            let result = match &request.tool_call {
                Ok(_) => self.host.call(session, &request, emit).await,
                Err(error) => Err(error.clone()),
            };
            let response = {
                let mut buffer = self.responses.lock().unwrap();
                let message =
                    buffer.get_or_insert_with(|| Message::user().with_generated_id_if_missing());
                message.add_tool_response_with_metadata(
                    request.id.clone(),
                    result,
                    request.metadata.as_ref(),
                );
                message
                    .content
                    .last()
                    .and_then(MessageContent::as_tool_response)
                    .cloned()
                    .expect("response added")
            };
            handled = true;
            if let Some(hook) = &self.post_hook {
                if let Err(error) = hook
                    .after_call(session, conversation, &request, &response)
                    .await
                {
                    tracing::warn!(%error, "post tool call hook failed");
                }
            }
        }
        if !handled {
            return not_applicable();
        }
        let response = self
            .responses
            .lock()
            .unwrap()
            .take()
            .expect("responses buffered");
        applied([E::from(emit.message(response))])
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S: MaybeSend + MaybeSync> McpHost<S> for Lease {
    async fn tools(&self, _session: &S) -> Result<Vec<Tool>> {
        Ok(self
            .catalog(CancellationToken::new())
            .await?
            .entries
            .iter()
            .map(|entry| {
                let mut tool = entry.tool.clone();
                tool.meta.get_or_insert_with(Default::default).0.insert(
                    super::BINDING_META_KEY.into(),
                    serde_json::Value::String(self.id().into()),
                );
                tool
            })
            .collect())
    }
    async fn prompt_parts(
        &self,
        _session: &S,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        Ok(self.instructions(CancellationToken::new()).await)
    }
    async fn call(
        &self,
        _session: &S,
        request: &ToolRequest,
        emit: &Emitter,
    ) -> Result<CallToolResult, ErrorData> {
        if request
            .tool_meta
            .as_ref()
            .and_then(|meta| meta.get(super::BINDING_META_KEY))
            .and_then(serde_json::Value::as_str)
            != Some(self.id())
        {
            return Err(ErrorData::invalid_request(
                "MCP binding expired; request the tool again",
                None,
            ));
        }
        let call = request.tool_call.as_ref().map_err(Clone::clone)?;
        let catalog = self
            .catalog(emit.cancel_token().clone())
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let entry = catalog
            .get(&call.name)
            .ok_or_else(|| ErrorData::invalid_params("tool is not in this MCP binding", None))?;
        let cancel = emit.cancel_token().child_token();
        let _guard = cancel.clone().drop_guard();
        let future = entry.owner.call_tool(
            &request.id,
            CallToolRequestParams::new(entry.server_name.clone())
                .with_arguments(call.arguments.clone().unwrap_or_default()),
            cancel,
        );
        let mut notifications = entry.owner.subscribe();
        tokio::pin!(future);
        loop {
            tokio::select! {
                result = &mut future => return result,
                notification = async { match &mut notifications { Some(receiver) => receiver.recv().await, None => futures::future::pending().await } } => {
                    match notification {
                        Ok(notification) => emit.emit(crate::events::AgentEvent::McpNotification((request.id.clone(), notification))),
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {},
                        Err(_) => notifications = None,
                    }
                }
            }
        }
    }
}
