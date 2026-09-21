//! OpenAI implementation of the live voice provider boundary.

use crate::{
    live::{LiveSessionEndReason, LiveSessionEvent},
    live_voice_provider::{
        DelegationUpdate, DelegationUpdateDelivery, LiveVoiceInputMessage, LiveVoiceProvider,
        ProviderConnection, ProviderConnectionEvent, WebRtcAnswer, WebRtcOffer,
    },
    openai_live::{
        ConnectedOpenAiLiveSession, OpenAiLiveClient, OpenAiLiveContext, OpenAiLiveContextChannel,
        OpenAiLiveDelegationId, OpenAiLiveEvent, OpenAiLiveEventKind, OpenAiLiveMessage,
        OpenAiLiveMessageRole, OpenAiLiveSessionConfig, OpenAiLiveSessionId,
    },
};
use anyhow::{bail, Result};
use async_trait::async_trait;
use std::{collections::VecDeque, time::Duration};
use tokio::{
    sync::broadcast::error::RecvError,
    time::{sleep_until, timeout, timeout_at, Instant},
};

const OPENAI_LIVE_MODEL: &str = "gpt-live-1";

const HTTP_SETUP_TIMEOUT: Duration = Duration::from_secs(15);
const SIDEBAND_ATTACH_TIMEOUT: Duration = Duration::from_secs(10);
const DELEGATION_UPDATE_ACK_TIMEOUT: Duration = Duration::from_secs(10);
pub struct OpenAiLiveVoiceProvider {
    client: OpenAiLiveClient,
    voice: String,
    instructions: String,
    delivery_failure_instructions: String,
}

impl OpenAiLiveVoiceProvider {
    pub fn new(
        api_key: impl Into<String>,
        voice: String,
        instructions: String,
        delivery_failure_instructions: String,
    ) -> Result<Self> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            bail!("OpenAI API key is empty");
        }
        if voice.trim().is_empty() {
            bail!("OpenAI Live voice is empty");
        }
        Ok(Self {
            client: OpenAiLiveClient::new(api_key),
            voice,
            instructions,
            delivery_failure_instructions,
        })
    }
}

#[async_trait]
impl LiveVoiceProvider for OpenAiLiveVoiceProvider {
    async fn start(
        &self,
        offer: WebRtcOffer,
        input_messages: Vec<LiveVoiceInputMessage>,
    ) -> Result<(WebRtcAnswer, Box<dyn ProviderConnection>)> {
        let config = OpenAiLiveSessionConfig {
            model: OPENAI_LIVE_MODEL.into(),
            instructions: self.instructions.clone(),
            voice: Some(self.voice.clone()),
            input_messages: input_messages
                .into_iter()
                .map(|message| OpenAiLiveMessage {
                    role: match message.role {
                        rmcp::model::Role::User => OpenAiLiveMessageRole::User,
                        rmcp::model::Role::Assistant => OpenAiLiveMessageRole::Assistant,
                    },
                    text: message.text,
                })
                .collect(),
            extra_session_fields: Default::default(),
        };
        let negotiation = timeout(
            HTTP_SETUP_TIMEOUT,
            self.client.webrtc(config).negotiate(offer.into_sdp()),
        )
        .await
        .map_err(|_| anyhow::anyhow!("OpenAI Live HTTP setup timed out"))??;
        let session_id = negotiation.session_id;
        let answer = WebRtcAnswer::new(negotiation.answer_sdp)
            .ok_or_else(|| anyhow::anyhow!("OpenAI Live returned an invalid WebRTC answer"))?;
        let sideband = connect_sideband(&self.client, session_id).await?;
        Ok((
            answer,
            Box::new(OpenAiProviderConnection {
                sideband,
                pending_events: VecDeque::new(),
                delivery_failure_instructions: self.delivery_failure_instructions.clone(),
            }),
        ))
    }
}

struct OpenAiProviderConnection {
    sideband: ConnectedOpenAiLiveSession,
    pending_events: VecDeque<ProviderConnectionEvent>,
    delivery_failure_instructions: String,
}

#[derive(Debug, PartialEq)]
enum AppendContextOutcome {
    Accepted,
    Rejected,
    TimedOut,
}

#[async_trait]
impl ProviderConnection for OpenAiProviderConnection {
    async fn next_event(&mut self) -> ProviderConnectionEvent {
        if let Some(event) = self.pending_events.pop_front() {
            return event;
        }
        loop {
            if let Some(event) = provider_connection_event(self.sideband.recv().await) {
                return event;
            }
        }
    }

    async fn send_delegation_update(
        &mut self,
        update: DelegationUpdate,
    ) -> Result<DelegationUpdateDelivery> {
        match self
            .append_context(
                Some(OpenAiLiveDelegationId(update.provider_delegation_id)),
                update.text,
            )
            .await?
        {
            AppendContextOutcome::Accepted => Ok(DelegationUpdateDelivery::Delivered),
            AppendContextOutcome::Rejected => {
                let _ = self
                    .append_context(None, self.delivery_failure_instructions.clone())
                    .await?;
                Ok(DelegationUpdateDelivery::Undelivered)
            }
            AppendContextOutcome::TimedOut => Ok(DelegationUpdateDelivery::Undelivered),
        }
    }

    async fn stop(&mut self) -> Result<()> {
        self.sideband.close().await
    }
}

impl OpenAiProviderConnection {
    async fn append_context(
        &mut self,
        delegation_id: Option<OpenAiLiveDelegationId>,
        text: String,
    ) -> Result<AppendContextOutcome> {
        let event_id = format!("event_{}", uuid::Uuid::new_v4());
        self.sideband
            .send(crate::openai_live::OpenAiLiveCommand::AppendContext {
                event_id: event_id.clone(),
                delegation_id,
                context: OpenAiLiveContext {
                    text,
                    channel: OpenAiLiveContextChannel::Commentary,
                },
            })
            .await?;

        match timeout(DELEGATION_UPDATE_ACK_TIMEOUT, async {
            loop {
                let event = self.sideband.recv().await;
                match append_context_response(&event, &event_id) {
                    Some(outcome) => return Ok(outcome),
                    None => match provider_connection_event(event) {
                        Some(
                            ProviderConnectionEvent::Closed
                            | ProviderConnectionEvent::Failed
                            | ProviderConnectionEvent::ReceiverLagged,
                        ) => {
                            bail!("OpenAI Live session ended while delivering a delegation update")
                        }
                        Some(event) => self.pending_events.push_back(event),
                        None => {}
                    },
                }
            }
        })
        .await
        {
            Ok(outcome) => outcome,
            Err(_) => Ok(AppendContextOutcome::TimedOut),
        }
    }
}

fn append_context_response(
    event: &std::result::Result<LiveSessionEvent<OpenAiLiveEvent>, RecvError>,
    event_id: &str,
) -> Option<AppendContextOutcome> {
    match event {
        Ok(LiveSessionEvent::Message(OpenAiLiveEvent {
            kind:
                OpenAiLiveEventKind::ContextAppended {
                    client_event_id: Some(client_event_id),
                    ..
                },
            ..
        })) if client_event_id == event_id => Some(AppendContextOutcome::Accepted),
        Ok(LiveSessionEvent::Message(OpenAiLiveEvent {
            kind:
                OpenAiLiveEventKind::Error {
                    client_event_id: Some(client_event_id),
                    ..
                },
            ..
        })) if client_event_id == event_id => Some(AppendContextOutcome::Rejected),
        _ => None,
    }
}

fn provider_connection_event(
    event: std::result::Result<LiveSessionEvent<OpenAiLiveEvent>, RecvError>,
) -> Option<ProviderConnectionEvent> {
    match event {
        Ok(LiveSessionEvent::Message(event)) => match event.kind {
            OpenAiLiveEventKind::TranscriptDelta {
                event_id,
                role,
                delta,
                start_ms,
                end_ms,
                ..
            } => Some(ProviderConnectionEvent::TranscriptDelta {
                event_id,
                role,
                text: delta,
                start_ms,
                end_ms,
            }),
            OpenAiLiveEventKind::DelegationCreated {
                event_id,
                delegation,
                ..
            } if matches!(
                &delegation.target,
                crate::openai_live::OpenAiLiveDelegationTarget::Client
            ) =>
            {
                Some(ProviderConnectionEvent::DelegationRequested {
                    event_id,
                    delegation_id: delegation.id.0,
                    offset_ms: delegation.offset_ms,
                })
            }
            OpenAiLiveEventKind::SessionClosed { .. } => Some(ProviderConnectionEvent::Closed),
            _ => None,
        },
        Ok(LiveSessionEvent::Ended {
            reason: LiveSessionEndReason::Closed,
            error: None,
        }) => Some(ProviderConnectionEvent::Closed),
        Err(RecvError::Lagged(_)) => Some(ProviderConnectionEvent::ReceiverLagged),
        Ok(LiveSessionEvent::Ended { .. }) | Err(RecvError::Closed) => {
            Some(ProviderConnectionEvent::Failed)
        }
    }
}

async fn connect_sideband(
    client: &OpenAiLiveClient,
    session_id: OpenAiLiveSessionId,
) -> Result<ConnectedOpenAiLiveSession> {
    let deadline = Instant::now() + SIDEBAND_ATTACH_TIMEOUT;
    loop {
        match timeout_at(
            deadline,
            client.existing_session(session_id.clone()).connect(),
        )
        .await
        {
            Ok(Ok(sideband)) => return Ok(sideband),
            Ok(Err(error)) => {
                let retry_at = Instant::now() + Duration::from_millis(200);
                if retry_at >= deadline {
                    return Err(error);
                }
                sleep_until(retry_at).await;
            }
            Err(_) => bail!("OpenAI Live sideband attachment timed out"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_must_be_valid() {
        assert!(OpenAiLiveVoiceProvider::new("", "voice".into(), "".into(), "".into()).is_err());
        assert!(OpenAiLiveVoiceProvider::new("key", String::new(), "".into(), "".into()).is_err());
    }

    #[test]
    fn maps_live_voice_observations() {
        let message = |kind| {
            Ok(LiveSessionEvent::Message(OpenAiLiveEvent {
                kind,
                raw: None,
            }))
        };
        let cases = [
            (
                message(OpenAiLiveEventKind::TranscriptDelta {
                    event_id: "event_transcript_1".into(),
                    client_event_id: None,
                    role: rmcp::model::Role::Assistant,
                    delta: "hello".into(),
                    start_ms: 10,
                    end_ms: 20,
                }),
                Some(ProviderConnectionEvent::TranscriptDelta {
                    event_id: "event_transcript_1".into(),
                    role: rmcp::model::Role::Assistant,
                    text: "hello".into(),
                    start_ms: 10,
                    end_ms: 20,
                }),
            ),
            (
                message(OpenAiLiveEventKind::SessionClosed {
                    event_id: "event_closed_1".into(),
                    client_event_id: Some("client_close_1".into()),
                    reason: "close_requested".into(),
                    session: serde_json::json!({ "id": "session_1" }),
                    usage: serde_json::json!({ "seconds": 1 }),
                }),
                Some(ProviderConnectionEvent::Closed),
            ),
            (
                message(OpenAiLiveEventKind::Error {
                    event_id: "event_error_1".into(),
                    error_type: "server_error".into(),
                    code: "provider_failed".into(),
                    message: "provider failed".into(),
                    parameter: None,
                    client_event_id: None,
                }),
                None,
            ),
            (
                Ok(LiveSessionEvent::Ended {
                    reason: LiveSessionEndReason::Closed,
                    error: None,
                }),
                Some(ProviderConnectionEvent::Closed),
            ),
            (
                Ok(LiveSessionEvent::Ended {
                    reason: LiveSessionEndReason::TransportFailed,
                    error: None,
                }),
                Some(ProviderConnectionEvent::Failed),
            ),
            (
                Err(RecvError::Lagged(1)),
                Some(ProviderConnectionEvent::ReceiverLagged),
            ),
            (
                Err(RecvError::Closed),
                Some(ProviderConnectionEvent::Failed),
            ),
            (
                message(OpenAiLiveEventKind::InputMuted {
                    event_id: "event_muted_1".into(),
                    client_event_id: None,
                }),
                None,
            ),
        ];

        for (event, expected) in cases {
            assert_eq!(provider_connection_event(event), expected);
        }
    }

    #[test]
    fn correlates_delegation_update_responses() {
        let message = |kind| {
            Ok(LiveSessionEvent::Message(OpenAiLiveEvent {
                kind,
                raw: None,
            }))
        };
        let accepted = message(OpenAiLiveEventKind::ContextAppended {
            event_id: "event_accepted".into(),
            channel: OpenAiLiveContextChannel::Commentary,
            client_event_id: Some("client_event".into()),
            start_ms: 10,
            end_ms: 20,
        });
        let rejected = message(OpenAiLiveEventKind::Error {
            event_id: "event_rejected".into(),
            error_type: "invalid_request_error".into(),
            code: "invalid_delegation".into(),
            message: "delegation is stale".into(),
            parameter: Some("delegation_id".into()),
            client_event_id: Some("client_event".into()),
        });

        assert_eq!(
            append_context_response(&accepted, "client_event"),
            Some(AppendContextOutcome::Accepted)
        );
        assert_eq!(
            append_context_response(&rejected, "client_event"),
            Some(AppendContextOutcome::Rejected)
        );
        assert_eq!(append_context_response(&accepted, "other_event"), None);
    }
}
