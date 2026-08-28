//! Native backend WebSocket transport for voice providers.
//! Enable with the `voice-websocket` feature.

use crate::voice::{VoiceSessionAnswerBootstrap, VoiceTransport, VoiceTransportRequest};
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::{stream::SplitSink, stream::SplitStream, SinkExt, StreamExt};
use http::Request;
use serde_json::Value;
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
type SocketSink = SplitSink<Socket, Message>;
type SocketStream = SplitStream<Socket>;

pub struct WebSocketVoiceTransport {
    sink: Mutex<Option<SocketSink>>,
    stream: Mutex<Option<SocketStream>>,
}

impl WebSocketVoiceTransport {
    pub fn new() -> Self {
        Self {
            sink: Mutex::new(None),
            stream: Mutex::new(None),
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
        let (endpoint, headers, _, initial_event) = request.take_connection_parts();
        let mut builder = Request::builder().uri(&endpoint);
        for (name, value) in headers {
            builder = builder.header(name, value);
        }
        let request = builder.body(())?;
        let (socket, _) = connect_async(request)
            .await
            .context("voice WebSocket connection failed")?;
        let (mut sink, stream) = socket.split();
        if let Some(initial) = initial_event {
            sink.send(Message::Text(initial.to_string().into())).await?;
        }
        *self.sink.lock().await = Some(sink);
        *self.stream.lock().await = Some(stream);
        Ok(VoiceSessionAnswerBootstrap::Connected)
    }

    async fn send(&self, event: Value) -> Result<()> {
        self.sink
            .lock()
            .await
            .as_mut()
            .context("voice WebSocket is not connected")?
            .send(Message::Text(event.to_string().into()))
            .await?;
        Ok(())
    }

    async fn receive(&self) -> Result<Option<Value>> {
        let stream = &mut *self.stream.lock().await;
        let stream = stream
            .as_mut()
            .context("voice WebSocket is not connected")?;
        while let Some(message) = stream.next().await {
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
        if let Some(sink) = self.sink.lock().await.as_mut() {
            sink.close().await?;
        }
        Ok(())
    }
}
