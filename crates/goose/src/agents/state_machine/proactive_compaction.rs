use anyhow::Result;

use crate::agents::state_machine::{last_effective_role, messages_since_kickoff};
use crate::context_mgmt::count_context_tokens;
use crate::conversation::message::Message;
use crate::conversation::{Conversation, EffectiveRole};
use crate::session::Session;

/// Reported usage stops at the inference that requested the tools, so the
/// results that answered it are not counted until the next request.
async fn unreported_tool_tokens(conversation: &Conversation) -> Result<i32> {
    let messages = conversation.messages();
    if last_effective_role(messages)? != EffectiveRole::Tool {
        return Ok(0);
    }
    let after_request = messages
        .iter()
        .rposition(Message::is_tool_call)
        .map_or(0, |index| index + 1);
    count_context_tokens(&messages[after_request..]).await
}

/// The context size when proactive compaction should run before the next
/// inference, or `None` when it should not.
pub(super) async fn proactive_compaction_tokens(
    session: &Session,
    conversation: &Conversation,
    context_limit: usize,
    threshold: f64,
) -> Result<Option<i32>> {
    if threshold <= 0.0 || threshold >= 1.0 {
        return Ok(None);
    }
    // An assistant tail ends the turn, so no inference follows.
    if last_effective_role(messages_since_kickoff(conversation)?)? == EffectiveRole::Assistant {
        return Ok(None);
    }
    let tokens = match session.usage.total_tokens {
        Some(tokens) => tokens + unreported_tool_tokens(conversation).await?,
        None => count_context_tokens(conversation.messages()).await?,
    };
    Ok((tokens > 0 && tokens as f64 / context_limit as f64 > threshold).then_some(tokens))
}
