//! Transport-neutral interfaces for full-duplex voice providers.
//!
//! A voice provider owns provider-specific signaling and JSON semantics. A
//! transport owns the live byte/media connection. Browser WebRTC, native
//! WebSocket, SIP sideband, and test transports can all feed the same session.

use anyhow::{bail, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::{broadcast, mpsc, Mutex};

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

/// Normalized events common to live voice providers. Unknown events retain the
/// raw payload so callers never lose access to provider-specific capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VoiceEvent {
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
        payload: Value,
    },
}

/// Provider-specific session bootstrap. WebRTC uses an SDP offer; WebSocket
/// transports generally use a direct connection bootstrap.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VoiceSessionBootstrap {
    WebRtcOffer { sdp: String },
    Direct,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSessionAnswer {
    pub bootstrap: VoiceSessionAnswerBootstrap,
    pub session_id: Option<String>,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VoiceSessionAnswerBootstrap {
    WebRtcAnswer { sdp: String },
    Connected,
}

/// Provider-specific connection metadata used by a transport.
pub struct VoiceTransportRequest {
    pub(crate) endpoint: String,
    pub(crate) headers: Vec<(String, String)>,
    pub bootstrap: VoiceSessionBootstrap,
    pub initial_event: Option<Value>,
}

impl VoiceTransportRequest {
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn take_connection_parts(
        self,
    ) -> (
        String,
        Vec<(String, String)>,
        VoiceSessionBootstrap,
        Option<Value>,
    ) {
        (
            self.endpoint,
            self.headers,
            self.bootstrap,
            self.initial_event,
        )
    }
}

/// Raw bidirectional transport. Implementations may be a browser/Tauri bridge,
/// a native WebSocket, a native WebRTC stack, or an in-memory test transport.
#[async_trait]
pub trait VoiceTransport: Send + Sync {
    async fn connect(&self, request: VoiceTransportRequest) -> Result<VoiceSessionAnswerBootstrap>;
    async fn send(&self, event: Value) -> Result<()>;
    async fn receive(&self) -> Result<Option<Value>>;
    async fn close(&self) -> Result<()>;
}

/// Provider-specific protocol behavior, independent of the media transport.
#[async_trait]
pub trait VoiceProvider: Send + Sync {
    fn name(&self) -> &str;
    fn transport_request(
        &self,
        config: &VoiceSessionConfig,
        bootstrap: VoiceSessionBootstrap,
    ) -> Result<VoiceTransportRequest>;
    fn decode_event(&self, event: Value) -> Result<VoiceEvent>;
    fn append_context_event(&self, context: VoiceContext) -> Result<Value>;
    fn complete_delegation_event(
        &self,
        delegation_id: &str,
        context: VoiceContext,
    ) -> Result<Value>;
    fn append_audio_event(&self, _audio: &[u8]) -> Result<Value> {
        bail!("this provider/transport does not accept JSON audio events")
    }
    fn pause_input_event(&self) -> Value;
    fn resume_input_event(&self) -> Value;
    fn close_event(&self) -> Value;
}

/// Stateful transport-neutral client. It owns event decoding and subscriptions;
/// the chosen transport owns only connection and byte delivery.
pub struct VoiceSession {
    provider: Arc<dyn VoiceProvider>,
    transport: Arc<dyn VoiceTransport>,
    events: broadcast::Sender<VoiceEvent>,
    session_id: Mutex<Option<String>>,
}

impl VoiceSession {
    pub async fn connect(
        provider: Arc<dyn VoiceProvider>,
        transport: Arc<dyn VoiceTransport>,
        config: VoiceSessionConfig,
        bootstrap: VoiceSessionBootstrap,
    ) -> Result<(Arc<Self>, VoiceSessionAnswer)> {
        let model = config.model.clone();
        let request = provider.transport_request(&config, bootstrap)?;
        let answer_bootstrap = transport.connect(request).await?;
        let (events, _) = broadcast::channel(256);
        let session = Arc::new(Self {
            provider,
            transport,
            events,
            session_id: Mutex::new(None),
        });
        let answer = VoiceSessionAnswer {
            bootstrap: answer_bootstrap,
            session_id: None,
            model,
        };
        Ok((session, answer))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<VoiceEvent> {
        self.events.subscribe()
    }

    /// Process one raw event delivered by a transport adapter.
    pub async fn handle_incoming(&self, raw: Value) -> Result<VoiceEvent> {
        let event = self.provider.decode_event(raw)?;
        if let VoiceEvent::SessionStarted { session_id } = &event {
            *self.session_id.lock().await = session_id.clone();
        }
        let _ = self.events.send(event.clone());
        Ok(event)
    }

    /// Continuously receive and process transport events until closure.
    pub async fn run(self: Arc<Self>) -> Result<()> {
        while let Some(raw) = self.transport.receive().await? {
            let closed = matches!(self.handle_incoming(raw).await?, VoiceEvent::SessionClosed);
            if closed {
                return Ok(());
            }
        }
        let _ = self.events.send(VoiceEvent::SessionClosed);
        Ok(())
    }

    pub async fn send_raw(&self, event: Value) -> Result<()> {
        self.transport.send(event).await
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
        let event_result = self.send_raw(self.provider.close_event()).await;
        let close_result = self.transport.close().await;
        let result = event_result.and(close_result);
        if result.is_ok() {
            let _ = self.events.send(VoiceEvent::SessionClosed);
        }
        result
    }
}

/// Channel-backed transport for Tauri and other foreign runtimes. The host owns
/// WebRTC; Rust owns the provider protocol and event processor.
pub struct BridgedVoiceTransport {
    outbound_tx: mpsc::Sender<Value>,
    outbound_rx: Mutex<Option<mpsc::Receiver<Value>>>,
    incoming_tx: tokio::sync::mpsc::Sender<Value>,
    incoming_rx: Mutex<tokio::sync::mpsc::Receiver<Value>>,
    connect_handler: Arc<
        dyn Fn(
                VoiceTransportRequest,
            )
                -> futures::future::BoxFuture<'static, Result<VoiceSessionAnswerBootstrap>>
            + Send
            + Sync,
    >,
}

impl BridgedVoiceTransport {
    pub fn new<F, Fut>(connect_handler: F) -> Self
    where
        F: Fn(VoiceTransportRequest) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<VoiceSessionAnswerBootstrap>> + Send + 'static,
    {
        let (outbound_tx, outbound_rx) = mpsc::channel(256);
        let (incoming_tx, incoming_rx) = mpsc::channel(256);
        Self {
            outbound_tx,
            outbound_rx: Mutex::new(Some(outbound_rx)),
            incoming_tx,
            incoming_rx: Mutex::new(incoming_rx),
            connect_handler: Arc::new(move |request| Box::pin(connect_handler(request))),
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

#[async_trait]
impl VoiceTransport for BridgedVoiceTransport {
    async fn connect(&self, request: VoiceTransportRequest) -> Result<VoiceSessionAnswerBootstrap> {
        (self.connect_handler)(request).await
    }
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
