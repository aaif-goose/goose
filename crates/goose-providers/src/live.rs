//! Runtime primitives for provider-specific live sessions.
//!
//! A protocol translates typed commands and events, while a transport only
//! delivers provider wire messages. Connected sessions own their receive task.

use anyhow::{bail, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{broadcast, watch};

#[async_trait]
pub trait LiveTransport: Send + Sync {
    async fn send(&self, message: Value) -> Result<()>;
    async fn receive(&self) -> Result<Option<Value>>;
    async fn close(&self) -> Result<()>;
}

pub trait LiveProtocol: Send + Sync + 'static {
    type Command: Send + 'static;
    type Event: Clone + Send + 'static;

    fn encode(&self, command: Self::Command) -> Result<Value>;
    fn decode(&self, message: Value) -> Result<Self::Event>;
    fn closed_event(&self) -> Self::Event;
    fn is_closed(&self, event: &Self::Event) -> bool;
}

pub struct LiveSession<P: LiveProtocol> {
    protocol: Arc<P>,
    transport: Arc<dyn LiveTransport>,
    events: broadcast::Sender<P::Event>,
    closed: watch::Sender<bool>,
}

impl<P: LiveProtocol> LiveSession<P> {
    pub fn connect(protocol: Arc<P>, transport: Arc<dyn LiveTransport>) -> Arc<Self> {
        let (events, _) = broadcast::channel(256);
        let (closed, _) = watch::channel(false);
        let session = Arc::new(Self {
            protocol,
            transport,
            events,
            closed,
        });
        tokio::spawn(session.clone().receive_loop());
        session
    }

    pub fn subscribe(&self) -> broadcast::Receiver<P::Event> {
        self.events.subscribe()
    }

    pub async fn send(&self, command: P::Command) -> Result<()> {
        if *self.closed.borrow() {
            bail!("live session is closed");
        }
        self.transport.send(self.protocol.encode(command)?).await
    }

    pub async fn close(&self) -> Result<()> {
        if !*self.closed.borrow() {
            let _ = self.closed.send(true);
            let _ = self.events.send(self.protocol.closed_event());
        }
        self.transport.close().await
    }

    async fn receive_loop(self: Arc<Self>) {
        loop {
            let message = match self.transport.receive().await {
                Ok(Some(message)) => message,
                Ok(None) | Err(_) => break,
            };
            let event = match self.protocol.decode(message) {
                Ok(event) => event,
                Err(_) => break,
            };
            let is_closed = self.protocol.is_closed(&event);
            let _ = self.events.send(event);
            if is_closed {
                break;
            }
        }
        if !*self.closed.borrow() {
            let _ = self.closed.send(true);
            let _ = self.events.send(self.protocol.closed_event());
        }
    }
}
