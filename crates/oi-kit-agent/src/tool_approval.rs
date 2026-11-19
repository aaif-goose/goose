use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{ensure, Result};
use async_trait::async_trait;
use oi_kit_provider_types::conversation::message::{
    ActionRequiredData, Message, MessageContent, ToolRequest,
};
use oi_kit_provider_types::conversation::Conversation;
use oi_kit_provider_types::maybe_send::{MaybeSend, MaybeSync};
use oi_kit_provider_types::permission::Permission;
use rmcp::model::CallToolRequestParams;

use crate::operation::{
    applied, messages_since_kickoff, not_applicable, yielded, Emitter, Operation, OperationResult,
};
use crate::tool_call_state::{add_error_response, pending_tool_requests, request_executable};

const APPROVAL_OPERATION: &str = "tool_approval";
const SEEN: &str = "seen";

pub const DEFAULT_DECLINED_MESSAGE: &str = "The user declined to run this tool.";

#[derive(Debug)]
pub enum ApprovalDecision {
    Allow,
    Deny { reason: String },
    Ask { prompt: Option<String> },
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ToolApproval<S>: MaybeSend + MaybeSync {
    async fn check(
        &self,
        session: &S,
        conversation: &Conversation,
        requests: &[&ToolRequest],
    ) -> Result<Vec<ApprovalDecision>>;

    async fn on_confirmation(
        &self,
        _session: &S,
        _request: &ToolRequest,
        _permission: &Permission,
    ) -> Result<()> {
        Ok(())
    }
}

pub fn confirmation_response_message(tool_call_id: &str, permission: Permission) -> Message {
    Message::user()
        .with_visibility(false, false)
        .with_content(MessageContent::action_required_tool_confirmation_response(
            tool_call_id.to_string(),
            permission,
        ))
        .with_generated_id_if_missing()
}

fn seen_message(ids: &[&str]) -> Message {
    let mut message = Message::user().with_visibility(false, false);
    message
        .metadata
        .set_operation_note(APPROVAL_OPERATION, SEEN, serde_json::json!(ids));
    message
}

/// Optional policy step. Place it before tool handlers; it yields the turn while
/// any of its confirmation requests are unanswered.
pub struct ToolApprovalOperation<'a, S> {
    approval: Arc<dyn ToolApproval<S> + 'a>,
    declined_message: String,
}

impl<'a, S> ToolApprovalOperation<'a, S> {
    pub fn new(approval: Arc<dyn ToolApproval<S> + 'a>) -> Self {
        Self {
            approval,
            declined_message: DEFAULT_DECLINED_MESSAGE.to_string(),
        }
    }

    pub fn with_declined_message(mut self, message: impl Into<String>) -> Self {
        self.declined_message = message.into();
        self
    }

    async fn apply_user_answer<E: From<Message>>(
        &self,
        session: &S,
        request: &ToolRequest,
        permission: &Permission,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        self.approval
            .on_confirmation(session, request, permission)
            .await?;
        if matches!(permission, Permission::AllowOnce | Permission::AlwaysAllow) {
            return applied([E::from(seen_message(&[&request.id]))]);
        }
        let mut tool_responses = Message::user();
        add_error_response(&mut tool_responses, request, &self.declined_message);
        applied([E::from(emit.message(tool_responses))])
    }

    async fn check_new_calls<E: From<Message>>(
        &self,
        session: &S,
        conversation: &Conversation,
        unchecked: Vec<(&ToolRequest, &CallToolRequestParams)>,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let requests: Vec<_> = unchecked.iter().map(|(request, _)| *request).collect();
        let decisions = self
            .approval
            .check(session, conversation, &requests)
            .await?;
        ensure!(
            decisions.len() == unchecked.len(),
            "tool approval returned {} decisions for {} tool calls",
            decisions.len(),
            unchecked.len()
        );

        let mut effects = Vec::new();
        let mut tool_responses = Message::user();
        let mut seen = Vec::new();
        for ((request, call), decision) in unchecked.into_iter().zip(decisions) {
            match decision {
                ApprovalDecision::Allow => seen.push(request.id.as_str()),
                ApprovalDecision::Deny { reason } => {
                    add_error_response(&mut tool_responses, request, &reason)
                }
                ApprovalDecision::Ask { prompt } => {
                    effects.push(confirmation_request(request, call, prompt));
                }
            }
        }
        if !seen.is_empty() {
            effects.push(seen_message(&seen));
        }
        if !tool_responses.content.is_empty() {
            effects.push(emit.message(tool_responses));
        }
        applied(effects.into_iter().map(E::from))
    }
}

fn confirmation_request(
    request: &ToolRequest,
    call: &CallToolRequestParams,
    prompt: Option<String>,
) -> Message {
    Message::assistant()
        .with_action_required(
            request.id.clone(),
            call.name.to_string(),
            call.arguments.clone().unwrap_or_default(),
            prompt,
        )
        .user_only()
        .with_generated_id_if_missing()
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for ToolApprovalOperation<'_, S>
where
    S: MaybeSend + MaybeSync + 'static,
    E: From<Message> + MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        APPROVAL_OPERATION
    }

    async fn run(
        &self,
        session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let messages = messages_since_kickoff(conversation)?;
        let seen: HashSet<_> = messages
            .iter()
            .filter(|message| !message.is_agent_visible() && !message.is_user_visible())
            .filter_map(|message| message.metadata.operation_note(APPROVAL_OPERATION, SEEN))
            .filter_map(serde_json::Value::as_array)
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .collect();
        let mut confirmations = HashSet::new();
        let mut answers = HashMap::new();
        for action in messages
            .iter()
            .flat_map(|message| &message.content)
            .filter_map(|content| match content {
                MessageContent::ActionRequired(action) => Some(&action.data),
                _ => None,
            })
        {
            match action {
                ActionRequiredData::ToolConfirmation { id, .. } => {
                    confirmations.insert(id.as_str());
                }
                ActionRequiredData::ToolConfirmationResponse { id, permission } => {
                    answers.insert(id.as_str(), permission);
                }
                _ => {}
            }
        }

        let mut unchecked = Vec::new();
        let mut waiting = false;
        let mut legacy_denials = Message::user();
        for request in pending_tool_requests(messages) {
            let id = request.id.as_str();
            let Ok(call) = &request.tool_call else {
                continue;
            };
            let legacy = request_executable(request);
            if seen.contains(id) {
                continue;
            }
            if confirmations.contains(id) {
                if let Some(&permission) = answers.get(id) {
                    if legacy == Some(true)
                        && matches!(permission, Permission::AllowOnce | Permission::AlwaysAllow)
                    {
                        continue;
                    }
                    return self
                        .apply_user_answer(session, request, permission, emit)
                        .await;
                }
                waiting = true;
            } else if legacy == Some(false) {
                add_error_response(&mut legacy_denials, request, &self.declined_message);
            } else {
                unchecked.push((request, call));
            }
        }
        if !legacy_denials.content.is_empty() {
            return applied([E::from(emit.message(legacy_denials))]);
        }
        if !unchecked.is_empty() {
            return self
                .check_new_calls(session, conversation, unchecked, emit)
                .await;
        }
        if waiting {
            return yielded();
        }
        not_applicable()
    }
}
