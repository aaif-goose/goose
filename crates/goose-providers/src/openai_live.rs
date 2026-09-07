//! OpenAI GPT Live alpha API.
//!
//! The client owns OpenAI configuration and protocol semantics. WebSocket and
//! WebRTC connectors own their distinct connection establishment flows.

use crate::live::{LiveProtocol, LiveSession, LiveSessionEvent, LiveTransport};
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use rmcp::model::Role;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};
use tokio::{
    sync::{broadcast, broadcast::error::RecvError},
    time::{Duration, timeout},
};
use uuid::Uuid;

pub const DEFAULT_OPENAI_LIVE_HTTP_URL: &str = "https://api.openai.com/v1/live";
pub const DEFAULT_OPENAI_LIVE_WEBSOCKET_URL: &str = "wss://api.openai.com/v1/live";
pub const DEFAULT_OPENAI_LIVE_ALPHA_SELECTOR: &str = "quicksilver=v2";
const SESSION_START_TIMEOUT: Duration = Duration::from_secs(10);

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpenAiLiveSessionId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpenAiLiveDelegationId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpenAiLiveTurnId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpenAiLiveItemId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenAiLiveDelegationTarget {
    Client,
    Responses,
    Other(String),
}

impl From<String> for OpenAiLiveDelegationId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for OpenAiLiveDelegationId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiLiveDelegation {
    pub id: OpenAiLiveDelegationId,
    pub target: OpenAiLiveDelegationTarget,
    pub task: String,
    pub source_turn_id: Option<OpenAiLiveTurnId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenAiLiveProjectedTurnUpdateKind {
    Created,
    Delta,
    Done,
}

pub enum OpenAiLiveCommand {
    AppendContext(OpenAiLiveContext),
    AppendDelegationContext {
        delegation_id: OpenAiLiveDelegationId,
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
        session_id: Option<OpenAiLiveSessionId>,
    },
    TranscriptFragment {
        item_id: OpenAiLiveItemId,
        role: Role,
        text: String,
        start_ms: u64,
        end_ms: u64,
    },
    ProjectedTurn {
        turn_id: OpenAiLiveTurnId,
        role: Option<Role>,
        text: String,
        start_ms: u64,
        end_ms: u64,
        kind: OpenAiLiveProjectedTurnUpdateKind,
    },
    OutputAudioDelta {
        audio: Vec<u8>,
        start_ms: Option<u64>,
        end_ms: Option<u64>,
    },
    DelegationCreated {
        delegation: OpenAiLiveDelegation,
    },
    ContextAppended {
        start_ms: Option<u64>,
        end_ms: Option<u64>,
    },
    DelegationContextAppended {
        delegation_id: OpenAiLiveDelegationId,
        start_ms: Option<u64>,
        end_ms: Option<u64>,
    },
    Usage {
        usage: Value,
    },
    Error {
        error_type: Option<String>,
        code: Option<String>,
        message: String,
        parameter: Option<String>,
        client_event_id: Option<String>,
    },
    InputPaused,
    InputResumed,
    SessionClosed {
        reason: Option<String>,
        usage: Option<Value>,
    },
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
            session,
            events,
            pending_events: Default::default(),
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
        client.connect_transport(transport).ready().await
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
    pub session_id: Option<OpenAiLiveSessionId>,
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
            .map(|value| OpenAiLiveSessionId(value.to_owned()));
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

pub type OpenAiLiveSession = LiveSession<OpenAiLiveProtocol>;
pub type OpenAiLiveSessionEvent = LiveSessionEvent<OpenAiLiveEvent>;

pub struct ConnectedOpenAiLiveSession {
    pub session: OpenAiLiveSession,
    events: broadcast::Receiver<OpenAiLiveSessionEvent>,
    pending_events: std::collections::VecDeque<OpenAiLiveSessionEvent>,
}

impl ConnectedOpenAiLiveSession {
    /// Waits for OpenAI to confirm startup while buffering earlier events.
    pub async fn ready(mut self) -> Result<Self> {
        timeout(SESSION_START_TIMEOUT, async {
            loop {
                match receive_authoritative_event(&mut self.events).await? {
                    LiveSessionEvent::Message(event)
                        if matches!(event.kind, OpenAiLiveEventKind::SessionStarted { .. }) =>
                    {
                        return Ok(self);
                    }
                    LiveSessionEvent::Message(event)
                        if matches!(event.kind, OpenAiLiveEventKind::Error { .. }) =>
                    {
                        if let OpenAiLiveEventKind::Error { message, .. } = event.kind {
                            bail!("OpenAI Live startup failed: {message}");
                        }
                    }
                    LiveSessionEvent::Ended { error, .. } => {
                        if let Some(error) = error {
                            return Err(anyhow::anyhow!(error.to_string()));
                        }
                        bail!("OpenAI Live session ended before startup");
                    }
                    event @ LiveSessionEvent::Message(_) => {
                        self.pending_events.push_back(event);
                    }
                }
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("OpenAI Live session startup timed out"))?
    }

    pub async fn recv(&mut self) -> Result<OpenAiLiveSessionEvent> {
        if let Some(event) = self.pending_events.pop_front() {
            return Ok(event);
        }
        receive_authoritative_event(&mut self.events).await
    }
}

async fn receive_authoritative_event(
    events: &mut broadcast::Receiver<OpenAiLiveSessionEvent>,
) -> Result<OpenAiLiveSessionEvent> {
    match events.recv().await {
        Ok(event) => Ok(event),
        Err(RecvError::Lagged(count)) => {
            bail!("OpenAI Live authoritative event receiver lagged by {count} events")
        }
        Err(RecvError::Closed) => bail!("OpenAI Live authoritative event stream closed"),
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
                if delegation_id.0.trim().is_empty() {
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
        let kind = match event_type {
            "session.started" => OpenAiLiveEventKind::SessionStarted {
                session_id: event
                    .pointer("/session/id")
                    .and_then(Value::as_str)
                    .map(|value| OpenAiLiveSessionId(value.to_owned())),
            },
            "input_transcript.added" => transcript_fragment(&event, Role::User)?,
            "output_transcript.added" => transcript_fragment(&event, Role::Assistant)?,
            "turn.created" => {
                complete_projected_turn(&event, OpenAiLiveProjectedTurnUpdateKind::Created)?
            }
            "turn.delta" => projected_turn_delta(&event)?,
            "turn.done" => {
                complete_projected_turn(&event, OpenAiLiveProjectedTurnUpdateKind::Done)?
            }
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
                            .to_owned()
                            .into(),
                        target: delegation_target(item)?,
                        task: delegation_task(item)?,
                        source_turn_id: optional_string(
                            item.get("user_bidi_turn_id"),
                            "delegation user_bidi_turn_id",
                        )?
                        .map(OpenAiLiveTurnId),
                    },
                }
            }
            "session.context.appended" => OpenAiLiveEventKind::ContextAppended {
                start_ms: optional_u64(event.get("start_ms"), "start_ms")?,
                end_ms: optional_u64(event.get("end_ms"), "end_ms")?,
            },
            "delegation.context.appended" => OpenAiLiveEventKind::DelegationContextAppended {
                delegation_id: event
                    .get("delegation_item_id")
                    .and_then(Value::as_str)
                    .context("delegation.context.appended is missing delegation_item_id")?
                    .to_owned()
                    .into(),
                start_ms: optional_u64(event.get("start_ms"), "start_ms")?,
                end_ms: optional_u64(event.get("end_ms"), "end_ms")?,
            },
            "session.usage.updated" => OpenAiLiveEventKind::Usage {
                usage: usage(&event)?.context("OpenAI Live event is missing usage object")?,
            },
            "input_audio.paused" => OpenAiLiveEventKind::InputPaused,
            "input_audio.resumed" => OpenAiLiveEventKind::InputResumed,
            "session.closed" => OpenAiLiveEventKind::SessionClosed {
                reason: optional_string(event.get("reason"), "session.closed reason")?,
                usage: usage(&event)?,
            },
            "error" | "response.error" => live_error(&event, "/error")?,
            "response.failed" => live_error(&event, "/response/error")?,
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
        matches!(event.kind, OpenAiLiveEventKind::SessionClosed { .. })
    }
}

fn transcript_fragment(event: &Value, role: Role) -> Result<OpenAiLiveEventKind> {
    Ok(OpenAiLiveEventKind::TranscriptFragment {
        item_id: OpenAiLiveItemId(required_string(event.pointer("/item/id"), "item.id")?),
        role,
        text: required_string(event.pointer("/item/text"), "item.text")?,
        start_ms: required_u64(event.get("start_ms"), "start_ms")?,
        end_ms: required_u64(event.get("end_ms"), "end_ms")?,
    })
}

fn complete_projected_turn(
    event: &Value,
    kind: OpenAiLiveProjectedTurnUpdateKind,
) -> Result<OpenAiLiveEventKind> {
    Ok(OpenAiLiveEventKind::ProjectedTurn {
        turn_id: OpenAiLiveTurnId(required_string(event.pointer("/turn/id"), "turn.id")?),
        role: optional_role(event.pointer("/turn/role"))?,
        text: required_string(event.pointer("/turn/transcript"), "turn.transcript")?,
        start_ms: required_u64(event.pointer("/turn/start_ms"), "turn.start_ms")?,
        end_ms: required_u64(event.pointer("/turn/end_ms"), "turn.end_ms")?,
        kind,
    })
}

fn projected_turn_delta(event: &Value) -> Result<OpenAiLiveEventKind> {
    Ok(OpenAiLiveEventKind::ProjectedTurn {
        turn_id: OpenAiLiveTurnId(required_string(event.get("turn_id"), "turn_id")?),
        role: optional_role(event.get("role"))?,
        text: required_string(event.get("delta"), "delta")?,
        start_ms: required_u64(event.get("start_ms"), "start_ms")?,
        end_ms: required_u64(event.get("end_ms"), "end_ms")?,
        kind: OpenAiLiveProjectedTurnUpdateKind::Delta,
    })
}

fn optional_role(value: Option<&Value>) -> Result<Option<Role>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(role)) if role == "user" => Ok(Some(Role::User)),
        Some(Value::String(role)) if role == "assistant" => Ok(Some(Role::Assistant)),
        Some(Value::String(role)) => bail!("unsupported OpenAI Live transcript role `{role}`"),
        Some(_) => bail!("OpenAI Live transcript role is not a string"),
    }
}

fn delegation_task(item: &Value) -> Result<String> {
    let content = item
        .get("content")
        .and_then(Value::as_array)
        .context("delegation is missing content")?;
    let text_parts = content
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("input_text"))
        .collect::<Vec<_>>();
    if text_parts.len() != 1 {
        bail!("delegation must contain exactly one input_text task");
    }
    text_parts[0]
        .get("text")
        .and_then(Value::as_str)
        .context("delegation input_text task is missing text")
        .map(str::to_owned)
}

fn delegation_target(item: &Value) -> Result<OpenAiLiveDelegationTarget> {
    match item
        .get("target")
        .and_then(Value::as_str)
        .context("delegation is missing target")?
    {
        "client" => Ok(OpenAiLiveDelegationTarget::Client),
        "responses" => Ok(OpenAiLiveDelegationTarget::Responses),
        target => Ok(OpenAiLiveDelegationTarget::Other(target.to_owned())),
    }
}

fn usage(event: &Value) -> Result<Option<Value>> {
    match event.get("usage") {
        None | Some(Value::Null) => Ok(None),
        Some(usage) if usage.is_object() => Ok(Some(usage.clone())),
        Some(_) => bail!("OpenAI Live usage is not an object"),
    }
}

fn live_error(event: &Value, pointer: &str) -> Result<OpenAiLiveEventKind> {
    let error = event
        .pointer(pointer)
        .with_context(|| format!("OpenAI Live error is missing {pointer}"))?;
    Ok(OpenAiLiveEventKind::Error {
        error_type: optional_string(error.get("type"), "error.type")?,
        code: optional_string(error.get("code"), "error.code")?,
        message: required_string(error.get("message"), "error.message")?,
        parameter: optional_string(error.get("param"), "error.param")?,
        client_event_id: optional_string(error.get("event_id"), "error.event_id")?,
    })
}

fn required_string(value: Option<&Value>, field: &str) -> Result<String> {
    value
        .and_then(Value::as_str)
        .with_context(|| format!("OpenAI Live event is missing {field}"))
        .map(str::to_owned)
}

fn optional_string(value: Option<&Value>, field: &str) -> Result<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.to_owned())),
        Some(_) => bail!("OpenAI Live event field {field} is not a string"),
    }
}

fn required_u64(value: Option<&Value>, field: &str) -> Result<u64> {
    value
        .and_then(Value::as_u64)
        .with_context(|| format!("OpenAI Live event is missing {field}"))
}

fn optional_u64(value: Option<&Value>, field: &str) -> Result<Option<u64>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .with_context(|| format!("OpenAI Live event field {field} is not an unsigned integer"))
            .map(Some),
    }
}

fn event_id() -> String {
    format!("event_{}", Uuid::new_v4())
}

fn text_event(
    kind: &str,
    delegation_id: Option<&OpenAiLiveDelegationId>,
    context: OpenAiLiveContext,
) -> Result<Value> {
    let channel = match context.channel {
        OpenAiLiveContextChannel::Speakable => "speakable",
        OpenAiLiveContextChannel::Commentary => "commentary",
    };
    let mut event = json!({ "type": kind, "event_id": event_id(), "channel": channel, "content": [{ "type": "input_text", "text": context.text }] });
    if let Some(id) = delegation_id {
        event["delegation_item_id"] = json!(id.0);
    }
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser_live_transport::BrowserLiveTransport;

    fn config() -> OpenAiLiveSessionConfig {
        OpenAiLiveSessionConfig {
            model: "gpt-live-1-marble-alpha".into(),
            instructions: "help".into(),
            voice: None,
            initial_items: vec![],
            experimental: Default::default(),
        }
    }

    fn decode(event: Value) -> OpenAiLiveEventKind {
        OpenAiLiveProtocol.decode(event).unwrap().kind
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

    #[tokio::test]
    async fn ready_buffers_events_before_session_started() {
        let transport = Arc::new(BrowserLiveTransport::new());
        let connected = OpenAiLiveClient::new("test").connect_transport(transport.clone());
        transport
            .push_incoming(json!({ "type": "session.usage.updated", "usage": {} }))
            .await
            .unwrap();
        transport
            .push_incoming(json!({ "type": "session.started", "session": { "id": "session_1" } }))
            .await
            .unwrap();

        let mut connected = connected.ready().await.unwrap();
        assert!(matches!(
            connected.recv().await.unwrap(),
            LiveSessionEvent::Message(OpenAiLiveEvent {
                kind: OpenAiLiveEventKind::Usage { .. },
                ..
            })
        ));
    }

    #[tokio::test]
    async fn authoritative_receiver_reports_lag() {
        let transport = Arc::new(BrowserLiveTransport::new());
        let mut connected = OpenAiLiveClient::new("test").connect_transport(transport.clone());

        for sequence in 0..600 {
            transport
                .push_incoming(json!({ "type": "future.event", "sequence": sequence }))
                .await
                .unwrap();
        }

        let error = connected.recv().await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("authoritative event receiver lagged")
        );
    }

    #[test]
    fn actual_fragment_and_projected_turn_keep_distinct_identity_and_timing() {
        let actual = json!({ "type": "output_transcript.added", "start_ms": 3200, "end_ms": 3400,
            "item": { "id": "item_1", "text": " It's five" } });
        let projected = json!({ "type": "turn.created", "turn": { "id": "turn_1",
            "role": "assistant", "start_ms": 4400, "end_ms": 5800,
            "transcript": " It's five." } });

        let OpenAiLiveEventKind::TranscriptFragment {
            item_id,
            role,
            text,
            start_ms,
            end_ms,
        } = decode(actual)
        else {
            panic!("expected actual transcript fragment");
        };
        assert_eq!(item_id.0, "item_1");
        assert_eq!(role, Role::Assistant);
        assert_eq!(text, " It's five");
        assert_eq!((start_ms, end_ms), (3200, 3400));

        let input = json!({ "type": "input_transcript.added", "start_ms": 400, "end_ms": 600,
            "item": { "id": "item_2", "text": "What" } });
        assert!(
            matches!(decode(input), OpenAiLiveEventKind::TranscriptFragment { role, .. }
            if role == Role::User)
        );

        let OpenAiLiveEventKind::ProjectedTurn {
            turn_id,
            role,
            text,
            start_ms,
            end_ms,
            kind,
        } = decode(projected)
        else {
            panic!("expected projected turn");
        };
        assert_eq!(turn_id.0, "turn_1");
        assert_eq!(
            (role, text.as_str()),
            (Some(Role::Assistant), " It's five.")
        );
        assert_eq!((start_ms, end_ms), (4400, 5800));
        assert_eq!(kind, OpenAiLiveProjectedTurnUpdateKind::Created);
    }

    #[test]
    fn projected_turn_updates_and_delegation_keep_relationships() {
        let create = json!({ "type": "turn.created", "turn": { "id": "turn_1", "role": "user",
            "start_ms": 400, "end_ms": 600, "transcript": "Please" } });
        let delta = json!({ "type": "turn.delta", "turn_id": "turn_1",
            "start_ms": 600, "end_ms": 800, "delta": " delegate this" });
        let done = json!({ "type": "turn.done", "turn": { "id": "turn_1", "role": "user",
            "start_ms": 400, "end_ms": 1000, "transcript": "Please delegate this" } });

        for (event, expected_kind) in [
            (create, OpenAiLiveProjectedTurnUpdateKind::Created),
            (delta, OpenAiLiveProjectedTurnUpdateKind::Delta),
            (done, OpenAiLiveProjectedTurnUpdateKind::Done),
        ] {
            let OpenAiLiveEventKind::ProjectedTurn {
                turn_id,
                role,
                text,
                kind,
                ..
            } = decode(event)
            else {
                panic!("expected projected turn");
            };
            assert_eq!(turn_id.0, "turn_1");
            assert_eq!(kind, expected_kind);
            match expected_kind {
                OpenAiLiveProjectedTurnUpdateKind::Created => {}
                OpenAiLiveProjectedTurnUpdateKind::Delta => {
                    assert_eq!((role, text.as_str()), (None, " delegate this"));
                }
                OpenAiLiveProjectedTurnUpdateKind::Done => assert_eq!(
                    (role, text.as_str()),
                    (Some(Role::User), "Please delegate this")
                ),
            }
        }

        let delegation = json!({ "type": "delegation.created", "offset_ms": 800, "item": {
            "id": "item_1", "target": "client", "user_bidi_turn_id": "turn_1",
            "content": [{ "type": "input_text", "text": "Inspect this" }] } });
        let OpenAiLiveEventKind::DelegationCreated { delegation } = decode(delegation) else {
            panic!("expected delegation");
        };
        assert_eq!(delegation.id.0, "item_1");
        assert_eq!(delegation.target, OpenAiLiveDelegationTarget::Client);
        assert_eq!(delegation.task, "Inspect this");
        assert_eq!(delegation.source_turn_id.unwrap().0, "turn_1");

        let unsupported = json!({ "type": "delegation.created", "item": { "id": "item_2",
            "target": "future_target", "content": [{ "type": "input_text", "text": "Elsewhere" }] } });
        let OpenAiLiveEventKind::DelegationCreated { delegation } = decode(unsupported) else {
            panic!("expected delegation");
        };
        assert!(
            matches!(delegation.target, OpenAiLiveDelegationTarget::Other(target)
            if target == "future_target")
        );
    }

    #[test]
    fn context_acknowledgements_and_errors_preserve_correlation() {
        let accepted = json!({ "type": "session.context.appended",
            "start_ms": 800, "end_ms": 1200 });
        let rejected = json!({ "type": "error", "error": { "type": "invalid_request_error",
            "code": "empty_array", "message": "content cannot be empty", "param": "content",
            "event_id": "client_event_1" } });
        let delegation_accepted = json!({ "type": "delegation.context.appended",
            "delegation_item_id": "item_1", "start_ms": 1200, "end_ms": 1600 });

        let OpenAiLiveEventKind::ContextAppended { start_ms, end_ms } = decode(accepted) else {
            panic!("expected context acknowledgement");
        };
        assert_eq!(start_ms, Some(800));
        assert_eq!(end_ms, Some(1200));

        let OpenAiLiveEventKind::Error {
            error_type,
            code,
            message,
            parameter,
            client_event_id,
        } = decode(rejected)
        else {
            panic!("expected error");
        };
        assert_eq!(error_type.as_deref(), Some("invalid_request_error"));
        assert_eq!(code.as_deref(), Some("empty_array"));
        assert_eq!(message, "content cannot be empty");
        assert_eq!(parameter.as_deref(), Some("content"));
        assert_eq!(client_event_id.as_deref(), Some("client_event_1"));

        let OpenAiLiveEventKind::DelegationContextAppended {
            delegation_id,
            start_ms,
            end_ms,
        } = decode(delegation_accepted)
        else {
            panic!("expected delegation context acknowledgement");
        };
        assert_eq!(delegation_id.0, "item_1");
        assert_eq!(start_ms, Some(1200));
        assert_eq!(end_ms, Some(1600));
    }

    #[test]
    fn close_pause_and_unknown_events_preserve_semantics() {
        let closed = json!({ "type": "session.closed", "reason": "client_request",
            "usage": { "audio_duration_ms": 8000 } });

        let OpenAiLiveEventKind::SessionClosed { reason, usage } = decode(closed) else {
            panic!("expected session close");
        };
        assert_eq!(reason.as_deref(), Some("client_request"));
        let usage = usage.unwrap();
        assert_eq!(usage["audio_duration_ms"], 8000);
        assert!(usage.get("total_tokens").is_none());
        assert!(matches!(
            decode(json!({ "type": "session.closed" })),
            OpenAiLiveEventKind::SessionClosed { usage: None, .. }
        ));
        assert!(matches!(
            decode(json!({ "type": "input_audio.paused" })),
            OpenAiLiveEventKind::InputPaused
        ));
        assert!(matches!(
            decode(json!({ "type": "input_audio.resumed" })),
            OpenAiLiveEventKind::InputResumed
        ));
        assert!(matches!(
            decode(json!({ "type": "future.event" })),
            OpenAiLiveEventKind::Other { event_type } if event_type == "future.event"
        ));
    }

    #[test]
    fn malformed_supported_event_is_rejected() {
        let error = OpenAiLiveProtocol
            .decode(json!({
                "type": "input_transcript.added",
                "start_ms": 0,
                "end_ms": 100,
                "item": { "text": "hello" }
            }))
            .unwrap_err();
        assert!(error.to_string().contains("item.id"));
    }

    #[test]
    fn encodes_delegation_context() {
        let protocol = OpenAiLiveProtocol;
        let encoded = protocol
            .encode(OpenAiLiveCommand::AppendDelegationContext {
                delegation_id: OpenAiLiveDelegationId("item_1".into()),
                context: OpenAiLiveContext {
                    text: "done".into(),
                    channel: OpenAiLiveContextChannel::Speakable,
                },
            })
            .unwrap();
        assert_eq!(encoded["delegation_item_id"], "item_1");
    }
}
