//! Goose-specific inference request preparation.

use crate::agents::system_prompt_state;
#[cfg(feature = "code-mode")]
use crate::agents::ExtensionManager;
use crate::agents::PromptManager;
use crate::config::GooseMode;
use crate::session::{Session, SessionManager};
use crate::tool_inspection::ToolInspectionManager;
use anyhow::Result;
use async_trait::async_trait;
use goose_agent::inference::{InferenceRequestPreparer, PreparedInferenceRequest};
use goose_agent::operation::{messages_since_kickoff, InferenceInput};
use goose_providers::conversation::message::Message;
use goose_providers::conversation::Conversation;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct GooseInferenceRequestPreparer<'a> {
    #[cfg(feature = "code-mode")]
    pub(crate) extension_manager: Arc<ExtensionManager>,
    pub(crate) goose_mode: &'a Mutex<GooseMode>,
    pub(crate) prompt_manager: &'a Mutex<PromptManager>,
    pub(crate) tool_inspection_manager: &'a ToolInspectionManager,
    pub(crate) context_limit: usize,
    pub(crate) session_manager: Arc<SessionManager>,
    pub(crate) provider_manages_own_context: bool,
    pub(crate) toolshim: bool,
}

#[async_trait]
impl InferenceRequestPreparer<Session> for GooseInferenceRequestPreparer<'_> {
    async fn prepare(
        &self,
        session: &Session,
        conversation: &Conversation,
        input: InferenceInput,
    ) -> Result<PreparedInferenceRequest> {
        #[cfg(feature = "code-mode")]
        let code_execution_mode = self
            .extension_manager
            .is_extension_enabled(
                crate::agents::platform_extensions::code_execution::EXTENSION_NAME,
            )
            .await;
        #[cfg(not(feature = "code-mode"))]
        let code_execution_mode = false;

        let goose_mode = *self.goose_mode.lock().await;
        if goose_mode == GooseMode::SmartApprove {
            self.tool_inspection_manager
                .apply_tool_annotations(&input.tools);
        }
        let tools =
            crate::agents::reply_parts::prepare_inference_tools(input.tools, code_execution_mode);
        let mut prompt_parts = input.prompt_parts;
        if self.toolshim {
            prompt_parts.push((
                "tools".to_string(),
                crate::providers::toolshim::tool_json_instructions(&tools),
            ));
        }
        let sections = self
            .prompt_manager
            .lock()
            .await
            .build_system_prompt_sections(&session.working_dir, prompt_parts, goose_mode);
        // A provider that owns the conversation context replays nothing, so there
        // is no prefix to keep stable and a fresh CLI context needs the live prompt.
        let (system_prompt, system_update) = if self.provider_manages_own_context {
            (sections.render(), None)
        } else {
            let frozen = system_prompt_state::freeze(
                &self.session_manager,
                &session.id,
                sections,
                conversation,
            )
            .await?;
            (frozen.system, frozen.update)
        };
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
        let mut additional_messages: Vec<Message> = crate::agents::moim::turn_context_event(
            &session.working_dir,
            context_limit,
            input.moim_parts,
            turn_start,
        )
        .filter(|event| Some(event.as_concat_text()) != last)
        .into_iter()
        .collect();
        // A system update has to be the last message before the assistant turn.
        if let Some(pending) = system_update {
            additional_messages.push(pending.message.clone());
            pending.commit(&self.session_manager, &session.id).await?;
        }
        Ok(PreparedInferenceRequest {
            system_prompt,
            tools,
            additional_messages,
        })
    }
}
