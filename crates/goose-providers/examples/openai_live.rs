//! OpenAI Live low-level API examples.

use anyhow::Result;
use goose_providers::{
    browser_live_transport::BrowserLiveTransport,
    openai_live::{OpenAiLiveClient, OpenAiLiveSessionConfig},
};
use std::sync::Arc;

fn config() -> OpenAiLiveSessionConfig {
    OpenAiLiveSessionConfig {
        model: "gpt-live-1".into(),
        instructions: "Be concise.".into(),
        voice: Some("marin".into()),
        input_messages: vec![],
        extra_session_fields: Default::default(),
    }
}

#[cfg(feature = "live-websocket")]
async fn primary_websocket(client: &OpenAiLiveClient) -> Result<()> {
    let mut connected = client.websocket(config()).connect().await?;
    let event = connected.recv().await?;
    println!("{event:?}");
    connected.close().await
}

async fn browser_webrtc(client: &OpenAiLiveClient, offer_sdp: String) -> Result<()> {
    let negotiation = client.webrtc(config()).negotiate(offer_sdp).await?;
    let transport = Arc::new(BrowserLiveTransport::new());

    // Forward `transport.take_outbound()` to RTCDataChannel.send in the browser,
    // and pass data-channel messages to `transport.push_incoming()`.
    let mut connected = negotiation.bind(transport).await?;
    let event = connected.recv().await?;
    println!("{event:?}");
    connected.close().await
}

fn main() {
    let _ = OpenAiLiveClient::from_env;
    let _ = browser_webrtc;
    #[cfg(feature = "live-websocket")]
    let _ = primary_websocket;
}
