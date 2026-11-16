use anyhow::Result;

use crate::agents::StateMachineServices;
use crate::conversation::message::Message;
use crate::hooks::{HookContext, HookEvent};

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

pub(crate) fn is_session_start_message(message: &Message) -> bool {
    message
        .metadata
        .operation_note(ENTRY_HOOK_NOTE_SCOPE, SESSION_START_NOTE)
        .is_some()
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
}
