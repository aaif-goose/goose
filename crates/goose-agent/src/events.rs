#[cfg(not(feature = "std"))]
use crate::prelude::*;
use goose_provider_types::conversation::{
    message::{Message, MessageUsage},
    token_usage::ProviderUsage,
    Conversation,
};
#[cfg(feature = "mcp-interop")]
use rmcp::model::ServerNotification;

#[derive(Clone, Debug)]
pub enum AgentEvent {
    Message(Message),
    Usage(ProviderUsage),
    MessageUsage {
        message_id: Option<String>,
        usage: MessageUsage,
    },
    #[cfg(feature = "mcp-interop")]
    McpNotification((String, ServerNotification)),
    HistoryReplaced(Conversation),
}
