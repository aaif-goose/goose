//! OpenAI GPT-Live alpha protocol support.
//!
//! This unreleased API supports browser-owned WebRTC and backend WebSocket
//! transports. The provider owns configuration and event semantics; transport
//! implementations own media and byte delivery.

use crate::voice::*;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde_json::{json, Value};
use uuid::Uuid;

pub const DEFAULT_OPENAI_LIVE_HTTP_URL: &str = "https://api.openai.com/v1/live";
pub const DEFAULT_OPENAI_LIVE_WEBSOCKET_URL: &str = "wss://api.openai.com/v1/live";
pub const DEFAULT_OPENAI_LIVE_ALPHA_SELECTOR: &str = "quicksilver=v2";

pub struct OpenAiLiveProvider {
    api_key: String,
    http_endpoint: String,
    websocket_endpoint: String,
    alpha_selector: String,
}

impl OpenAiLiveProvider {
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

    fn event_id() -> String {
        format!("event_{}", Uuid::new_v4())
    }

    fn session_json(config: &VoiceSessionConfig, include_model: bool) -> Value {
        let initial_items = config.initial_items.iter().map(|message| {
            let role = match message.role {
                VoiceMessageRole::User => "user",
                VoiceMessageRole::Assistant => "assistant",
                VoiceMessageRole::Developer => "developer",
            };
            json!({ "type": "message", "role": role, "content": [{ "type": "input_text", "text": message.text }] })
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
        if !matches!(config.delegation, VoiceDelegationMode::Disabled) {
            session["delegation"] = match config.delegation {
                VoiceDelegationMode::Client => json!({ "type": "client" }),
                VoiceDelegationMode::Provider => json!({ "type": "responses" }),
                VoiceDelegationMode::Disabled => unreachable!(),
            };
        }
        if let Some(object) = session.as_object_mut() {
            object.extend(config.extra.clone());
        }
        session
    }
}

#[async_trait]
impl VoiceProvider for OpenAiLiveProvider {
    fn name(&self) -> &str {
        "openai-live"
    }

    fn transport_request(
        &self,
        config: &VoiceSessionConfig,
        bootstrap: VoiceSessionBootstrap,
    ) -> Result<VoiceTransportRequest> {
        let headers = vec![
            ("Authorization".into(), format!("Bearer {}", self.api_key)),
            ("OpenAI-Alpha".into(), self.alpha_selector.clone()),
        ];
        Ok(match bootstrap {
            VoiceSessionBootstrap::WebRtcOffer { sdp } => {
                if sdp.trim().is_empty() {
                    bail!("WebRTC SDP offer is empty");
                }
                VoiceTransportRequest {
                    endpoint: self.http_endpoint.clone(),
                    headers,
                    bootstrap: VoiceSessionBootstrap::WebRtcOffer { sdp },
                    initial_event: Some(Self::session_json(config, true)),
                }
            }
            VoiceSessionBootstrap::Direct => VoiceTransportRequest {
                endpoint: format!(
                    "{}?model={}",
                    self.websocket_endpoint,
                    urlencoding::encode(&config.model)
                ),
                headers,
                bootstrap: VoiceSessionBootstrap::Direct,
                initial_event: Some(json!({
                    "type": "session.update",
                    "event_id": Self::event_id(),
                    "session": Self::session_json(config, false),
                })),
            },
        })
    }

    fn decode_event(&self, event: Value) -> Result<VoiceEvent> {
        let event_type = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let text = || {
            event
                .pointer("/item/text")
                .or_else(|| event.pointer("/turn/transcript"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        Ok(match event_type {
            "session.started" => VoiceEvent::SessionStarted {
                session_id: event
                    .pointer("/session/id")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            },
            "input_transcript.added" => VoiceEvent::InputTranscriptDelta { text: text() },
            "output_transcript.added" => VoiceEvent::OutputTranscriptDelta { text: text() },
            "turn.done" if event.pointer("/turn/role").and_then(Value::as_str) == Some("user") => {
                VoiceEvent::InputTranscriptCompleted { text: text() }
            }
            "turn.done" => VoiceEvent::OutputTranscriptCompleted { text: text() },
            "output_audio.delta" => VoiceEvent::OutputAudioDelta {
                audio: event
                    .get("audio")
                    .or_else(|| event.get("delta"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                start_ms: event.get("start_ms").and_then(Value::as_u64),
                end_ms: event.get("end_ms").and_then(Value::as_u64),
            },
            "delegation.created" => {
                let item = event
                    .get("item")
                    .context("delegation.created is missing item")?;
                VoiceEvent::DelegationCreated {
                    delegation: VoiceDelegation {
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
            "session.context.appended" => VoiceEvent::ContextAppended,
            "delegation.context.appended" => VoiceEvent::DelegationContextAppended {
                delegation_id: event
                    .get("delegation_item_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
            },
            "session.usage.updated" => VoiceEvent::Usage {
                usage: event.clone(),
            },
            "session.closed" => VoiceEvent::SessionClosed,
            "error" | "response.error" | "response.failed" => VoiceEvent::Error {
                message: event
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("OpenAI Live reported an error")
                    .into(),
                raw: Some(event),
            },
            _ => VoiceEvent::Other {
                event_type: event_type.into(),
                payload: event,
            },
        })
    }

    fn append_context_event(&self, context: VoiceContext) -> Result<Value> {
        Ok(text_event("session.context.append", None, context, true)?)
    }
    fn complete_delegation_event(&self, id: &str, context: VoiceContext) -> Result<Value> {
        if id.trim().is_empty() {
            bail!("delegation ID is empty");
        }
        Ok(text_event(
            "delegation.context.append",
            Some(id),
            context,
            false,
        )?)
    }
    fn append_audio_event(&self, audio: &[u8]) -> Result<Value> {
        if audio.is_empty() {
            bail!("audio payload is empty");
        }
        Ok(
            json!({ "type": "input_audio.append", "event_id": Self::event_id(), "audio": BASE64.encode(audio) }),
        )
    }
    fn pause_input_event(&self) -> Value {
        json!({ "type": "input_audio.pause", "event_id": Self::event_id() })
    }
    fn resume_input_event(&self) -> Value {
        json!({ "type": "input_audio.resume", "event_id": Self::event_id() })
    }
    fn close_event(&self) -> Value {
        json!({ "type": "session.close", "event_id": Self::event_id() })
    }
}

fn text_event(
    kind: &str,
    delegation_id: Option<&str>,
    context: VoiceContext,
    allow_developer: bool,
) -> Result<Value> {
    let channel = match context.channel {
        VoiceContextChannel::Speakable => "speakable",
        VoiceContextChannel::Commentary => "commentary",
        VoiceContextChannel::Developer if allow_developer => "developer",
        VoiceContextChannel::Developer => {
            bail!("developer channel is not valid for delegation context")
        }
    };
    let mut event = json!({
        "type": kind, "event_id": OpenAiLiveProvider::event_id(), "channel": channel,
        "content": [{ "type": "input_text", "text": context.text }]
    });
    if let Some(id) = delegation_id {
        event["delegation_item_id"] = json!(id);
    }
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_bootstrap_places_model_in_url() {
        let provider = OpenAiLiveProvider::new("test");
        let request = provider
            .transport_request(
                &VoiceSessionConfig {
                    model: "gpt-live-1-marble-alpha".into(),
                    instructions: "help".into(),
                    voice: None,
                    initial_items: vec![],
                    delegation: VoiceDelegationMode::Client,
                    extra: Default::default(),
                },
                VoiceSessionBootstrap::Direct,
            )
            .unwrap();
        assert!(request.endpoint.contains("model=gpt-live-1-marble-alpha"));
        assert!(request.initial_event.unwrap()["session"]["model"].is_null());
    }

    #[test]
    fn encodes_and_decodes_delegation() {
        let provider = OpenAiLiveProvider::new("test");
        let decoded = provider
            .decode_event(json!({ "type": "delegation.created", "item": {
                "id": "item_1", "content": [{ "type": "input_text", "text": "inspect this" }]
            }}))
            .unwrap();
        assert!(matches!(decoded, VoiceEvent::DelegationCreated { .. }));
        let encoded = provider
            .complete_delegation_event(
                "item_1",
                VoiceContext {
                    text: "done".into(),
                    channel: VoiceContextChannel::Speakable,
                },
            )
            .unwrap();
        assert_eq!(encoded["delegation_item_id"], "item_1");
    }
}
