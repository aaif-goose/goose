//! Runs the command named by a kickoff message that starts with `/`.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use oi_kit_provider_types::conversation::Conversation;
use oi_kit_provider_types::maybe_send::{MaybeSend, MaybeSync};

use crate::command::Command;
use crate::operation::{
    messages_since_kickoff, not_applicable, Emitter, Operation, OperationResult,
};

#[cfg(not(target_arch = "wasm32"))]
type Resolver<'a, S, E> =
    dyn for<'c> Fn(Vec<Command<'c, S, E>>) -> Vec<Command<'c, S, E>> + Send + Sync + 'a;

#[cfg(target_arch = "wasm32")]
type Resolver<'a, S, E> = dyn for<'c> Fn(Vec<Command<'c, S, E>>) -> Vec<Command<'c, S, E>> + 'a;

pub struct SlashCommandOperation<'a, S, E: MaybeSend + 'static> {
    operations: Vec<Arc<dyn Operation<S, E> + 'a>>,
    resolver: Option<Box<Resolver<'a, S, E>>>,
}

impl<'a, S, E: MaybeSend + 'static> SlashCommandOperation<'a, S, E> {
    pub fn new(operations: Vec<Arc<dyn Operation<S, E> + 'a>>) -> Self {
        Self {
            operations,
            resolver: None,
        }
    }

    pub fn with_resolver(
        mut self,
        resolver: impl for<'c> Fn(Vec<Command<'c, S, E>>) -> Vec<Command<'c, S, E>>
            + MaybeSend
            + MaybeSync
            + 'a,
    ) -> Self {
        self.resolver = Some(Box::new(resolver));
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
        let [kickoff] = messages_since_kickoff(conversation)? else {
            return not_applicable();
        };
        let text = kickoff.agent_visible_content().as_concat_text();
        let Some(invocation) = text.trim().strip_prefix('/') else {
            return not_applicable();
        };
        let (name, arguments) = invocation.split_once(' ').unwrap_or((invocation, ""));
        let name = name.to_lowercase();

        let mut commands = Vec::new();
        for operation in &self.operations {
            commands.extend(operation.commands(session).await?);
        }
        if let Some(resolver) = &self.resolver {
            commands = resolver(commands);
        }
        let Some(command) = commands.iter().find(|command| command.answers_to(&name)) else {
            return not_applicable();
        };
        command
            .run(arguments.trim(), session, conversation, emit)
            .await
    }
}
