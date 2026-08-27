//! Native backend WebSocket transport for voice providers.
//! Enable with the `voice-websocket` feature.

use crate::voice::{VoiceSessionAnswerBootstrap, VoiceTransport, VoiceTransportRequest};
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use http::Request;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

pub struct WebSocketVoiceTransport {
    socket: Mutex<Option<Socket>>,
}

impl WebSocketVoiceTransport {
    pub fn new() -> Self {
        Self {
            socket: Mutex::new(None),
        }
    }
}

impl Default for WebSocketVoiceTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VoiceTransport for WebSocketVoiceTransport {
    async fn connect(&self, request: VoiceTransportRequest) -> Result<VoiceSessionAnswerBootstrap> {
        let initial_event = request.initial_event;
        let mut builder = Request::builder().uri(&request.endpoint);
        for (name, value) in request.headers {
            builder = builder.header(name, value);
        }
        let request = builder.body(())?;
        let (mut socket, _) = connect_async(request)
            .await
            .context("voice WebSocket connection failed")?;
        if let Some(initial) = initial_event {
            socket
                .send(Message::Text(initial.to_string().into()))
                .await?;
        }
        *self.socket.lock().await = Some(socket);
        Ok(VoiceSessionAnswerBootstrap::Connected)
    }

    async fn send(&self, event: Value) -> Result<()> {
        self.socket
            .lock()
            .await
            .as_mut()
            .context("voice WebSocket is not connected")?
            .send(Message::Text(event.to_string().into()))
            .await?;
        Ok(())
    }

    async fn receive(&self) -> Result<Option<Value>> {
        let socket = &mut *self.socket.lock().await;
        let socket = socket
            .as_mut()
            .context("voice WebSocket is not connected")?;
        while let Some(message) = socket.next().await {
            match message? {
                Message::Text(text) => return Ok(Some(serde_json::from_str(&text)?)),
                Message::Binary(bytes) => return Ok(Some(serde_json::from_slice(&bytes)?)),
                Message::Close(_) => return Ok(None),
                _ => {}
            }
        }
        Ok(None)
    }

    async fn close(&self) -> Result<()> {
        if let Some(socket) = self.socket.lock().await.as_mut() {
            socket.close(None).await?;
        }
        Ok(())
    }
}
