//! Goose-specific inference request preparation.

use crate::agents::extension_manager::{ExtensionLease, ExtensionManager};
use crate::agents::state_machine::awaits_tool_responses;
use crate::agents::PromptManager;
use crate::config::GooseMode;
use crate::session::Session;
use crate::tool_inspection::ToolInspectionManager;
use anyhow::Result;
use async_trait::async_trait;
use goose_agent::inference::{InferenceRequestPreparer, PreparedInferenceRequest};
use goose_agent::operation::{messages_since_kickoff, InferenceInput};
use goose_providers::conversation::message::Message;
use goose_providers::conversation::Conversation;
use std::sync::{Arc, Mutex as StdMutex};

pub struct GooseInferenceRequestPreparer<'a> {
    pub(crate) extension_manager: Arc<ExtensionManager>,
    pub(crate) extension_lease: Arc<StdMutex<Option<Arc<ExtensionLease>>>>,
    pub(crate) tool_inspection_manager: &'a ToolInspectionManager,
    pub(crate) context_limit: usize,
}

#[async_trait]
impl InferenceRequestPreparer<Session> for GooseInferenceRequestPreparer<'_> {
    async fn prepare_session(&self, session: &Session) -> Result<Option<Session>> {
        let (session, lease) = self
            .extension_manager
            .current_session_snapshot(session)
            .await;
        // Tool requests run under the lease of the inference that made them, so an
        // approval answered after that lease is gone expires instead of picking up a new one.
        let awaiting = match &session.conversation {
            Some(conversation) => awaits_tool_responses(messages_since_kickoff(conversation)?),
            None => false,
        };
        if !awaiting {
            *self
                .extension_lease
                .lock()
                .expect("extension lease unavailable") = Some(Arc::new(lease));
        }
        Ok(Some(session))
    }

    async fn prepare(
        &self,
        session: &Session,
        conversation: &Conversation,
        input: InferenceInput,
    ) -> Result<PreparedInferenceRequest> {
        #[cfg(feature = "code-mode")]
        let code_execution_mode = self
            .extension_lease
            .lock()
            .expect("extension lease unavailable")
            .as_ref()
            .is_some_and(|lease| {
                lease.is_enabled(crate::agents::platform_extensions::code_execution::EXTENSION_NAME)
            });
        #[cfg(not(feature = "code-mode"))]
        let code_execution_mode = false;

        let goose_mode = session.goose_mode;
        if goose_mode == GooseMode::SmartApprove {
            self.tool_inspection_manager
                .apply_tool_annotations(&input.tools);
        }
        let tools =
            crate::agents::reply_parts::prepare_inference_tools(input.tools, code_execution_mode);
        let system_prompt =
            PromptManager::new().build_system_prompt(session, input.prompt_parts, goose_mode)?;
        let turn = messages_since_kickoff(conversation)?;
        let turn_start = turn
            .first()
            .and_then(|message| chrono::DateTime::from_timestamp(message.created, 0))
            .map(|timestamp| timestamp.with_timezone(&chrono::Local))
            .unwrap_or_else(chrono::Local::now);
        let last = turn
            .iter()
            .rev()
            .find(|message| message.is_turn_context())
            .map(Message::as_concat_text);
        let context_limit = Some(self.context_limit);
        let additional_messages = crate::agents::moim::turn_context_event(
            &session.working_dir,
            context_limit,
            input.moim_parts,
            turn_start,
        )
        .filter(|event| Some(event.as_concat_text()) != last)
        .into_iter()
        .collect();
        Ok(PreparedInferenceRequest {
            system_prompt,
            tools,
            additional_messages,
        })
    }
}
