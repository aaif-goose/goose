//! Runtime primitives for provider-specific live sessions.
//!
//! A protocol translates typed commands and events, while a transport only
//! delivers provider wire messages. Connected sessions own their receive task.

use anyhow::{bail, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::{
    sync::{broadcast, watch},
    task::JoinHandle,
    time::{timeout, Duration},
};

const CLOSE_ACKNOWLEDGEMENT_TIMEOUT: Duration = Duration::from_secs(5);

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

    /// Command requesting a graceful shutdown, sent by [`LiveSession::close`].
    /// Protocols without one return `None` and are closed at the transport.
    fn close_command(&self) -> Option<Self::Command>;

    /// Whether the provider has acknowledged that the session is over.
    fn is_close_acknowledgement(&self, event: &Self::Event) -> bool;
}

/// Why a live session stopped delivering provider events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveSessionEnd {
    /// The provider acknowledged shutdown, or the transport closed cleanly.
    Closed,
    /// The transport failed.
    TransportFailed,
    /// A provider message could not be decoded.
    DecodeFailed,
}

/// Session lifecycle wrapped around protocol events, so consumers can tell a
/// clean shutdown from a failure without inspecting provider event payloads.
#[derive(Debug, Clone)]
pub enum LiveSessionEvent<E> {
    Message(E),
    Ended {
        reason: LiveSessionEnd,
        error: Option<Arc<anyhow::Error>>,
    },
}

impl<E> LiveSessionEvent<E> {
    fn ended(reason: LiveSessionEnd, error: Option<anyhow::Error>) -> Self {
        Self::Ended {
            reason,
            error: error.map(Arc::new),
        }
    }
}

pub struct LiveSession<P: LiveProtocol> {
    protocol: Arc<P>,
    transport: Arc<dyn LiveTransport>,
    events: broadcast::Sender<LiveSessionEvent<P::Event>>,
    ended: watch::Sender<Option<LiveSessionEnd>>,
    receive_task: JoinHandle<()>,
}

impl<P: LiveProtocol> LiveSession<P> {
    pub fn connect(protocol: Arc<P>, transport: Arc<dyn LiveTransport>) -> Arc<Self> {
        let (events, _) = broadcast::channel(256);
        let (ended, _) = watch::channel(None);
        Arc::new_cyclic(|weak: &std::sync::Weak<Self>| {
            let receive_task = tokio::spawn(receive_loop(
                weak.clone(),
                protocol.clone(),
                transport.clone(),
                events.clone(),
                ended.clone(),
            ));
            Self {
                protocol,
                transport,
                events,
                ended,
                receive_task,
            }
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LiveSessionEvent<P::Event>> {
        self.events.subscribe()
    }

    pub fn end_reason(&self) -> Option<LiveSessionEnd> {
        *self.ended.borrow()
    }

    pub async fn send(&self, command: P::Command) -> Result<()> {
        if let Some(reason) = self.end_reason() {
            bail!("live session ended ({reason:?})");
        }
        self.transport.send(self.protocol.encode(command)?).await
    }

    /// Requests graceful shutdown, waits briefly for the provider to
    /// acknowledge, then closes the transport. Safe to call more than once.
    pub async fn close(&self) -> Result<()> {
        if self.end_reason().is_some() {
            return self.transport.close().await;
        }

        let mut ended = self.ended.subscribe();
        if let Some(command) = self.protocol.close_command() {
            self.send(command).await?;
            let _ = timeout(CLOSE_ACKNOWLEDGEMENT_TIMEOUT, async {
                while ended.borrow_and_update().is_none() {
                    if ended.changed().await.is_err() {
                        break;
                    }
                }
            })
            .await;
        }

        let result = self.transport.close().await;
        finish(&self.ended, &self.events, LiveSessionEnd::Closed, None);
        result
    }
}

impl<P: LiveProtocol> Drop for LiveSession<P> {
    fn drop(&mut self) {
        self.receive_task.abort();
    }
}

fn finish<E: Clone>(
    ended: &watch::Sender<Option<LiveSessionEnd>>,
    events: &broadcast::Sender<LiveSessionEvent<E>>,
    reason: LiveSessionEnd,
    error: Option<anyhow::Error>,
) {
    if ended.borrow().is_some() {
        return;
    }
    ended.send_replace(Some(reason));
    let _ = events.send(LiveSessionEvent::ended(reason, error));
}

async fn receive_loop<P: LiveProtocol>(
    session: std::sync::Weak<LiveSession<P>>,
    protocol: Arc<P>,
    transport: Arc<dyn LiveTransport>,
    events: broadcast::Sender<LiveSessionEvent<P::Event>>,
    ended: watch::Sender<Option<LiveSessionEnd>>,
) {
    loop {
        if session.upgrade().is_none() {
            return;
        }
        let (reason, error) = match transport.receive().await {
            Ok(Some(message)) => match protocol.decode(message) {
                Ok(event) => {
                    let acknowledged = protocol.is_close_acknowledgement(&event);
                    let _ = events.send(LiveSessionEvent::Message(event));
                    if !acknowledged {
                        continue;
                    }
                    (LiveSessionEnd::Closed, None)
                }
                Err(error) => (LiveSessionEnd::DecodeFailed, Some(error)),
            },
            Ok(None) => (LiveSessionEnd::Closed, None),
            Err(error) => (LiveSessionEnd::TransportFailed, Some(error)),
        };
        finish(&ended, &events, reason, error);
        return;
    }
}
