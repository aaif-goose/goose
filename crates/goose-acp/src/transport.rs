pub mod http;
pub mod websocket;

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{
        ws::{rejection::WebSocketUpgradeRejection, WebSocketUpgrade},
        State,
    },
    http::{header, Method, Request},
    response::Response,
    routing::{delete, get, post},
    Router,
};
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};
use tower_http::cors::{Any, CorsLayer};

use crate::server_factory::AcpServer;

pub(crate) const HEADER_SESSION_ID: &str = "Acp-Session-Id";
pub(crate) const EVENT_STREAM_MIME_TYPE: &str = "text/event-stream";
pub(crate) const JSON_MIME_TYPE: &str = "application/json";

pub(crate) struct TransportSession {
    pub to_agent_tx: mpsc::Sender<String>,
    pub from_agent_rx: Arc<Mutex<mpsc::UnboundedReceiver<String>>>,
    pub handle: tokio::task::JoinHandle<()>,
}

pub(crate) fn accepts_mime_type(request: &Request<Body>, mime_type: &str) -> bool {
    request
        .headers()
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|accept| accept.contains(mime_type))
}

pub(crate) fn accepts_json_and_sse(request: &Request<Body>) -> bool {
    request
        .headers()
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|accept| {
            accept.contains(JSON_MIME_TYPE) && accept.contains(EVENT_STREAM_MIME_TYPE)
        })
}

pub(crate) fn content_type_is_json(request: &Request<Body>) -> bool {
    request
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with(JSON_MIME_TYPE))
}

pub(crate) fn get_session_id(request: &Request<Body>) -> Option<String> {
    request
        .headers()
        .get(HEADER_SESSION_ID)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

pub(crate) fn is_jsonrpc_request(value: &Value) -> bool {
    value.get("method").is_some() && value.get("id").is_some()
}

pub(crate) fn is_jsonrpc_notification(value: &Value) -> bool {
    value.get("method").is_some() && value.get("id").is_none()
}

pub(crate) fn is_jsonrpc_response(value: &Value) -> bool {
    value.get("id").is_some() && (value.get("result").is_some() || value.get("error").is_some())
}

pub(crate) fn is_initialize_request(value: &Value) -> bool {
    value.get("method").is_some_and(|m| m == "initialize") && value.get("id").is_some()
}

async fn handle_get(
    ws_upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    State(state): State<(Arc<http::HttpState>, Arc<websocket::WsState>)>,
    request: Request<Body>,
) -> Response {
    match ws_upgrade {
        Ok(ws) => websocket::handle_get(state.1, ws).await,
        Err(_) => http::handle_get(state.0, request).await,
    }
}

async fn health() -> &'static str {
    "ok"
}

pub fn create_router(server: Arc<AcpServer>) -> Router {
    let http_state = Arc::new(http::HttpState::new(server.clone()));
    let ws_state = Arc::new(websocket::WsState::new(server));

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::ACCEPT,
            HEADER_SESSION_ID.parse().unwrap(),
            header::SEC_WEBSOCKET_VERSION,
            header::SEC_WEBSOCKET_KEY,
            header::CONNECTION,
            header::UPGRADE,
        ]);

    Router::new()
        .route("/health", get(health))
        .route("/status", get(health))
        .route(
            "/acp",
            post(http::handle_post).with_state(http_state.clone()),
        )
        .route(
            "/acp",
            get(handle_get).with_state((http_state.clone(), ws_state)),
        )
        .route("/acp", delete(http::handle_delete).with_state(http_state))
        .layer(cors)
}
