//! Interfaces for full-duplex voice providers.
//!
//! Providers prepare provider-specific signaling or connection plans. Trusted
//! signalers execute authenticated WebRTC negotiation, while connections own
//! only established live event delivery.

use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
use tokio::{
    sync::{Mutex, broadcast, mpsc, watch},
    time::{Duration, timeout},
};

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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceDelegationMode {
    #[default]
    Disabled,
    Client,
    Provider,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VoiceContextChannel {
    Speakable,
    Commentary,
    Developer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceContext {
    pub text: String,
    pub channel: VoiceContextChannel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceDelegation {
    pub id: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_turn_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceEvent {
    pub kind: VoiceEventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<Value>,
}

impl VoiceEvent {
    pub fn provider(kind: VoiceEventKind, raw: Value) -> Self {
        Self {
            kind,
            raw: Some(raw),
        }
    }

    pub fn session(kind: VoiceEventKind) -> Self {
        Self { kind, raw: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VoiceEventKind {
    SessionStarted {
        session_id: Option<String>,
    },
    InputTranscriptDelta {
        text: String,
    },
    InputTranscriptCompleted {
        text: String,
    },
    OutputTranscriptDelta {
        text: String,
    },
    OutputTranscriptCompleted {
        text: String,
    },
    OutputAudioDelta {
        audio: String,
        start_ms: Option<u64>,
        end_ms: Option<u64>,
    },
    DelegationCreated {
        delegation: VoiceDelegation,
    },
    ContextAppended,
    DelegationContextAppended {
        delegation_id: String,
    },
    Usage {
        usage: Value,
    },
    Error {
        message: String,
        raw: Option<Value>,
    },
    SessionClosed,
    Other {
        event_type: String,
    },
}

pub struct WebRtcSignalingPlan {
    pub(crate) endpoint: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) offer_sdp: String,
    pub(crate) session: Value,
    pub(crate) model: String,
}

impl WebRtcSignalingPlan {
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn take_parts(self) -> (String, Vec<(String, String)>, String, Value, String) {
        (
            self.endpoint,
            self.headers,
            self.offer_sdp,
            self.session,
            self.model,
        )
    }
}

pub struct WebSocketConnectionPlan {
    pub(crate) endpoint: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) after_connect: Vec<Value>,
    pub(crate) model: String,
}

#[cfg(test)]
impl WebSocketConnectionPlan {
    pub(crate) fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub(crate) fn after_connect(&self) -> &[Value] {
        &self.after_connect
    }
}

impl WebSocketConnectionPlan {
    pub fn take_parts(self) -> (String, Vec<(String, String)>, Vec<Value>, String) {
        (self.endpoint, self.headers, self.after_connect, self.model)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiatedWebRtcSession {
    pub answer_sdp: String,
    pub session_id: Option<String>,
    pub model: String,
}

#[async_trait]
pub trait VoiceSignaler: Send + Sync {
    async fn negotiate(&self, plan: WebRtcSignalingPlan) -> Result<NegotiatedWebRtcSession>;
}

#[async_trait]
pub trait VoiceConnection: Send + Sync {
    async fn send(&self, event: Value) -> Result<()>;
    async fn receive(&self) -> Result<Option<Value>>;
    async fn close(&self) -> Result<()>;
}

pub trait VoiceProvider: Send + Sync {
    fn name(&self) -> &str;
    fn prepare_webrtc(
        &self,
        config: &VoiceSessionConfig,
        offer_sdp: String,
    ) -> Result<WebRtcSignalingPlan>;
    fn prepare_websocket(&self, config: &VoiceSessionConfig) -> Result<WebSocketConnectionPlan>;
    fn decode_event(&self, event: Value) -> Result<VoiceEvent>;
    fn append_context_event(&self, context: VoiceContext) -> Result<Value>;
    fn complete_delegation_event(
        &self,
        delegation_id: &str,
        context: VoiceContext,
    ) -> Result<Value>;
    fn append_audio_event(&self, _audio: &[u8]) -> Result<Value> {
        bail!("this provider/connection does not accept JSON audio events")
    }
    fn pause_input_event(&self) -> Value;
    fn resume_input_event(&self) -> Value;
    fn close_event(&self) -> Value;
}

pub struct VoiceSessionMetadata {
    pub session_id: Option<String>,
    pub model: String,
}

impl From<&NegotiatedWebRtcSession> for VoiceSessionMetadata {
    fn from(session: &NegotiatedWebRtcSession) -> Self {
        Self {
            session_id: session.session_id.clone(),
            model: session.model.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VoiceSessionState {
    Open,
    Closing,
    Closed,
}

pub struct VoiceSession {
    provider: Arc<dyn VoiceProvider>,
    connection: Arc<dyn VoiceConnection>,
    model: String,
    session_id: Mutex<Option<String>>,
    state_tx: watch::Sender<VoiceSessionState>,
    events: broadcast::Sender<VoiceEvent>,
}

impl VoiceSession {
    pub fn new(
        provider: Arc<dyn VoiceProvider>,
        connection: Arc<dyn VoiceConnection>,
        metadata: VoiceSessionMetadata,
    ) -> Arc<Self> {
        let (events, _) = broadcast::channel(256);
        let (state_tx, _) = watch::channel(VoiceSessionState::Open);
        Arc::new(Self {
            provider,
            connection,
            model: metadata.model,
            session_id: Mutex::new(metadata.session_id),
            state_tx,
            events,
        })
    }

    pub async fn session_id(&self) -> Option<String> {
        self.session_id.lock().await.clone()
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn subscribe(&self) -> broadcast::Receiver<VoiceEvent> {
        self.events.subscribe()
    }

    pub async fn handle_incoming(&self, raw: Value) -> Result<VoiceEvent> {
        let event = self.provider.decode_event(raw)?;
        if let VoiceEventKind::SessionStarted { session_id } = &event.kind {
            *self.session_id.lock().await = session_id.clone();
        }
        let _ = self.events.send(event.clone());
        Ok(event)
    }

    pub async fn run(self: Arc<Self>) -> Result<()> {
        while let Some(raw) = self.connection.receive().await? {
            if matches!(
                self.handle_incoming(raw).await?.kind,
                VoiceEventKind::SessionClosed
            ) {
                let _ = self.state_tx.send(VoiceSessionState::Closed);
                return Ok(());
            }
        }
        let _ = self.state_tx.send(VoiceSessionState::Closed);
        let _ = self
            .events
            .send(VoiceEvent::session(VoiceEventKind::SessionClosed));
        Ok(())
    }

    pub async fn send_raw(&self, event: Value) -> Result<()> {
        self.connection.send(event).await
    }

    pub async fn append_context(&self, context: VoiceContext) -> Result<()> {
        self.send_raw(self.provider.append_context_event(context)?)
            .await
    }

    pub async fn complete_delegation(&self, id: &str, context: VoiceContext) -> Result<()> {
        self.send_raw(self.provider.complete_delegation_event(id, context)?)
            .await
    }

    pub async fn append_audio(&self, audio: &[u8]) -> Result<()> {
        self.send_raw(self.provider.append_audio_event(audio)?)
            .await
    }

    pub async fn pause_input(&self) -> Result<()> {
        self.send_raw(self.provider.pause_input_event()).await
    }

    pub async fn resume_input(&self) -> Result<()> {
        self.send_raw(self.provider.resume_input_event()).await
    }

    pub async fn close(&self) -> Result<()> {
        if *self.state_tx.borrow() == VoiceSessionState::Closed {
            return Ok(());
        }

        let mut state_rx = self.state_tx.subscribe();
        if *state_rx.borrow() == VoiceSessionState::Open {
            let _ = self.state_tx.send(VoiceSessionState::Closing);
            self.send_raw(self.provider.close_event()).await?;
        }

        let confirmed = timeout(Duration::from_secs(5), async {
            while *state_rx.borrow_and_update() != VoiceSessionState::Closed {
                if state_rx.changed().await.is_err() {
                    break;
                }
            }
        })
        .await
        .is_ok();

        self.connection.close().await?;
        if !confirmed {
            let _ = self.state_tx.send(VoiceSessionState::Closed);
            let _ = self
                .events
                .send(VoiceEvent::session(VoiceEventKind::SessionClosed));
        }
        Ok(())
    }
}

pub struct BridgedVoiceConnection {
    outbound_tx: mpsc::Sender<Value>,
    outbound_rx: Mutex<Option<mpsc::Receiver<Value>>>,
    incoming_tx: mpsc::Sender<Value>,
    incoming_rx: Mutex<mpsc::Receiver<Value>>,
}

impl BridgedVoiceConnection {
    pub fn new() -> Self {
        let (outbound_tx, outbound_rx) = mpsc::channel(256);
        let (incoming_tx, incoming_rx) = mpsc::channel(256);
        Self {
            outbound_tx,
            outbound_rx: Mutex::new(Some(outbound_rx)),
            incoming_tx,
            incoming_rx: Mutex::new(incoming_rx),
        }
    }

    pub async fn take_outbound(&self) -> Result<mpsc::Receiver<Value>> {
        self.outbound_rx
            .lock()
            .await
            .take()
            .ok_or_else(|| anyhow::anyhow!("outbound voice receiver was already taken"))
    }

    pub async fn push_incoming(&self, event: Value) -> Result<()> {
        self.incoming_tx.send(event).await.map_err(Into::into)
    }
}

impl Default for BridgedVoiceConnection {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VoiceConnection for BridgedVoiceConnection {
    async fn send(&self, event: Value) -> Result<()> {
        self.outbound_tx.send(event).await.map_err(Into::into)
    }

    async fn receive(&self) -> Result<Option<Value>> {
        Ok(self.incoming_rx.lock().await.recv().await)
    }

    async fn close(&self) -> Result<()> {
        Ok(())
    }
}
