use anyhow::Result;
use goose_provider_types::conversation::Conversation;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};

use crate::operation::{Emitter, OperationFuture, OperationResult};

pub struct ParsedCommand<'a> {
    pub name: &'a str,
    pub arguments: &'a str,
}

pub fn parse_slash_command(message: &str) -> Option<ParsedCommand<'_>> {
    let command = message.trim().strip_prefix('/')?;
    let (name, arguments) = command
        .split_once(' ')
        .map(|(command, params)| (command, params.trim()))
        .unwrap_or((command, ""));
    Some(ParsedCommand { name, arguments })
}

pub fn normalize_command_name(name: &str) -> String {
    name.trim_start_matches('/').to_lowercase()
}

trait CommandAction<S, E: MaybeSend + 'static>: MaybeSend + MaybeSync {
    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        session: &'a S,
        conversation: &'a Conversation,
        emit: &'a Emitter,
    ) -> OperationFuture<'a, Result<OperationResult<E>>>;
}

struct BoundCommandAction<'a, T: ?Sized, F> {
    target: &'a T,
    execute: F,
}

impl<S, E, T, F> CommandAction<S, E> for BoundCommandAction<'_, T, F>
where
    E: MaybeSend + 'static,
    T: MaybeSync + ?Sized,
    F: MaybeSend
        + MaybeSync
        + for<'a> Fn(
            &'a T,
            &'a str,
            &'a S,
            &'a Conversation,
            &'a Emitter,
        ) -> OperationFuture<'a, Result<OperationResult<E>>>,
{
    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        session: &'a S,
        conversation: &'a Conversation,
        emit: &'a Emitter,
    ) -> OperationFuture<'a, Result<OperationResult<E>>> {
        (self.execute)(self.target, arguments, session, conversation, emit)
    }
}

pub struct Command<'a, S, E: MaybeSend + 'static> {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub input_hint: Option<String>,
    pub meta: serde_json::Map<String, serde_json::Value>,
    action: Box<dyn CommandAction<S, E> + 'a>,
}

impl<'a, S, E: MaybeSend + 'static> Command<'a, S, E> {
    pub fn new<T, F>(
        name: impl AsRef<str>,
        description: impl Into<String>,
        target: &'a T,
        execute: F,
    ) -> Self
    where
        T: MaybeSync + ?Sized,
        F: MaybeSend
            + MaybeSync
            + for<'b> Fn(
                &'b T,
                &'b str,
                &'b S,
                &'b Conversation,
                &'b Emitter,
            ) -> OperationFuture<'b, Result<OperationResult<E>>>
            + 'a,
    {
        Self {
            name: normalize_command_name(name.as_ref()),
            aliases: Vec::new(),
            description: description.into(),
            input_hint: None,
            meta: serde_json::Map::new(),
            action: Box::new(BoundCommandAction { target, execute }),
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
            .execute(arguments, session, conversation, emit)
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
