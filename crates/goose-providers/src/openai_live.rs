//! OpenAI GPT Live alpha API.
//!
//! The client owns OpenAI configuration and protocol semantics. WebSocket and
//! WebRTC connectors own their distinct connection establishment flows.

use crate::live::{LiveProtocol, LiveSession, LiveSessionEnd, LiveSessionEvent, LiveTransport};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::broadcast;
use uuid::Uuid;

pub const DEFAULT_OPENAI_LIVE_HTTP_URL: &str = "https://api.openai.com/v1/live";
pub const DEFAULT_OPENAI_LIVE_WEBSOCKET_URL: &str = "wss://api.openai.com/v1/live";
pub const DEFAULT_OPENAI_LIVE_ALPHA_SELECTOR: &str = "quicksilver=v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiLiveSessionConfig {
    pub model: String,
    pub instructions: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    #[serde(default)]
    pub initial_items: Vec<OpenAiLiveMessage>,
    #[serde(default)]
    pub experimental: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiLiveMessage {
    pub role: OpenAiLiveMessageRole,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenAiLiveMessageRole {
    System,
    User,
    Assistant,
    Developer,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenAiLiveContextChannel {
    Speakable,
    Commentary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiLiveContext {
    pub text: String,
    pub channel: OpenAiLiveContextChannel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiLiveDelegation {
    pub id: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_turn_id: Option<String>,
}

pub enum OpenAiLiveCommand {
    AppendContext(OpenAiLiveContext),
    AppendDelegationContext {
        delegation_id: String,
        context: OpenAiLiveContext,
    },
    AppendAudio(Vec<u8>),
    PauseInput,
    ResumeInput,
    Close,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiLiveEvent {
    pub kind: OpenAiLiveEventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAiLiveEventKind {
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
        audio: Vec<u8>,
        start_ms: Option<u64>,
        end_ms: Option<u64>,
    },
    DelegationCreated {
        delegation: OpenAiLiveDelegation,
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
    },
    SessionClosed,
    Other {
        event_type: String,
    },
}

#[derive(Clone)]
pub struct OpenAiLiveClient {
    api_key: String,
    http_endpoint: String,
    websocket_endpoint: String,
    alpha_selector: String,
}

impl OpenAiLiveClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            http_endpoint: DEFAULT_OPENAI_LIVE_HTTP_URL.into(),
            websocket_endpoint: DEFAULT_OPENAI_LIVE_WEBSOCKET_URL.into(),
            alpha_selector: DEFAULT_OPENAI_LIVE_ALPHA_SELECTOR.into(),
        }
    }

    pub fn from_env() -> Result<Self> {
        let key = std::env::var("OPENAI_API_KEY").context("OPENAI_API_KEY is not set")?;
        if key.trim().is_empty() {
            bail!("OPENAI_API_KEY is empty");
        }
        Ok(Self::new(key))
    }

    pub fn with_http_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.http_endpoint = endpoint.into();
        self
    }

    pub fn with_websocket_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.websocket_endpoint = endpoint.into();
        self
    }

    pub fn with_alpha_selector(mut self, selector: impl Into<String>) -> Self {
        self.alpha_selector = selector.into();
        self
    }

    pub fn websocket(&self, config: OpenAiLiveSessionConfig) -> OpenAiLiveWebSocketConnector {
        OpenAiLiveWebSocketConnector {
            client: self.clone(),
            config,
        }
    }

    pub fn webrtc(&self, config: OpenAiLiveSessionConfig) -> OpenAiLiveWebRtcConnector {
        OpenAiLiveWebRtcConnector {
            client: self.clone(),
            config,
        }
    }

    fn connect_transport(&self, transport: Arc<dyn LiveTransport>) -> ConnectedOpenAiLiveSession {
        let (session, events) = LiveSession::connect(Arc::new(OpenAiLiveProtocol), transport);
        ConnectedOpenAiLiveSession {
            session: OpenAiLiveSession(session),
            events,
        }
    }

    fn headers(&self) -> Vec<(String, String)> {
        vec![
            ("Authorization".into(), format!("Bearer {}", self.api_key)),
            ("OpenAI-Alpha".into(), self.alpha_selector.clone()),
        ]
    }

    fn session_json(config: &OpenAiLiveSessionConfig, include_model: bool) -> Result<Value> {
        const RESERVED: &[&str] = &[
            "model",
            "instructions",
            "initial_items",
            "audio",
            "delegation",
        ];
        if let Some(field) = config
            .experimental
            .keys()
            .find(|field| RESERVED.contains(&field.as_str()))
        {
            bail!("experimental session field `{field}` conflicts with typed configuration");
        }
        let initial_items = config.initial_items.iter().map(|message| {
            let role = match message.role {
                OpenAiLiveMessageRole::System => "system",
                OpenAiLiveMessageRole::User => "user",
                OpenAiLiveMessageRole::Assistant => "assistant",
                OpenAiLiveMessageRole::Developer => "developer",
            };
            let content_type = if matches!(message.role, OpenAiLiveMessageRole::Assistant) { "output_text" } else { "input_text" };
            json!({ "type": "message", "role": role, "content": [{ "type": content_type, "text": message.text }] })
        }).collect::<Vec<_>>();
        let mut session = json!({
            "instructions": config.instructions,
            "initial_items": initial_items,
        });
        if include_model {
            session["model"] = json!(config.model);
        }
        if let Some(voice) = &config.voice {
            session["audio"] = json!({ "output": { "voice": voice } });
        }
        session["delegation"] = json!({ "type": "client" });
        session
            .as_object_mut()
            .unwrap()
            .extend(config.experimental.clone());
        Ok(session)
    }
}

pub struct OpenAiLiveWebSocketConnector {
    client: OpenAiLiveClient,
    config: OpenAiLiveSessionConfig,
}

pub struct OpenAiLiveWebSocketRequest {
    pub endpoint: String,
    pub headers: Vec<(String, String)>,
    pub initial_messages: Vec<Value>,
    pub model: String,
}

impl OpenAiLiveWebSocketConnector {
    pub fn request(self) -> Result<OpenAiLiveWebSocketRequest> {
        let separator = if self.client.websocket_endpoint.contains('?') {
            '&'
        } else {
            '?'
        };
        Ok(OpenAiLiveWebSocketRequest {
            endpoint: format!(
                "{}{separator}model={}",
                self.client.websocket_endpoint,
                urlencoding::encode(&self.config.model)
            ),
            headers: self.client.headers(),
            initial_messages: vec![json!({
                "type": "session.update",
                "event_id": event_id(),
                "session": OpenAiLiveClient::session_json(&self.config, false)?,
            })],
            model: self.config.model,
        })
    }

    #[cfg(feature = "live-websocket")]
    pub async fn connect(self) -> Result<ConnectedOpenAiLiveSession> {
        let client = self.client.clone();
        let transport = Arc::new(
            crate::live_transport_websocket::WebSocketLiveTransport::connect(self.request()?)
                .await?,
        );
        let mut connected = client.connect_transport(transport);
        loop {
            match connected.events.recv().await? {
                LiveSessionEvent::Message(event)
                    if matches!(event.kind, OpenAiLiveEventKind::SessionStarted { .. }) =>
                {
                    return Ok(connected);
                }
                LiveSessionEvent::Message(event)
                    if matches!(event.kind, OpenAiLiveEventKind::Error { .. }) =>
                {
                    if let OpenAiLiveEventKind::Error { message } = event.kind {
                        bail!("OpenAI Live startup failed: {message}");
                    }
                }
                LiveSessionEvent::Ended { error, .. } => {
                    if let Some(error) = error {
                        return Err(anyhow::anyhow!(error.to_string()));
                    }
                    bail!("OpenAI Live session ended before startup");
                }
                LiveSessionEvent::Message(_) => {}
            }
        }
    }
}

pub struct OpenAiLiveWebRtcConnector {
    client: OpenAiLiveClient,
    config: OpenAiLiveSessionConfig,
}

pub struct OpenAiLiveWebRtcRequest {
    pub endpoint: String,
    pub headers: Vec<(String, String)>,
    pub offer_sdp: String,
    pub session: Value,
    pub model: String,
}

pub struct OpenAiLiveWebRtcNegotiation {
    pub answer_sdp: String,
    pub session_id: Option<String>,
    pub model: String,
    client: OpenAiLiveClient,
}

impl OpenAiLiveWebRtcNegotiation {
    pub fn bind(self, transport: Arc<dyn LiveTransport>) -> ConnectedOpenAiLiveSession {
        self.client.connect_transport(transport)
    }
}

impl OpenAiLiveWebRtcConnector {
    pub fn request(self, offer_sdp: impl Into<String>) -> Result<OpenAiLiveWebRtcRequest> {
        let offer_sdp = offer_sdp.into();
        if offer_sdp.trim().is_empty() {
            bail!("WebRTC SDP offer is empty");
        }
        Ok(OpenAiLiveWebRtcRequest {
            endpoint: self.client.http_endpoint.clone(),
            headers: self.client.headers(),
            offer_sdp,
            session: OpenAiLiveClient::session_json(&self.config, true)?,
            model: self.config.model,
        })
    }

    pub async fn negotiate(
        self,
        offer_sdp: impl Into<String>,
    ) -> Result<OpenAiLiveWebRtcNegotiation> {
        let client = self.client.clone();
        let request = self.request(offer_sdp)?;
        let form = reqwest::multipart::Form::new()
            .text("sdp", request.offer_sdp)
            .text("session", request.session.to_string());
        let mut http_request = reqwest::Client::new()
            .post(request.endpoint)
            .multipart(form);
        for (name, value) in request.headers {
            http_request = http_request.header(name, value);
        }
        let response = http_request
            .header(reqwest::header::ACCEPT, "application/sdp")
            .send()
            .await?;
        let status = response.status();
        let session_id = response
            .headers()
            .get("openai-session-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let answer_sdp = response.text().await?;
        if !status.is_success() {
            bail!("OpenAI Live signaling failed ({status}): {answer_sdp}");
        }
        Ok(OpenAiLiveWebRtcNegotiation {
            answer_sdp,
            session_id,
            model: request.model,
            client,
        })
    }
}

/// An established OpenAI Live session. Wraps the generic session runtime so
/// callers name one concrete type instead of `LiveSession<OpenAiLiveProtocol>`.
#[derive(Clone)]
pub struct OpenAiLiveSession(LiveSession<OpenAiLiveProtocol>);

pub type OpenAiLiveSessionEvent = LiveSessionEvent<OpenAiLiveEvent>;

pub struct ConnectedOpenAiLiveSession {
    pub session: OpenAiLiveSession,
    pub events: broadcast::Receiver<OpenAiLiveSessionEvent>,
}

impl OpenAiLiveSession {
    pub fn subscribe(&self) -> broadcast::Receiver<OpenAiLiveSessionEvent> {
        self.0.subscribe()
    }

    pub fn end_reason(&self) -> Option<LiveSessionEnd> {
        self.0.end_reason()
    }

    pub async fn send(&self, command: OpenAiLiveCommand) -> Result<()> {
        self.0.send(command).await
    }

    pub async fn append_context(&self, context: OpenAiLiveContext) -> Result<()> {
        self.send(OpenAiLiveCommand::AppendContext(context)).await
    }

    pub async fn append_delegation_context(
        &self,
        delegation_id: impl Into<String>,
        context: OpenAiLiveContext,
    ) -> Result<()> {
        self.send(OpenAiLiveCommand::AppendDelegationContext {
            delegation_id: delegation_id.into(),
            context,
        })
        .await
    }

    pub async fn append_audio(&self, audio: Vec<u8>) -> Result<()> {
        self.send(OpenAiLiveCommand::AppendAudio(audio)).await
    }

    pub async fn pause_input(&self) -> Result<()> {
        self.send(OpenAiLiveCommand::PauseInput).await
    }

    pub async fn resume_input(&self) -> Result<()> {
        self.send(OpenAiLiveCommand::ResumeInput).await
    }

    /// Requests graceful shutdown and closes the transport.
    pub async fn close(&self) -> Result<()> {
        self.0.close().await
    }
}

pub struct OpenAiLiveProtocol;

impl LiveProtocol for OpenAiLiveProtocol {
    type Command = OpenAiLiveCommand;
    type Event = OpenAiLiveEvent;

    fn encode(&self, command: Self::Command) -> Result<Value> {
        match command {
            OpenAiLiveCommand::AppendContext(context) => {
                text_event("session.context.append", None, context)
            }
            OpenAiLiveCommand::AppendDelegationContext {
                delegation_id,
                context,
            } => {
                if delegation_id.trim().is_empty() {
                    bail!("delegation ID is empty");
                }
                text_event("delegation.context.append", Some(&delegation_id), context)
            }
            OpenAiLiveCommand::AppendAudio(audio) => {
                if audio.is_empty() {
                    bail!("audio payload is empty");
                }
                Ok(
                    json!({ "type": "input_audio.append", "event_id": event_id(), "audio": BASE64.encode(audio) }),
                )
            }
            OpenAiLiveCommand::PauseInput => {
                Ok(json!({ "type": "input_audio.pause", "event_id": event_id() }))
            }
            OpenAiLiveCommand::ResumeInput => {
                Ok(json!({ "type": "input_audio.resume", "event_id": event_id() }))
            }
            OpenAiLiveCommand::Close => {
                Ok(json!({ "type": "session.close", "event_id": event_id() }))
            }
        }
    }

    fn decode(&self, event: Value) -> Result<Self::Event> {
        let event_type = event
            .get("type")
            .and_then(Value::as_str)
            .context("OpenAI Live event is missing type")?;
        let text = || {
            event
                .pointer("/item/text")
                .or_else(|| event.pointer("/turn/transcript"))
                .and_then(Value::as_str)
                .context("OpenAI Live transcript event is missing text")
                .map(str::to_owned)
        };
        let kind = match event_type {
            "session.started" => OpenAiLiveEventKind::SessionStarted {
                session_id: event
                    .pointer("/session/id")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            },
            "input_transcript.added" => OpenAiLiveEventKind::InputTranscriptDelta { text: text()? },
            "output_transcript.added" => {
                OpenAiLiveEventKind::OutputTranscriptDelta { text: text()? }
            }
            "turn.done" if event.pointer("/turn/role").and_then(Value::as_str) == Some("user") => {
                OpenAiLiveEventKind::InputTranscriptCompleted { text: text()? }
            }
            "turn.done" => OpenAiLiveEventKind::OutputTranscriptCompleted { text: text()? },
            "output_audio.delta" => OpenAiLiveEventKind::OutputAudioDelta {
                audio: BASE64.decode(
                    event
                        .get("audio")
                        .or_else(|| event.get("delta"))
                        .and_then(Value::as_str)
                        .context("output_audio.delta is missing audio")?,
                )?,
                start_ms: event.get("start_ms").and_then(Value::as_u64),
                end_ms: event.get("end_ms").and_then(Value::as_u64),
            },
            "delegation.created" => {
                let item = event
                    .get("item")
                    .context("delegation.created is missing item")?;
                OpenAiLiveEventKind::DelegationCreated {
                    delegation: OpenAiLiveDelegation {
                        id: item
                            .get("id")
                            .and_then(Value::as_str)
                            .context("delegation is missing id")?
                            .into(),
                        prompt: item
                            .get("content")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(|part| part.get("text").and_then(Value::as_str))
                            .collect::<Vec<_>>()
                            .join("\n"),
                        user_turn_id: item
                            .get("user_bidi_turn_id")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    },
                }
            }
            "session.context.appended" => OpenAiLiveEventKind::ContextAppended,
            "delegation.context.appended" => OpenAiLiveEventKind::DelegationContextAppended {
                delegation_id: event
                    .get("delegation_item_id")
                    .and_then(Value::as_str)
                    .context("delegation.context.appended is missing delegation_item_id")?
                    .into(),
            },
            "session.usage.updated" => OpenAiLiveEventKind::Usage {
                usage: event.clone(),
            },
            "session.closed" => OpenAiLiveEventKind::SessionClosed,
            "error" | "response.error" | "response.failed" => OpenAiLiveEventKind::Error {
                message: event
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("OpenAI Live reported an error")
                    .into(),
            },
            _ => OpenAiLiveEventKind::Other {
                event_type: event_type.into(),
            },
        };
        Ok(OpenAiLiveEvent {
            kind,
            raw: Some(event),
        })
    }

    fn close_command(&self) -> Option<Self::Command> {
        Some(OpenAiLiveCommand::Close)
    }

    fn is_close_acknowledgement(&self, event: &Self::Event) -> bool {
        matches!(event.kind, OpenAiLiveEventKind::SessionClosed)
    }
}

fn event_id() -> String {
    format!("event_{}", Uuid::new_v4())
}

fn text_event(
    kind: &str,
    delegation_id: Option<&str>,
    context: OpenAiLiveContext,
) -> Result<Value> {
    let channel = match context.channel {
        OpenAiLiveContextChannel::Speakable => "speakable",
        OpenAiLiveContextChannel::Commentary => "commentary",
    };
    let mut event = json!({ "type": kind, "event_id": event_id(), "channel": channel, "content": [{ "type": "input_text", "text": context.text }] });
    if let Some(id) = delegation_id {
        event["delegation_item_id"] = json!(id);
    }
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> OpenAiLiveSessionConfig {
        OpenAiLiveSessionConfig {
            model: "gpt-live-1-marble-alpha".into(),
            instructions: "help".into(),
            voice: None,
            initial_items: vec![],
            experimental: Default::default(),
        }
    }

    #[test]
    fn websocket_bootstrap_places_model_in_url() {
        let request = OpenAiLiveClient::new("test")
            .websocket(config())
            .request()
            .unwrap();
        assert!(request.endpoint.contains("model=gpt-live-1-marble-alpha"));
        assert!(request.initial_messages[0]["session"]["model"].is_null());
    }

    #[test]
    fn initial_message_roles_use_valid_content_types() {
        let roles = [
            (OpenAiLiveMessageRole::System, "system", "input_text"),
            (OpenAiLiveMessageRole::Developer, "developer", "input_text"),
            (OpenAiLiveMessageRole::User, "user", "input_text"),
            (OpenAiLiveMessageRole::Assistant, "assistant", "output_text"),
        ];
        for (role, expected_role, expected_content_type) in roles {
            let mut config = config();
            config.initial_items.push(OpenAiLiveMessage {
                role,
                text: "hello".into(),
            });
            let session = OpenAiLiveClient::session_json(&config, true).unwrap();
            assert_eq!(session["initial_items"][0]["role"], expected_role);
            assert_eq!(
                session["initial_items"][0]["content"][0]["type"],
                expected_content_type
            );
        }
    }

    #[test]
    fn encodes_and_decodes_delegation() {
        let protocol = OpenAiLiveProtocol;
        let decoded = protocol.decode(json!({ "type": "delegation.created", "item": { "id": "item_1", "content": [{ "type": "input_text", "text": "inspect this" }] }})).unwrap();
        assert!(matches!(
            decoded.kind,
            OpenAiLiveEventKind::DelegationCreated { .. }
        ));
        let encoded = protocol
            .encode(OpenAiLiveCommand::AppendDelegationContext {
                delegation_id: "item_1".into(),
                context: OpenAiLiveContext {
                    text: "done".into(),
                    channel: OpenAiLiveContextChannel::Speakable,
                },
            })
            .unwrap();
        assert_eq!(encoded["delegation_item_id"], "item_1");
    }
}
