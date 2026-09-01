//! Runtime primitives for provider-specific live sessions.
//!
//! A protocol translates typed commands and events, while a transport only
//! delivers provider wire messages. Connected sessions own their receive task.

use anyhow::{bail, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::{
    sync::{broadcast, watch, Mutex},
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveSessionEnd {
    Closed,
    TransportFailed,
    DecodeFailed,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiveSessionState {
    Open,
    Closing,
    Ended(LiveSessionEnd),
}

pub struct LiveSession<P: LiveProtocol> {
    protocol: Arc<P>,
    transport: Arc<dyn LiveTransport>,
    events: broadcast::Sender<LiveSessionEvent<P::Event>>,
    state: watch::Sender<LiveSessionState>,
    transition_lock: Mutex<()>,
    receive_task: JoinHandle<()>,
}

impl<P: LiveProtocol> LiveSession<P> {
    pub fn connect(protocol: Arc<P>, transport: Arc<dyn LiveTransport>) -> Arc<Self> {
        let (events, _) = broadcast::channel(256);
        let (state, _) = watch::channel(LiveSessionState::Open);
        Arc::new_cyclic(|weak: &std::sync::Weak<Self>| {
            let receive_task = tokio::spawn(receive_loop(
                weak.clone(),
                protocol.clone(),
                transport.clone(),
                events.clone(),
                state.clone(),
            ));
            Self {
                protocol,
                transport,
                events,
                state,
                transition_lock: Mutex::new(()),
                receive_task,
            }
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LiveSessionEvent<P::Event>> {
        self.events.subscribe()
    }

    pub fn end_reason(&self) -> Option<LiveSessionEnd> {
        match *self.state.borrow() {
            LiveSessionState::Ended(reason) => Some(reason),
            LiveSessionState::Open | LiveSessionState::Closing => None,
        }
    }

    pub async fn send(&self, command: P::Command) -> Result<()> {
        match *self.state.borrow() {
            LiveSessionState::Open => {}
            LiveSessionState::Closing => bail!("live session is closing"),
            LiveSessionState::Ended(reason) => bail!("live session ended ({reason:?})"),
        }
        self.transport.send(self.protocol.encode(command)?).await
    }

    /// Requests graceful shutdown and closes the transport. Concurrent callers
    /// share one shutdown operation and wait for the same terminal state.
    pub async fn close(&self) -> Result<()> {
        let mut state = self.state.subscribe();
        let owns_shutdown = {
            let _guard = self.transition_lock.lock().await;
            match *self.state.borrow() {
                LiveSessionState::Open => {
                    self.state.send_replace(LiveSessionState::Closing);
                    true
                }
                LiveSessionState::Closing => false,
                LiveSessionState::Ended(_) => return Ok(()),
            }
        };

        if !owns_shutdown {
            wait_until_ended(&mut state).await;
            return Ok(());
        }

        let close_command = self.protocol.close_command();
        let waits_for_acknowledgement = close_command.is_some();
        let graceful_result = match close_command {
            Some(command) => match self.protocol.encode(command) {
                Ok(message) => self.transport.send(message).await,
                Err(error) => Err(error),
            },
            None => Ok(()),
        };

        if graceful_result.is_ok() && waits_for_acknowledgement {
            let _ = timeout(CLOSE_ACKNOWLEDGEMENT_TIMEOUT, wait_until_ended(&mut state)).await;
        }

        let transport_result = self.transport.close().await;
        finish(&self.state, &self.events, LiveSessionEnd::Closed, None);
        graceful_result.and(transport_result)
    }
}

impl<P: LiveProtocol> Drop for LiveSession<P> {
    fn drop(&mut self) {
        self.receive_task.abort();
    }
}

async fn wait_until_ended(state: &mut watch::Receiver<LiveSessionState>) -> LiveSessionEnd {
    loop {
        if let LiveSessionState::Ended(reason) = *state.borrow_and_update() {
            return reason;
        }
        if state.changed().await.is_err() {
            return LiveSessionEnd::Closed;
        }
    }
}

fn finish<E: Clone>(
    state: &watch::Sender<LiveSessionState>,
    events: &broadcast::Sender<LiveSessionEvent<E>>,
    reason: LiveSessionEnd,
    error: Option<anyhow::Error>,
) {
    if matches!(*state.borrow(), LiveSessionState::Ended(_)) {
        return;
    }
    state.send_replace(LiveSessionState::Ended(reason));
    let _ = events.send(LiveSessionEvent::ended(reason, error));
}

async fn receive_loop<P: LiveProtocol>(
    session: std::sync::Weak<LiveSession<P>>,
    protocol: Arc<P>,
    transport: Arc<dyn LiveTransport>,
    events: broadcast::Sender<LiveSessionEvent<P::Event>>,
    state: watch::Sender<LiveSessionState>,
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
        finish(&state, &events, reason, error);
        return;
    }
}
