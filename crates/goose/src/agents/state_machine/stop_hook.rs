use anyhow::Result;
use async_trait::async_trait;
use goose_agent::procedure::{kickoff_id, ProcedureRuntime};
use goose_agent::stop_hook::{StopHook, StopHookDecision};
use rmcp::model::Role;
use std::sync::{Arc, Mutex};

use crate::conversation::message::{Message, SystemNotificationType};
use crate::conversation::Conversation;
use crate::hooks::{HookContext, HookDecision, HookEvent, HookManager};
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
    /// Procedure control runtime, shared with the pre- and post-tool
    /// hooks. When set, ending the turn while the procedure is in a
    /// non-terminal state is denied, so the model keeps working the
    /// procedure instead of stopping early. `None` preserves the previous
    /// behavior exactly.
    procedure: Option<Arc<Mutex<ProcedureRuntime>>>,
}

impl GooseStopHook {
    pub fn new(hook_manager: HookManager) -> Self {
        Self {
            hook_manager,
            procedure: None,
        }
    }

    pub fn with_procedure(mut self, runtime: Arc<Mutex<ProcedureRuntime>>) -> Self {
        self.procedure = Some(runtime);
        self
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
impl StopHook<Session> for GooseStopHook {
    async fn on_stop(
        &self,
        session: &Session,
        conversation: &Conversation,
    ) -> Result<StopHookDecision> {
        if let Some(runtime) = &self.procedure {
            let kickoff = kickoff_id(conversation);
            if let Err(reason) = runtime.lock().unwrap().finish_check(kickoff.as_deref()) {
                return Ok(StopHookDecision::Deny {
                    reason: format!(
                        "Procedure control blocked ending this turn:\n\n{reason}\n\n\
                         Continue the procedure before trying to stop again."
                    ),
                    user_message: Some(Message::assistant().with_system_notification(
                        SystemNotificationType::InlineMessage,
                        "Procedure control blocked ending this turn: the procedure is not complete.",
                    )),
                });
            }
        }
        let context = HookContext::new(HookEvent::Stop, &session.id)
            .with_last_assistant_message(Self::trailing_assistant_text(conversation.messages()))
            .with_working_dir(session.working_dir.to_string_lossy().into_owned());
        match self
            .hook_manager
            .emit_blocking(HookEvent::Stop, context)
            .await
        {
            HookDecision::Allow => Ok(StopHookDecision::Allow),
            HookDecision::Deny { reason, plugin } => Ok(StopHookDecision::Deny {
                reason: denial_reason(&plugin, &reason),
                user_message: Some(denial_user_message(&plugin)),
            }),
        }
    }
}
