pub(crate) mod goose_serve;
mod manager;
mod payloads;
mod registry;
mod search;
mod writer;

pub(crate) use goose_serve::resolve_goose_binary;
pub use manager::{AcpSessionInfo, GooseAcpManager};
pub use registry::{AcpRunningSession, AcpSessionRegistry};
pub use search::{search_sessions_via_exports, SessionSearchResult};
pub use writer::TauriMessageWriter;

use std::path::PathBuf;
use std::sync::Arc;

use acp_client::MessageWriter;
use tauri::Emitter;

/// Build a composite registry key: `{session_id}__{persona_id}` when a
/// persona is active, or plain `session_id` for backward compatibility.
///
/// This assumes neither component contains `__`.
pub fn make_composite_key(session_id: &str, persona_id: Option<&str>) -> String {
    match persona_id {
        Some(pid) if !pid.is_empty() => format!("{session_id}__{pid}"),
        _ => session_id.to_string(),
    }
}

pub fn split_composite_key(key: &str) -> (&str, Option<&str>) {
    match key.split_once("__") {
        Some((session_id, persona_id)) if !persona_id.is_empty() => (session_id, Some(persona_id)),
        _ => (key, None),
    }
}

/// High-level service for running ACP prompts through an agent driver.
///
/// The actual response content is streamed to the frontend via Tauri events
/// emitted by [`TauriMessageWriter`]; the returned `Result` only signals
/// whether the request was successfully dispatched.
pub struct AcpService;

impl AcpService {
    pub async fn prepare_session(
        app_handle: tauri::AppHandle,
        session_id: String,
        provider_id: String,
        working_dir: PathBuf,
        persona_id: Option<String>,
    ) -> Result<(), String> {
        let manager = GooseAcpManager::start(app_handle).await?;

        manager
            .prepare_session(
                make_composite_key(&session_id, persona_id.as_deref()),
                session_id,
                provider_id,
                working_dir,
                None, // no existing agent session ID — goose binary owns sessions
            )
            .await
    }

    /// Send a prompt to the given ACP provider and stream the response via
    /// Tauri events.
    #[allow(clippy::too_many_arguments)]
    pub async fn send_prompt(
        app_handle: tauri::AppHandle,
        registry: Arc<AcpSessionRegistry>,
        session_id: String,
        provider_id: String,
        prompt: String,
        working_dir: PathBuf,
        system_prompt: Option<String>,
        persona_id: Option<String>,
        persona_name: Option<String>,
        images: Vec<(String, String)>,
    ) -> Result<(), String> {
        let registry_key = make_composite_key(&session_id, persona_id.as_deref());
        let cancel_token = registry.register(&registry_key, &provider_id);

        let writer_impl = Arc::new(TauriMessageWriter::new(
            app_handle.clone(),
            session_id.clone(),
            Some(provider_id.clone()),
            persona_id.clone(),
            persona_name.clone(),
        ));
        registry.set_assistant_message_id(
            &registry_key,
            writer_impl.assistant_message_id().to_string(),
        );
        let writer: Arc<dyn MessageWriter> = writer_impl.clone();

        // Build the effective prompt, including persona instructions when
        // available. When there is no extra context we pass the raw prompt
        // for backward compatibility.
        let has_system = system_prompt.as_ref().is_some_and(|s| !s.is_empty());
        let effective_prompt = if has_system {
            let mut parts = Vec::new();
            if let Some(ref sp) = system_prompt {
                if !sp.is_empty() {
                    parts.push(format!(
                        "<persona-instructions>\n{sp}\n</persona-instructions>"
                    ));
                }
            }
            parts.push(format!("<user-message>\n{prompt}\n</user-message>"));
            parts.join("\n\n")
        } else {
            prompt.clone()
        };

        let manager = GooseAcpManager::start(app_handle.clone()).await?;
        let result = manager
            .send_prompt(
                registry_key.clone(),
                session_id.clone(),
                provider_id.clone(),
                working_dir,
                None, // no existing agent session ID — goose binary owns sessions
                writer,
                effective_prompt,
                images,
            )
            .await;

        registry.deregister(&registry_key);
        drop(cancel_token);

        if let Err(ref error) = result {
            // Emit an error event so the frontend can display it
            let _ = app_handle.emit(
                "acp:error",
                serde_json::json!({
                    "sessionId": session_id,
                    "messageId": writer_impl.assistant_message_id(),
                    "error": error,
                }),
            );
        }

        result
    }
}
