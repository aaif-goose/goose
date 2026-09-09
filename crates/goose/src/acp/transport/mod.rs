pub mod auth;
#[cfg(any(feature = "rustls-tls", feature = "native-tls"))]
pub mod tls;

use std::sync::Arc;

use agent_client_protocol_http::{AcpHttpServer, CorsOptions, ServerOptions};
use axum::{
    extract::{Request, State},
    http::{header, HeaderName, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::Response,
    routing::get,
    Router,
};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

use crate::acp::server::GooseAgentConnection;
use crate::acp::server_factory::AcpServer;

// The upstream ACP HTTP server only supports exact origin allowlists for
// WebSocket upgrades; Goose applies its richer loopback predicate before this.
const UPSTREAM_WS_ALLOWED_ORIGIN: &str = "http://goose.local";
const OPAQUE_ORIGIN: &str = "null";
const FILE_ORIGIN: &str = "file://";

#[derive(Clone)]
struct AcpOriginPolicy {
    exact_origins: Arc<[HeaderValue]>,
    allow_loopback: bool,
}

impl AcpOriginPolicy {
    fn loopback() -> Self {
        Self {
            exact_origins: Vec::new().into(),
            allow_loopback: true,
        }
    }

    fn exact(origins: Vec<HeaderValue>) -> Self {
        Self {
            exact_origins: origins.into(),
            allow_loopback: false,
        }
    }

    fn loopback_and(origins: Vec<HeaderValue>) -> Self {
        Self {
            exact_origins: origins.into(),
            allow_loopback: true,
        }
    }

    fn local_default() -> Self {
        Self::loopback_and(Self::file_origins(Vec::new()))
    }

    fn file_origins(mut origins: Vec<HeaderValue>) -> Vec<HeaderValue> {
        origins.extend([
            HeaderValue::from_static(OPAQUE_ORIGIN),
            HeaderValue::from_static(FILE_ORIGIN),
        ]);
        origins
    }

    fn origin_allowed(&self, origin: &HeaderValue) -> bool {
        if self
            .exact_origins
            .iter()
            .any(|allowed_origin| allowed_origin == origin)
        {
            return true;
        }

        if !self.allow_loopback {
            return false;
        }

        let Ok(origin) = origin.to_str() else {
            return false;
        };

        let Ok(url) = url::Url::parse(origin) else {
            return false;
        };

        if !matches!(url.scheme(), "http" | "https") {
            return false;
        }

        match url.host() {
            Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
            Some(url::Host::Ipv4(addr)) => addr.is_loopback(),
            Some(url::Host::Ipv6(addr)) => addr.is_loopback(),
            None => false,
        }
    }
}

fn acp_http_options() -> ServerOptions {
    ServerOptions {
        path: "/acp".to_string(),
        cors: CorsOptions::allow_origins([UPSTREAM_WS_ALLOWED_ORIGIN])
            .expect("static origin is valid"),
        health_endpoint: false,
    }
}

fn header_contains_token(value: Option<&HeaderValue>, token: &str) -> bool {
    value
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case(token))
        })
}

fn is_websocket_upgrade(request: &Request) -> bool {
    request.method() == Method::GET
        && header_contains_token(request.headers().get(header::CONNECTION), "upgrade")
        && request
            .headers()
            .get(header::UPGRADE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
}

async fn enforce_websocket_origin(
    State(policy): State<AcpOriginPolicy>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if is_websocket_upgrade(&request) {
        if let Some(origin) = request.headers().get(header::ORIGIN) {
            if !policy.origin_allowed(origin) {
                return Err(StatusCode::FORBIDDEN);
            }
        }

        request.headers_mut().insert(
            header::ORIGIN,
            HeaderValue::from_static(UPSTREAM_WS_ALLOWED_ORIGIN),
        );
    }

    Ok(next.run(request).await)
}

fn acp_cors_layer(policy: AcpOriginPolicy) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, _request_parts| {
            policy.origin_allowed(origin)
        }))
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::ACCEPT,
            HeaderName::from_static("x-secret-key"),
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
        ])
}

/// CORS for the auxiliary routes (`/health`, `/status`, MCP app proxy) served by
/// `goose serve`. This allows the `x-secret-key` auth header the proxy routes
/// rely on.
fn aux_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::ACCEPT,
            HeaderName::from_static("x-secret-key"),
        ])
}

fn create_acp_router_inner(server: Arc<AcpServer>, policy: AcpOriginPolicy) -> Router {
    AcpHttpServer::new(move || GooseAgentConnection::new(server.clone()))
        .with_options(acp_http_options())
        .into_router()
        .layer(axum::middleware::from_fn_with_state(
            policy,
            enforce_websocket_origin,
        ))
}

fn create_acp_router_with_policy(
    server: Arc<AcpServer>,
    policy: AcpOriginPolicy,
    secret_key: Option<String>,
) -> Router {
    let mut acp_routes = create_acp_router_inner(server, policy.clone());
    if let Some(secret_key) = secret_key {
        acp_routes = acp_routes.layer(axum::middleware::from_fn_with_state(
            secret_key,
            auth::check_acp_token,
        ));
    }
    acp_routes.layer(acp_cors_layer(policy))
}

/// The bare ACP HTTP/WebSocket router (POST/GET/DELETE on `/acp`), without auth
/// or goose-specific auxiliary routes.
pub fn create_acp_router(server: Arc<AcpServer>) -> Router {
    create_acp_router_with_policy(server, AcpOriginPolicy::loopback(), None)
}

async fn health() -> &'static str {
    "ok"
}

/// The full standalone ACP server router used by `goose serve`: ACP transport,
/// optional token auth, health/status endpoints, and the MCP app proxy.
pub fn create_router(
    server: Arc<AcpServer>,
    secret_key: String,
    require_token: bool,
    additional_allowed_origins: Vec<HeaderValue>,
) -> Router {
    let policy = if !additional_allowed_origins.is_empty() {
        AcpOriginPolicy::exact(additional_allowed_origins)
    } else if require_token {
        AcpOriginPolicy::local_default()
    } else {
        AcpOriginPolicy::loopback()
    };
    let acp_routes =
        create_acp_router_with_policy(server, policy, require_token.then_some(secret_key.clone()));

    let aux_routes = Router::new()
        .route("/health", get(health))
        .route("/status", get(health))
        .merge(super::mcp_app_proxy::routes(secret_key))
        .layer(aux_cors_layer());

    acp_routes.merge(aux_routes)
}
