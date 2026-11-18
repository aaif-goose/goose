use anyhow::Result;
use goose_providers::conversation::token_usage::{ProviderUsage, Usage as TokenUsage};

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

pub(super) fn enrich(session: &Session, effects: &mut [GooseEffect]) {
    for index in 0..effects.len() {
        let (usage, replaces_conversation) = match &effects[index] {
            GooseEffect::RecordUsage(usage) => (usage.clone(), false),
            GooseEffect::CompactConversation {
                usage: Some(usage), ..
            } => (usage.clone(), true),
            _ => continue,
        };
        let (cost, cost_source) = resolve_usage_cost(session.provider_name.as_deref(), &usage);

        let mut enriched = usage.clone();
        enriched.cost = cost;
        enriched.cost_source = cost_source;

        if !replaces_conversation {
            attach_to_last_assistant(effects, &enriched);
        }
        match &mut effects[index] {
            GooseEffect::RecordUsage(usage) => *usage = enriched,
            GooseEffect::CompactConversation { usage, .. } => *usage = Some(enriched),
            _ => {}
        }
    }
}

pub(super) async fn record(
    session_manager: &SessionManager,
    session: &Session,
    usage: &ProviderUsage,
    replaces_conversation: bool,
) -> Result<()> {
    let ledger = MessageUsage::from_provider_usage(usage, replaces_conversation);
    let mut current_usage = usage.usage;
    if let Some(context_tokens) = usage.context_tokens {
        current_usage.total_tokens = Some(context_tokens.clamp(0, i32::MAX as i64) as _);
    }
    session_manager
        .record_usage_metrics(
            &session.id,
            session.schedule_id.clone(),
            current_usage,
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
