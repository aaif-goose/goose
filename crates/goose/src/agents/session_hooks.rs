use anyhow::Result;

use crate::agents::StateMachineServices;
use crate::conversation::message::Message;
use crate::hooks::{HookContext, HookEvent};
use crate::session::Session;

const ENTRY_HOOK_NOTE_SCOPE: &str = "entry_hook";
const SESSION_START_NOTE: &str = "session_start";

/// Records that a client fired `SessionStart` when it opened the session, so the first
/// prompt does not fire it again. Hidden from the user when the hooks printed no banner.
pub(crate) fn session_start_message(banners: &[String]) -> Message {
    let mut message = Message::assistant();
    if !banners.is_empty() {
        message = message.with_text(banners.join("\n"));
    }
    let mut message = message.with_visibility(!banners.is_empty(), false);
    message
        .metadata
        .set_operation_note(ENTRY_HOOK_NOTE_SCOPE, SESSION_START_NOTE, true.into());
    message
}

fn is_session_start_message(message: &Message) -> bool {
    message
        .metadata
        .operation_note(ENTRY_HOOK_NOTE_SCOPE, SESSION_START_NOTE)
        .is_some()
}

fn is_user_prompt(message: &Message) -> bool {
    message.role == rmcp::model::Role::User && !message.is_tool_response()
}

impl StateMachineServices {
    /// Emit a lifecycle hook event with no extra context. Useful for events
    /// that have no matcher (e.g. `SessionStart`, `SessionEnd`).
    pub async fn emit_hook(&self, event: HookEvent, session_id: &str) {
        if !self.hook_manager.has_hooks(event) {
            return;
        }
        self.hook_manager
            .emit(event, HookContext::new(event, session_id))
            .await;
    }

    /// Fires `SessionStart` for a client opening the session and returns the hooks'
    /// banners, which the client shows before the first prompt.
    pub async fn emit_session_start(&self, session_id: &str) -> Result<Vec<String>> {
        let event = HookEvent::SessionStart;
        if !self.hook_manager.has_hooks(event) {
            return Ok(Vec::new());
        }
        let banners = self
            .hook_manager
            .emit_collecting_banners(event, HookContext::new(event, session_id))
            .await;
        self.session_manager
            .add_message(session_id, &session_start_message(&banners))
            .await?;
        Ok(banners)
    }

    pub(crate) async fn emit_hooks_for_saved_prompt(&self, session_id: &str) -> Result<()> {
        if !self.hook_manager.has_hooks(HookEvent::SessionStart)
            && !self.hook_manager.has_hooks(HookEvent::UserPromptSubmit)
        {
            return Ok(());
        }
        let session = self.session_manager.get_session(session_id, true).await?;
        let Some((prompt, earlier)) = session
            .conversation
            .as_ref()
            .and_then(|conversation| conversation.messages().split_last())
        else {
            return Ok(());
        };
        if !is_user_prompt(prompt) {
            return Ok(());
        }
        if !earlier.iter().any(|message| {
            is_session_start_message(message)
                || (is_user_prompt(message) && message.is_user_visible())
        }) {
            self.hook_manager
                .emit(
                    HookEvent::SessionStart,
                    HookContext::new(HookEvent::SessionStart, session_id)
                        .with_working_dir(session.working_dir.to_string_lossy()),
                )
                .await;
        }
        self.emit_user_prompt_submit(&session, prompt.agent_visible_content().as_concat_text())
            .await;
        Ok(())
    }

    pub(super) async fn emit_user_prompt_submit(&self, session: &Session, prompt: String) {
        if prompt.is_empty() {
            return;
        }
        self.hook_manager
            .emit(
                HookEvent::UserPromptSubmit,
                HookContext::new(HookEvent::UserPromptSubmit, &session.id)
                    .with_message(prompt)
                    .with_working_dir(session.working_dir.to_string_lossy()),
            )
            .await;
    }
}
