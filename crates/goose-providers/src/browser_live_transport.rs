//! Message bridge for browser-owned WebRTC data channels.
//!
//! Audio remains on WebRTC media tracks; this transport carries only protocol
//! control messages and events between Rust and the browser data channel.

use crate::live::LiveTransport;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{mpsc, watch, Mutex};

pub struct BrowserLiveTransport {
    outbound_tx: mpsc::Sender<Value>,
    outbound_rx: Mutex<Option<mpsc::Receiver<Value>>>,
    incoming_tx: mpsc::Sender<Value>,
    incoming_rx: Mutex<mpsc::Receiver<Value>>,
    closed_tx: watch::Sender<bool>,
}

impl BrowserLiveTransport {
    pub fn new() -> Self {
        let (outbound_tx, outbound_rx) = mpsc::channel(256);
        let (incoming_tx, incoming_rx) = mpsc::channel(256);
        let (closed_tx, _) = watch::channel(false);
        Self {
            outbound_tx,
            outbound_rx: Mutex::new(Some(outbound_rx)),
            incoming_tx,
            incoming_rx: Mutex::new(incoming_rx),
            closed_tx,
        }
    }

    pub async fn take_outbound(&self) -> Result<mpsc::Receiver<Value>> {
        self.outbound_rx
            .lock()
            .await
            .take()
            .ok_or_else(|| anyhow::anyhow!("browser live receiver was already taken"))
    }

    pub async fn push_incoming(&self, message: Value) -> Result<()> {
        let mut closed = self.closed_tx.subscribe();
        tokio::select! {
            result = self.incoming_tx.send(message) => result.map_err(Into::into),
            _ = closed.wait_for(|closed| *closed) => anyhow::bail!("browser live transport is closed"),
        }
    }
}

impl Default for BrowserLiveTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LiveTransport for BrowserLiveTransport {
    async fn send(&self, message: Value) -> Result<()> {
        self.outbound_tx.send(message).await.map_err(Into::into)
    }

    async fn receive(&self) -> Result<Option<Value>> {
        let mut closed = self.closed_tx.subscribe();
        let mut incoming = self.incoming_rx.lock().await;
        tokio::select! {
            message = incoming.recv() => Ok(message),
            _ = closed.wait_for(|closed| *closed) => Ok(None),
        }
    }

    async fn close(&self) -> Result<()> {
        self.closed_tx.send_replace(true);
        Ok(())
    }
}
