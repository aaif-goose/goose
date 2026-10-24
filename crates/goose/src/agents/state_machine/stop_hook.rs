//! Lets stop hooks accept or block a completed assistant turn.

use async_trait::async_trait;
use goose_agent::hook::{Hook, HookDecision};
use goose_agent::stop_hook::Stop;
use rmcp::model::Role;

use crate::conversation::message::{Message, SystemNotificationType};
use crate::hooks::{HookContext, HookEvent, HookManager, PluginHookDecision};
use crate::session::Session;

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

pub fn deny_limit_warning(limit: u32) -> Message {
    Message::assistant().with_system_notification(
        SystemNotificationType::InlineMessage,
        format!(
            "Stop hooks blocked the turn from ending more than {limit} consecutive times \
             \u{2014} overriding and ending turn to avoid an infinite loop. Set \
             GOOSE_STOP_HOOK_BLOCK_CAP to raise this limit."
        ),
    )
}

pub struct GooseStopHook {
    hook_manager: HookManager,
}

impl GooseStopHook {
    pub fn new(hook_manager: HookManager) -> Self {
        Self { hook_manager }
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
impl<'a> Hook<Session, Stop<'a>> for GooseStopHook {
    async fn decide(&self, session: &Session, event: &Stop<'a>) -> HookDecision {
        let context = HookContext::new(HookEvent::Stop, &session.id)
            .with_last_assistant_message(Self::trailing_assistant_text(
                event.conversation.messages(),
            ))
            .with_working_dir(session.working_dir.to_string_lossy().into_owned());
        match self
            .hook_manager
            .emit_blocking(HookEvent::Stop, context)
            .await
        {
            PluginHookDecision::Allow => HookDecision::Allow,
            PluginHookDecision::Deny { reason, plugin } => HookDecision::Deny {
                reason: denial_reason(&plugin, &reason),
                user_message: Some(denial_user_message(&plugin)),
            },
        }
    }
}
