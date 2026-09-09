use anyhow::Result;
use async_trait::async_trait;

use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::{
    messages_since_kickoff, not_applicable, Emitter, Operation, OperationResult,
};
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::hooks::{HookContext, HookEvent, HookManager};
use crate::session::Session;

pub struct EntryHookOperation {
    hook_manager: HookManager,
}

impl EntryHookOperation {
    pub fn new(hook_manager: HookManager) -> Self {
        Self { hook_manager }
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for EntryHookOperation {
    fn name(&self) -> &'static str {
        "entry_hook"
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        _emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        if messages.iter().any(|message| {
            message.role == rmcp::model::Role::Assistant
                && ((message.is_user_visible() && message.is_agent_visible())
                    || message.error_kind().is_some())
        }) {
            return not_applicable();
        }

        let messages_before_kickoff =
            &conversation.messages()[..conversation.len() - messages.len()];
        if !messages_before_kickoff.iter().any(|message| {
            message.role == rmcp::model::Role::User
                && message.is_user_visible()
                && !message.is_tool_response()
        }) {
            self.hook_manager
                .emit(
                    HookEvent::SessionStart,
                    HookContext::new(HookEvent::SessionStart, &session.id)
                        .with_working_dir(session.working_dir.to_string_lossy().to_string()),
                )
                .await;
        }

        let prompt = messages
            .first()
            .map(Message::as_concat_text)
            .unwrap_or_default();
        if !prompt.is_empty() {
            self.hook_manager
                .emit(
                    HookEvent::UserPromptSubmit,
                    HookContext::new(HookEvent::UserPromptSubmit, &session.id)
                        .with_message(prompt)
                        .with_working_dir(session.working_dir.to_string_lossy().to_string()),
                )
                .await;
        }

        not_applicable()
    }
}
