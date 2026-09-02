use anyhow::Result;
use goose_providers::conversation::token_usage::{ProviderUsage, Usage as TokenUsage};

use crate::agents::state_machine::{ConversationEffect, GooseEffect};
use crate::conversation::message::MessageUsage;
use crate::conversation::Conversation;
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
        let (cost, cost_source) = crate::providers::canonical_cost::resolve_usage_cost(
            session.provider_name.as_deref(),
            &usage,
        );

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::Message;
    use goose_providers::conversation::token_usage::{CostSource, Usage};

    #[test]
    fn enrich_propagates_user_configured_cost_to_effect_and_message() {
        let overrides = r#"[{"provider":"test-provider","model":"negotiated","input_usd_per_million_tokens":1.0,"output_usd_per_million_tokens":2.0}]"#;
        let _guard = env_lock::lock_env([("GOOSE_PRICING_OVERRIDES", Some(overrides))]);
        let session = Session {
            provider_name: Some("test-provider".to_string()),
            ..Session::default()
        };
        let usage = ProviderUsage::new(
            "negotiated".to_string(),
            Usage::new(Some(1_000_000), Some(500_000), None),
        );
        let mut effects = vec![
            Message::assistant().with_text("done").into(),
            GooseEffect::RecordUsage(usage),
        ];

        enrich(&session, &mut effects);

        let GooseEffect::RecordUsage(usage) = &effects[1] else {
            panic!("expected usage effect");
        };
        assert_eq!(usage.cost, Some(2.0));
        assert_eq!(usage.cost_source, Some(CostSource::UserConfigured));

        let GooseEffect::Conversation(ConversationEffect::AppendMessage(message)) = &effects[0]
        else {
            panic!("expected assistant message effect");
        };
        let message_usage = message.metadata.usage.as_ref().unwrap();
        assert_eq!(message_usage.cost, Some(2.0));
        assert_eq!(message_usage.cost_source, Some(CostSource::UserConfigured));
    }
}
