use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use goose_provider_types::conversation::Conversation;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};

use crate::command::{normalize_command_name, parse_slash_command, CommandResolver};
use crate::operation::{
    messages_since_kickoff, not_applicable, Emitter, Operation, OperationResult,
};

pub struct SlashCommandOperation<'a, S, E: MaybeSend + 'static> {
    operations: Vec<Arc<dyn Operation<S, E> + 'a>>,
    resolver: Option<Arc<dyn CommandResolver<S, E> + 'a>>,
}

impl<'a, S, E: MaybeSend + 'static> SlashCommandOperation<'a, S, E> {
    pub fn new(operations: Vec<Arc<dyn Operation<S, E> + 'a>>) -> Self {
        Self {
            operations,
            resolver: None,
        }
    }

    pub fn with_resolver(mut self, resolver: impl CommandResolver<S, E> + 'a) -> Self {
        self.resolver = Some(Arc::new(resolver));
        self
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for SlashCommandOperation<'_, S, E>
where
    S: MaybeSend + MaybeSync,
    E: MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        "slash_command"
    }

    async fn run(
        &self,
        session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let messages = messages_since_kickoff(conversation)?;
        let Some(user_message) = messages.first() else {
            return not_applicable();
        };
        if messages.len() != 1 {
            return not_applicable();
        }
        let message_text = user_message.agent_visible_content().as_concat_text();
        let Some(command) = parse_slash_command(&message_text) else {
            return not_applicable();
        };
        let command_name = normalize_command_name(command.name);

        let mut commands = Vec::new();
        for operation in &self.operations {
            commands.extend(operation.commands(session).await?);
        }
        if let Some(resolver) = &self.resolver {
            commands = resolver.resolve(commands);
        }
        let Some(action) = commands
            .iter()
            .find(|candidate| candidate.matches(&command_name))
        else {
            return not_applicable();
        };

        action
            .execute(command.arguments, session, conversation, emit)
            .await
    }
}
