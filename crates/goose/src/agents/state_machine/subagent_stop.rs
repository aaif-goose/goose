use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::conversation::message::Message;
use crate::session::SessionManager;

pub(super) fn cancellation_note(child_ids: &[String]) -> Message {
    let text = child_ids
        .iter()
        .map(|child_id| {
            format!("Subagent {child_id} was cancelled before it finished and will not run again.")
        })
        .collect::<Vec<_>>()
        .join("\n");
    Message::user().with_text(text).with_visibility(false, true)
}

pub(crate) struct CancelSubagentsOnStop {
    session_manager: Arc<SessionManager>,
    parent_id: String,
    cancel: CancellationToken,
    failed_before_stop: bool,
}

impl CancelSubagentsOnStop {
    pub(crate) fn new(
        session_manager: Arc<SessionManager>,
        parent_id: String,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            session_manager,
            parent_id,
            cancel,
            failed_before_stop: false,
        }
    }

    pub(crate) fn record_error(&mut self) {
        if !self.cancel.is_cancelled() {
            self.failed_before_stop = true;
        }
    }
}

impl Drop for CancelSubagentsOnStop {
    fn drop(&mut self) {
        if self.failed_before_stop || !self.cancel.is_cancelled() {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let session_manager = self.session_manager.clone();
        let parent_id = std::mem::take(&mut self.parent_id);
        runtime.spawn(async move {
            if let Err(error) = session_manager
                .cancel_foreground_subagents(&parent_id, cancellation_note)
                .await
            {
                tracing::error!(%error, parent_id, "Failed to record cancelled foreground subagents");
            }
        });
    }
}
