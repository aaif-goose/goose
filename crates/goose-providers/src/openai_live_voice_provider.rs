//! OpenAI Live implementation of the provider-neutral live voice contract.

use crate::{
    live::{LiveSessionEndReason, LiveSessionEvent},
    live_voice_provider::{
        LiveVoiceCapabilities, LiveVoiceConfig, LiveVoiceMediaAnswer, LiveVoiceMediaKind,
        LiveVoiceMediaRequest, LiveVoiceProvider, LiveVoiceRole, ProjectedTurn,
        ProjectedTurnUpdateKind, ProviderCommand, ProviderCommandId, ProviderConnection,
        ProviderControl, ProviderDelegation, ProviderDelegationId, ProviderDelegationTarget,
        ProviderDispatchResult, ProviderEvent, ProviderEventReceiver, ProviderEventStreamError,
        ProviderStartupObservation, ProviderUsage, TranscriptFragment,
    },
    openai_live::{
        ConnectedOpenAiLiveSession, OpenAiLiveClient, OpenAiLiveCommand, OpenAiLiveContext,
        OpenAiLiveContextChannel, OpenAiLiveDelegationTarget, OpenAiLiveEventKind,
        OpenAiLiveProjectedTurnUpdateKind, OpenAiLiveSession, OpenAiLiveSessionConfig,
        OpenAiLiveSessionId,
    },
};
use anyhow::{bail, Result};
use async_trait::async_trait;
use futures::future::{BoxFuture, FutureExt, Shared};
use rmcp::model::Role;
use serde_json::Value;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    sync::{broadcast::error::RecvError, mpsc},
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
const ADAPTER_CLEANUP_TIMEOUT: Duration = Duration::from_secs(18);

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

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn is_configured(&self) -> bool {
        self.client.is_some()
    }
}

#[async_trait]
impl LiveVoiceProvider for OpenAiLiveVoiceProvider {
    fn capabilities(&self) -> LiveVoiceCapabilities {
        LiveVoiceCapabilities {
            available: self.config.enabled && self.client.is_some(),
            audio_input: true,
            audio_output: true,
            interruption: true,
            transcription: true,
            client_delegation: true,
            typed_input: false,
            media: LiveVoiceMediaKind::WebRtc,
        }
    }

    async fn start(
        &self,
        voice_config: LiveVoiceConfig,
        media: LiveVoiceMediaRequest,
    ) -> Result<ProviderConnection> {
        if !self.config.enabled {
            bail!("OpenAI Live voice is disabled");
        }
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("OpenAI Live credentials are unavailable"))?
            .clone();
        let LiveVoiceMediaRequest::WebRtc { offer_sdp } = media;
        let openai_config = OpenAiLiveSessionConfig {
            model: self.config.model.clone(),
            instructions: voice_config.instructions,
            voice: Some(self.config.voice.clone()),
            initial_items: Vec::new(),
            experimental: Default::default(),
        };
        let negotiation = timeout(
            HTTP_SETUP_TIMEOUT,
            client.webrtc(openai_config).negotiate(offer_sdp),
        )
        .await
        .map_err(|_| anyhow::anyhow!("OpenAI Live HTTP setup timed out"))??;
        let session_id = creation_session_id(negotiation.session_id.clone())?;

        let sideband = connect_sideband(&client, session_id.clone()).await?;
        Ok(build_connection(
            negotiation.answer_sdp,
            session_id,
            sideband,
        ))
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

fn creation_session_id(session_id: Option<OpenAiLiveSessionId>) -> Result<OpenAiLiveSessionId> {
    session_id
        .filter(|id| !id.0.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("OpenAI Live creation response has no session identity"))
}

#[derive(Default)]
struct AdapterState {
    ready: bool,
    remote_closed: bool,
    pending_input: Option<PendingInput>,
    pending_delegation: Option<PendingDelegation>,
}

struct PendingInput {
    native_event_id: String,
    command_id: ProviderCommandId,
    should_pause: bool,
}

struct PendingDelegation {
    native_event_id: String,
    command_id: ProviderCommandId,
    delegation_id: ProviderDelegationId,
}

type SharedCloseFuture = Shared<BoxFuture<'static, Result<(), String>>>;

struct OpenAiLiveVoiceControl {
    session_id: OpenAiLiveSessionId,
    session: OpenAiLiveSession,
    event_tx: mpsc::UnboundedSender<Result<ProviderEvent, ProviderEventStreamError>>,
    state: Arc<Mutex<AdapterState>>,
    close_future: SharedCloseFuture,
}

fn build_connection(
    answer_sdp: String,
    session_id: OpenAiLiveSessionId,
    sideband: ConnectedOpenAiLiveSession,
) -> ProviderConnection {
    let session = sideband.session.clone();
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let state = Arc::new(Mutex::new(AdapterState::default()));
    let close_future = shared_close_future(session.clone());
    let control = Arc::new(OpenAiLiveVoiceControl {
        session_id: session_id.clone(),
        session,
        event_tx: event_tx.clone(),
        state: state.clone(),
        close_future: close_future.clone(),
    });
    tokio::spawn(run_event_pump(
        sideband,
        session_id,
        event_tx,
        state.clone(),
        close_future,
    ));
    ProviderConnection {
        media_answer: LiveVoiceMediaAnswer::WebRtc { answer_sdp },
        control,
        events: ProviderEventReceiver::new(event_rx),
    }
}

fn shared_close_future(session: OpenAiLiveSession) -> SharedCloseFuture {
    async move {
        timeout(ADAPTER_CLEANUP_TIMEOUT, session.close())
            .await
            .map_err(|_| "OpenAI Live adapter cleanup timed out".to_string())?
            .map_err(|error| error.to_string())
    }
    .boxed()
    .shared()
}

#[async_trait]
impl ProviderControl for OpenAiLiveVoiceControl {
    async fn dispatch(&self, command: ProviderCommand) -> ProviderDispatchResult {
        match command {
            ProviderCommand::ValidateStartupObservation { observation, .. } => {
                self.validate_startup_observation(observation)
            }
            ProviderCommand::PauseInput { command_id } => {
                self.dispatch_input_command(command_id, true).await
            }
            ProviderCommand::ResumeInput { command_id } => {
                self.dispatch_input_command(command_id, false).await
            }
            ProviderCommand::DeliverDelegationResult {
                command_id,
                delegation_id,
                result,
            } => {
                self.dispatch_delegation_result(command_id, delegation_id, result)
                    .await
            }
        }
    }

    async fn close(&self) -> Result<()> {
        let shutdown_owner = self.close_future.clone();
        tokio::spawn(async move {
            let _ = shutdown_owner.await;
        });
        self.close_future.clone().await.map_err(anyhow::Error::msg)
    }
}

impl OpenAiLiveVoiceControl {
    fn validate_startup_observation(
        &self,
        observation: ProviderStartupObservation,
    ) -> ProviderDispatchResult {
        if observation.0 != self.session_id.0 {
            return ProviderDispatchResult::Rejected(
                "OpenAI Live bootstrap identity does not match creation identity".into(),
            );
        }
        if mark_ready(&self.state) {
            let _ = self.event_tx.send(Ok(ProviderEvent::Ready));
        }
        ProviderDispatchResult::Dispatched
    }

    async fn dispatch_input_command(
        &self,
        command_id: ProviderCommandId,
        should_pause: bool,
    ) -> ProviderDispatchResult {
        let native_event_id = native_command_id(&command_id);
        {
            let mut state = self
                .state
                .lock()
                .expect("OpenAI Live adapter lock poisoned");
            if state.pending_input.is_some() {
                return ProviderDispatchResult::Rejected(
                    "an OpenAI Live input command is awaiting acknowledgement".into(),
                );
            }
            state.pending_input = Some(PendingInput {
                native_event_id: native_event_id.clone(),
                command_id,
                should_pause,
            });
        }
        let command = if should_pause {
            OpenAiLiveCommand::PauseInput {
                event_id: native_event_id.clone(),
            }
        } else {
            OpenAiLiveCommand::ResumeInput {
                event_id: native_event_id.clone(),
            }
        };
        match self.session.send(command).await {
            Ok(()) => ProviderDispatchResult::Dispatched,
            Err(error) => {
                clear_pending_input(&self.state, &native_event_id);
                ProviderDispatchResult::Rejected(error.to_string())
            }
        }
    }

    async fn dispatch_delegation_result(
        &self,
        command_id: ProviderCommandId,
        delegation_id: ProviderDelegationId,
        result: String,
    ) -> ProviderDispatchResult {
        let native_event_id = native_command_id(&command_id);
        {
            let mut state = self
                .state
                .lock()
                .expect("OpenAI Live adapter lock poisoned");
            if state.pending_delegation.is_some() {
                return ProviderDispatchResult::Rejected(
                    "an OpenAI Live delegation result is awaiting acknowledgement".into(),
                );
            }
            state.pending_delegation = Some(PendingDelegation {
                native_event_id: native_event_id.clone(),
                command_id,
                delegation_id: delegation_id.clone(),
            });
        }
        let command = OpenAiLiveCommand::AppendDelegationContext {
            event_id: native_event_id.clone(),
            delegation_id: crate::openai_live::OpenAiLiveDelegationId(delegation_id.0),
            context: OpenAiLiveContext {
                text: result,
                channel: OpenAiLiveContextChannel::Speakable,
            },
        };
        match self.session.send(command).await {
            Ok(()) => ProviderDispatchResult::Dispatched,
            Err(error) => {
                clear_pending_delegation(&self.state, &native_event_id);
                ProviderDispatchResult::Rejected(error.to_string())
            }
        }
    }
}

fn native_command_id(command_id: &ProviderCommandId) -> String {
    format!("goose_{}", command_id.0)
}

fn mark_ready(state: &Arc<Mutex<AdapterState>>) -> bool {
    let mut state = state.lock().expect("OpenAI Live adapter lock poisoned");
    !std::mem::replace(&mut state.ready, true)
}

fn report_failure_and_start_cleanup(
    event_tx: &mpsc::UnboundedSender<Result<ProviderEvent, ProviderEventStreamError>>,
    message: impl Into<String>,
    close_future: SharedCloseFuture,
) {
    let _ = event_tx.send(Err(ProviderEventStreamError::Failed(message.into())));
    tokio::spawn(async move {
        let _ = close_future.await;
    });
}

async fn run_event_pump(
    mut sideband: ConnectedOpenAiLiveSession,
    session_id: OpenAiLiveSessionId,
    event_tx: mpsc::UnboundedSender<Result<ProviderEvent, ProviderEventStreamError>>,
    state: Arc<Mutex<AdapterState>>,
    close_future: SharedCloseFuture,
) {
    let mut cleanup_started = false;
    loop {
        match sideband.recv().await {
            Ok(LiveSessionEvent::Message(event)) => {
                match map_provider_event(event.kind, &session_id, &state) {
                    Ok(Some(event)) => {
                        let _ = event_tx.send(Ok(event));
                    }
                    Ok(None) => {}
                    Err(error) => {
                        if !cleanup_started {
                            report_failure_and_start_cleanup(
                                &event_tx,
                                error,
                                close_future.clone(),
                            );
                            cleanup_started = true;
                        }
                    }
                }
            }
            Ok(LiveSessionEvent::Ended { reason, error }) => {
                let remote_closed = state
                    .lock()
                    .expect("OpenAI Live adapter lock poisoned")
                    .remote_closed;
                if !cleanup_started
                    && (!remote_closed || reason != LiveSessionEndReason::Closed || error.is_some())
                {
                    let detail =
                        error.map_or_else(|| format!("{reason:?}"), |value| value.to_string());
                    let _ = event_tx.send(Err(ProviderEventStreamError::Failed(format!(
                        "OpenAI Live session ended: {detail}"
                    ))));
                }
                return;
            }
            Err(RecvError::Lagged(count)) => {
                if !cleanup_started {
                    let _ = event_tx.send(Err(ProviderEventStreamError::Lagged(count)));
                    let shutdown_owner = close_future.clone();
                    tokio::spawn(async move {
                        let _ = shutdown_owner.await;
                    });
                    cleanup_started = true;
                }
            }
            Err(RecvError::Closed) => {
                if !cleanup_started {
                    report_failure_and_start_cleanup(
                        &event_tx,
                        "OpenAI Live authoritative event stream closed",
                        close_future,
                    );
                }
                return;
            }
        }
    }
}

fn map_provider_event(
    event: OpenAiLiveEventKind,
    session_id: &OpenAiLiveSessionId,
    state: &Arc<Mutex<AdapterState>>,
) -> std::result::Result<Option<ProviderEvent>, String> {
    Ok(match event {
        OpenAiLiveEventKind::SessionStarted {
            session_id: observed,
        } => {
            if observed.as_ref() != Some(session_id) {
                return Err(
                    "OpenAI Live sideband identity does not match creation identity".into(),
                );
            }
            mark_ready(state).then_some(ProviderEvent::Ready)
        }
        OpenAiLiveEventKind::TranscriptFragment {
            item_id,
            role,
            text,
            start_ms,
            end_ms,
        } => Some(ProviderEvent::TranscriptFragment(TranscriptFragment {
            item_id: item_id.0,
            role: map_role(role),
            text,
            start_ms,
            end_ms,
        })),
        OpenAiLiveEventKind::ProjectedTurn {
            turn_id,
            role,
            text,
            start_ms,
            end_ms,
            kind,
        } => {
            let role = role.map(map_role);
            Some(ProviderEvent::ProjectedTurn(ProjectedTurn {
                turn_id: turn_id.0,
                role,
                text,
                start_ms,
                end_ms,
                kind: match kind {
                    OpenAiLiveProjectedTurnUpdateKind::Created => ProjectedTurnUpdateKind::Created,
                    OpenAiLiveProjectedTurnUpdateKind::Delta => ProjectedTurnUpdateKind::Delta,
                    OpenAiLiveProjectedTurnUpdateKind::Done => ProjectedTurnUpdateKind::Done,
                },
            }))
        }
        OpenAiLiveEventKind::OutputAudioDelta { .. } => None,
        OpenAiLiveEventKind::DelegationCreated { delegation } => {
            Some(ProviderEvent::DelegationRequested(ProviderDelegation {
                id: ProviderDelegationId(delegation.id.0),
                target: match delegation.target {
                    OpenAiLiveDelegationTarget::Client => ProviderDelegationTarget::Client,
                    OpenAiLiveDelegationTarget::Responses => {
                        ProviderDelegationTarget::Unsupported("responses".into())
                    }
                    OpenAiLiveDelegationTarget::Other(value) => {
                        ProviderDelegationTarget::Unsupported(value)
                    }
                },
                task: delegation.task,
                source_turn_id: delegation.source_turn_id.map(|id| id.0),
            }))
        }
        OpenAiLiveEventKind::DelegationContextAppended { delegation_id, .. } => {
            let pending = {
                let mut state = state.lock().expect("OpenAI Live adapter lock poisoned");
                let pending = state.pending_delegation.take().ok_or_else(|| {
                    "OpenAI Live sent an uncorrelated delegation acknowledgement".to_string()
                })?;
                if pending.delegation_id.0 != delegation_id.0 {
                    state.pending_delegation = Some(pending);
                    return Err("OpenAI Live delegation acknowledgement identity mismatch".into());
                }
                pending
            };
            Some(ProviderEvent::DelegationResultAccepted {
                command_id: pending.command_id,
                delegation_id: pending.delegation_id,
            })
        }
        OpenAiLiveEventKind::InputPaused => Some(map_input_acknowledgement(state, true)?),
        OpenAiLiveEventKind::InputResumed => Some(map_input_acknowledgement(state, false)?),
        OpenAiLiveEventKind::Usage { usage } => {
            Some(ProviderEvent::UsageUpdated(map_usage(&usage)))
        }
        OpenAiLiveEventKind::Error {
            message,
            client_event_id,
            ..
        } => {
            let command_id = take_rejected_command(state, client_event_id.as_deref())?;
            Some(ProviderEvent::CommandRejected {
                command_id: Some(command_id),
                reason: message,
            })
        }
        OpenAiLiveEventKind::SessionClosed { reason, usage } => {
            state
                .lock()
                .expect("OpenAI Live adapter lock poisoned")
                .remote_closed = true;
            Some(ProviderEvent::RemoteClosed {
                reason,
                usage: usage.as_ref().map(map_usage),
            })
        }
        OpenAiLiveEventKind::ContextAppended { .. } | OpenAiLiveEventKind::Other { .. } => None,
    })
}

fn map_input_acknowledgement(
    state: &Arc<Mutex<AdapterState>>,
    is_paused: bool,
) -> std::result::Result<ProviderEvent, String> {
    let pending = state
        .lock()
        .expect("OpenAI Live adapter lock poisoned")
        .pending_input
        .take()
        .ok_or_else(|| "OpenAI Live sent an uncorrelated input acknowledgement".to_string())?;
    if pending.should_pause != is_paused {
        return Err("OpenAI Live input acknowledgement did not match the pending command".into());
    }
    Ok(if is_paused {
        ProviderEvent::InputPaused {
            command_id: pending.command_id,
        }
    } else {
        ProviderEvent::InputResumed {
            command_id: pending.command_id,
        }
    })
}

fn take_rejected_command(
    state: &Arc<Mutex<AdapterState>>,
    native_id: Option<&str>,
) -> std::result::Result<ProviderCommandId, String> {
    let native_id =
        native_id.ok_or_else(|| "OpenAI Live returned an uncorrelated error".to_string())?;
    let mut state = state.lock().expect("OpenAI Live adapter lock poisoned");
    if state
        .pending_input
        .as_ref()
        .is_some_and(|pending| pending.native_event_id == native_id)
    {
        return Ok(state
            .pending_input
            .take()
            .expect("pending input exists")
            .command_id);
    }
    if state
        .pending_delegation
        .as_ref()
        .is_some_and(|pending| pending.native_event_id == native_id)
    {
        return Ok(state
            .pending_delegation
            .take()
            .expect("pending delegation exists")
            .command_id);
    }
    Err("OpenAI Live returned an error for an unknown command".into())
}

fn clear_pending_input(state: &Arc<Mutex<AdapterState>>, native_event_id: &str) {
    let mut state = state.lock().expect("OpenAI Live adapter lock poisoned");
    if state
        .pending_input
        .as_ref()
        .is_some_and(|pending| pending.native_event_id == native_event_id)
    {
        state.pending_input = None;
    }
}

fn clear_pending_delegation(state: &Arc<Mutex<AdapterState>>, native_event_id: &str) {
    let mut state = state.lock().expect("OpenAI Live adapter lock poisoned");
    if state
        .pending_delegation
        .as_ref()
        .is_some_and(|pending| pending.native_event_id == native_event_id)
    {
        state.pending_delegation = None;
    }
}

fn map_role(role: Role) -> LiveVoiceRole {
    match role {
        Role::User => LiveVoiceRole::User,
        Role::Assistant => LiveVoiceRole::Assistant,
    }
}

fn map_usage(usage: &Value) -> ProviderUsage {
    ProviderUsage {
        input_tokens: usage.get("input_tokens").and_then(Value::as_u64),
        output_tokens: usage.get("output_tokens").and_then(Value::as_u64),
        total_tokens: usage.get("total_tokens").and_then(Value::as_u64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{browser_live_transport::BrowserLiveTransport, openai_live::OpenAiLiveTurnId};

    fn test_state() -> Arc<Mutex<AdapterState>> {
        Arc::new(Mutex::new(AdapterState::default()))
    }

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
        assert!(!provider.capabilities().available);
        assert!(!provider.capabilities().typed_input);
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
    fn semantic_mapping_preserves_native_fields() {
        let state = test_state();
        let transcript = map_provider_event(
            OpenAiLiveEventKind::TranscriptFragment {
                item_id: crate::openai_live::OpenAiLiveItemId("item-1".into()),
                role: Role::User,
                text: "exact text".into(),
                start_ms: 1,
                end_ms: 9,
            },
            &OpenAiLiveSessionId("session-1".into()),
            &state,
        )
        .unwrap()
        .unwrap();
        let projected = map_provider_event(
            OpenAiLiveEventKind::ProjectedTurn {
                turn_id: OpenAiLiveTurnId("turn-1".into()),
                role: None,
                text: " delta".into(),
                start_ms: 10,
                end_ms: 20,
                kind: OpenAiLiveProjectedTurnUpdateKind::Delta,
            },
            &OpenAiLiveSessionId("session-1".into()),
            &state,
        )
        .unwrap()
        .unwrap();
        let delegation = map_provider_event(
            OpenAiLiveEventKind::DelegationCreated {
                delegation: crate::openai_live::OpenAiLiveDelegation {
                    id: crate::openai_live::OpenAiLiveDelegationId("delegation-1".into()),
                    target: OpenAiLiveDelegationTarget::Client,
                    task: "do work".into(),
                    source_turn_id: Some(OpenAiLiveTurnId("turn-1".into())),
                },
            },
            &OpenAiLiveSessionId("session-1".into()),
            &state,
        )
        .unwrap()
        .unwrap();
        let usage = map_provider_event(
            OpenAiLiveEventKind::Usage {
                usage: serde_json::json!({
                    "input_tokens": 3, "output_tokens": 5, "audio_duration_ms": 100
                }),
            },
            &OpenAiLiveSessionId("session-1".into()),
            &state,
        )
        .unwrap()
        .unwrap();
        assert!(matches!(
            transcript,
            ProviderEvent::TranscriptFragment(TranscriptFragment {
                item_id, role: LiveVoiceRole::User, text, start_ms: 1, end_ms: 9
            }) if item_id == "item-1" && text == "exact text"
        ));
        assert!(
            matches!(projected, ProviderEvent::ProjectedTurn(ProjectedTurn {
            turn_id, text, kind: ProjectedTurnUpdateKind::Delta, ..
        }) if turn_id == "turn-1" && text == " delta")
        );
        assert!(matches!(
            delegation,
            ProviderEvent::DelegationRequested(ProviderDelegation {
                id,
                target: ProviderDelegationTarget::Client,
                task,
                source_turn_id: Some(source_turn_id),
            }) if id.0 == "delegation-1" && task == "do work" && source_turn_id == "turn-1"
        ));
        assert!(matches!(
            usage,
            ProviderEvent::UsageUpdated(ProviderUsage {
                input_tokens: Some(3),
                output_tokens: Some(5),
                total_tokens: None
            })
        ));
    }

    #[test]
    fn readiness_latches_once_and_rejects_mismatched_sideband_identity() {
        let state = test_state();
        let expected = OpenAiLiveSessionId("session-1".into());
        assert_eq!(
            map_provider_event(
                OpenAiLiveEventKind::SessionStarted {
                    session_id: Some(expected.clone()),
                },
                &expected,
                &state,
            )
            .unwrap(),
            Some(ProviderEvent::Ready)
        );
        assert_eq!(
            map_provider_event(
                OpenAiLiveEventKind::SessionStarted {
                    session_id: Some(expected.clone()),
                },
                &expected,
                &state,
            )
            .unwrap(),
            None
        );
        assert!(map_provider_event(
            OpenAiLiveEventKind::SessionStarted {
                session_id: Some(OpenAiLiveSessionId("other".into()))
            },
            &expected,
            &state
        )
        .is_err());
    }

    #[tokio::test]
    async fn bootstrap_and_sideband_readiness_share_one_latch() {
        let transport = Arc::new(BrowserLiveTransport::new());
        let connected = OpenAiLiveClient::new("key").connect_transport(transport.clone());
        let mut connection = build_connection(
            "answer".into(),
            OpenAiLiveSessionId("session-1".into()),
            connected,
        );
        assert_eq!(
            connection
                .control
                .dispatch(ProviderCommand::ValidateStartupObservation {
                    command_id: ProviderCommandId("bootstrap-1".into()),
                    observation: ProviderStartupObservation("session-1".into()),
                })
                .await,
            ProviderDispatchResult::Dispatched
        );
        transport
            .push_incoming(serde_json::json!({
                "type": "session.started", "session": { "id": "session-1" }
            }))
            .await
            .unwrap();
        assert!(matches!(
            connection.events.recv().await,
            Some(Ok(ProviderEvent::Ready))
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(20), connection.events.recv())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn commands_are_serialized_and_acknowledgements_restore_capacity() {
        let transport = Arc::new(BrowserLiveTransport::new());
        let mut outbound = transport.take_outbound().await.unwrap();
        let connected = OpenAiLiveClient::new("key").connect_transport(transport.clone());
        let mut connection = build_connection(
            "answer".into(),
            OpenAiLiveSessionId("session-1".into()),
            connected,
        );

        assert_eq!(
            connection
                .control
                .dispatch(ProviderCommand::PauseInput {
                    command_id: ProviderCommandId("input-1".into()),
                })
                .await,
            ProviderDispatchResult::Dispatched
        );
        assert!(matches!(
            connection
                .control
                .dispatch(ProviderCommand::ResumeInput {
                    command_id: ProviderCommandId("input-2".into()),
                })
                .await,
            ProviderDispatchResult::Rejected(_)
        ));
        let pause = outbound.recv().await.unwrap();
        assert_eq!(pause["event_id"], "goose_input-1");
        transport
            .push_incoming(serde_json::json!({ "type": "input_audio.paused" }))
            .await
            .unwrap();
        assert!(matches!(
            connection.events.recv().await,
            Some(Ok(ProviderEvent::InputPaused { command_id })) if command_id.0 == "input-1"
        ));

        assert_eq!(
            connection
                .control
                .dispatch(ProviderCommand::ResumeInput {
                    command_id: ProviderCommandId("input-2".into()),
                })
                .await,
            ProviderDispatchResult::Dispatched
        );
        let resume = outbound.recv().await.unwrap();
        assert_eq!(resume["event_id"], "goose_input-2");
        transport
            .push_incoming(serde_json::json!({
                "type": "error",
                "error": { "message": "rejected", "event_id": "goose_input-2" }
            }))
            .await
            .unwrap();
        assert!(matches!(
            connection.events.recv().await,
            Some(Ok(ProviderEvent::CommandRejected {
                command_id: Some(command_id),
                reason,
            })) if command_id.0 == "input-2" && reason == "rejected"
        ));

        assert_eq!(
            connection
                .control
                .dispatch(ProviderCommand::DeliverDelegationResult {
                    command_id: ProviderCommandId("delegation-1".into()),
                    delegation_id: ProviderDelegationId("item-1".into()),
                    result: "not supported".into(),
                })
                .await,
            ProviderDispatchResult::Dispatched
        );
        let append = outbound.recv().await.unwrap();
        assert_eq!(append["event_id"], "goose_delegation-1");
        assert_eq!(append["delegation_item_id"], "item-1");
        transport
            .push_incoming(serde_json::json!({
                "type": "delegation.context.appended",
                "delegation_item_id": "item-1"
            }))
            .await
            .unwrap();
        assert!(matches!(
            connection.events.recv().await,
            Some(Ok(ProviderEvent::DelegationResultAccepted { command_id, delegation_id }))
                if command_id.0 == "delegation-1" && delegation_id.0 == "item-1"
        ));
    }

    #[tokio::test]
    async fn fatal_identity_failure_keeps_draining_remote_close_metadata() {
        let transport = Arc::new(BrowserLiveTransport::new());
        let mut outbound = transport.take_outbound().await.unwrap();
        let connected = OpenAiLiveClient::new("key").connect_transport(transport.clone());
        let mut connection = build_connection(
            "answer".into(),
            OpenAiLiveSessionId("session-1".into()),
            connected,
        );

        transport
            .push_incoming(serde_json::json!({
                "type": "session.started", "session": { "id": "other-session" }
            }))
            .await
            .unwrap();
        assert!(matches!(
            connection.events.recv().await,
            Some(Err(ProviderEventStreamError::Failed(_)))
        ));
        assert_eq!(outbound.recv().await.unwrap()["type"], "session.close");
        let caller = tokio::spawn({
            let control = connection.control.clone();
            async move { control.close().await }
        });
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());

        transport
            .push_incoming(serde_json::json!({
                "type": "session.closed",
                "reason": "client_request",
                "usage": { "total_tokens": 9 }
            }))
            .await
            .unwrap();
        assert!(matches!(
            connection.events.recv().await,
            Some(Ok(ProviderEvent::RemoteClosed {
                reason: Some(reason),
                usage: Some(ProviderUsage { total_tokens: Some(9), .. })
            })) if reason == "client_request"
        ));
        connection.control.close().await.unwrap();
    }
}
