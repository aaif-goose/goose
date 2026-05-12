//! Per-connection state. Server→client messages fan out to a connection-scoped
//! stream, a per-session stream for each active `sessionId`, and an
//! all-outbound stream consumed by WebSocket.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use anyhow::Result;
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{error, info, trace, warn};

use crate::acp::adapters::{ReceiverToAsyncRead, SenderToAsyncWrite};
use crate::acp::server_factory::AcpServer;

const OUTBOUND_BROADCAST_CAPACITY: usize = 1024;

/// Buffers messages emitted before a subscriber attaches (e.g. session
/// notifications that land before the client opens the session GET stream).
const PRE_SUBSCRIBE_BUFFER_CAPACITY: usize = 1024;

#[derive(Clone, Debug)]
pub(crate) enum ResponseRoute {
    Connection,
    Session(String),
}

struct OutboundStream {
    tx: broadcast::Sender<String>,
    pre_subscribe_buffer: Mutex<Option<VecDeque<String>>>,
}

impl OutboundStream {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(OUTBOUND_BROADCAST_CAPACITY);
        Self {
            tx,
            pre_subscribe_buffer: Mutex::new(Some(VecDeque::new())),
        }
    }

    async fn push(&self, msg: String) {
        let mut guard = self.pre_subscribe_buffer.lock().await;
        match guard.as_mut() {
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
                drop(guard);
                let _ = self.tx.send(msg);
            }
        }
    }

    async fn subscribe_with_replay(&self) -> (Vec<String>, broadcast::Receiver<String>) {
        let mut guard = self.pre_subscribe_buffer.lock().await;
        let receiver = self.tx.subscribe();
        let replay = guard.take().map(Vec::from).unwrap_or_default();
        (replay, receiver)
    }
}

pub(crate) struct Connection {
    pub to_agent_tx: mpsc::Sender<String>,
    /// Consumed once by `handle_initialize` to read the synchronous initialize
    /// response before the router task takes over.
    pub init_receiver: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
    pub init_complete: Mutex<bool>,
    pub agent_handle: tokio::task::JoinHandle<()>,
    pub router_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,

    connection_stream: Arc<OutboundStream>,
    session_streams: Arc<RwLock<HashMap<String, Arc<OutboundStream>>>>,
    all_outbound: Arc<OutboundStream>,
    pending_routes: Arc<Mutex<HashMap<Value, ResponseRoute>>>,
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

    pub async fn create_connection(&self) -> Result<(String, Arc<Connection>)> {
        let (to_agent_tx, to_agent_rx) = mpsc::channel::<String>(256);
        let (from_agent_tx, from_agent_rx) = mpsc::unbounded_channel::<String>();

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
            init_receiver: Mutex::new(Some(from_agent_rx)),
            init_complete: Mutex::new(false),
            agent_handle,
            router_handle: Mutex::new(None),
            connection_stream: Arc::new(OutboundStream::new()),
            session_streams: Arc::new(RwLock::new(HashMap::new())),
            all_outbound: Arc::new(OutboundStream::new()),
            pending_routes: Arc::new(Mutex::new(HashMap::new())),
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
    pub async fn start_router(self: &Arc<Self>) {
        let mut complete = self.init_complete.lock().await;
        if *complete {
            return;
        }
        let Some(mut rx) = self.init_receiver.lock().await.take() else {
            return;
        };

        let me = self.clone();
        let handle = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                me.route_outbound(msg).await;
            }
        });
        *self.router_handle.lock().await = Some(handle);
        *complete = true;
    }

    async fn route_outbound(self: &Arc<Self>, msg: String) {
        self.all_outbound.push(msg.clone()).await;

        let parsed: Option<Value> = serde_json::from_str(&msg).ok();
        let target = match parsed.as_ref() {
            Some(v) => self.classify(v).await,
            None => Target::Connection,
        };

        match target {
            Target::Connection => {
                trace!(target = "connection", "→ connection-scoped stream");
                self.connection_stream.push(msg).await;
            }
            Target::Session(sid) => {
                trace!(target = %sid, "→ session-scoped stream");
                let stream = self.get_or_create_session_stream(&sid).await;
                stream.push(msg).await;
            }
        }
    }

    async fn classify(self: &Arc<Self>, v: &Value) -> Target {
        let has_method = v.get("method").is_some();
        let has_id = v.get("id").is_some();
        let has_result_or_error = v.get("result").is_some() || v.get("error").is_some();

        if has_method {
            if let Some(sid) = extract_session_id_from_params(v) {
                return Target::Session(sid);
            }
            return Target::Connection;
        }

        if has_id && has_result_or_error {
            let id = v.get("id").cloned().unwrap_or(Value::Null);
            let route = self.pending_routes.lock().await.remove(&id);
            return match route {
                Some(ResponseRoute::Session(sid)) => Target::Session(sid),
                Some(ResponseRoute::Connection) | None => Target::Connection,
            };
        }

        Target::Connection
    }

    pub async fn record_pending_route(&self, id: Value, route: ResponseRoute) {
        if id.is_null() {
            return;
        }
        self.pending_routes.lock().await.insert(id, route);
    }

    pub async fn subscribe_connection_stream(&self) -> (Vec<String>, broadcast::Receiver<String>) {
        self.connection_stream.subscribe_with_replay().await
    }

    pub async fn subscribe_session_stream(
        &self,
        session_id: &str,
    ) -> Option<(Vec<String>, broadcast::Receiver<String>)> {
        let stream = self.session_streams.read().await.get(session_id).cloned()?;
        Some(stream.subscribe_with_replay().await)
    }

    pub async fn ensure_session(&self, session_id: &str) {
        self.get_or_create_session_stream(session_id).await;
    }

    async fn get_or_create_session_stream(&self, session_id: &str) -> Arc<OutboundStream> {
        if let Some(s) = self.session_streams.read().await.get(session_id) {
            return s.clone();
        }
        let mut w = self.session_streams.write().await;
        w.entry(session_id.to_string())
            .or_insert_with(|| Arc::new(OutboundStream::new()))
            .clone()
    }

    pub async fn subscribe_all_outbound(&self) -> (Vec<String>, broadcast::Receiver<String>) {
        self.all_outbound.subscribe_with_replay().await
    }

    pub async fn shutdown(&self) {
        self.agent_handle.abort();
        if let Some(h) = self.router_handle.lock().await.take() {
            h.abort();
        }
    }
}

#[derive(Debug)]
enum Target {
    Connection,
    Session(String),
}

fn extract_session_id_from_params(v: &Value) -> Option<String> {
    v.get("params")
        .and_then(|p| p.get("sessionId"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    fn fake_connection() -> (Arc<Connection>, mpsc::UnboundedSender<String>) {
        let (to_agent_tx, _to_agent_rx) = mpsc::channel::<String>(256);
        let (from_agent_tx, from_agent_rx) = mpsc::unbounded_channel::<String>();

        let agent_handle = tokio::spawn(async {
            std::future::pending::<()>().await;
        });

        let connection = Arc::new(Connection {
            to_agent_tx,
            init_receiver: Mutex::new(Some(from_agent_rx)),
            init_complete: Mutex::new(false),
            agent_handle,
            router_handle: Mutex::new(None),
            connection_stream: Arc::new(OutboundStream::new()),
            session_streams: Arc::new(RwLock::new(HashMap::new())),
            all_outbound: Arc::new(OutboundStream::new()),
            pending_routes: Arc::new(Mutex::new(HashMap::new())),
        });

        (connection, from_agent_tx)
    }

    #[tokio::test]
    async fn buffers_connection_scoped_messages_before_first_subscribe() {
        let (conn, agent_tx) = fake_connection();
        conn.start_router().await;

        agent_tx
            .send(r#"{"id":1,"result":{"capabilities":{}}}"#.to_string())
            .unwrap();

        tokio::time::sleep(Duration::from_millis(20)).await;

        let (replay, _rx) = conn.subscribe_connection_stream().await;
        assert_eq!(replay.len(), 1);
        assert!(replay[0].contains("\"capabilities\""));

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn routes_session_scoped_notification_to_session_stream() {
        let (conn, agent_tx) = fake_connection();
        conn.start_router().await;

        conn.ensure_session("sess_abc").await;

        let (_, mut rx) = conn.subscribe_session_stream("sess_abc").await.unwrap();
        agent_tx
            .send(
                r#"{"method":"session/update","params":{"sessionId":"sess_abc","update":{}}}"#
                    .to_string(),
            )
            .unwrap();

        let got = timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(got.contains("session/update"));

        let (replay, _) = conn.subscribe_connection_stream().await;
        assert!(
            replay.is_empty(),
            "connection stream should not have session-scoped messages"
        );

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn routes_response_using_pending_route_table() {
        let (conn, agent_tx) = fake_connection();
        conn.start_router().await;

        conn.ensure_session("sess_xyz").await;
        conn.record_pending_route(
            Value::from(42),
            ResponseRoute::Session("sess_xyz".to_string()),
        )
        .await;

        let (_, mut rx) = conn.subscribe_session_stream("sess_xyz").await.unwrap();
        agent_tx
            .send(r#"{"id":42,"result":{"stopReason":"end_turn"}}"#.to_string())
            .unwrap();

        let got = timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(got.contains("\"stopReason\""));

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn websocket_all_outbound_sees_everything() {
        let (conn, agent_tx) = fake_connection();
        conn.start_router().await;

        agent_tx
            .send(r#"{"id":1,"result":{}}"#.to_string())
            .unwrap();
        agent_tx
            .send(r#"{"method":"session/update","params":{"sessionId":"s1"}}"#.to_string())
            .unwrap();

        tokio::time::sleep(Duration::from_millis(20)).await;

        let (replay, _all_rx) = conn.subscribe_all_outbound().await;
        assert_eq!(replay.len(), 2);
        assert!(replay[0].contains("\"id\":1"));
        assert!(replay[1].contains("session/update"));

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn unknown_session_subscribe_returns_none() {
        let (conn, _agent_tx) = fake_connection();
        conn.start_router().await;

        assert!(conn.subscribe_session_stream("nope").await.is_none());

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn pre_subscribe_buffer_is_bounded() {
        let (conn, agent_tx) = fake_connection();
        conn.start_router().await;

        for i in 0..(PRE_SUBSCRIBE_BUFFER_CAPACITY + 50) {
            agent_tx
                .send(format!(r#"{{"id":{},"result":{{}}}}"#, i))
                .unwrap();
        }

        tokio::time::sleep(Duration::from_millis(50)).await;

        let (replay, _rx) = conn.subscribe_connection_stream().await;
        assert_eq!(replay.len(), PRE_SUBSCRIBE_BUFFER_CAPACITY);
    }
}
