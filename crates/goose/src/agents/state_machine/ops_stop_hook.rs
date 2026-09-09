//! Lets stop hooks accept or block a completed assistant turn.

use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::Role;

use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::{
    applied, ends_turn, messages_since_kickoff, not_applicable, yielded, yielded_with, Emitter,
    Operation, OperationResult,
};
use crate::conversation::message::{Message, SystemNotificationType};
use crate::conversation::Conversation;
use crate::hooks::{HookContext, HookDecision, HookEvent, HookManager};
use crate::session::Session;

pub(super) const DENIED: &str = "denied";

fn denial_context_message(plugin: &str, reason: &str) -> Message {
    Message::user()
        .with_text(format!(
            "Stop hook `{plugin}` blocked ending this turn:\n\n\
             {reason}\n\n\
             Address this policy hook denial before trying to stop again."
        ))
        .with_visibility(false, true)
}

fn denial_notification(plugin: &str) -> Message {
    Message::assistant().with_system_notification(
        SystemNotificationType::InlineMessage,
        format!("Stop hook `{plugin}` blocked ending this turn."),
    )
}

fn block_cap_warning(plugin: &str, cap: u32) -> Message {
    Message::assistant().with_system_notification(
        SystemNotificationType::InlineMessage,
        format!(
            "Stop hook `{plugin}` blocked the turn from ending more than {cap} consecutive times \
             \u{2014} overriding and ending turn to avoid an infinite loop. Set \
             GOOSE_STOP_HOOK_BLOCK_CAP to raise this limit."
        ),
    )
}

pub struct StopHookOperation {
    hook_manager: HookManager,
    block_cap: u32,
}

impl StopHookOperation {
    pub fn new(hook_manager: HookManager, block_cap: u32) -> Self {
        Self {
            hook_manager,
            block_cap,
        }
    }

    fn trailing_assistant_text(messages: &[Message]) -> String {
        messages
            .iter()
            .rev()
            .take_while(|message| message.role == Role::Assistant)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .filter(|message| message.is_user_visible())
            .map(|message| message.user_visible_content().as_concat_text())
            .collect()
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for StopHookOperation {
    fn name(&self) -> &'static str {
        "stop_hook"
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        if !ends_turn(messages) {
            return not_applicable();
        }
        let last_assistant_text = Self::trailing_assistant_text(messages);

        let context = HookContext::new(HookEvent::Stop, &session.id)
            .with_last_assistant_message(last_assistant_text)
            .with_working_dir(session.working_dir.to_string_lossy().into_owned());
        match self
            .hook_manager
            .emit_blocking(HookEvent::Stop, context)
            .await
        {
            HookDecision::Allow => yielded(),
            HookDecision::Deny { reason, plugin } => {
                let blocks = messages
                    .iter()
                    .filter(|message| self.message_meta(message, DENIED).is_some())
                    .count() as u32
                    + 1;
                if blocks > self.block_cap {
                    let warning = block_cap_warning(&plugin, self.block_cap);
                    let warning = emit.message(warning).await;
                    yielded_with([warning.into()])
                } else {
                    let mut denial = denial_context_message(&plugin, &reason);
                    self.set_message_meta(&mut denial, DENIED, serde_json::json!(true));
                    emit.message(denial_notification(&plugin)).await;
                    applied([denial.into()])
                }
            }
        }
    }
}
