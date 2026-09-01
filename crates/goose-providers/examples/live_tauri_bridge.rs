//! Sketch of a browser-owned WebRTC integration.

use anyhow::Result;
use goose_providers::{
    browser_live_transport::BrowserLiveTransport,
    openai_live::{OpenAiLiveClient, OpenAiLiveDelegationMode, OpenAiLiveSessionConfig},
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let client = OpenAiLiveClient::from_env()?;
    let negotiation = client
        .webrtc(OpenAiLiveSessionConfig {
            model: "gpt-live-1-marble-alpha".into(),
            instructions: "Be concise.".into(),
            voice: Some("marin".into()),
            initial_items: vec![],
            delegation: OpenAiLiveDelegationMode::Client,
            experimental: Default::default(),
        })
        .negotiate("offer-from-browser")
        .await?;
    println!("install SDP answer: {}", negotiation.answer_sdp);

    let transport = Arc::new(BrowserLiveTransport::new());
    let mut outbound = transport.take_outbound().await?;
    let _session = negotiation.bind(transport.clone());

    tokio::spawn(async move {
        while let Some(message) = outbound.recv().await {
            println!("send over browser data channel: {message}");
        }
    });
    transport
        .push_incoming(serde_json::json!({ "type": "session.started" }))
        .await?;
    Ok(())
}
