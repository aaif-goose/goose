//! End-to-end Tauri/browser WebRTC integration example.
//!
//! This file shows the Rust half. Copy the TypeScript companion from
//! `examples/voice_tauri.ts` into the Tauri frontend. Add these commands and
//! managed state to the application's builder as shown in `run()` below.
//! The project API key remains in Rust; the browser owns WebRTC media.

use anyhow::{Context, Result};
use goose_providers::{
    openai_live::OpenAiLiveProvider,
    voice::{
        VoiceContext, VoiceContextChannel, VoiceEvent, VoiceMessage, VoiceMessageRole,
        VoiceProvider, VoiceSessionBootstrap, VoiceSessionConfig, VoiceTransportRequest,
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

struct BrowserVoiceSession;

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

/// Brokers the authenticated WebRTC offer/answer exchange in Rust, keeping the
/// project API key out of the WebView.
#[tauri::command]
async fn start_voice(
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
    let transport = state
        .provider
        .transport_request(
            &config,
            VoiceSessionBootstrap::WebRtcOffer { sdp: request.sdp },
        )
        .map_err(|error| error.to_string())?;
    let (endpoint, headers, bootstrap, initial_event) = transport.take_connection_parts();
    let VoiceSessionBootstrap::WebRtcOffer { sdp } = bootstrap else {
        return Err("OpenAI did not produce a WebRTC bootstrap".into());
    };
    let session = initial_event
        .context("missing session configuration")
        .map_err(|e| e.to_string())?;
    let form = reqwest::multipart::Form::new()
        .text("sdp", sdp)
        .text("session", session.to_string());
    let mut broker = reqwest::Client::new().post(endpoint).multipart(form);
    for (name, value) in headers {
        broker = broker.header(name, value);
    }
    broker = broker.header(reqwest::header::ACCEPT, "application/sdp");
    let response = broker.send().await.map_err(|e| e.to_string())?;
    let status = response.status();
    let session_id = response
        .headers()
        .get("openai-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let answer = response.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("Live signaling failed ({status}): {answer}"));
    }
    *state.session.lock().await = Some(BrowserVoiceSession);
    Ok(StartVoiceResponse {
        sdp: answer,
        session_id,
        model: "gpt-live-1-marble-alpha".into(),
    })
}

/// Decodes an event received by RTCDataChannel.onmessage in the frontend.
/// The normalized event is returned and also emitted for Rust/Tauri consumers.
#[tauri::command]
async fn voice_incoming(
    app: tauri::AppHandle,
    state: State<'_, VoiceState>,
    event: Value,
) -> Result<VoiceEvent, String> {
    let decoded = state
        .provider
        .decode_event(event)
        .map_err(|e| e.to_string())?;
    app.emit("voice-event", &decoded)
        .map_err(|e| e.to_string())?;
    Ok(decoded)
}

/// Returns provider JSON for RTCDataChannel.send in the frontend.
#[tauri::command]
async fn append_voice_context(state: State<'_, VoiceState>, text: String) -> Result<Value, String> {
    state
        .provider
        .append_context_event(VoiceContext {
            text,
            channel: VoiceContextChannel::Commentary,
        })
        .map_err(|e| e.to_string())
}

/// Returns a client-delegation result for RTCDataChannel.send.
#[tauri::command]
async fn complete_voice_delegation(
    state: State<'_, VoiceState>,
    request: CompleteDelegationRequest,
) -> Result<Value, String> {
    state
        .provider
        .complete_delegation_event(
            &request.delegation_id,
            VoiceContext {
                text: request.text,
                channel: VoiceContextChannel::Speakable,
            },
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn close_voice(state: State<'_, VoiceState>) -> Result<Value, String> {
    *state.session.lock().await = None;
    Ok(state.provider.close_event())
}

/// Call this from your Tauri application's `run()` function:
///
/// ```ignore
/// tauri::Builder::default()
///     .manage(voice_state()?)
///     .invoke_handler(voice_invoke_handler());
/// ```
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
        close_voice,
    ]
}

fn main() {
    // This example is intended to be copied into a Tauri application, where
    // generate_context! can use that application's tauri.conf.json.
    let _ = voice_state;
    let _ = voice_invoke_handler;
}
