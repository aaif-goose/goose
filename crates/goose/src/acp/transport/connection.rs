//! Connection-level state shared between HTTP and WebSocket transports.
//!
//! Each connection hosts one ACP agent task. All server→client messages for
//! the connection are multicast through a single broadcast channel; HTTP GET
//! SSE streams and WebSocket sinks subscribe to that channel. POSTs (and WS
//! text frames) forward client→server messages into the agent over an mpsc.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use anyhow::Result;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{error, info, warn};

use crate::acp::adapters::{ReceiverToAsyncRead, SenderToAsyncWrite};
use crate::acp::server_factory::AcpServer;

/// Broadcast capacity for agent→client messages. Large enough to buffer a
/// typical prompt's streaming notifications even if the subscriber is briefly
/// slow (e.g. during reconnect).
const OUTBOUND_BROADCAST_CAPACITY: usize = 1024;

/// Maximum number of server→client messages to retain while no subscriber is
/// attached. In the HTTP flow the client opens `GET /acp` only after receiving
/// the initialize response, so any notifications or server-initiated requests
/// emitted by the agent in that window would otherwise be broadcast to zero
/// subscribers and permanently lost. We buffer them here and replay on the
/// first subscribe. On overflow the oldest message is dropped with a warning.
const PRE_SUBSCRIBE_BUFFER_CAPACITY: usize = 1024;

pub(crate) struct Connection {
    /// Send client→server messages into the agent.
    pub to_agent_tx: mpsc::Sender<String>,
    /// Subscribe here to receive all server→client messages for this connection.
    pub outbound_tx: broadcast::Sender<String>,
    /// Pulled exactly once during `initialize` to read the synchronous response
    /// that must be returned as the HTTP 200 body before any broadcast
    /// subscribers exist. `None` once consumed.
    pub init_receiver: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
    /// Set once the initialize handler has captured the initialize response and
    /// handed ownership of the agent output pump over to the broadcast fan-out.
    pub init_complete: Mutex<bool>,
    /// Handle to the agent task; aborted on connection termination.
    pub agent_handle: tokio::task::JoinHandle<()>,
    /// Handle to the fan-out pump task; aborted on connection termination.
    pub pump_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pre_subscribe_buffer: Arc<Mutex<Option<VecDeque<String>>>>,
}

pub(crate) struct ConnectionRegistry {
    pub server: Arc<AcpServer>,
    connections: RwLock<HashMap<String, Arc<Connection>>>,
}

impl ConnectionRegistry {
    pub fn new(server: Arc<AcpServer>) -> Self {
        Self {
            server,
            connections: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new connection, spawn the ACP agent task, and return
    /// (connection_id, connection). The initialize request body should be sent
    /// via `connection.to_agent_tx` and the synchronous initialize response
    /// read via `consume_initialize_response`.
    pub async fn create_connection(&self) -> Result<(String, Arc<Connection>)> {
        let (to_agent_tx, to_agent_rx) = mpsc::channel::<String>(256);
        let (from_agent_tx, from_agent_rx) = mpsc::unbounded_channel::<String>();
        let (outbound_tx, _) = broadcast::channel::<String>(OUTBOUND_BROADCAST_CAPACITY);

        let agent = self.server.create_agent().await?;
        let connection_id = uuid::Uuid::new_v4().to_string();

        let read_stream = ReceiverToAsyncRead::new(to_agent_rx);
        let write_stream = SenderToAsyncWrite::new(from_agent_tx);
        let fut =
            crate::acp::server::serve(agent, read_stream.compat(), write_stream.compat_write());

        let conn_id_for_task = connection_id.clone();
        let agent_handle = tokio::spawn(async move {
            if let Err(e) = fut.await {
                error!(connection_id = %conn_id_for_task, "ACP agent task error: {}", e);
            }
        });

        let connection = Arc::new(Connection {
            to_agent_tx,
            outbound_tx,
            init_receiver: Mutex::new(Some(from_agent_rx)),
            init_complete: Mutex::new(false),
            agent_handle,
            pump_handle: Mutex::new(None),
            pre_subscribe_buffer: Arc::new(Mutex::new(Some(VecDeque::new()))),
        });

        self.connections
            .write()
            .await
            .insert(connection_id.clone(), connection.clone());

        info!(connection_id = %connection_id, "Connection created");
        Ok((connection_id, connection))
    }

    pub async fn get(&self, connection_id: &str) -> Option<Arc<Connection>> {
        self.connections.read().await.get(connection_id).cloned()
    }

    pub async fn remove(&self, connection_id: &str) -> Option<Arc<Connection>> {
        self.connections.write().await.remove(connection_id)
    }
}

impl Connection {
    /// After the synchronous initialize response has been consumed, spawn a
    /// task that forwards all remaining agent output to the broadcast channel.
    /// Idempotent.
    pub async fn start_fanout(self: &Arc<Self>) {
        let mut complete = self.init_complete.lock().await;
        if *complete {
            return;
        }
        let Some(mut rx) = self.init_receiver.lock().await.take() else {
            return;
        };
        let outbound_tx = self.outbound_tx.clone();
        let buffer = self.pre_subscribe_buffer.clone();
        let handle = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                let mut buf_guard = buffer.lock().await;
                match buf_guard.as_mut() {
                    Some(buf) => {
                        if buf.len() >= PRE_SUBSCRIBE_BUFFER_CAPACITY {
                            warn!(
                                "Pre-subscribe buffer full ({} messages); dropping oldest",
                                PRE_SUBSCRIBE_BUFFER_CAPACITY
                            );
                            buf.pop_front();
                        }
                        buf.push_back(msg);
                    }
                    None => {
                        drop(buf_guard);
                        let _ = outbound_tx.send(msg);
                    }
                }
            }
        });
        *self.pump_handle.lock().await = Some(handle);
        *complete = true;
    }

    pub async fn subscribe_with_replay(&self) -> (Vec<String>, broadcast::Receiver<String>) {
        let mut guard = self.pre_subscribe_buffer.lock().await;
        let receiver = self.outbound_tx.subscribe();
        let replay = guard.take().map(Vec::from).unwrap_or_default();
        (replay, receiver)
    }

    /// Terminate the connection: abort the agent task and the fan-out pump.
    pub async fn shutdown(&self) {
        self.agent_handle.abort();
        if let Some(h) = self.pump_handle.lock().await.take() {
            h.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    fn fake_connection() -> (Arc<Connection>, mpsc::UnboundedSender<String>) {
        let (to_agent_tx, _to_agent_rx) = mpsc::channel::<String>(256);
        let (from_agent_tx, from_agent_rx) = mpsc::unbounded_channel::<String>();
        let (outbound_tx, _) = broadcast::channel::<String>(OUTBOUND_BROADCAST_CAPACITY);

        let agent_handle = tokio::spawn(async {
            std::future::pending::<()>().await;
        });

        let connection = Arc::new(Connection {
            to_agent_tx,
            outbound_tx,
            init_receiver: Mutex::new(Some(from_agent_rx)),
            init_complete: Mutex::new(false),
            agent_handle,
            pump_handle: Mutex::new(None),
            pre_subscribe_buffer: Arc::new(Mutex::new(Some(VecDeque::new()))),
        });

        (connection, from_agent_tx)
    }

    #[tokio::test]
    async fn buffers_messages_emitted_before_first_subscribe() {
        let (conn, agent_tx) = fake_connection();
        conn.start_fanout().await;

        agent_tx.send("one".to_string()).unwrap();
        agent_tx.send("two".to_string()).unwrap();
        agent_tx.send("three".to_string()).unwrap();

        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(20)).await;

        let (replay, _rx) = conn.subscribe_with_replay().await;
        assert_eq!(replay, vec!["one", "two", "three"]);

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn switches_to_live_broadcast_after_subscribe() {
        let (conn, agent_tx) = fake_connection();
        conn.start_fanout().await;

        let (replay, mut rx) = conn.subscribe_with_replay().await;
        assert!(replay.is_empty());

        agent_tx.send("live-one".to_string()).unwrap();
        agent_tx.send("live-two".to_string()).unwrap();

        let got1 = timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let got2 = timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got1, "live-one");
        assert_eq!(got2, "live-two");

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn pre_subscribe_buffer_is_bounded() {
        let (conn, agent_tx) = fake_connection();
        conn.start_fanout().await;

        for i in 0..(PRE_SUBSCRIBE_BUFFER_CAPACITY + 50) {
            agent_tx.send(format!("m{}", i)).unwrap();
        }

        tokio::time::sleep(Duration::from_millis(50)).await;

        let (replay, _rx) = conn.subscribe_with_replay().await;
        assert_eq!(replay.len(), PRE_SUBSCRIBE_BUFFER_CAPACITY);
        assert_eq!(
            replay.last().unwrap(),
            &format!("m{}", PRE_SUBSCRIBE_BUFFER_CAPACITY + 49)
        );
        assert_eq!(replay.first().unwrap(), &format!("m{}", 50));

        conn.shutdown().await;
    }
}
