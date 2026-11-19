use oi_kit_provider_types::conversation::{
    message::{Message, MessageUsage},
    token_usage::ProviderUsage,
    Conversation,
};
use rmcp::model::ServerNotification;

/// How a reply finished after retries and stop-hook resolution.
///
/// Yielded once, as the last [`AgentEvent`] of a turn. Success is explicit so
/// callers cannot treat a provider failure that ended the stream cleanly as a
/// normal completion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplyOutcome {
    Completed,
    Failed { message: String },
    Cancelled,
}

#[derive(Clone, Debug)]
pub enum AgentEvent {
    Message(Message),
    Usage(ProviderUsage),
    MessageUsage {
        message_id: Option<String>,
        usage: MessageUsage,
    },
    McpNotification((String, ServerNotification)),
    HistoryReplaced(Conversation),
    ReplyOutcome(ReplyOutcome),
}
