//! Provider-neutral interfaces for live, bidirectional voice sessions.
//!
//! Voice providers differ from text completion providers: they establish a
//! long-lived media/event session, accept incremental context, and may delegate
//! work to the client while audio is flowing.

use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, pin::Pin};

/// An asynchronous stream of provider events.
pub type VoiceEventStream = Pin<Box<dyn Stream<Item = Result<VoiceEvent>> + Send>>;

/// Static settings used to establish a voice session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSessionConfig {
    pub model: String,
    pub instructions: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    #[serde(default)]
    pub initial_items: Vec<VoiceMessage>,
    #[serde(default)]
    pub delegation: VoiceDelegationMode,
    #[serde(default)]
    pub extra: BTreeMap<String, Value>,
}

/// A text message supplied while creating a voice session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceMessage {
    pub role: VoiceMessageRole,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VoiceMessageRole {
    User,
    Assistant,
    Developer,
}

/// Where delegated reasoning is performed.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceDelegationMode {
    #[default]
    Disabled,
    Client,
    Provider,
}

/// Text channels have different conversational semantics.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VoiceContextChannel {
    Speakable,
    Commentary,
    Developer,
}

/// Context appended to the active session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceContext {
    pub text: String,
    pub channel: VoiceContextChannel,
}

/// A question delegated by the live model to the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceDelegation {
    pub id: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_turn_id: Option<String>,
}

/// Events normalized across live voice providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VoiceEvent {
    SessionStarted { session_id: Option<String> },
    InputTranscriptDelta { text: String },
    InputTranscriptCompleted { text: String },
    OutputTranscriptDelta { text: String },
    OutputTranscriptCompleted { text: String },
    DelegationCreated { delegation: VoiceDelegation },
    ContextAppended,
    DelegationContextAppended { delegation_id: String },
    Usage { usage: Value },
    Error { message: String, raw: Option<Value> },
    SessionClosed,
    Other { event_type: String, payload: Value },
}

/// Media signaling supplied by an application-specific transport.
///
/// The first implementation uses a WebRTC SDP offer/answer exchange. Keeping
/// signaling separate lets UI crates own microphone playback and data channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSessionOffer {
    pub sdp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSessionAnswer {
    pub sdp: String,
    pub session_id: Option<String>,
    pub model: String,
}

/// A provider capable of creating live voice sessions.
#[async_trait]
pub trait VoiceProvider: Send + Sync {
    fn name(&self) -> &str;

    /// Exchanges transport signaling and creates a live voice session.
    async fn create_session(
        &self,
        config: &VoiceSessionConfig,
        offer: VoiceSessionOffer,
    ) -> Result<VoiceSessionAnswer>;

    /// Decodes one raw provider event into the provider-neutral event model.
    fn decode_event(&self, event: Value) -> Result<VoiceEvent>;

    /// Encodes incremental context for the provider's event channel.
    fn append_context_event(&self, context: VoiceContext) -> Result<Value>;

    /// Encodes a result for a client-owned delegation.
    fn complete_delegation_event(
        &self,
        delegation_id: &str,
        context: VoiceContext,
    ) -> Result<Value>;

    /// Encodes a graceful session-close event.
    fn close_event(&self) -> Value;
}
