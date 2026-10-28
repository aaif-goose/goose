use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use goose_agent::tool_approval::confirmation_response_message;

use crate::agents::state_machine::messages_since_kickoff;
use crate::agents::state_machine::ops_toolcalling::request_was_advertised;
use crate::conversation::message::{
    ActionRequiredData, Message, MessageContent, ToolConfirmationRequest, ToolRequest,
};
use crate::conversation::Conversation;
use crate::permission::Permission;
use crate::session::SessionManager;

struct ApprovalState {
    answered: HashSet<String>,
    approval_requests: HashSet<String>,
    approval_responses: HashMap<String, Permission>,
    tool_requests: Vec<ToolRequest>,
}

impl ApprovalState {
    fn from_messages(messages: &[Message]) -> Self {
        let mut answered = HashSet::new();
        let mut approval_requests = HashSet::new();
        let mut approval_responses = HashMap::new();
        let mut tool_requests = Vec::new();

        for message in messages {
            for content in &message.content {
                match content {
                    MessageContent::ToolResponse(response) => {
                        answered.insert(response.id.clone());
                    }
                    MessageContent::ToolRequest(request)
                        if request_was_advertised(messages, request) =>
                    {
                        tool_requests.push(request.clone());
                    }
                    MessageContent::ActionRequired(action) => match &action.data {
                        ActionRequiredData::ToolConfirmation { id, .. } => {
                            approval_requests.insert(id.clone());
                        }
                        ActionRequiredData::ToolConfirmationResponse { id, permission } => {
                            approval_responses.insert(id.clone(), permission.clone());
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }

        Self {
            answered,
            approval_requests,
            approval_responses,
            tool_requests,
        }
    }

    fn has_confirmation_request(&self, request_id: &str) -> bool {
        self.approval_requests.contains(request_id)
            && self
                .tool_requests
                .iter()
                .any(|request| request.id == request_id)
    }

    fn has_unapplied_confirmation_response(&self) -> bool {
        self.tool_requests.iter().any(|request| {
            !self.answered.contains(&request.id)
                && self.approval_requests.contains(&request.id)
                && self.approval_responses.contains_key(&request.id)
        })
    }

    fn confirmation_response(&self, request_id: &str) -> Option<&Permission> {
        self.approval_responses.get(request_id)
    }

    fn has_tool_response(&self, request_id: &str) -> bool {
        self.answered.contains(request_id)
    }
}

fn active_turn_messages(conversation: &Conversation) -> &[Message] {
    let messages = conversation.messages();
    messages
        .iter()
        .rposition(|message| {
            message.role == rmcp::model::Role::User
                && message.is_user_visible()
                && !message.is_tool_response()
        })
        .map(|start| &messages[start..])
        .unwrap_or(messages)
}

pub(crate) fn pending_tool_confirmations(
    conversation: &Conversation,
) -> Vec<ToolConfirmationRequest> {
    let mut answered = HashSet::new();
    let mut confirmation_responses = HashSet::new();
    let mut confirmation_requests = Vec::new();

    for message in active_turn_messages(conversation) {
        for content in &message.content {
            match content {
                MessageContent::ToolResponse(response) => {
                    answered.insert(response.id.clone());
                }
                MessageContent::ActionRequired(action) => match &action.data {
                    ActionRequiredData::ToolConfirmation {
                        id,
                        tool_name,
                        arguments,
                        prompt,
                    } => confirmation_requests.push(ToolConfirmationRequest {
                        id: id.clone(),
                        tool_name: tool_name.clone(),
                        arguments: arguments.clone(),
                        prompt: prompt.clone(),
                    }),
                    ActionRequiredData::ToolConfirmationResponse { id, .. } => {
                        confirmation_responses.insert(id.clone());
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

    confirmation_requests
        .into_iter()
        .filter(|request| {
            !answered.contains(&request.id) && !confirmation_responses.contains(&request.id)
        })
        .collect()
}

pub(crate) fn has_unapplied_tool_confirmation_response(conversation: &Conversation) -> bool {
    ApprovalState::from_messages(active_turn_messages(conversation))
        .has_unapplied_confirmation_response()
}

pub(crate) async fn persist_tool_confirmation_decision(
    session_manager: &SessionManager,
    session_id: &str,
    request_id: &str,
    permission: &Permission,
) -> Result<()> {
    let session = session_manager.get_session(session_id, true).await?;
    let conversation = session
        .conversation
        .as_ref()
        .ok_or_else(|| anyhow!("Session {session_id} has no conversation"))?;
    let approval_state = ApprovalState::from_messages(messages_since_kickoff(conversation)?);

    if !approval_state.has_confirmation_request(request_id) {
        return Err(anyhow!(
            "Tool request {request_id} is not awaiting confirmation in session {session_id}"
        ));
    }

    if let Some(persisted_permission) = approval_state.confirmation_response(request_id) {
        return if persisted_permission == permission {
            Ok(())
        } else {
            Err(anyhow!(
                "Tool request {request_id} already has a different confirmation decision"
            ))
        };
    }

    if approval_state.has_tool_response(request_id) {
        return Err(anyhow!("Tool request {request_id} already has a response"));
    }

    session_manager
        .add_message(
            session_id,
            &confirmation_response_message(request_id, permission.clone()),
        )
        .await
}
