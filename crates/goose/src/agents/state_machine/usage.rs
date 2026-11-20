use anyhow::Result;
use oi_kit_providers::conversation::token_usage::{ProviderUsage, Usage as TokenUsage};

use crate::agents::state_machine::GooseEffect;
use crate::conversation::message::MessageUsage;
use crate::conversation::Conversation;
use crate::session::{Session, SessionManager};

pub(super) fn is_compaction(session: &Session, effects: &[GooseEffect]) -> bool {
    effects
        .iter()
        .any(|effect| matches!(effect, GooseEffect::CompactConversation(_)))
        || session
            .conversation
            .as_ref()
            .is_some_and(oi_kit_agent::compaction::awaits_compaction_reply)
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
