//! Sketch of a Tauri/browser WebRTC bridge.
//!
//! The host's `RTCPeerConnection` creates the SDP offer and forwards every data
//! channel message to `push_incoming`. Rust processes events through
//! `VoiceSession`; outbound JSON is read from `outbound()` and sent with
//! `RTCDataChannel.send`.

use anyhow::Result;
use goose_providers::{
    openai_live::OpenAiLiveProvider,
    voice::{
        BridgedVoiceTransport, VoiceDelegationMode, VoiceProvider, VoiceSession,
        VoiceSessionAnswerBootstrap, VoiceSessionBootstrap, VoiceSessionConfig,
    },
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let provider: Arc<dyn VoiceProvider> = Arc::new(OpenAiLiveProvider::from_env()?);

    // In Tauri, this callback invokes frontend code which:
    // 1. creates/configures RTCPeerConnection,
    // 2. POSTs or relays the provider's transport request,
    // 3. installs the SDP answer, and
    // 4. returns the answer below.
    let transport = Arc::new(BridgedVoiceTransport::new(|request| async move {
        println!("bridge this request to WebRTC: {}", request.endpoint());
        Ok(VoiceSessionAnswerBootstrap::WebRtcAnswer {
            sdp: "answer-from-browser".into(),
        })
    }));
    let mut outbound = transport.take_outbound().await?;

    let (session, answer) = VoiceSession::connect(
        provider,
        transport.clone(),
        VoiceSessionConfig {
            model: "gpt-live-1-marble-alpha".into(),
            instructions: "Be concise.".into(),
            voice: Some("marin".into()),
            initial_items: vec![],
            delegation: VoiceDelegationMode::Client,
            extra: Default::default(),
        },
        VoiceSessionBootstrap::WebRtcOffer {
            sdp: "offer-from-browser".into(),
        },
    )
    .await?;
    println!("negotiated voice bootstrap: {:?}", answer.bootstrap);

    // Tauri forwards `outbound.recv()` payloads to RTCDataChannel.send().
    tokio::spawn(async move {
        while let Ok(event) = outbound.recv().await {
            println!("send over browser data channel: {event}");
        }
    });

    // The frontend's data-channel onmessage callback calls this method.
    transport
        .push_incoming(serde_json::json!({ "type": "session.started" }))
        .await?;
    let _runner = tokio::spawn(session.run());
    Ok(())
}
