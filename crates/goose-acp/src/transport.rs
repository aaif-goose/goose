pub mod http;
pub mod websocket;

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{
        ws::{rejection::WebSocketUpgradeRejection, WebSocketUpgrade},
        Extension, State,
    },
    http::{header, Method, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Router,
};
use serde_json::Value;
use subtle::ConstantTimeEq;
use tokio::sync::{mpsc, Mutex};
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::server_factory::AcpServer;

pub(crate) const HEADER_SESSION_ID: &str = "Acp-Session-Id";
pub(crate) const EVENT_STREAM_MIME_TYPE: &str = "text/event-stream";
pub(crate) const JSON_MIME_TYPE: &str = "application/json";
pub(crate) const WS_AUTH_SUBPROTOCOL_PREFIX: &str = "goose-acp-auth.";

#[derive(Clone, Debug)]
pub(crate) struct AcpAuthContext {
    pub websocket_protocol: Option<String>,
}

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
    auth_context: Option<Extension<AcpAuthContext>>,
    State(state): State<(Arc<http::HttpState>, Arc<websocket::WsState>)>,
    request: Request<Body>,
) -> Response {
    match ws_upgrade {
        Ok(ws) => websocket::handle_get(state.1, ws, auth_context.map(|context| context.0)).await,
        Err(_) => http::handle_get(state.0, request).await,
    }
}

async fn health() -> &'static str {
    "ok"
}

fn is_websocket_upgrade(request: &Request<Body>) -> bool {
    request
        .headers()
        .get(header::UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
}

fn constant_time_token_matches(expected: &str, actual: &str) -> bool {
    expected.as_bytes().ct_eq(actual.as_bytes()).into()
}

fn extract_bearer_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = value.strip_prefix("Bearer ")?;
    Some(token.trim())
}

fn extract_websocket_auth_protocol(
    headers: &axum::http::HeaderMap,
    expected_token: &str,
) -> Option<String> {
    let protocols = headers.get(header::SEC_WEBSOCKET_PROTOCOL)?.to_str().ok()?;

    protocols
        .split(',')
        .map(str::trim)
        .find(|protocol| {
            protocol
                .strip_prefix(WS_AUTH_SUBPROTOCOL_PREFIX)
                .is_some_and(|token| constant_time_token_matches(expected_token, token))
        })
        .map(ToOwned::to_owned)
}

async fn require_acp_auth(
    State(auth_token): State<Arc<str>>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let authorized = if is_websocket_upgrade(&request) {
        if let Some(protocol) =
            extract_websocket_auth_protocol(request.headers(), auth_token.as_ref())
        {
            request.extensions_mut().insert(AcpAuthContext {
                websocket_protocol: Some(protocol),
            });
        }

        request
            .extensions()
            .get::<AcpAuthContext>()
            .is_some_and(|context| context.websocket_protocol.is_some())
            || extract_bearer_token(request.headers())
                .is_some_and(|token| constant_time_token_matches(auth_token.as_ref(), token))
    } else {
        extract_bearer_token(request.headers())
            .is_some_and(|token| constant_time_token_matches(auth_token.as_ref(), token))
    };

    if !authorized {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    next.run(request).await
}

pub fn create_router(server: Arc<AcpServer>, auth_token: Arc<str>) -> Router {
    let http_state = Arc::new(http::HttpState::new(server.clone()));
    let ws_state = Arc::new(websocket::WsState::new(server));

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _| {
            is_allowed_origin(origin.as_bytes())
        }))
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            HEADER_SESSION_ID.parse().unwrap(),
            header::SEC_WEBSOCKET_PROTOCOL,
            header::SEC_WEBSOCKET_VERSION,
            header::SEC_WEBSOCKET_KEY,
            header::CONNECTION,
            header::UPGRADE,
        ]);

    let acp_routes = Router::new()
        .route(
            "/acp",
            post(http::handle_post).with_state(http_state.clone()),
        )
        .route(
            "/acp",
            get(handle_get).with_state((http_state.clone(), ws_state)),
        )
        .route("/acp", delete(http::handle_delete).with_state(http_state))
        .route_layer(middleware::from_fn_with_state(auth_token, require_acp_auth));

    Router::new()
        .route("/health", get(health))
        .route("/status", get(health))
        .merge(acp_routes)
        .layer(cors)
}

fn is_allowed_origin(origin: &[u8]) -> bool {
    if origin == b"tauri://localhost" || origin == b"https://tauri.localhost" {
        return true;
    }
    if let Some(rest) = origin.strip_prefix(b"http://") {
        let host = rest.split(|&b| b == b':').next().unwrap_or(rest);
        return host == b"localhost" || host == b"127.0.0.1";
    }
    false
}

#[cfg(test)]
mod tests {
    use axum::{
        extract::Extension,
        http::{header::SEC_WEBSOCKET_PROTOCOL, Request, StatusCode},
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    use super::*;

    fn auth_test_router(token: Arc<str>) -> Router {
        Router::new()
            .route("/acp", get(|| async { StatusCode::OK }))
            .route(
                "/ws",
                get(|Extension(context): Extension<AcpAuthContext>| async move {
                    let protocol = context
                        .websocket_protocol
                        .expect("missing websocket protocol");
                    (
                        StatusCode::SWITCHING_PROTOCOLS,
                        [(SEC_WEBSOCKET_PROTOCOL, protocol)],
                    )
                }),
            )
            .route_layer(middleware::from_fn_with_state(token, require_acp_auth))
    }

    #[tokio::test]
    async fn acp_auth_rejects_missing_auth_header() {
        let response = auth_test_router(Arc::<str>::from("secret-token"))
            .oneshot(Request::builder().uri("/acp").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn acp_auth_rejects_wrong_auth_header() {
        let response = auth_test_router(Arc::<str>::from("secret-token"))
            .oneshot(
                Request::builder()
                    .uri("/acp")
                    .header(header::AUTHORIZATION, "Bearer wrong-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn acp_auth_accepts_matching_auth_header() {
        let response = auth_test_router(Arc::<str>::from("secret-token"))
            .oneshot(
                Request::builder()
                    .uri("/acp")
                    .header(header::AUTHORIZATION, "Bearer secret-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn acp_auth_accepts_matching_websocket_subprotocol() {
        let protocol = format!("{WS_AUTH_SUBPROTOCOL_PREFIX}secret-token");
        let response = auth_test_router(Arc::<str>::from("secret-token"))
            .oneshot(
                Request::builder()
                    .uri("/ws")
                    .header(header::CONNECTION, "Upgrade")
                    .header(header::UPGRADE, "websocket")
                    .header(header::SEC_WEBSOCKET_VERSION, "13")
                    .header(header::SEC_WEBSOCKET_KEY, "dGhlIHNhbXBsZSBub25jZQ==")
                    .header(header::SEC_WEBSOCKET_PROTOCOL, protocol.clone())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
        assert_eq!(
            response
                .headers()
                .get(header::SEC_WEBSOCKET_PROTOCOL)
                .unwrap(),
            &protocol
        );
    }

    #[test]
    fn cors_accepts_localhost_with_any_port() {
        assert!(is_allowed_origin(b"http://localhost"));
        assert!(is_allowed_origin(b"http://localhost:1420"));
        assert!(is_allowed_origin(b"http://localhost:1520"));
        assert!(is_allowed_origin(b"http://localhost:5173"));
        assert!(is_allowed_origin(b"http://127.0.0.1"));
        assert!(is_allowed_origin(b"http://127.0.0.1:3000"));
    }

    #[test]
    fn cors_accepts_tauri_origins() {
        assert!(is_allowed_origin(b"tauri://localhost"));
        assert!(is_allowed_origin(b"https://tauri.localhost"));
    }

    #[test]
    fn cors_rejects_non_local_origins() {
        assert!(!is_allowed_origin(b"http://evil.com"));
        assert!(!is_allowed_origin(b"https://localhost"));
        assert!(!is_allowed_origin(b"http://localhost.evil.com"));
        assert!(!is_allowed_origin(b"http://127.0.0.1.evil.com"));
        assert!(!is_allowed_origin(b""));
    }
}
