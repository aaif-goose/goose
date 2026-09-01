#![cfg(feature = "live-websocket")]

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use goose_providers::openai_live::{OpenAiLiveClient, OpenAiLiveSessionConfig};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_hdr_async, tungstenite::Message};

#[tokio::test]
#[allow(clippy::result_large_err)]
async fn websocket_connect_performs_handshake_and_waits_until_ready() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = accept_hdr_async(stream, |request: &http::Request<()>, response| {
            assert_eq!(request.headers()["authorization"], "Bearer test-key");
            assert_eq!(request.headers()["openai-alpha"], "test-alpha");
            Ok(response)
        })
        .await
        .unwrap();
        let message = socket.next().await.unwrap().unwrap();
        let Message::Text(text) = message else {
            panic!("expected initial text message");
        };
        let session_update: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(session_update["type"], "session.update");
        socket
            .send(Message::Text(
                json!({ "type": "session.started", "session": { "id": "session_1" } })
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
    });

    let connected = OpenAiLiveClient::new("test-key")
        .with_alpha_selector("test-alpha")
        .with_websocket_endpoint(format!("ws://{address}/v1/live"))
        .websocket(OpenAiLiveSessionConfig {
            model: "gpt-live-test".into(),
            instructions: "test".into(),
            voice: None,
            initial_items: vec![],
            experimental: Default::default(),
        })
        .connect()
        .await?;

    let _session = connected.session;
    server.await?;
    Ok(())
}
