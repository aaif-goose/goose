pub mod connection;
pub mod http;
pub mod websocket;

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{
        ws::{rejection::WebSocketUpgradeRejection, WebSocketUpgrade},
        State,
    },
    http::{header, HeaderName, Method, Request},
    response::Response,
    routing::{delete, get, post},
    Router,
};
use serde_json::Value;
use tower_http::cors::{Any, CorsLayer};

use crate::acp::server_factory::AcpServer;

pub(crate) const HEADER_CONNECTION_ID: &str = "Acp-Connection-Id";
pub(crate) const HEADER_SESSION_ID: &str = "Acp-Session-Id";
pub(crate) const EVENT_STREAM_MIME_TYPE: &str = "text/event-stream";
pub(crate) const JSON_MIME_TYPE: &str = "application/json";

pub(crate) fn accepts_mime_type(request: &Request<Body>, mime_type: &str) -> bool {
    request
        .headers()
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|accept| accept.contains(mime_type))
}

pub(crate) fn content_type_is_json(request: &Request<Body>) -> bool {
    request
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with(JSON_MIME_TYPE))
}

pub(crate) fn header_value(request: &Request<Body>, name: &str) -> Option<String> {
    request
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

pub(crate) fn is_jsonrpc_request_with_id(value: &Value) -> bool {
    value.get("method").is_some() && value.get("id").is_some()
}

pub(crate) fn is_jsonrpc_notification(value: &Value) -> bool {
    value.get("method").is_some() && value.get("id").is_none()
}

pub(crate) fn is_jsonrpc_response(value: &Value) -> bool {
    value.get("id").is_some()
        && value.get("method").is_none()
        && (value.get("result").is_some() || value.get("error").is_some())
}

pub(crate) fn is_initialize_request(value: &Value) -> bool {
    value.get("method").is_some_and(|m| m == "initialize") && value.get("id").is_some()
}

/// Methods that are scoped to a session and require an Acp-Session-Id header.
pub(crate) fn method_requires_session_header(method: &str) -> bool {
    matches!(
        method,
        "session/prompt"
            | "session/cancel"
            | "session/load"
            | "session/set_mode"
            | "session/set_model"
    )
}

async fn handle_get(
    ws_upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    State(state): State<Arc<connection::ConnectionRegistry>>,
    request: Request<Body>,
) -> Response {
    match ws_upgrade {
        Ok(ws) => websocket::handle_ws_upgrade(state, ws).await,
        Err(_) => http::handle_get(state, request).await,
    }
}

async fn health() -> &'static str {
    "ok"
}

pub fn create_router(server: Arc<AcpServer>, secret_key: String) -> Router {
    let registry = Arc::new(connection::ConnectionRegistry::new(server));

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::ACCEPT,
            HeaderName::from_static("acp-connection-id"),
            HeaderName::from_static("acp-session-id"),
            header::SEC_WEBSOCKET_VERSION,
            header::SEC_WEBSOCKET_KEY,
            header::CONNECTION,
            header::UPGRADE,
        ])
        .expose_headers([
            HeaderName::from_static("acp-connection-id"),
            HeaderName::from_static("acp-session-id"),
        ]);

    Router::new()
        .route("/health", get(health))
        .route("/status", get(health))
        .route("/acp", post(http::handle_post).with_state(registry.clone()))
        .route("/acp", get(handle_get).with_state(registry.clone()))
        .route("/acp", delete(http::handle_delete).with_state(registry))
        .merge(super::mcp_app_proxy::routes(secret_key))
        .layer(cors)
}
