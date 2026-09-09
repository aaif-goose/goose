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
    let claim = session_manager
        .action_required()
        .claim_response(session_id, elicitation_id)
        .await?;

    session_manager
        .add_message(session_id, response_message)
        .await?;

    claim.submit(response)
}

pub(crate) async fn complete_elicitation_with_generated_message(
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
