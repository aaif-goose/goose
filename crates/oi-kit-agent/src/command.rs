//! Commands that operations offer and a dispatcher runs.

use std::marker::PhantomData;

use anyhow::Result;
use oi_kit_provider_types::conversation::Conversation;
use oi_kit_provider_types::maybe_send::{MaybeSend, MaybeSync};

use crate::operation::{Emitter, OperationFuture, OperationResult};

pub struct CommandCall<'c, 'a, S> {
    pub arguments: &'c str,
    pub session: &'c S,
    pub conversation: &'c Conversation,
    pub emit: &'c Emitter,
    // An action may borrow from the operation that created it for `'a`. This
    // marker tells the compiler that `'a` outlives every call, so the action's
    // future can hold both borrows.
    scope: PhantomData<&'c &'a ()>,
}

#[cfg(not(target_arch = "wasm32"))]
type Action<'a, S, E> = dyn for<'c> Fn(CommandCall<'c, 'a, S>) -> OperationFuture<'c, Result<OperationResult<E>>>
    + Send
    + Sync
    + 'a;

#[cfg(target_arch = "wasm32")]
type Action<'a, S, E> =
    dyn for<'c> Fn(CommandCall<'c, 'a, S>) -> OperationFuture<'c, Result<OperationResult<E>>> + 'a;

pub struct Command<'a, S, E> {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub input_hint: Option<String>,
    pub meta: serde_json::Map<String, serde_json::Value>,
    action: Box<Action<'a, S, E>>,
}

impl<'a, S, E> Command<'a, S, E> {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        action: impl for<'c> Fn(CommandCall<'c, 'a, S>) -> OperationFuture<'c, Result<OperationResult<E>>>
            + MaybeSend
            + MaybeSync
            + 'a,
    ) -> Self {
        Self {
            name: name.into(),
            aliases: Vec::new(),
            description: description.into(),
            input_hint: None,
            meta: serde_json::Map::new(),
            action: Box::new(action),
        }
    }

    pub fn answers_to(&self, name: &str) -> bool {
        std::iter::once(&self.name)
            .chain(&self.aliases)
            .any(|own| own == name)
    }

    pub async fn run(
        &self,
        arguments: &str,
        session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        (self.action)(CommandCall {
            arguments,
            session,
            conversation,
            emit,
            scope: PhantomData,
        })
        .await
    }
}
