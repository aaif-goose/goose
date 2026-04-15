use async_trait::async_trait;
use tauri::Emitter;

use acp_client::{MessageWriter, SessionInfoUpdate, SessionModelState};

use super::payloads::{
    model_options_from_state, DonePayload, MessageCreatedPayload, ModelStatePayload,
    SessionInfoPayload, TextPayload, ToolCallPayload, ToolResultPayload, ToolTitlePayload,
};

/// A [`MessageWriter`] implementation that streams ACP output to the frontend
/// via Tauri events. No local persistence — the goose binary is the sole
/// source of truth for messages.
pub struct TauriMessageWriter {
    app_handle: tauri::AppHandle,
    session_id: String,
    provider_id: Option<String>,
    assistant_message_id: String,
}

impl TauriMessageWriter {
    pub fn new(
        app_handle: tauri::AppHandle,
        session_id: String,
        provider_id: Option<String>,
        persona_id: Option<String>,
        persona_name: Option<String>,
    ) -> Self {
        let assistant_message_id = uuid::Uuid::new_v4().to_string();

        let _ = app_handle.emit(
            "acp:message_created",
            MessageCreatedPayload {
                session_id: session_id.clone(),
                message_id: assistant_message_id.clone(),
                persona_id: persona_id.clone(),
                persona_name: persona_name.clone(),
            },
        );

        Self {
            app_handle,
            session_id,
            provider_id,
            assistant_message_id,
        }
    }

    pub fn assistant_message_id(&self) -> &str {
        &self.assistant_message_id
    }
}

#[async_trait]
impl MessageWriter for TauriMessageWriter {
    async fn append_text(&self, text: &str) {
        if text.is_empty() {
            return;
        }

        let _ = self.app_handle.emit(
            "acp:text",
            TextPayload {
                session_id: self.session_id.clone(),
                message_id: self.assistant_message_id.clone(),
                text: text.to_string(),
            },
        );
    }

    async fn finalize(&self) {
        let _ = self.app_handle.emit(
            "acp:done",
            DonePayload {
                session_id: self.session_id.clone(),
                message_id: self.assistant_message_id.clone(),
            },
        );
    }

    async fn record_tool_call(
        &self,
        tool_call_id: &str,
        title: &str,
        _raw_input: Option<&serde_json::Value>,
    ) {
        let _ = self.app_handle.emit(
            "acp:tool_call",
            ToolCallPayload {
                session_id: self.session_id.clone(),
                message_id: self.assistant_message_id.clone(),
                tool_call_id: tool_call_id.to_string(),
                title: title.to_string(),
            },
        );
    }

    async fn update_tool_call_title(
        &self,
        tool_call_id: &str,
        title: Option<&str>,
        _raw_input: Option<&serde_json::Value>,
    ) {
        if let Some(title) = title {
            let _ = self.app_handle.emit(
                "acp:tool_title",
                ToolTitlePayload {
                    session_id: self.session_id.clone(),
                    message_id: self.assistant_message_id.clone(),
                    tool_call_id: tool_call_id.to_string(),
                    title: title.to_string(),
                },
            );
        }
    }

    async fn record_tool_result(&self, content: &str) {
        let _ = self.app_handle.emit(
            "acp:tool_result",
            ToolResultPayload {
                session_id: self.session_id.clone(),
                message_id: self.assistant_message_id.clone(),
                content: content.to_string(),
            },
        );
    }

    async fn on_session_info_update(&self, info: &SessionInfoUpdate) {
        let _ = self.app_handle.emit(
            "acp:session_info",
            SessionInfoPayload {
                session_id: self.session_id.clone(),
                title: info.title.value().cloned(),
            },
        );
    }

    async fn on_model_state_update(&self, state: &SessionModelState) {
        let current_model_name = state
            .available_models
            .iter()
            .find(|m| m.model_id == state.current_model_id)
            .map(|m| m.name.clone());
        let available_models = model_options_from_state(state);
        let _ = self.app_handle.emit(
            "acp:model_state",
            ModelStatePayload {
                session_id: self.session_id.clone(),
                provider_id: self.provider_id.clone(),
                current_model_id: state.current_model_id.to_string(),
                current_model_name,
                available_models,
            },
        );
    }
}
