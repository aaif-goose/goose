use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use goose_provider_types::conversation::message::Message;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum HookDecision {
    Allow,
    Deny {
        reason: String,
        user_message: Option<Message>,
    },
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait Hook<S, E>: MaybeSend + MaybeSync {
    async fn decide(&self, session: &S, event: &E) -> Result<HookDecision>;
}

pub async fn run_hooks<S, E>(
    hooks: &[Arc<dyn Hook<S, E>>],
    session: &S,
    event: &E,
) -> Result<HookDecision> {
    for hook in hooks {
        let decision = hook.decide(session, event).await?;
        if matches!(decision, HookDecision::Deny { .. }) {
            return Ok(decision);
        }
    }
    Ok(HookDecision::Allow)
}
