use anyhow::Result;
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Response, Sse},
};
use http_body_util::BodyExt;
use serde_json::Value;
use std::{collections::HashMap, convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{error, info};

use super::*;
use crate::acp::adapters::{ReceiverToAsyncRead, SenderToAsyncWrite};
use crate::acp::server_factory::AcpServer;

pub(crate) struct HttpState {
    server: Arc<AcpServer>,
    // Keyed by acp_session_id: a connection-scoped UUID serving many Goose sessions.
    sessions: RwLock<HashMap<String, TransportSession>>,
}

impl HttpState {
    pub fn new(server: Arc<AcpServer>) -> Self {
        Self {
            server,
            sessions: RwLock::new(HashMap::new()),
        }
    }

    async fn create_session(&self) -> Result<String, StatusCode> {
        let (to_agent_tx, to_agent_rx) = mpsc::channel::<String>(256);
        let (from_agent_tx, from_agent_rx) = mpsc::unbounded_channel::<String>();

        let agent = self.server.create_agent().await.map_err(|e| {
            error!("Failed to create agent: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let acp_session_id = uuid::Uuid::new_v4().to_string();

        let read_stream = ReceiverToAsyncRead::new(to_agent_rx);
        let write_stream = SenderToAsyncWrite::new(from_agent_tx);
        let fut =
            crate::acp::server::serve(agent, read_stream.compat(), write_stream.compat_write());
        let handle = tokio::spawn(async move {
            if let Err(e) = fut.await {
                error!("ACP session error: {}", e);
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

        info!(acp_session_id = %acp_session_id, "Session created");
        Ok(acp_session_id)
    }

    async fn has_session(&self, acp_session_id: &str) -> bool {
        self.sessions.read().await.contains_key(acp_session_id)
    }

    async fn remove_session(&self, acp_session_id: &str) {
        if let Some(session) = self.sessions.write().await.remove(acp_session_id) {
            session.handle.abort();
            info!(acp_session_id = %acp_session_id, "Session removed");
        }
    }

    async fn send_message(&self, acp_session_id: &str, message: String) -> Result<(), StatusCode> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(acp_session_id).ok_or(StatusCode::NOT_FOUND)?;
        session
            .to_agent_tx
            .send(message)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }

    async fn get_receiver(
        &self,
        acp_session_id: &str,
    ) -> Result<Arc<Mutex<mpsc::UnboundedReceiver<String>>>, StatusCode> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(acp_session_id).ok_or(StatusCode::NOT_FOUND)?;
        Ok(session.from_agent_rx.clone())
    }
}

fn create_sse_stream(
    receiver: Arc<Mutex<mpsc::UnboundedReceiver<String>>>,
    cleanup: Option<(Arc<HttpState>, String)>,
) -> Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let stream = async_stream::stream! {
        let mut rx = receiver.lock().await;
        while let Some(msg) = rx.recv().await {
            yield Ok::<_, Infallible>(axum::response::sse::Event::default().data(msg));
        }
        if let Some((state, acp_session_id)) = cleanup {
            state.remove_session(&acp_session_id).await;
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text(""),
    )
}

async fn handle_initialize(state: Arc<HttpState>, json_message: &Value) -> Response {
    let acp_session_id = match state.create_session().await {
        Ok(id) => id,
        Err(status) => return status.into_response(),
    };

    let message_str = serde_json::to_string(json_message).unwrap();
    if let Err(status) = state.send_message(&acp_session_id, message_str).await {
        state.remove_session(&acp_session_id).await;
        return status.into_response();
    }

    let receiver = match state.get_receiver(&acp_session_id).await {
        Ok(r) => r,
        Err(status) => {
            state.remove_session(&acp_session_id).await;
            return status.into_response();
        }
    };

    let sse = create_sse_stream(receiver, Some((state.clone(), acp_session_id.clone())));
    let mut response = sse.into_response();
    response
        .headers_mut()
        .insert(HEADER_SESSION_ID, acp_session_id.parse().unwrap());
    response
}

async fn handle_request(
    state: Arc<HttpState>,
    acp_session_id: String,
    json_message: &Value,
) -> Response {
    if !state.has_session(&acp_session_id).await {
        return (StatusCode::NOT_FOUND, "Session not found").into_response();
    }

    let message_str = serde_json::to_string(json_message).unwrap();
    if let Err(status) = state.send_message(&acp_session_id, message_str).await {
        return status.into_response();
    }

    let receiver = match state.get_receiver(&acp_session_id).await {
        Ok(r) => r,
        Err(status) => return status.into_response(),
    };

    create_sse_stream(receiver, None).into_response()
}

async fn handle_notification_or_response(
    state: Arc<HttpState>,
    acp_session_id: String,
    json_message: &Value,
) -> Response {
    if !state.has_session(&acp_session_id).await {
        return (StatusCode::NOT_FOUND, "Session not found").into_response();
    }

    let message_str = serde_json::to_string(json_message).unwrap();
    if let Err(status) = state.send_message(&acp_session_id, message_str).await {
        return status.into_response();
    }

    StatusCode::ACCEPTED.into_response()
}

pub(crate) async fn handle_post(
    State(state): State<Arc<HttpState>>,
    request: Request<Body>,
) -> Response {
    if !accepts_json_and_sse(&request) {
        return (
            StatusCode::NOT_ACCEPTABLE,
            "Not Acceptable: Client must accept both application/json and text/event-stream",
        )
            .into_response();
    }

    if !content_type_is_json(&request) {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Unsupported Media Type: Content-Type must be application/json",
        )
            .into_response();
    }

    let acp_session_id = get_session_id(&request);

    let body_bytes = match request.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => {
            error!("Failed to read request body: {}", e);
            return (StatusCode::BAD_REQUEST, "Failed to read request body").into_response();
        }
    };

    let json_message: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse JSON: {}", e);
            return (StatusCode::BAD_REQUEST, format!("Invalid JSON: {}", e)).into_response();
        }
    };

    if json_message.is_array() {
        return (
            StatusCode::NOT_IMPLEMENTED,
            "Batch requests are not supported",
        )
            .into_response();
    }

    if is_initialize_request(&json_message) {
        handle_initialize(state.clone(), &json_message).await
    } else if is_jsonrpc_request(&json_message) {
        let Some(id) = acp_session_id else {
            return (
                StatusCode::BAD_REQUEST,
                "Bad Request: Acp-Session-Id header required",
            )
                .into_response();
        };
        handle_request(state.clone(), id, &json_message).await
    } else if is_jsonrpc_notification(&json_message) || is_jsonrpc_response(&json_message) {
        let Some(id) = acp_session_id else {
            return (
                StatusCode::BAD_REQUEST,
                "Bad Request: Acp-Session-Id header required",
            )
                .into_response();
        };
        handle_notification_or_response(state.clone(), id, &json_message).await
    } else {
        (StatusCode::BAD_REQUEST, "Invalid JSON-RPC message").into_response()
    }
}

pub(crate) async fn handle_get(state: Arc<HttpState>, request: Request<Body>) -> Response {
    if !accepts_mime_type(&request, EVENT_STREAM_MIME_TYPE) {
        return (
            StatusCode::NOT_ACCEPTABLE,
            "Not Acceptable: Client must accept text/event-stream",
        )
            .into_response();
    }

    let acp_session_id = match get_session_id(&request) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                "Bad Request: Acp-Session-Id header required",
            )
                .into_response();
        }
    };

    if !state.has_session(&acp_session_id).await {
        return (StatusCode::NOT_FOUND, "Session not found").into_response();
    }

    let receiver = match state.get_receiver(&acp_session_id).await {
        Ok(r) => r,
        Err(status) => return status.into_response(),
    };

    let stream = async_stream::stream! {
        let mut rx = receiver.lock().await;
        while let Some(msg) = rx.recv().await {
            yield Ok::<_, Infallible>(axum::response::sse::Event::default().data(msg));
        }
    };

    Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text(""),
        )
        .into_response()
}

pub(crate) async fn handle_delete(
    State(state): State<Arc<HttpState>>,
    request: Request<Body>,
) -> Response {
    let acp_session_id = match get_session_id(&request) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                "Bad Request: Acp-Session-Id header required",
            )
                .into_response();
        }
    };

    if !state.has_session(&acp_session_id).await {
        return (StatusCode::NOT_FOUND, "Session not found").into_response();
    }

    state.remove_session(&acp_session_id).await;
    StatusCode::ACCEPTED.into_response()
}
