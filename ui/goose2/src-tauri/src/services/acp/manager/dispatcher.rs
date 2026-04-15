use std::collections::HashMap;
use std::sync::Arc;

use acp_client::MessageWriter;
use agent_client_protocol::{
    Client, ContentBlock as AcpContentBlock, PermissionOptionId, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome,
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelectOptions, SessionInfoUpdate, SessionModelState, SessionNotification,
    SessionUpdate,
};
use async_trait::async_trait;
use tauri::Emitter;
use tokio::sync::Mutex;

use crate::services::acp::payloads::{
    model_options_from_state, DonePayload, MessageCreatedPayload, ModelOption, ModelStatePayload,
    ReplayCompletePayload, SessionBoundPayload, SessionInfoPayload, TextPayload, ToolCallPayload,
    ToolResultPayload, ToolTitlePayload,
};

fn extract_user_message(raw: &str) -> &str {
    const OPEN: &str = "<user-message>\n";
    const CLOSE: &str = "\n</user-message>";
    if let Some(start) = raw.find(OPEN) {
        let inner = start + OPEN.len();
        if raw[inner..].ends_with(CLOSE) {
            return &raw[inner..raw.len() - CLOSE.len()];
        }
    }
    raw
}
fn model_options_from_select_options(options: &SessionConfigSelectOptions) -> Vec<ModelOption> {
    match options {
        SessionConfigSelectOptions::Ungrouped(values) => values
            .iter()
            .map(|value| ModelOption {
                id: value.value.to_string(),
                name: value.name.clone(),
            })
            .collect(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| group.options.iter())
            .map(|value| ModelOption {
                id: value.value.to_string(),
                name: value.name.clone(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[derive(Clone)]
pub(super) struct SessionRoute {
    pub(super) local_session_id: String,
    pub(super) provider_id: Option<String>,
    pub(super) writer: Option<Arc<dyn MessageWriter>>,
    pub(super) canceled: bool,
    pub(super) replay_message_id: Option<String>,
    pub(super) replay_events: u32,
}

pub(super) struct SessionEventDispatcher {
    app_handle: tauri::AppHandle,
    routes: Arc<Mutex<HashMap<String, SessionRoute>>>,
}

impl SessionEventDispatcher {
    pub(super) fn new(
        app_handle: tauri::AppHandle,
        routes: Arc<Mutex<HashMap<String, SessionRoute>>>,
    ) -> Self {
        Self { app_handle, routes }
    }

    pub(super) async fn bind_session(
        &self,
        goose_session_id: &str,
        local_session_id: &str,
        provider_id: Option<&str>,
    ) {
        let mut routes = self.routes.lock().await;
        routes
            .entry(goose_session_id.to_string())
            .and_modify(|route| {
                route.local_session_id = local_session_id.to_string();
                if let Some(pid) = provider_id {
                    route.provider_id = Some(pid.to_string());
                }
            })
            .or_insert_with(|| SessionRoute {
                local_session_id: local_session_id.to_string(),
                provider_id: provider_id.map(ToString::to_string),
                writer: None,
                canceled: false,
                replay_message_id: None,
                replay_events: 0,
            });

        let _ = self.app_handle.emit(
            "acp:session_bound",
            SessionBoundPayload {
                session_id: local_session_id.to_string(),
                goose_session_id: goose_session_id.to_string(),
            },
        );
    }

    pub(super) async fn attach_writer(
        &self,
        goose_session_id: &str,
        local_session_id: &str,
        provider_id: Option<&str>,
        writer: Arc<dyn MessageWriter>,
    ) {
        let mut routes = self.routes.lock().await;
        routes.insert(
            goose_session_id.to_string(),
            SessionRoute {
                local_session_id: local_session_id.to_string(),
                provider_id: provider_id.map(ToString::to_string),
                writer: Some(writer),
                canceled: false,
                replay_message_id: None,
                replay_events: 0,
            },
        );
    }

    pub(super) async fn clear_writer(&self, goose_session_id: &str) {
        let mut routes = self.routes.lock().await;
        if let Some(route) = routes.get_mut(goose_session_id) {
            route.writer = None;
            route.canceled = false;
        }
    }

    pub(super) async fn mark_canceled(&self, goose_session_id: &str) -> bool {
        let mut routes = self.routes.lock().await;
        if let Some(route) = routes.get_mut(goose_session_id) {
            let had_writer = route.writer.is_some();
            route.writer = None;
            route.canceled = had_writer;
            return had_writer;
        }
        false
    }

    pub(super) async fn is_canceled(&self, goose_session_id: &str) -> bool {
        let routes = self.routes.lock().await;
        routes
            .get(goose_session_id)
            .is_some_and(|route| route.canceled)
    }

    /// Finalize any in-progress replay message and clear the replay state.
    /// Called after `load_session` completes to mark the last replayed
    /// assistant message as done.
    pub(super) async fn finalize_replay(&self, goose_session_id: &str) {
        let mut routes = self.routes.lock().await;
        if let Some(route) = routes.get_mut(goose_session_id) {
            if let Some(message_id) = route.replay_message_id.take() {
                let _ = self.app_handle.emit(
                    "acp:done",
                    DonePayload {
                        session_id: route.local_session_id.clone(),
                        message_id,
                    },
                );
            }
        }
    }

    pub(super) fn emit_session_info(&self, local_session_id: &str, info: &SessionInfoUpdate) {
        let _ = self.app_handle.emit(
            "acp:session_info",
            SessionInfoPayload {
                session_id: local_session_id.to_string(),
                title: info.title.value().cloned(),
            },
        );
    }

    pub(super) fn emit_model_state(
        &self,
        local_session_id: &str,
        provider_id: Option<&str>,
        state: &SessionModelState,
    ) {
        let current_model_name = state
            .available_models
            .iter()
            .find(|model| model.model_id == state.current_model_id)
            .map(|model| model.name.clone());
        let available_models = model_options_from_state(state);
        let _ = self.app_handle.emit(
            "acp:model_state",
            ModelStatePayload {
                session_id: local_session_id.to_string(),
                provider_id: provider_id.map(ToString::to_string),
                current_model_id: state.current_model_id.to_string(),
                current_model_name,
                available_models,
            },
        );
    }

    pub(super) fn emit_model_state_from_options(
        &self,
        local_session_id: &str,
        provider_id: Option<&str>,
        options: &[SessionConfigOption],
    ) {
        let Some(option) = options
            .iter()
            .find(|option| option.category == Some(SessionConfigOptionCategory::Model))
        else {
            return;
        };

        let SessionConfigKind::Select(select) = &option.kind else {
            return;
        };
        let current_model_id = select.current_value.to_string();
        let available_models = model_options_from_select_options(&select.options);
        let current_model_name = match &select.options {
            SessionConfigSelectOptions::Ungrouped(values) => values
                .iter()
                .find(|value| value.value == select.current_value)
                .map(|value| value.name.clone()),
            SessionConfigSelectOptions::Grouped(groups) => groups
                .iter()
                .flat_map(|group| group.options.iter())
                .find(|value| value.value == select.current_value)
                .map(|value| value.name.clone()),
            _ => None,
        };

        let _ = self.app_handle.emit(
            "acp:model_state",
            ModelStatePayload {
                session_id: local_session_id.to_string(),
                provider_id: provider_id.map(ToString::to_string),
                current_model_id,
                current_model_name,
                available_models,
            },
        );
    }

    pub(super) fn emit_replay_complete(&self, local_session_id: &str) {
        let _ = self.app_handle.emit(
            "acp:replay_complete",
            ReplayCompletePayload {
                session_id: local_session_id.to_string(),
            },
        );
    }

    pub(super) async fn get_replay_event_count(&self, goose_session_id: &str) -> u32 {
        let routes = self.routes.lock().await;
        routes
            .get(goose_session_id)
            .map(|r| r.replay_events)
            .unwrap_or(0)
    }
}

fn extract_content_preview(content: &[agent_client_protocol::ToolCallContent]) -> Option<String> {
    for item in content {
        match item {
            agent_client_protocol::ToolCallContent::Content(content_item) => {
                if let AcpContentBlock::Text(text) = &content_item.content {
                    let preview: String = text.text.chars().take(500).collect();
                    return Some(if text.text.len() > 500 {
                        format!("{preview}…")
                    } else {
                        preview
                    });
                }
            }
            agent_client_protocol::ToolCallContent::Diff(diff) => {
                return Some(format!(
                    "{}{}",
                    diff.path.display(),
                    if diff.old_text.is_some() {
                        " (modified)"
                    } else {
                        " (new)"
                    }
                ));
            }
            agent_client_protocol::ToolCallContent::Terminal(terminal) => {
                return Some(format!("Terminal: {}", terminal.terminal_id.0));
            }
            _ => {}
        }
    }

    None
}

#[async_trait(?Send)]
impl Client for SessionEventDispatcher {
    async fn request_permission(
        &self,
        args: RequestPermissionRequest,
    ) -> agent_client_protocol::Result<RequestPermissionResponse> {
        let option_id = args
            .options
            .first()
            .map(|option| option.option_id.clone())
            .unwrap_or_else(|| PermissionOptionId::new("approve"));

        Ok(RequestPermissionResponse::new(
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id)),
        ))
    }

    async fn session_notification(
        &self,
        notification: SessionNotification,
    ) -> agent_client_protocol::Result<()> {
        let goose_session_id = notification.session_id.to_string();
        let route = {
            let routes = self.routes.lock().await;
            routes.get(&goose_session_id).cloned()
        };

        let Some(route) = route else {
            return Ok(());
        };

        match &notification.update {
            SessionUpdate::SessionInfoUpdate(info) => {
                self.emit_session_info(&route.local_session_id, info);
                return Ok(());
            }
            SessionUpdate::ConfigOptionUpdate(update) => {
                self.emit_model_state_from_options(
                    &route.local_session_id,
                    route.provider_id.as_deref(),
                    &update.config_options,
                );
                return Ok(());
            }
            _ => {}
        }

        // Live streaming path — writer is present
        if let Some(writer) = route.writer {
            match &notification.update {
                SessionUpdate::AgentMessageChunk(chunk) => {
                    if let AcpContentBlock::Text(text) = &chunk.content {
                        writer.append_text(&text.text).await;
                    }
                }
                SessionUpdate::ToolCall(tool_call) => {
                    writer
                        .record_tool_call(
                            &tool_call.tool_call_id.0,
                            &tool_call.title,
                            tool_call.raw_input.as_ref(),
                        )
                        .await;
                }
                SessionUpdate::ToolCallUpdate(update) => {
                    if update.fields.title.is_some() || update.fields.raw_input.is_some() {
                        writer
                            .update_tool_call_title(
                                &update.tool_call_id.0,
                                update.fields.title.as_deref(),
                                update.fields.raw_input.as_ref(),
                            )
                            .await;
                    }
                    if let Some(content) = &update.fields.content {
                        if let Some(result) = extract_content_preview(content) {
                            writer.record_tool_result(&result).await;
                        }
                    }
                }
                _ => {}
            }
            return Ok(());
        }

        // Replay path — no writer, emit Tauri events directly.
        // This handles messages replayed by load_session.
        let local_session_id = route.local_session_id.clone();
        match &notification.update {
            SessionUpdate::UserMessageChunk(chunk) => {
                if let AcpContentBlock::Text(text) = &chunk.content {
                    // Finalize any in-progress assistant message and count replay event
                    {
                        let mut routes = self.routes.lock().await;
                        if let Some(route) = routes.get_mut(&goose_session_id) {
                            if let Some(prev_msg_id) = route.replay_message_id.take() {
                                let _ = self.app_handle.emit(
                                    "acp:done",
                                    DonePayload {
                                        session_id: local_session_id.clone(),
                                        message_id: prev_msg_id,
                                    },
                                );
                            }
                            route.replay_events += 1;
                        }
                    }

                    let display_text = extract_user_message(&text.text);
                    let message_id = uuid::Uuid::new_v4().to_string();
                    let _ = self.app_handle.emit(
                        "acp:replay_user_message",
                        serde_json::json!({
                            "sessionId": local_session_id,
                            "messageId": message_id,
                            "text": display_text,
                        }),
                    );
                }
            }
            SessionUpdate::AgentMessageChunk(chunk) => {
                if let AcpContentBlock::Text(text) = &chunk.content {
                    // Check if we already have a replay message in progress
                    let replay_msg_id = {
                        let routes = self.routes.lock().await;
                        routes
                            .get(&goose_session_id)
                            .and_then(|r| r.replay_message_id.clone())
                    };

                    let message_id = if let Some(id) = replay_msg_id {
                        id
                    } else {
                        // Start a new assistant message
                        let new_id = uuid::Uuid::new_v4().to_string();
                        let _ = self.app_handle.emit(
                            "acp:message_created",
                            MessageCreatedPayload {
                                session_id: local_session_id.clone(),
                                message_id: new_id.clone(),
                                persona_id: None,
                                persona_name: None,
                            },
                        );
                        let mut routes = self.routes.lock().await;
                        if let Some(route) = routes.get_mut(&goose_session_id) {
                            route.replay_message_id = Some(new_id.clone());
                            route.replay_events += 1;
                        }
                        new_id
                    };

                    let _ = self.app_handle.emit(
                        "acp:text",
                        TextPayload {
                            session_id: local_session_id,
                            message_id: message_id.clone(),
                            text: text.text.clone(),
                        },
                    );
                }
            }
            SessionUpdate::ToolCall(tool_call) => {
                let replay_msg_id = {
                    let routes = self.routes.lock().await;
                    routes
                        .get(&goose_session_id)
                        .and_then(|r| r.replay_message_id.clone())
                };

                if let Some(message_id) = replay_msg_id {
                    let _ = self.app_handle.emit(
                        "acp:tool_call",
                        ToolCallPayload {
                            session_id: local_session_id,
                            message_id,
                            tool_call_id: tool_call.tool_call_id.0.to_string(),
                            title: tool_call.title.clone(),
                        },
                    );
                }
            }
            SessionUpdate::ToolCallUpdate(update) => {
                let replay_msg_id = {
                    let routes = self.routes.lock().await;
                    routes
                        .get(&goose_session_id)
                        .and_then(|r| r.replay_message_id.clone())
                };

                if let Some(message_id) = replay_msg_id {
                    if let Some(title) = &update.fields.title {
                        let _ = self.app_handle.emit(
                            "acp:tool_title",
                            ToolTitlePayload {
                                session_id: local_session_id.clone(),
                                message_id: message_id.clone(),
                                tool_call_id: update.tool_call_id.0.to_string(),
                                title: title.clone(),
                            },
                        );
                    }
                    if let Some(content) = &update.fields.content {
                        // During replay, always emit a tool_result so the
                        // frontend can mark the tool request as completed.
                        // Fall back to a generic summary when the content
                        // type has no preview extractor.
                        let result =
                            extract_content_preview(content).unwrap_or_else(|| "Done".to_string());
                        let _ = self.app_handle.emit(
                            "acp:tool_result",
                            ToolResultPayload {
                                session_id: local_session_id,
                                message_id,
                                content: result,
                            },
                        );
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
}
#[cfg(test)]
#[path = "dispatcher_tests.rs"]
mod tests;
