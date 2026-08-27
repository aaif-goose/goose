//! OpenAI GPT-Live alpha voice provider.
//!
//! This module intentionally models the unreleased protocol without coupling it
//! to a browser or native audio stack. The caller owns WebRTC and sends/receives
//! JSON events on its data channel.

use crate::voice::{
    VoiceContext, VoiceContextChannel, VoiceDelegation, VoiceDelegationMode, VoiceEvent,
    VoiceMessageRole, VoiceProvider, VoiceSessionAnswer, VoiceSessionConfig, VoiceSessionOffer,
};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

pub const DEFAULT_OPENAI_LIVE_URL: &str = "https://api.openai.com/v1/live";
pub const DEFAULT_OPENAI_LIVE_ALPHA_SELECTOR: &str = "live=v1";

/// Client for the unreleased OpenAI GPT-Live WebRTC signaling and event API.
pub struct OpenAiLiveProvider {
    client: reqwest::Client,
    api_key: String,
    endpoint: String,
    alpha_selector: String,
}

impl OpenAiLiveProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            endpoint: DEFAULT_OPENAI_LIVE_URL.into(),
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

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    pub fn with_alpha_selector(mut self, selector: impl Into<String>) -> Self {
        self.alpha_selector = selector.into();
        self
    }

    fn event_id() -> String {
        format!("event_{}", Uuid::new_v4())
    }
}

#[async_trait]
impl VoiceProvider for OpenAiLiveProvider {
    fn name(&self) -> &str {
        "openai-live"
    }

    async fn create_session(
        &self,
        config: &VoiceSessionConfig,
        offer: VoiceSessionOffer,
    ) -> Result<VoiceSessionAnswer> {
        if offer.sdp.trim().is_empty() {
            bail!("WebRTC SDP offer is empty");
        }
        let initial_items = config
            .initial_items
            .iter()
            .map(|message| {
                let role = match message.role {
                    VoiceMessageRole::User => "user",
                    VoiceMessageRole::Assistant => "assistant",
                    VoiceMessageRole::Developer => "developer",
                };
                json!({
                    "type": "message",
                    "role": role,
                    "content": [{ "type": "input_text", "text": message.text }]
                })
            })
            .collect::<Vec<_>>();
        let mut session = json!({
            "model": config.model,
            "instructions": config.instructions,
            "initial_items": initial_items,
        });
        if let Some(voice) = &config.voice {
            session["audio"] = json!({ "output": { "voice": voice } });
        }
        session["delegation"] = match config.delegation {
            VoiceDelegationMode::Disabled => Value::Null,
            VoiceDelegationMode::Client => json!({ "type": "client" }),
            VoiceDelegationMode::Provider => json!({ "type": "responses" }),
        };
        if let Some(object) = session.as_object_mut() {
            object.extend(config.extra.clone());
        }

        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .header("OpenAI-Alpha", &self.alpha_selector)
            .header(reqwest::header::ACCEPT, "application/sdp")
            .multipart(
                reqwest::multipart::Form::new()
                    .text("sdp", offer.sdp)
                    .text("session", session.to_string()),
            )
            .send()
            .await
            .context("failed to create OpenAI Live session")?;
        let status = response.status();
        let session_id = response
            .headers()
            .get("openai-session-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let body = response.text().await?;
        if !status.is_success() {
            let detail = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|value| value.pointer("/error/message")?.as_str().map(str::to_owned))
                .unwrap_or(body);
            bail!("OpenAI Live session creation failed ({status}): {detail}");
        }
        if body.trim().is_empty() {
            bail!("OpenAI Live returned an empty SDP answer");
        }
        Ok(VoiceSessionAnswer {
            sdp: body,
            session_id,
            model: config.model.clone(),
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
            "delegation.created" => {
                let item = event
                    .get("item")
                    .context("delegation.created is missing item")?;
                let id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .context("delegation is missing id")?;
                let prompt = item
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|part| part.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n");
                VoiceEvent::DelegationCreated {
                    delegation: VoiceDelegation {
                        id: id.into(),
                        prompt,
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
        let channel = context_channel(context.channel, true)?;
        Ok(json!({
            "type": "session.context.append",
            "event_id": Self::event_id(),
            "channel": channel,
            "content": [{ "type": "input_text", "text": context.text }]
        }))
    }

    fn complete_delegation_event(
        &self,
        delegation_id: &str,
        context: VoiceContext,
    ) -> Result<Value> {
        if delegation_id.trim().is_empty() {
            bail!("delegation ID is empty");
        }
        let channel = context_channel(context.channel, false)?;
        Ok(json!({
            "type": "delegation.context.append",
            "event_id": Self::event_id(),
            "delegation_item_id": delegation_id,
            "channel": channel,
            "content": [{ "type": "input_text", "text": context.text }]
        }))
    }

    fn close_event(&self) -> Value {
        json!({ "type": "session.close", "event_id": Self::event_id() })
    }
}

fn context_channel(channel: VoiceContextChannel, allow_developer: bool) -> Result<&'static str> {
    Ok(match channel {
        VoiceContextChannel::Speakable => "speakable",
        VoiceContextChannel::Commentary => "commentary",
        VoiceContextChannel::Developer if allow_developer => "developer",
        VoiceContextChannel::Developer => {
            bail!("developer channel is not valid for delegation context")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_client_delegation() {
        let provider = OpenAiLiveProvider::new("test");
        let event = provider.decode_event(json!({
            "type": "delegation.created",
            "item": { "id": "item_1", "user_bidi_turn_id": "turn_1", "content": [{ "type": "input_text", "text": "inspect this" }] }
        })).unwrap();
        match event {
            VoiceEvent::DelegationCreated { delegation } => {
                assert_eq!(delegation.id, "item_1");
                assert_eq!(delegation.prompt, "inspect this");
            }
            _ => panic!("wrong event"),
        }
    }

    #[test]
    fn encodes_delegation_result() {
        let provider = OpenAiLiveProvider::new("test");
        let event = provider
            .complete_delegation_event(
                "item_1",
                VoiceContext {
                    text: "done".into(),
                    channel: VoiceContextChannel::Speakable,
                },
            )
            .unwrap();
        assert_eq!(event["type"], "delegation.context.append");
        assert_eq!(event["delegation_item_id"], "item_1");
    }
}
