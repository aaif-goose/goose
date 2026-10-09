use anyhow::Result;
use rmcp::model::ElicitationAction;
use serde_json::Value;

use crate::action_required_manager::ElicitationOutcome;
use crate::conversation::message::{Message, MessageContent};
use crate::session::SessionManager;

fn elicitation_response_user_data(response: &ElicitationOutcome) -> Value {
    match response {
        ElicitationOutcome::Accept(user_data) => user_data.clone(),
        ElicitationOutcome::Decline | ElicitationOutcome::Cancel => serde_json::json!({}),
    }
}

fn elicitation_response_action(response: &ElicitationOutcome) -> ElicitationAction {
    match response {
        ElicitationOutcome::Accept(_) => ElicitationAction::Accept,
        ElicitationOutcome::Decline => ElicitationAction::Decline,
        ElicitationOutcome::Cancel => ElicitationAction::Cancel,
    }
}

fn generated_elicitation_response_message(
    elicitation_id: &str,
    response: &ElicitationOutcome,
) -> Message {
    Message::user()
        .with_generated_id()
        .with_content(MessageContent::action_required_elicitation_response(
            elicitation_id.to_string(),
            elicitation_response_user_data(response),
            elicitation_response_action(response),
        ))
        .agent_only()
}

pub(crate) async fn complete_elicitation_with_message(
    session_manager: &SessionManager,
    session_id: &str,
    elicitation_id: &str,
    response: ElicitationOutcome,
    response_message: &Message,
) -> Result<()> {
    let session_id = answering_session(session_manager, session_id, elicitation_id).await?;
    let claim = session_manager
        .action_required()
        .claim_response(&session_id, elicitation_id)
        .await?;

    session_manager
        .add_message(&session_id, response_message)
        .await?;

    claim.submit(response)
}

/// A subagent's elicitations are shown in its parent's turn, so the parent's client answers
/// them on the parent's session.
async fn answering_session(
    session_manager: &SessionManager,
    session_id: &str,
    elicitation_id: &str,
) -> Result<String> {
    let Some(owner) = session_manager
        .action_required()
        .pending_session(elicitation_id)
        .await
    else {
        return Ok(session_id.to_string());
    };
    let owner_is_a_subagent = owner != session_id
        && session_manager
            .get_session(&owner, false)
            .await?
            .parent_session_id
            .as_deref()
            == Some(session_id);
    Ok(if owner_is_a_subagent {
        owner
    } else {
        session_id.to_string()
    })
}

pub async fn complete_elicitation_with_generated_message(
    session_manager: &SessionManager,
    session_id: &str,
    elicitation_id: &str,
    response: ElicitationOutcome,
) -> Result<()> {
    let response_message = generated_elicitation_response_message(elicitation_id, &response);
    complete_elicitation_with_message(
        session_manager,
        session_id,
        elicitation_id,
        response,
        &response_message,
    )
    .await
}
