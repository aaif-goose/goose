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
pub const DEFAULT_DENY_LIMIT: u32 = 8;

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

fn deny_limit_warning(limit: u32) -> Message {
    Message::assistant().with_system_notification(
        SystemNotificationType::InlineMessage,
        format!(
            "Stop hooks blocked the turn from ending more than {limit} consecutive times \
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
    deny_limit: u32,
}

impl StopHookOperation {
    pub fn new(hook_manager: HookManager, deny_limit: u32) -> Self {
        Self {
            hook_manager,
            deny_limit,
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
        if denials > self.deny_limit {
            let warning = emit.message(deny_limit_warning(self.deny_limit));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::state_machine::RunStatus;
    use crate::plugins::discovery::{DiscoveredPlugin, PluginScope};
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    struct BlockingStopHook {
        _temp_dir: tempfile::TempDir,
        plugin_dir: std::path::PathBuf,
    }

    impl BlockingStopHook {
        fn new() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            let plugin_dir = temp_dir.path().join("test-plugin");
            std::fs::create_dir_all(plugin_dir.join("hooks")).unwrap();
            std::fs::write(
                plugin_dir.join("hooks/hooks.json"),
                r#"{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "sh ${PLUGIN_ROOT}/hook.sh"}]}]}}"#,
            )
            .unwrap();
            std::fs::write(
                plugin_dir.join("hook.sh"),
                "#!/bin/sh\necho \"not done yet\" >&2\nexit 2\n",
            )
            .unwrap();
            Self {
                _temp_dir: temp_dir,
                plugin_dir,
            }
        }

        fn hook_manager(&self) -> HookManager {
            HookManager::from_plugins_for_test(vec![DiscoveredPlugin {
                name: "test-plugin".into(),
                root: self.plugin_dir.clone(),
                scope: PluginScope::Project,
            }])
        }
    }

    async fn run_status(
        operation: &StopHookOperation,
        messages: Vec<Message>,
    ) -> Result<RunStatus> {
        let (tx, _rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, CancellationToken::new());
        match operation
            .run(
                &Session::default(),
                &Conversation::new_unvalidated(messages),
                &emit,
            )
            .await?
        {
            OperationResult::Applied(step) => Ok(step.status),
            OperationResult::NotApplicable => panic!("stop hook should apply at the end of a turn"),
        }
    }

    #[tokio::test]
    async fn the_deny_limit_counts_only_the_current_turn() -> Result<()> {
        let hook = BlockingStopHook::new();
        let operation = StopHookOperation::new(hook.hook_manager(), 1);
        let mut denial = Message::user()
            .with_text("denied")
            .with_visibility(false, true);
        operation.set_message_meta(&mut denial, DENIED, serde_json::json!(true));

        let previous_turn = vec![
            Message::user().with_text("first"),
            Message::assistant().with_text("done"),
            denial.clone(),
            Message::assistant().with_text("done"),
        ];
        let mut messages = previous_turn.clone();
        messages.extend([
            Message::user().with_text("second"),
            Message::assistant().with_text("done"),
        ]);
        assert!(matches!(
            run_status(&operation, messages).await?,
            RunStatus::Continuing
        ));

        let mut messages = previous_turn;
        messages.extend([
            Message::user().with_text("second"),
            Message::assistant().with_text("done"),
            denial,
            Message::assistant().with_text("done"),
        ]);
        assert!(matches!(
            run_status(&operation, messages).await?,
            RunStatus::Yielded
        ));
        Ok(())
    }
}
