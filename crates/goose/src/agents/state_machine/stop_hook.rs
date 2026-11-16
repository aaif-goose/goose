use anyhow::Result;
use rmcp::model::Role;

use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::{
    ends_turn, messages_since_kickoff, Emitter, RunStatus, StepResult,
};
use crate::conversation::message::{Message, SystemNotificationType};
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

const STOP_HOOK: &str = "stop_hook";

pub(super) async fn run_stop_hooks(
    hook_manager: &HookManager,
    block_cap: u32,
    session: &Session,
    emit: &Emitter,
) -> Result<StepResult<GooseEffect>> {
    let yielded = |effects| StepResult {
        effects,
        applied_step: None,
        status: RunStatus::Yielded,
    };
    let Some(conversation) = &session.conversation else {
        return Ok(yielded(Vec::new()));
    };
    let messages = messages_since_kickoff(conversation)?;
    if !ends_turn(messages) {
        return Ok(yielded(Vec::new()));
    }
    let context = HookContext::new(HookEvent::Stop, &session.id)
        .with_last_assistant_message(trailing_assistant_text(conversation.messages()))
        .with_working_dir(session.working_dir.to_string_lossy().into_owned());
    let (reason, plugin) = match hook_manager.emit_blocking(HookEvent::Stop, context).await {
        HookDecision::Allow => return Ok(yielded(Vec::new())),
        HookDecision::Deny { reason, plugin } => (reason, plugin),
    };
    let denials = messages
        .iter()
        .filter(|message| message.metadata.operation_note(STOP_HOOK, DENIED).is_some())
        .count() as u32
        + 1;
    if denials > block_cap {
        let warning = emit.message(block_cap_warning(block_cap));
        return Ok(yielded(vec![warning.into()]));
    }
    let mut denial_message = Message::user()
        .with_text(denial_reason(&plugin, &reason))
        .with_visibility(false, true);
    denial_message
        .metadata
        .set_operation_note(STOP_HOOK, DENIED, serde_json::json!(true));
    emit.message(denial_user_message(&plugin));
    Ok(StepResult {
        effects: vec![denial_message.into()],
        applied_step: Some(STOP_HOOK),
        status: RunStatus::Continuing,
    })
}
