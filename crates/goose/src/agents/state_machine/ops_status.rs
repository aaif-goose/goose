use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use oi_kit_providers::base::Provider;
use oi_kit_providers::model::ModelConfig;

use crate::agents::state_machine::{
    messages_since_kickoff, yielded_with, Command, ConversationEffect, Emitter, GooseEffect,
    Operation, OperationResult,
};
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::session::Session;

pub struct StatusOperation {
    provider: Arc<dyn Provider>,
    model_config: ModelConfig,
}

impl StatusOperation {
    pub fn new(provider: Arc<dyn Provider>, model_config: ModelConfig) -> Self {
        Self {
            provider,
            model_config,
        }
    }

    fn clear(conversation: &Conversation, emit: &Emitter) -> Result<OperationResult<GooseEffect>> {
        let command = messages_since_kickoff(conversation)?
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("clear command conversation has no kickoff message"))?
            .with_visibility(true, false);
        let response = Message::assistant()
            .with_text("Conversation cleared")
            .with_visibility(true, false);
        let command = emit.message(command);
        let response = emit.message(response);
        yielded_with([
            Conversation::default().into(),
            command.into(),
            response.into(),
        ])
    }

    async fn status(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let context_limit = crate::context_limit::get_context_limit(
            self.provider.as_ref(),
            &self.model_config.model_name,
        )
        .await?;
        let context_tokens = session.usage.total_tokens.unwrap_or(0);
        let lifetime_tokens = session.accumulated_usage.total_tokens.unwrap_or(0);
        let context_pct = if context_limit > 0 {
            format!(
                "{}%",
                ((context_tokens as f64 / context_limit as f64) * 100.0)
                    .round()
                    .min(100.0) as usize
            )
        } else {
            "N/A".to_string()
        };
        let response = Message::assistant()
            .with_text(format!(
                "**Session status**\n\n\
                 - Session ID: `{}`\n\
                 - Model: {}\n\
                 - Provider: {}\n\
                 - Mode: {}\n\
                 - Tokens (lifetime): {}\n\
                 - Context: {} / {} tokens ({})",
                session.id,
                self.model_config.model_name,
                self.provider.get_name(),
                session.goose_mode,
                lifetime_tokens,
                context_tokens,
                context_limit,
                context_pct,
            ))
            .with_visibility(true, false);
        let command_message = messages_since_kickoff(conversation)?
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("status command conversation has no kickoff message"))?;
        let message_id = command_message
            .id
            .clone()
            .ok_or_else(|| anyhow!("Persisted slash command message has no id"))?;
        emit.message(command_message.with_visibility(true, false));
        let response = emit.message(response);
        yielded_with([
            ConversationEffect::SetMessageVisibility {
                message_id,
                user_visible: true,
                agent_visible: false,
            }
            .into(),
            response.into(),
        ])
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for StatusOperation {
    fn name(&self) -> &'static str {
        "status"
    }

    async fn commands(&self, _session: &Session) -> Result<Vec<Command<'_, Session, GooseEffect>>> {
        let mut commands = vec![Command::new(
            "status",
            "Show session status: session ID, model, provider, mode, and token usage",
            move |call| Box::pin(self.status(call.session, call.conversation, call.emit)),
        )];
        if !self.provider.manages_own_context() {
            commands.push(Command::new(
                "clear",
                "Clear the conversation history",
                |call| Box::pin(async move { Self::clear(call.conversation, call.emit) }),
            ));
        }
        Ok(commands)
    }
}
