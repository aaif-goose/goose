//! Handles a recognized slash command before the normal agent turn begins.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::{
    messages_since_kickoff, not_applicable, Emitter, Operation, OperationResult, SlashCommand,
};
use crate::conversation::Conversation;
use crate::session::Session;

pub struct SlashCommandOperation<'a> {
    operations: Vec<Arc<dyn Operation<Session, GooseEffect> + 'a>>,
}

fn parse_slash_command(message: &str) -> Option<SlashCommand<'_>> {
    let mut message = message.trim();
    if matches!(
        message,
        "/compact" | "Please compact this conversation" | "/summarize"
    ) {
        message = "/compact";
    }
    let command = message.strip_prefix('/')?;
    let (command, params_str) = command
        .split_once(' ')
        .map(|(command, params)| (command, params.trim()))
        .unwrap_or((command, ""));
    Some(SlashCommand {
        command,
        params_str,
    })
}

impl<'a> SlashCommandOperation<'a> {
    pub fn new(operations: Vec<Arc<dyn Operation<Session, GooseEffect> + 'a>>) -> Self {
        Self { operations }
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for SlashCommandOperation<'_> {
    fn name(&self) -> &'static str {
        "slash_command"
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        let Some(user_message) = messages.first() else {
            return not_applicable();
        };
        let message_text = user_message.as_concat_text();
        if messages.len() != 1 {
            return not_applicable();
        }
        let Some(command) = parse_slash_command(&message_text) else {
            return not_applicable();
        };

        for operation in &self.operations {
            match operation
                .run_command(&command, session, conversation, emit)
                .await?
            {
                OperationResult::NotApplicable => {}
                applied @ OperationResult::Applied(_) => return Ok(applied),
            }
        }

        not_applicable()
    }
}
