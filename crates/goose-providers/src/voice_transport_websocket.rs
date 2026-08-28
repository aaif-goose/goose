//! Native backend WebSocket connection for voice providers.
//! Enable with the `voice-websocket` feature.

use crate::voice::{VoiceConnection, WebSocketConnectionPlan};
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

pub struct WebSocketVoiceConnection {
    sink: Mutex<SocketSink>,
    stream: Mutex<SocketStream>,
}

impl WebSocketVoiceConnection {
    pub async fn connect(plan: WebSocketConnectionPlan) -> Result<Self> {
        let (endpoint, headers, after_connect, _) = plan.take_parts();
        let mut builder = Request::builder().uri(&endpoint);
        for (name, value) in headers {
            builder = builder.header(name, value);
        }
        let request = builder.body(())?;
        let (socket, _) = connect_async(request)
            .await
            .context("voice WebSocket connection failed")?;
        let (mut sink, stream) = socket.split();
        for event in after_connect {
            sink.send(Message::Text(event.to_string().into())).await?;
        }
        Ok(Self {
            sink: Mutex::new(sink),
            stream: Mutex::new(stream),
        })
    }
}

#[async_trait]
impl VoiceConnection for WebSocketVoiceConnection {
    async fn send(&self, event: Value) -> Result<()> {
        self.sink
            .lock()
            .await
            .send(Message::Text(event.to_string().into()))
            .await?;
        Ok(())
    }

    async fn receive(&self) -> Result<Option<Value>> {
        let stream = &mut *self.stream.lock().await;
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
        self.sink.lock().await.close().await?;
        Ok(())
    }
}
