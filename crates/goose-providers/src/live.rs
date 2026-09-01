//! Runtime primitives for provider-specific live sessions.
//!
//! A single actor owns transport I/O and lifecycle transitions so sends,
//! shutdown, and incoming events have deterministic ordering.

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::{
    sync::{broadcast, mpsc, oneshot, watch},
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
    fn close_command(&self) -> Option<Self::Command>;
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

enum ActorCommand<C> {
    Send {
        command: C,
        result: oneshot::Sender<Result<()>>,
    },
    Close {
        result: oneshot::Sender<Result<()>>,
    },
}

pub struct LiveSession<P: LiveProtocol> {
    commands: mpsc::Sender<ActorCommand<P::Command>>,
    events: broadcast::Sender<LiveSessionEvent<P::Event>>,
    ended: watch::Receiver<Option<LiveSessionEnd>>,
}

impl<P: LiveProtocol> Clone for LiveSession<P> {
    fn clone(&self) -> Self {
        Self {
            commands: self.commands.clone(),
            events: self.events.clone(),
            ended: self.ended.clone(),
        }
    }
}

impl<P: LiveProtocol> LiveSession<P> {
    pub fn connect(
        protocol: Arc<P>,
        transport: Arc<dyn LiveTransport>,
    ) -> (Self, broadcast::Receiver<LiveSessionEvent<P::Event>>) {
        let (commands, command_rx) = mpsc::channel(256);
        let (events, initial_events) = broadcast::channel(256);
        let (ended_tx, ended) = watch::channel(None);
        tokio::spawn(run_actor(
            protocol,
            transport,
            command_rx,
            events.clone(),
            ended_tx,
        ));
        (
            Self {
                commands,
                events,
                ended,
            },
            initial_events,
        )
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
        let (result_tx, result_rx) = oneshot::channel();
        self.commands
            .send(ActorCommand::Send {
                command,
                result: result_tx,
            })
            .await
            .map_err(|_| anyhow!("live session ended"))?;
        result_rx.await.map_err(|_| anyhow!("live session ended"))?
    }

    pub async fn close(&self) -> Result<()> {
        if self.end_reason().is_some() {
            return Ok(());
        }
        let (result_tx, result_rx) = oneshot::channel();
        self.commands
            .send(ActorCommand::Close { result: result_tx })
            .await
            .map_err(|_| anyhow!("live session ended"))?;
        result_rx.await.map_err(|_| anyhow!("live session ended"))?
    }
}

async fn run_actor<P: LiveProtocol>(
    protocol: Arc<P>,
    transport: Arc<dyn LiveTransport>,
    mut commands: mpsc::Receiver<ActorCommand<P::Command>>,
    events: broadcast::Sender<LiveSessionEvent<P::Event>>,
    ended: watch::Sender<Option<LiveSessionEnd>>,
) {
    let mut close_waiters = Vec::new();
    let mut closing = false;
    let close_timeout = tokio::time::sleep(CLOSE_ACKNOWLEDGEMENT_TIMEOUT);
    tokio::pin!(close_timeout);

    let (reason, error) = loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(ActorCommand::Send { command, result }) if !closing => {
                    let encoded = protocol.encode(command);
                    let send_result = match encoded {
                        Ok(message) => transport.send(message).await,
                        Err(error) => Err(error),
                    };
                    let failed = send_result.is_err();
                    let error = send_result.as_ref().err().map(|error| anyhow!(error.to_string()));
                    let _ = result.send(send_result);
                    if failed {
                        break (LiveSessionEnd::TransportFailed, error);
                    }
                }
                Some(ActorCommand::Send { result, .. }) => {
                    let _ = result.send(Err(anyhow!("live session is closing")));
                }
                Some(ActorCommand::Close { result }) => {
                    close_waiters.push(result);
                    if !closing {
                        closing = true;
                        close_timeout.as_mut().reset(tokio::time::Instant::now() + CLOSE_ACKNOWLEDGEMENT_TIMEOUT);
                        if let Some(command) = protocol.close_command() {
                            let close_result = match protocol.encode(command) {
                                Ok(message) => transport.send(message).await,
                                Err(error) => Err(error),
                            };
                            if let Err(error) = close_result {
                                break (LiveSessionEnd::TransportFailed, Some(error));
                            }
                        } else {
                            break (LiveSessionEnd::Closed, None);
                        }
                    }
                }
                None => break (LiveSessionEnd::Closed, None),
            },
            incoming = transport.receive() => match incoming {
                Ok(Some(message)) => match protocol.decode(message) {
                    Ok(event) => {
                        let acknowledged = protocol.is_close_acknowledgement(&event);
                        let _ = events.send(LiveSessionEvent::Message(event));
                        if acknowledged {
                            break (LiveSessionEnd::Closed, None);
                        }
                    }
                    Err(error) => break (LiveSessionEnd::DecodeFailed, Some(error)),
                },
                Ok(None) => break (LiveSessionEnd::Closed, None),
                Err(error) => break (LiveSessionEnd::TransportFailed, Some(error)),
            },
            _ = &mut close_timeout, if closing => break (LiveSessionEnd::Closed, None),
        }
    };

    let close_result = timeout(CLOSE_ACKNOWLEDGEMENT_TIMEOUT, transport.close()).await;
    let close_error = match close_result {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error),
        Err(_) => Some(anyhow!("live transport close timed out")),
    };
    let final_error = error.or(close_error);
    ended.send_replace(Some(reason));
    let _ = events.send(LiveSessionEvent::ended(
        reason,
        final_error.as_ref().map(|error| anyhow!(error.to_string())),
    ));
    for waiter in close_waiters {
        let result = match &final_error {
            Some(error) => Err(anyhow!(error.to_string())),
            None => Ok(()),
        };
        let _ = waiter.send(result);
    }
}
