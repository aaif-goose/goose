use anyhow::Result;
use async_trait::async_trait;
use goose_provider_types::conversation::Conversation;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};

use crate::operation::{Emitter, OperationResult};

pub struct CommandInvocation<'a> {
    pub name: &'a str,
    pub arguments: &'a str,
}

pub fn parse_slash_command(message: &str) -> Option<CommandInvocation<'_>> {
    let command = message.trim().strip_prefix('/')?;
    let (name, arguments) = command
        .split_once(' ')
        .map(|(command, params)| (command, params.trim()))
        .unwrap_or((command, ""));
    Some(CommandInvocation { name, arguments })
}

pub fn normalize_command_name(name: &str) -> String {
    name.trim_start_matches('/').to_lowercase()
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait CommandAction<S, E: MaybeSend + 'static>: MaybeSend + MaybeSync {
    async fn execute(
        &self,
        command: &CommandInvocation<'_>,
        session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>>;
}

pub struct Command<'a, S, E: MaybeSend + 'static> {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub input_hint: Option<String>,
    pub meta: serde_json::Map<String, serde_json::Value>,
    action: &'a dyn CommandAction<S, E>,
}

impl<'a, S, E: MaybeSend + 'static> Command<'a, S, E> {
    pub fn new(
        name: impl AsRef<str>,
        description: impl Into<String>,
        action: &'a dyn CommandAction<S, E>,
    ) -> Self {
        Self {
            name: normalize_command_name(name.as_ref()),
            aliases: Vec::new(),
            description: description.into(),
            input_hint: None,
            meta: serde_json::Map::new(),
            action,
        }
    }

    pub fn with_aliases(mut self, aliases: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        self.aliases = aliases
            .into_iter()
            .map(|alias| normalize_command_name(alias.as_ref()))
            .collect();
        self
    }

    pub fn with_input_hint(mut self, input_hint: impl Into<String>) -> Self {
        self.input_hint = Some(input_hint.into());
        self
    }

    pub fn with_meta(mut self, meta: serde_json::Map<String, serde_json::Value>) -> Self {
        self.meta = meta;
        self
    }

    pub fn matches(&self, name: &str) -> bool {
        let name = normalize_command_name(name);
        self.name == name || self.aliases.iter().any(|alias| alias == &name)
    }

    pub async fn execute(
        &self,
        arguments: &str,
        session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        self.action
            .execute(
                &CommandInvocation {
                    name: &self.name,
                    arguments,
                },
                session,
                conversation,
                emit,
            )
            .await
    }
}

pub trait CommandResolver<S, E: MaybeSend + 'static>: MaybeSend + MaybeSync {
    fn resolve<'a>(&self, commands: Vec<Command<'a, S, E>>) -> Vec<Command<'a, S, E>>;
}

impl<S, E, F> CommandResolver<S, E> for F
where
    E: MaybeSend + 'static,
    F: MaybeSend + MaybeSync + for<'a> Fn(Vec<Command<'a, S, E>>) -> Vec<Command<'a, S, E>>,
{
    fn resolve<'a>(&self, commands: Vec<Command<'a, S, E>>) -> Vec<Command<'a, S, E>> {
        self(commands)
    }
}
