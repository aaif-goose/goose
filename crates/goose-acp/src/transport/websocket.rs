use anyhow::Result;
use axum::{
    body::Body,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use futures::{SinkExt, StreamExt};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{debug, error, info, warn};

use super::{HEADER_SESSION_ID, TransportSession};
use crate::adapters::{ReceiverToAsyncRead, SenderToAsyncWrite};
use crate::server_factory::AcpServer;

const ACP_WS_TOKEN_ENV: &str = "GOOSE_ACP_WS_TOKEN";

pub(crate) struct WsState {
    server: Arc<AcpServer>,
    // Keyed by acp_session_id: a connection-scoped UUID serving many Goose sessions.
    sessions: RwLock<HashMap<String, TransportSession>>,
}

impl WsState {
    pub fn new(server: Arc<AcpServer>) -> Self {
        Self {
            server,
            sessions: RwLock::new(HashMap::new()),
        }
    }

    async fn create_connection(&self) -> Result<String> {
        let (to_agent_tx, to_agent_rx) = mpsc::channel::<String>(256);
        let (from_agent_tx, from_agent_rx) = mpsc::unbounded_channel::<String>();

        let agent = self.server.create_agent().await?;

        let acp_session_id = uuid::Uuid::new_v4().to_string();

        let read_stream = ReceiverToAsyncRead::new(to_agent_rx);
        let write_stream = SenderToAsyncWrite::new(from_agent_tx);
        let fut = crate::server::serve(agent, read_stream.compat(), write_stream.compat_write());
        let handle = tokio::spawn(async move {
            if let Err(e) = fut.await {
                error!("ACP WebSocket session error: {}", e);
            }
        });

        self.sessions.write().await.insert(
            acp_session_id.clone(),
            TransportSession {
                to_agent_tx,
                from_agent_rx: Arc::new(Mutex::new(from_agent_rx)),
                handle,
            },
        );

        info!(acp_session_id = %acp_session_id, "WebSocket connection created");
        Ok(acp_session_id)
    }

    async fn remove_connection(&self, acp_session_id: &str) {
        if let Some(session) = self.sessions.write().await.remove(acp_session_id) {
            session.handle.abort();
            info!(acp_session_id = %acp_session_id, "WebSocket connection removed");
        }
    }
}

fn request_token(request: &Request<Body>) -> Option<String> {
    request.uri().query().and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.into_owned())
    })
}

fn bridge_token_authorized_with_expected(
    request: &Request<Body>,
    expected_token: Option<&str>,
) -> bool {
    match expected_token {
        Some(expected) => request_token(request).as_deref() == Some(expected),
        None => true,
    }
}

fn configured_bridge_token() -> Option<String> {
    std::env::var(ACP_WS_TOKEN_ENV)
        .ok()
        .filter(|token| !token.is_empty())
}

pub(crate) async fn handle_get(
    state: Arc<WsState>,
    ws: WebSocketUpgrade,
    request: Request<Body>,
) -> Response {
    if !bridge_token_authorized_with_expected(&request, configured_bridge_token().as_deref()) {
        warn!("Rejected unauthorized ACP WebSocket connection");
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let acp_session_id = match state.create_connection().await {
        Ok(id) => id,
        Err(e) => {
            error!("Failed to create WebSocket connection: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create WebSocket connection",
            )
                .into_response();
        }
    };

    let mut response = ws.on_upgrade({
        let acp_session_id = acp_session_id.clone();
        move |socket| handle_ws(socket, state, acp_session_id)
    });
    response
        .headers_mut()
        .insert(HEADER_SESSION_ID, acp_session_id.parse().unwrap());
    response
}

pub(crate) async fn handle_ws(socket: WebSocket, state: Arc<WsState>, acp_session_id: String) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    let (to_agent, from_agent) = {
        let sessions = state.sessions.read().await;
        match sessions.get(&acp_session_id) {
            Some(session) => (session.to_agent_tx.clone(), session.from_agent_rx.clone()),
            None => {
                error!(acp_session_id = %acp_session_id, "Session not found after creation");
                return;
            }
        }
    };

    debug!(acp_session_id = %acp_session_id, "Starting bidirectional message loop");

    let mut from_agent_rx = from_agent.lock().await;

    loop {
        tokio::select! {
            Some(msg_result) = ws_rx.next() => {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        let text_str = text.to_string();
                        debug!(acp_session_id = %acp_session_id, "Client → Agent: {} bytes", text_str.len());
                        if let Err(e) = to_agent.send(text_str).await {
                            error!(acp_session_id = %acp_session_id, "Failed to send to agent: {}", e);
                            break;
                        }
                    }
                    Ok(Message::Close(frame)) => {
                        debug!(acp_session_id = %acp_session_id, "Client closed connection: {:?}", frame);
                        break;
                    }
                    Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {
                        // Axum handles ping/pong automatically
                        continue;
                    }
                    Ok(Message::Binary(_)) => {
                        warn!(acp_session_id = %acp_session_id, "Ignoring binary message (ACP uses text)");
                        continue;
                    }
                    Err(e) => {
                        error!(acp_session_id = %acp_session_id, "WebSocket error: {}", e);
                        break;
                    }
                }
            }

            Some(text) = from_agent_rx.recv() => {
                debug!(acp_session_id = %acp_session_id, "Agent → Client: {} bytes", text.len());
                if let Err(e) = ws_tx.send(Message::Text(text.into())).await {
                    error!(acp_session_id = %acp_session_id, "Failed to send to client: {}", e);
                    break;
                }
            }

            else => {
                debug!(acp_session_id = %acp_session_id, "Both channels closed");
                break;
            }
        }
    }

    debug!(acp_session_id = %acp_session_id, "Cleaning up connection");
    state.remove_connection(&acp_session_id).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(uri: &str) -> Request<Body> {
        Request::builder()
            .uri(uri)
            .body(Body::empty())
            .expect("request")
    }

    #[test]
    fn request_token_reads_query_param() {
        let request = request("/acp?foo=bar&token=test-token");
        assert_eq!(request_token(&request).as_deref(), Some("test-token"));
    }

    #[test]
    fn request_token_returns_none_when_missing() {
        let request = request("/acp?foo=bar");
        assert_eq!(request_token(&request), None);
    }

    #[test]
    fn bridge_token_auth_is_disabled_without_expected_token() {
        let request = request("/acp");
        assert!(bridge_token_authorized_with_expected(&request, None));
    }

    #[test]
    fn bridge_token_auth_requires_exact_match() {
        let missing = request("/acp");
        let wrong = request("/acp?token=wrong");
        let correct = request("/acp?token=expected-token");

        assert!(!bridge_token_authorized_with_expected(
            &missing,
            Some("expected-token")
        ));
        assert!(!bridge_token_authorized_with_expected(
            &wrong,
            Some("expected-token")
        ));
        assert!(bridge_token_authorized_with_expected(
            &correct,
            Some("expected-token")
        ));
    }
}
