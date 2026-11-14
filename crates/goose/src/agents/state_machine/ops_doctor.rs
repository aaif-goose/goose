//! Runs session diagnostics and feeds repair context back into the turn.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use rmcp::model::Role;

use crate::agents::state_machine::{
    applied, builtin_command, messages_since_kickoff, yielded_with, Command, ConversationEffect,
    Emitter, GooseEffect, Operation, OperationFuture, OperationResult,
};
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::session::{Session, SessionManager};

pub struct DoctorOperation {
    session_manager: Arc<SessionManager>,
}

impl DoctorOperation {
    pub fn new(session_manager: Arc<SessionManager>) -> Self {
        Self { session_manager }
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for DoctorOperation {
    fn name(&self) -> &'static str {
        "doctor"
    }

    async fn commands<'a>(
        &'a self,
        _session: &'a Session,
    ) -> Result<Vec<Command<'a, Session, GooseEffect>>> {
        Ok(vec![builtin_command(
            "doctor",
            "Check that your Goose setup is working",
            self,
            Self::doctor_command,
        )])
    }
}

impl DoctorOperation {
    fn doctor_command<'a>(
        &'a self,
        _arguments: &'a str,
        session: &'a Session,
        conversation: &'a Conversation,
        emit: &'a Emitter,
    ) -> OperationFuture<'a, Result<OperationResult<GooseEffect>>> {
        Box::pin(async move {
            let command_message = messages_since_kickoff(conversation)?
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("doctor command conversation has no kickoff message"))?;
            let message_id = command_message
                .id
                .clone()
                .ok_or_else(|| anyhow!("Persisted slash command message has no id"))?;
            let result = match crate::doctor::run(&self.session_manager, session).await {
                Ok(message) => message,
                Err(error) => Message::assistant().with_text(error.to_string()),
            };

            if result.role == Role::Assistant {
                let command_message = command_message.with_visibility(true, false);
                let result = result.with_visibility(true, false);
                emit.message(command_message);
                let result = emit.message(result);
                return yielded_with([
                    ConversationEffect::SetMessageVisibility {
                        message_id,
                        user_visible: true,
                        agent_visible: false,
                    }
                    .into(),
                    result.into(),
                ]);
            }

            applied([
                ConversationEffect::SetMessageVisibility {
                    message_id,
                    user_visible: true,
                    agent_visible: false,
                }
                .into(),
                result.with_visibility(false, true).into(),
            ])
        })
    }
}
