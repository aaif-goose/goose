//! End-to-end Tauri/browser WebRTC integration example.

use anyhow::Result;
use goose_providers::{
    browser_live_transport::BrowserLiveTransport,
    live::LiveSession,
    openai_live::{
        OpenAiLiveClient, OpenAiLiveCommand, OpenAiLiveContext, OpenAiLiveContextChannel,
        OpenAiLiveDelegationMode, OpenAiLiveEvent, OpenAiLiveMessage, OpenAiLiveMessageRole,
        OpenAiLiveProtocol, OpenAiLiveSessionConfig,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tauri::{Emitter, State};
use tokio::sync::Mutex;

struct LiveState {
    client: OpenAiLiveClient,
    session: Mutex<Option<BrowserLiveSession>>,
}

struct BrowserLiveSession {
    transport: Arc<BrowserLiveTransport>,
    session: Arc<LiveSession<OpenAiLiveProtocol>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartLiveRequest {
    sdp: String,
    initial_context: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartLiveResponse {
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
async fn start_live(
    app: tauri::AppHandle,
    state: State<'_, LiveState>,
    request: StartLiveRequest,
) -> Result<StartLiveResponse, String> {
    let negotiation = state
        .client
        .webrtc(OpenAiLiveSessionConfig {
            model: "gpt-live-1-marble-alpha".into(),
            instructions: "Be concise. Wait until the user speaks.".into(),
            voice: Some("marin".into()),
            initial_items: vec![OpenAiLiveMessage {
                role: OpenAiLiveMessageRole::Developer,
                text: request.initial_context,
            }],
            delegation: OpenAiLiveDelegationMode::Client,
            experimental: Default::default(),
        })
        .negotiate(request.sdp)
        .await
        .map_err(|error| error.to_string())?;

    let response = StartLiveResponse {
        sdp: negotiation.answer_sdp.clone(),
        session_id: negotiation.session_id.clone(),
        model: negotiation.model.clone(),
    };
    let transport = Arc::new(BrowserLiveTransport::new());
    let mut outbound = transport
        .take_outbound()
        .await
        .map_err(|error| error.to_string())?;
    let session = negotiation.bind(transport.clone());

    let command_app = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(command) = outbound.recv().await {
            let _ = command_app.emit("live-command", command);
        }
    });

    let mut events = session.subscribe();
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = events.recv().await {
            let _ = app.emit("live-event", event);
        }
    });

    *state.session.lock().await = Some(BrowserLiveSession { transport, session });
    Ok(response)
}

#[tauri::command]
async fn live_incoming(state: State<'_, LiveState>, event: Value) -> Result<(), String> {
    let transport = active_session(&state).await?.transport.clone();
    transport
        .push_incoming(event)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn append_live_context(state: State<'_, LiveState>, text: String) -> Result<(), String> {
    active_session(&state)
        .await?
        .session
        .send(OpenAiLiveCommand::AppendContext(OpenAiLiveContext {
            text,
            channel: OpenAiLiveContextChannel::Commentary,
        }))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn complete_live_delegation(
    state: State<'_, LiveState>,
    request: CompleteDelegationRequest,
) -> Result<(), String> {
    active_session(&state)
        .await?
        .session
        .send(OpenAiLiveCommand::CompleteDelegation {
            delegation_id: request.delegation_id,
            context: OpenAiLiveContext {
                text: request.text,
                channel: OpenAiLiveContextChannel::Speakable,
            },
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn close_live(state: State<'_, LiveState>) -> Result<(), String> {
    let session = state
        .session
        .lock()
        .await
        .take()
        .ok_or_else(|| "live session is not active".to_string())?;
    let _ = session.session.send(OpenAiLiveCommand::Close).await;
    session
        .session
        .close()
        .await
        .map_err(|error| error.to_string())
}

async fn active_session(state: &State<'_, LiveState>) -> Result<BrowserLiveSession, String> {
    state
        .session
        .lock()
        .await
        .as_ref()
        .map(|session| BrowserLiveSession {
            transport: session.transport.clone(),
            session: session.session.clone(),
        })
        .ok_or_else(|| "live session is not active".to_string())
}

fn live_state() -> Result<LiveState> {
    Ok(LiveState {
        client: OpenAiLiveClient::from_env()?,
        session: Mutex::new(None),
    })
}

fn live_invoke_handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static
{
    tauri::generate_handler![
        start_live,
        live_incoming,
        append_live_context,
        complete_live_delegation,
        close_live,
    ]
}

fn main() {
    let _ = live_state;
    let _ = live_invoke_handler;
    let _: Option<OpenAiLiveEvent> = None;
}
