use acp_client::SessionModelState;
use serde::Serialize;

/// Payload for the `acp:message_created` event.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MessageCreatedPayload {
    pub session_id: String,
    pub message_id: String,
    pub persona_id: Option<String>,
    pub persona_name: Option<String>,
}

/// Payload for the `acp:text` event.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextPayload {
    pub session_id: String,
    pub message_id: String,
    pub text: String,
}

/// Payload for the `acp:done` event.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DonePayload {
    pub session_id: String,
    pub message_id: String,
}

/// Payload for the `acp:tool_call` event.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolCallPayload {
    pub session_id: String,
    pub message_id: String,
    pub tool_call_id: String,
    pub title: String,
}

/// Payload for the `acp:tool_title` event.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolTitlePayload {
    pub session_id: String,
    pub message_id: String,
    pub tool_call_id: String,
    pub title: String,
}

/// Payload for the `acp:tool_result` event.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolResultPayload {
    pub session_id: String,
    pub message_id: String,
    pub content: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionInfoPayload {
    pub session_id: String,
    pub title: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionBoundPayload {
    pub session_id: String,
    pub goose_session_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReplayCompletePayload {
    pub session_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelOption {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelStatePayload {
    pub session_id: String,
    pub provider_id: Option<String>,
    pub current_model_id: String,
    pub current_model_name: Option<String>,
    pub available_models: Vec<ModelOption>,
}

pub(crate) fn model_options_from_state(state: &SessionModelState) -> Vec<ModelOption> {
    state
        .available_models
        .iter()
        .map(|model| ModelOption {
            id: model.model_id.to_string(),
            name: model.name.clone(),
        })
        .collect()
}
