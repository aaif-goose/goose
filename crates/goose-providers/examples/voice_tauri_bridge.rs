//! Sketch of a Tauri/browser WebRTC bridge.
//!
//! The host's `RTCPeerConnection` creates the SDP offer and forwards every data
//! channel message to `push_incoming`. Rust processes events through
//! `VoiceSession`; outbound JSON is read from `take_outbound()` and sent with
//! `RTCDataChannel.send`.

use anyhow::Result;
use async_trait::async_trait;
use goose_providers::{
    openai_live::OpenAiLiveProvider,
    voice::{
        BridgedVoiceConnection, NegotiatedWebRtcSession, VoiceDelegationMode, VoiceProvider,
        VoiceSession, VoiceSessionConfig, VoiceSessionMetadata, VoiceSignaler, WebRtcSignalingPlan,
    },
};
use std::sync::Arc;

struct BrowserSignaler;

#[async_trait]
impl VoiceSignaler for BrowserSignaler {
    async fn negotiate(&self, plan: WebRtcSignalingPlan) -> Result<NegotiatedWebRtcSession> {
        println!("bridge signaling through trusted Rust: {}", plan.endpoint());
        let (_, _, _, _, model) = plan.take_parts();
        Ok(NegotiatedWebRtcSession {
            answer_sdp: "answer-from-provider".into(),
            session_id: None,
            model,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let provider: Arc<dyn VoiceProvider> = Arc::new(OpenAiLiveProvider::from_env()?);

    let plan = provider.prepare_webrtc(
        &VoiceSessionConfig {
            model: "gpt-live-1-marble-alpha".into(),
            instructions: "Be concise.".into(),
            voice: Some("marin".into()),
            initial_items: vec![],
            delegation: VoiceDelegationMode::Client,
            extra: Default::default(),
        },
        "offer-from-browser".into(),
    )?;
    let negotiated = BrowserSignaler.negotiate(plan).await?;
    println!("install SDP answer: {}", &negotiated.answer_sdp);

    let connection = Arc::new(BridgedVoiceConnection::new());
    let mut outbound = connection.take_outbound().await?;
    let session = VoiceSession::new(
        provider,
        connection.clone(),
        VoiceSessionMetadata::from(&negotiated),
    );

    // Tauri forwards `outbound.recv()` payloads to RTCDataChannel.send().
    tokio::spawn(async move {
        while let Some(event) = outbound.recv().await {
            println!("send over browser data channel: {event}");
        }
    });

    // The frontend's data-channel onmessage callback calls this method.
    connection
        .push_incoming(serde_json::json!({ "type": "session.started" }))
        .await?;
    let _runner = tokio::spawn(session.run());
    Ok(())
}
