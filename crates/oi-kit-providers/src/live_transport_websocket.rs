//! Native WebSocket transport for OpenAI Live sessions.

use crate::{
    api_client::{ApiClient, TlsConfig},
    live::LiveTransport,
    openai_live::OpenAiLiveWebSocketRequest,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::{stream::SplitSink, stream::SplitStream, SinkExt, StreamExt};
use reqwest_websocket::{Message, Upgrade, WebSocket};
use serde_json::Value;
use tokio::{
    sync::Mutex,
    time::{timeout, Duration},
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

type SocketSink = SplitSink<WebSocket, Message>;
type SocketStream = SplitStream<WebSocket>;

pub struct WebSocketLiveTransport {
    sink: Mutex<SocketSink>,
    stream: Mutex<SocketStream>,
}

impl WebSocketLiveTransport {
    pub async fn connect(request: OpenAiLiveWebSocketRequest) -> Result<Self> {
        Self::connect_with_tls_config(request, None).await
    }

    pub(crate) async fn connect_with_tls_config(
        request: OpenAiLiveWebSocketRequest,
        tls_config: Option<&TlsConfig>,
    ) -> Result<Self> {
        let client = ApiClient::http_client_builder(tls_config)?
            .http1_only()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let mut websocket_request = client.get(request.endpoint);
        for (name, value) in request.headers {
            websocket_request = websocket_request.header(name, value);
        }
        let socket = timeout(CONNECT_TIMEOUT, async {
            websocket_request
                .upgrade()
                .send()
                .await?
                .into_websocket()
                .await
        })
        .await
        .context("live WebSocket connection timed out")?
        .context("live WebSocket connection failed")?;
        let (mut sink, stream) = socket.split();
        timeout(CONNECT_TIMEOUT, async {
            for message in request.initial_messages {
                sink.send(Message::Text(message.to_string())).await?;
            }
            Result::<()>::Ok(())
        })
        .await
        .context("live WebSocket initialization timed out")??;
        Ok(Self {
            sink: Mutex::new(sink),
            stream: Mutex::new(stream),
        })
    }
}

#[async_trait]
impl LiveTransport for WebSocketLiveTransport {
    async fn send(&self, message: Value) -> Result<()> {
        self.sink
            .lock()
            .await
            .send(Message::Text(message.to_string()))
            .await?;
        Ok(())
    }

    async fn receive(&self) -> Result<Option<Value>> {
        let stream = &mut *self.stream.lock().await;
        while let Some(message) = stream.next().await {
            match message? {
                Message::Text(text) => return Ok(Some(serde_json::from_str(&text)?)),
                Message::Binary(bytes) => return Ok(Some(serde_json::from_slice(&bytes)?)),
                Message::Close { .. } => return Ok(None),
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
