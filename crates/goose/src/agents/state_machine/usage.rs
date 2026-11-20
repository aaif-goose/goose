use anyhow::Result;
use oi_kit_providers::conversation::token_usage::{ProviderUsage, Usage as TokenUsage};

use crate::agents::state_machine::{ConversationEffect, GooseEffect};
use crate::conversation::message::MessageUsage;
use crate::conversation::Conversation;
use crate::providers::canonical_cost::resolve_usage_cost;
use crate::session::{Session, SessionManager};

fn attach_to_last_assistant(effects: &mut [GooseEffect], usage: &ProviderUsage) {
    let Some(message) = effects.iter_mut().rev().find_map(|effect| match effect {
        GooseEffect::Conversation(ConversationEffect::AppendMessage(message))
            if message.role == rmcp::model::Role::Assistant && message.error_kind().is_none() =>
        {
            Some(message)
        }
        _ => None,
    }) else {
        return;
    };
    message.metadata.usage = Some(Box::new(MessageUsage::from_provider_usage(usage, false)));
}

pub(super) fn is_compaction(session: &Session, effects: &[GooseEffect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, GooseEffect::CompactConversation(_)))
        || session
            .conversation
            .as_ref()
            .is_some_and(oi_kit_agent::compaction::awaits_compaction_reply)
}

pub(super) fn enrich(session: &Session, effects: &mut [GooseEffect], compaction: bool) {
    for index in 0..effects.len() {
        let GooseEffect::RecordUsage(usage) = &effects[index] else {
            continue;
        };
        let (cost, cost_source) = resolve_usage_cost(session.provider_name.as_deref(), usage);

        let mut enriched = usage.clone();
        enriched.cost = cost;
        enriched.cost_source = cost_source;

        if !compaction {
            attach_to_last_assistant(effects, &enriched);
        }
        if let GooseEffect::RecordUsage(usage) = &mut effects[index] {
            *usage = enriched;
        }
    }
}

pub(super) async fn record(
    session_manager: &SessionManager,
    session: &Session,
    usage: &ProviderUsage,
    compaction: bool,
) -> Result<()> {
    let ledger = MessageUsage::from_provider_usage(usage, compaction);
    session_manager
        .record_usage_metrics(
            &session.id,
            session.schedule_id.clone(),
            usage.usage,
            &usage.model,
            &ledger,
        )
        .await?;
    Ok(())
}

pub(super) async fn estimate_context(conversation: &Conversation) -> Result<TokenUsage> {
    let tokens = crate::context_mgmt::count_context_tokens(conversation.messages()).await?;
    Ok(TokenUsage::new(Some(tokens), None, Some(tokens)))
}
