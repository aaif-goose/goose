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

const DENIED: &str = "denied";
pub const DEFAULT_BLOCK_CAP: u32 = 8;

fn denial_reason(plugin: &str, reason: &str) -> String {
    format!(
        "Stop hook `{plugin}` blocked ending this turn:\n\n\
         {reason}\n\n\
         Address this policy hook denial before trying to stop again."
    )
}

fn denial_user_message(plugin: &str) -> Message {
    Message::assistant().with_system_notification(
        SystemNotificationType::InlineMessage,
        format!("Stop hook `{plugin}` blocked ending this turn."),
    )
}

fn block_cap_warning(block_cap: u32) -> Message {
    Message::assistant().with_system_notification(
        SystemNotificationType::InlineMessage,
        format!(
            "Stop hooks blocked the turn from ending more than {block_cap} consecutive times \
             \u{2014} overriding and ending turn to avoid an infinite loop. Set \
             GOOSE_STOP_HOOK_BLOCK_CAP to raise this limit."
        ),
    )
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
        let context = HookContext::new(HookEvent::Stop, &session.id)
            .with_last_assistant_message(trailing_assistant_text(conversation.messages()))
            .with_working_dir(session.working_dir.to_string_lossy().into_owned());
        let (reason, plugin) = match self
            .hook_manager
            .emit_blocking(HookEvent::Stop, context)
            .await
        {
            HookDecision::Allow => return yielded(),
            HookDecision::Deny { reason, plugin } => (reason, plugin),
        };
        let denials = messages
            .iter()
            .filter(|message| self.message_meta(message, DENIED).is_some())
            .count() as u32
            + 1;
        if denials > self.block_cap {
            let warning = emit.message(block_cap_warning(self.block_cap));
            return yielded_with([warning.into()]);
        }
        let mut denial_message = Message::user()
            .with_text(denial_reason(&plugin, &reason))
            .with_visibility(false, true);
        self.set_message_meta(&mut denial_message, DENIED, serde_json::json!(true));
        emit.message(denial_user_message(&plugin));
        applied([denial_message.into()])
    }
}
