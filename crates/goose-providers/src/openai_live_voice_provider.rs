//! OpenAI implementation of the live voice provider boundary.

use crate::{
    live::{LiveSessionEndReason, LiveSessionEvent},
    live_voice_provider::{
        LiveVoiceProvider, LiveVoiceProviderAvailability, ProviderConnection,
        ProviderConnectionEvent, WebRtcAnswer, WebRtcOffer,
    },
    openai_live::{
        ConnectedOpenAiLiveSession, OpenAiLiveClient, OpenAiLiveEvent, OpenAiLiveEventKind,
        OpenAiLiveSessionConfig, OpenAiLiveSessionId,
    },
};
use anyhow::{bail, Result};
use async_trait::async_trait;
use std::time::Duration;
use tokio::{
    sync::broadcast::error::RecvError,
    time::{sleep_until, timeout, timeout_at, Instant},
};

pub const OPENAI_LIVE_VOICE_GATE_ENV: &str = "GOOSE_LIVE_VOICE_ENABLED";
pub const OPENAI_LIVE_MODEL_ENV: &str = "GOOSE_LIVE_VOICE_MODEL";
pub const OPENAI_LIVE_VOICE_ENV: &str = "GOOSE_LIVE_VOICE";
pub const OPENAI_LIVE_API_KEY_ENV: &str = "OPENAI_API_KEY";
pub const DEFAULT_OPENAI_LIVE_MODEL: &str = "gpt-live-1-marble-alpha";
pub const DEFAULT_OPENAI_LIVE_VOICE: &str = "marin";

const HTTP_SETUP_TIMEOUT: Duration = Duration::from_secs(15);
const SIDEBAND_ATTACH_TIMEOUT: Duration = Duration::from_secs(10);
const SESSION_START_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenAiLiveVoiceConfig {
    pub enabled: bool,
    pub model: String,
    pub voice: String,
}

impl OpenAiLiveVoiceConfig {
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var(OPENAI_LIVE_VOICE_GATE_ENV)
                .is_ok_and(|value| value.eq_ignore_ascii_case("true") || value == "1"),
            model: std::env::var(OPENAI_LIVE_MODEL_ENV)
                .unwrap_or_else(|_| DEFAULT_OPENAI_LIVE_MODEL.into()),
            voice: std::env::var(OPENAI_LIVE_VOICE_ENV)
                .unwrap_or_else(|_| DEFAULT_OPENAI_LIVE_VOICE.into()),
        }
    }
}

pub struct OpenAiLiveVoiceProvider {
    client: Option<OpenAiLiveClient>,
    config: OpenAiLiveVoiceConfig,
}

impl OpenAiLiveVoiceProvider {
    pub fn from_env() -> Self {
        let config = OpenAiLiveVoiceConfig::from_env();
        let client = std::env::var(OPENAI_LIVE_API_KEY_ENV)
            .ok()
            .filter(|key| !key.trim().is_empty())
            .map(OpenAiLiveClient::new);
        Self { client, config }
    }

    pub fn new(api_key: impl Into<String>, config: OpenAiLiveVoiceConfig) -> Result<Self> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            bail!("OpenAI API key is empty");
        }
        Ok(Self {
            client: Some(OpenAiLiveClient::new(api_key)),
            config,
        })
    }
}

#[async_trait]
impl LiveVoiceProvider for OpenAiLiveVoiceProvider {
    fn availability(&self) -> LiveVoiceProviderAvailability {
        if !self.config.enabled {
            LiveVoiceProviderAvailability::Disabled
        } else if self.client.is_none() {
            LiveVoiceProviderAvailability::Unavailable
        } else {
            LiveVoiceProviderAvailability::Ready
        }
    }

    async fn start(
        &self,
        offer: WebRtcOffer,
    ) -> Result<(WebRtcAnswer, Box<dyn ProviderConnection>)> {
        if !self.config.enabled {
            bail!("OpenAI Live voice is disabled");
        }
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("OpenAI Live credentials are unavailable"))?
            .clone();
        let config = OpenAiLiveSessionConfig {
            model: self.config.model.clone(),
            instructions: String::new(),
            voice: Some(self.config.voice.clone()),
            initial_items: Vec::new(),
            experimental: Default::default(),
        };
        let negotiation = timeout(
            HTTP_SETUP_TIMEOUT,
            client.webrtc(config).negotiate(offer.into_sdp()),
        )
        .await
        .map_err(|_| anyhow::anyhow!("OpenAI Live HTTP setup timed out"))??;
        let session_id = creation_session_id(negotiation.session_id)?;
        let answer = WebRtcAnswer::new(negotiation.answer_sdp)
            .ok_or_else(|| anyhow::anyhow!("OpenAI Live returned an invalid WebRTC answer"))?;
        let mut sideband = connect_sideband(&client, session_id.clone()).await?;
        confirm_session_started(&mut sideband, &session_id).await?;
        Ok((answer, Box::new(OpenAiProviderConnection { sideband })))
    }
}

struct OpenAiProviderConnection {
    sideband: ConnectedOpenAiLiveSession,
}

#[async_trait]
impl ProviderConnection for OpenAiProviderConnection {
    async fn next_event(&mut self) -> ProviderConnectionEvent {
        loop {
            if let Some(event) = provider_connection_event(self.sideband.recv().await) {
                return event;
            }
        }
    }

    async fn stop(&mut self) -> Result<()> {
        self.sideband.session.close().await
    }
}

fn provider_connection_event(
    event: std::result::Result<LiveSessionEvent<OpenAiLiveEvent>, RecvError>,
) -> Option<ProviderConnectionEvent> {
    match event {
        Ok(LiveSessionEvent::Message(event)) => match event.kind {
            OpenAiLiveEventKind::SessionClosed { .. } => Some(ProviderConnectionEvent::Closed),
            OpenAiLiveEventKind::Error { .. } => Some(ProviderConnectionEvent::Failed),
            _ => None,
        },
        Ok(LiveSessionEvent::Ended {
            reason: LiveSessionEndReason::Closed,
            error: None,
        }) => Some(ProviderConnectionEvent::Closed),
        Ok(LiveSessionEvent::Ended { .. }) | Err(RecvError::Lagged(_)) | Err(RecvError::Closed) => {
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

async fn confirm_session_started(
    sideband: &mut ConnectedOpenAiLiveSession,
    expected_session_id: &OpenAiLiveSessionId,
) -> Result<()> {
    timeout(SESSION_START_TIMEOUT, async {
        loop {
            match sideband.recv().await {
                Ok(LiveSessionEvent::Message(event)) => match event.kind {
                    OpenAiLiveEventKind::SessionStarted { session_id }
                        if session_id.as_ref() == Some(expected_session_id) =>
                    {
                        return Ok(());
                    }
                    OpenAiLiveEventKind::SessionStarted { .. } => {
                        bail!("OpenAI Live sideband identity does not match creation identity")
                    }
                    OpenAiLiveEventKind::Error { message, .. } => {
                        bail!("OpenAI Live startup failed: {message}")
                    }
                    _ => {}
                },
                Ok(LiveSessionEvent::Ended { error, .. }) => {
                    if let Some(error) = error {
                        return Err(anyhow::anyhow!(error.to_string()));
                    }
                    bail!("OpenAI Live session ended before startup");
                }
                Err(RecvError::Lagged(count)) => {
                    bail!("OpenAI Live startup event receiver lagged by {count} events")
                }
                Err(RecvError::Closed) => bail!("OpenAI Live startup event stream closed"),
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("OpenAI Live session startup timed out"))?
}

fn creation_session_id(session_id: Option<OpenAiLiveSessionId>) -> Result<OpenAiLiveSessionId> {
    session_id
        .filter(|id| !id.0.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("OpenAI Live creation response has no session identity"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_is_gated_and_creation_identity_is_required() {
        let provider = OpenAiLiveVoiceProvider::new(
            "key",
            OpenAiLiveVoiceConfig {
                enabled: false,
                model: DEFAULT_OPENAI_LIVE_MODEL.into(),
                voice: DEFAULT_OPENAI_LIVE_VOICE.into(),
            },
        )
        .unwrap();
        assert_eq!(
            provider.availability(),
            LiveVoiceProviderAvailability::Disabled
        );
        assert!(creation_session_id(None).is_err());
        assert!(creation_session_id(Some(OpenAiLiveSessionId("  ".into()))).is_err());
        assert_eq!(
            creation_session_id(Some(OpenAiLiveSessionId("session-1".into())))
                .unwrap()
                .0,
            "session-1"
        );
    }

    #[test]
    fn maps_only_provider_terminal_facts() {
        let message = |kind| {
            Ok(LiveSessionEvent::Message(OpenAiLiveEvent {
                kind,
                raw: None,
            }))
        };
        let cases = [
            (
                message(OpenAiLiveEventKind::SessionClosed {
                    reason: None,
                    usage: None,
                }),
                Some(ProviderConnectionEvent::Closed),
            ),
            (
                message(OpenAiLiveEventKind::Error {
                    error_type: None,
                    code: None,
                    message: "provider failed".into(),
                    parameter: None,
                    client_event_id: None,
                }),
                Some(ProviderConnectionEvent::Failed),
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
                Some(ProviderConnectionEvent::Failed),
            ),
            (
                Err(RecvError::Closed),
                Some(ProviderConnectionEvent::Failed),
            ),
            (message(OpenAiLiveEventKind::InputPaused), None),
        ];

        for (event, expected) in cases {
            assert_eq!(provider_connection_event(event), expected);
        }
    }
}
