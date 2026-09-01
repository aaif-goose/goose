//! OpenAI Live WebSocket and browser-owned WebRTC API examples.

use anyhow::Result;
use goose_providers::{
    browser_live_transport::BrowserLiveTransport,
    openai_live::{OpenAiLiveClient, OpenAiLiveSessionConfig},
};
use std::sync::Arc;

fn config() -> OpenAiLiveSessionConfig {
    OpenAiLiveSessionConfig {
        model: "gpt-live-1-marble-alpha".into(),
        instructions: "Be concise.".into(),
        voice: Some("marin".into()),
        initial_items: vec![],
        experimental: Default::default(),
    }
}

#[cfg(feature = "live-websocket")]
async fn websocket(client: &OpenAiLiveClient) -> Result<()> {
    let mut connected = client.websocket(config()).connect().await?;
    let event = connected.recv().await?;
    println!("{event:?}");
    connected.session.close().await
}

async fn browser_webrtc(client: &OpenAiLiveClient, offer_sdp: String) -> Result<()> {
    let negotiation = client.webrtc(config()).negotiate(offer_sdp).await?;
    let transport = Arc::new(BrowserLiveTransport::new());

    // Forward `transport.take_outbound()` to RTCDataChannel.send in the browser,
    // and pass data-channel messages to `transport.push_incoming()`.
    let mut connected = negotiation.bind(transport).ready().await?;
    let event = connected.recv().await?;
    println!("{event:?}");
    connected.session.close().await
}

fn main() {
    let _ = OpenAiLiveClient::from_env;
    let _ = browser_webrtc;
    #[cfg(feature = "live-websocket")]
    let _ = websocket;
}
