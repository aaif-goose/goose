//! End-to-end Tauri/browser WebRTC integration example.
//!
//! Rust owns authenticated signaling and the provider session. The browser owns
//! microphone capture, playback, RTCPeerConnection, and RTCDataChannel.

use anyhow::Result;
use async_trait::async_trait;
use goose_providers::{
    openai_live::OpenAiLiveProvider,
    voice::{
        BridgedVoiceConnection, NegotiatedWebRtcSession, VoiceContext, VoiceContextChannel,
        VoiceMessage, VoiceMessageRole, VoiceProvider, VoiceSession, VoiceSessionConfig,
        VoiceSessionMetadata, VoiceSignaler, WebRtcSignalingPlan,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tauri::{Emitter, State};
use tokio::sync::Mutex;

struct VoiceState {
    provider: Arc<dyn VoiceProvider>,
    session: Mutex<Option<BrowserVoiceSession>>,
}

struct BrowserVoiceSession {
    connection: Arc<BridgedVoiceConnection>,
    session: Arc<VoiceSession>,
}

struct OpenAiWebRtcSignaler {
    client: reqwest::Client,
}

#[async_trait]
impl VoiceSignaler for OpenAiWebRtcSignaler {
    async fn negotiate(&self, plan: WebRtcSignalingPlan) -> Result<NegotiatedWebRtcSession> {
        let (endpoint, headers, offer_sdp, session, model) = plan.take_parts();
        let form = reqwest::multipart::Form::new()
            .text("sdp", offer_sdp)
            .text("session", session.to_string());
        let mut request = self.client.post(endpoint).multipart(form);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        let response = request
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
            anyhow::bail!("Live signaling failed ({status}): {answer_sdp}");
        }
        Ok(NegotiatedWebRtcSession {
            answer_sdp,
            session_id,
            model,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartVoiceRequest {
    sdp: String,
    initial_context: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartVoiceResponse {
    sdp: String,
    session_id: Option<String>,
    model: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompleteDelegationRequest {
    delegation_id: String,
    text: String,
}

#[tauri::command]
async fn start_voice(
    app: tauri::AppHandle,
    state: State<'_, VoiceState>,
    request: StartVoiceRequest,
) -> Result<StartVoiceResponse, String> {
    let config = VoiceSessionConfig {
        model: "gpt-live-1-marble-alpha".into(),
        instructions: "Be concise. Wait until the user speaks.".into(),
        voice: Some("marin".into()),
        initial_items: vec![VoiceMessage {
            role: VoiceMessageRole::Developer,
            text: request.initial_context,
        }],
        delegation: goose_providers::voice::VoiceDelegationMode::Client,
        extra: Default::default(),
    };
    let plan = state
        .provider
        .prepare_webrtc(&config, request.sdp)
        .map_err(|error| error.to_string())?;
    let negotiated = OpenAiWebRtcSignaler {
        client: reqwest::Client::new(),
    }
    .negotiate(plan)
    .await
    .map_err(|error| error.to_string())?;

    let connection = Arc::new(BridgedVoiceConnection::new());
    let mut outbound = connection
        .take_outbound()
        .await
        .map_err(|error| error.to_string())?;
    let session = VoiceSession::new(
        state.provider.clone(),
        connection.clone(),
        VoiceSessionMetadata::from(&negotiated),
    );

    let command_app = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(command) = outbound.recv().await {
            let _ = command_app.emit("voice-command", command);
        }
    });

    let mut events = session.subscribe();
    let event_app = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = events.recv().await {
            let _ = event_app.emit("voice-event", event);
        }
    });
    tauri::async_runtime::spawn(session.clone().run());

    *state.session.lock().await = Some(BrowserVoiceSession {
        connection,
        session,
    });
    Ok(StartVoiceResponse {
        sdp: negotiated.answer_sdp,
        session_id: negotiated.session_id,
        model: negotiated.model,
    })
}

#[tauri::command]
async fn voice_incoming(state: State<'_, VoiceState>, event: Value) -> Result<(), String> {
    let connection = state
        .session
        .lock()
        .await
        .as_ref()
        .map(|session| session.connection.clone())
        .ok_or_else(|| "voice session is not active".to_string())?;
    connection
        .push_incoming(event)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn append_voice_context(state: State<'_, VoiceState>, text: String) -> Result<(), String> {
    let session = state
        .session
        .lock()
        .await
        .as_ref()
        .map(|session| session.session.clone())
        .ok_or_else(|| "voice session is not active".to_string())?;
    session
        .append_context(VoiceContext {
            text,
            channel: VoiceContextChannel::Commentary,
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn complete_voice_delegation(
    state: State<'_, VoiceState>,
    request: CompleteDelegationRequest,
) -> Result<(), String> {
    let session = state
        .session
        .lock()
        .await
        .as_ref()
        .map(|session| session.session.clone())
        .ok_or_else(|| "voice session is not active".to_string())?;
    session
        .complete_delegation(
            &request.delegation_id,
            VoiceContext {
                text: request.text,
                channel: VoiceContextChannel::Speakable,
            },
        )
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn voice_session_id(state: State<'_, VoiceState>) -> Result<Option<String>, String> {
    let session = state
        .session
        .lock()
        .await
        .as_ref()
        .map(|session| session.session.clone())
        .ok_or_else(|| "voice session is not active".to_string())?;
    Ok(session.session_id().await)
}

#[tauri::command]
async fn close_voice(state: State<'_, VoiceState>) -> Result<(), String> {
    let session = state
        .session
        .lock()
        .await
        .take()
        .ok_or_else(|| "voice session is not active".to_string())?;
    session
        .session
        .close()
        .await
        .map_err(|error| error.to_string())
}

fn voice_state() -> Result<VoiceState> {
    Ok(VoiceState {
        provider: Arc::new(OpenAiLiveProvider::from_env()?),
        session: Mutex::new(None),
    })
}

fn voice_invoke_handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static
{
    tauri::generate_handler![
        start_voice,
        voice_incoming,
        append_voice_context,
        complete_voice_delegation,
        voice_session_id,
        close_voice,
    ]
}

fn main() {
    let _ = voice_state;
    let _ = voice_invoke_handler;
}
