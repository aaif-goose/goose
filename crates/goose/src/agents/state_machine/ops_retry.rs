//! Decides whether a completed response should finish, continue, or retry the turn.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::time::Duration;

use crate::agents::retry::{
    execute_on_failure_command_with_timeout, execute_success_checks_with_timeout,
};
use crate::agents::state_machine::{
    applied, ends_turn, messages_since_kickoff, not_applicable, yielded_with, ConversationEffect,
    Emitter, GooseEffect, Operation, OperationResult, SlashCommand,
};
use crate::agents::types::RetryConfig;
use crate::conversation::message::{Message, MessageErrorKind, SystemNotificationType};
use crate::conversation::Conversation;
use crate::session::Session;
use tokio::sync::Mutex;

pub(super) const NUDGED: &str = "nudged";
pub(super) const ATTEMPTS: &str = "attempts";

fn retry_error(error: &str) -> Message {
    Message::assistant().with_error(
        MessageErrorKind::Other,
        format!("Retry logic encountered an error: {error}"),
    )
}

pub struct RetryOperation<'a> {
    goal: &'a Mutex<Option<String>>,
    grind: &'a Mutex<Option<String>>,
    retry_timeout: Duration,
    on_failure_timeout: Duration,
}

impl<'a> RetryOperation<'a> {
    pub fn new(
        goal: &'a Mutex<Option<String>>,
        grind: &'a Mutex<Option<String>>,
        retry_timeout: Duration,
        on_failure_timeout: Duration,
    ) -> Self {
        Self {
            goal,
            grind,
            retry_timeout,
            on_failure_timeout,
        }
    }

    fn retry_config(session: &Session) -> Option<&RetryConfig> {
        session
            .recipe
            .as_ref()
            .and_then(|recipe| recipe.retry.as_ref())
    }

    fn reset_conversation(conversation: &Conversation) -> Result<Conversation> {
        let messages = messages_since_kickoff(conversation)?;
        let kickoff = conversation.len() - messages.len();
        Ok(Conversation::new_unvalidated(
            conversation.messages()[..=kickoff].to_vec(),
        ))
    }

    fn goal_was_nudged(&self, messages: &[Message]) -> bool {
        messages
            .iter()
            .any(|message| self.message_meta(message, NUDGED).is_some())
    }

    /// Attempts are counted on the kickoff message because a retry replaces the
    /// conversation with everything up to and including it — the only message
    /// that outlives the attempt it belongs to.
    fn attempts(&self, messages: &[Message]) -> u32 {
        messages
            .first()
            .and_then(|message| self.message_meta(message, ATTEMPTS))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for RetryOperation<'_> {
    fn name(&self) -> &'static str {
        "retry"
    }

    async fn run_command(
        &self,
        command: &SlashCommand<'_>,
        _session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let target = match command.command {
            "goal" => &self.goal,
            "grind" => &self.grind,
            _ => return not_applicable(),
        };
        let label = if command.command == "goal" {
            "goal"
        } else {
            "grind goal"
        };
        let params = command.params_str;
        let starts_turn = !params.is_empty() && !matches!(params, "off" | "clear" | "none");

        let response = if params.is_empty() {
            match target.lock().await.clone() {
                Some(value) => Message::assistant().with_text(format!("Current {label}: {value}")),
                None => Message::assistant().with_text(format!(
                    "No {label} set. Use `/{command_name} <description>` to set one.",
                    command_name = command.command
                )),
            }
        } else if !starts_turn {
            *target.lock().await = None;
            let text = if command.command == "goal" {
                "Goal cleared. The agent will finish normally."
            } else {
                "Grind cleared. The agent will finish normally."
            };
            Message::assistant().with_text(text)
        } else {
            *target.lock().await = Some(params.to_string());
            let text = if command.command == "goal" {
                format!(
                    "Goal set. The agent will verify this goal is met before finishing:\n\n> {params}"
                )
            } else {
                format!(
                    "Grind goal set. The agent will keep working until max_turns is reached:\n\n> {params}"
                )
            };
            Message::assistant().with_text(text)
        };

        let command_message = messages_since_kickoff(conversation)?
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("slash command conversation has no kickoff message"))?;
        let message_id = command_message
            .id
            .clone()
            .ok_or_else(|| anyhow!("Persisted slash command message has no id"))?;
        let command_message = command_message.with_visibility(true, false);
        let response = response.with_visibility(true, false);
        emit.message(command_message).await;
        let response = emit.message(response).await;

        let mut effects = vec![
            ConversationEffect::SetMessageVisibility {
                message_id,
                user_visible: true,
                agent_visible: false,
            }
            .into(),
            response.into(),
        ];
        if starts_turn {
            effects.push(
                Message::user()
                    .with_text(format!(
                        "Start working toward this goal now:\n\n**Goal:** {params}"
                    ))
                    .with_visibility(false, true)
                    .into(),
            );
        } else {
            return yielded_with(effects);
        }
        applied(effects)
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

        if !self.goal_was_nudged(messages) {
            if let Some(goal) = self.goal.lock().await.clone() {
                let nudge = format!(
                    "Before finishing, check whether the following goal has been fully met:\n\n\
                     **Goal:** {goal}\n\n\
                     If not, continue working toward it."
                );
                let mut message = Message::user()
                    .with_text(&nudge)
                    .with_visibility(false, true);
                self.set_message_meta(&mut message, NUDGED, serde_json::json!(true));
                emit.message(Message::assistant().with_system_notification(
                    SystemNotificationType::InlineMessage,
                    format!("Goal: {goal}"),
                ))
                .await;
                return applied([message.into()]);
            }
        }

        if let Some(grind) = self.grind.lock().await.clone() {
            let nudge = format!(
                "Keep working. The grind goal is not yet complete:\n\n\
                 **Goal:** {grind}\n\n\
                 Continue until it is fully done."
            );
            let message = Message::user()
                .with_text(&nudge)
                .with_visibility(false, true);
            emit.message(Message::assistant().with_system_notification(
                SystemNotificationType::InlineMessage,
                format!("Grind: {grind}"),
            ))
            .await;
            return applied([message.into()]);
        }

        *self.goal.lock().await = None;
        *self.grind.lock().await = None;

        let Some(retry_config) = Self::retry_config(session) else {
            return not_applicable();
        };

        let retry_timeout = retry_config
            .timeout_seconds
            .map(Duration::from_secs)
            .unwrap_or(self.retry_timeout);
        let success =
            execute_success_checks_with_timeout(&retry_config.checks, retry_timeout).await;
        let success = match success {
            Ok(success) => success,
            Err(error) => {
                let message = emit.message(retry_error(&error.to_string())).await;
                return applied([message.into()]);
            }
        };
        if success {
            return not_applicable();
        }

        let attempts = self.attempts(messages);
        if attempts >= retry_config.max_retries {
            let message = Message::assistant().with_error(
                MessageErrorKind::Other,
                format!(
                    "Maximum retry attempts ({}) exceeded. Unable to complete the task successfully.",
                    retry_config.max_retries
                ),
            );
            #[cfg(feature = "telemetry")]
            crate::posthog::emit_error(
                "retry_max_exceeded",
                &format!("Max retries ({}) exceeded", retry_config.max_retries),
            );
            let message = emit.message(message).await;
            return applied([message.into()]);
        }

        if let Some(command) = &retry_config.on_failure {
            let timeout = retry_config
                .on_failure_timeout_seconds
                .map(Duration::from_secs)
                .unwrap_or(self.on_failure_timeout);
            if let Err(error) = execute_on_failure_command_with_timeout(command, timeout).await {
                let message = emit.message(retry_error(&error.to_string())).await;
                return applied([message.into()]);
            }
        }

        let mut reset = Self::reset_conversation(conversation)?;
        if let Some(kickoff) = reset.messages_mut().last_mut() {
            self.set_message_meta(kickoff, ATTEMPTS, serde_json::json!(attempts + 1));
        }
        applied([reset.into()])
    }
}
