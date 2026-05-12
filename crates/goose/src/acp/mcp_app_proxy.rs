use axum::{
    extract::{ConnectInfo, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::RwLock;
use uuid::Uuid;

const GUEST_HTML_TTL_SECS: u64 = 300;
const GUEST_HTML_MAX_ENTRIES: usize = 64;
const MCP_APP_PROXY_HTML: &str = include_str!("templates/mcp_app_proxy.html");

type GuestHtmlStore = Arc<RwLock<HashMap<String, GuestHtmlEntry>>>;

#[derive(Clone)]
struct GuestHtmlEntry {
    html: String,
    csp: String,
    created: Instant,
}

#[derive(Deserialize)]
struct ProxyQuery {
    secret: String,
    connect_domains: Option<String>,
    resource_domains: Option<String>,
    frame_domains: Option<String>,
    base_uri_domains: Option<String>,
    script_domains: Option<String>,
}

#[derive(Deserialize)]
struct GuestQuery {
    nonce: String,
}

#[derive(Deserialize)]
struct StoreGuestBody {
    secret: String,
    html: String,
    csp: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoreGuestResponse {
    nonce: String,
    guest_url: String,
}

#[derive(Clone)]
struct AppState {
    secret_key: String,
    guest_store: GuestHtmlStore,
    guest_base_url: String,
}

#[derive(Clone)]
struct GuestState {
    guest_store: GuestHtmlStore,
}

fn normalize_csp_source(source: &str) -> Option<String> {
    let source = source.trim();
    if source.is_empty()
        || source
            .chars()
            .any(|c| c.is_ascii_whitespace() || matches!(c, ';' | ',' | '"' | '\''))
    {
        return None;
    }

    if let Some((scheme, rest)) = source.split_once("://") {
        let scheme = scheme.to_ascii_lowercase();
        if !matches!(scheme.as_str(), "http" | "https" | "ws" | "wss") {
            return None;
        }

        let authority = rest.split(['/', '?', '#']).next()?;
        if !is_valid_csp_host_source(authority) {
            return None;
        }

        return Some(format!("{scheme}://{}", authority.to_ascii_lowercase()));
    }

    if is_valid_csp_host_source(source) {
        return Some(source.to_ascii_lowercase());
    }

    None
}

fn is_valid_csp_host_source(source: &str) -> bool {
    if source.is_empty() || source == "*" || source.contains('@') {
        return false;
    }

    let (host, port) = split_host_and_port(source);
    if host.is_empty() {
        return false;
    }
    if port.is_some_and(|port| port.is_empty() || port.parse::<u16>().is_err()) {
        return false;
    }

    let host = host.strip_prefix("*.").unwrap_or(host);
    if host.eq_ignore_ascii_case("localhost")
        || host.parse::<std::net::Ipv4Addr>().is_ok()
        || host.parse::<std::net::Ipv6Addr>().is_ok()
    {
        return true;
    }

    !host.is_empty()
        && host.contains('.')
        && host
            .split('.')
            .all(|label| is_valid_dns_label(label) && label != "*")
}

fn split_host_and_port(source: &str) -> (&str, Option<&str>) {
    if let Some(remainder) = source.strip_prefix('[') {
        if let Some((host, tail)) = remainder.split_once(']') {
            let port = tail.strip_prefix(':');
            return (host, port);
        }
    }

    match source.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (host, Some(port)),
        _ => (source, None),
    }
}

fn is_valid_dns_label(label: &str) -> bool {
    !label.is_empty()
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn peer_addr_is_loopback(peer_addr: &SocketAddr) -> bool {
    peer_addr.ip().is_loopback()
}

fn parse_domains(domains: Option<&String>) -> Vec<String> {
    domains
        .map(|domains| {
            domains
                .split(',')
                .filter_map(normalize_csp_source)
                .collect()
        })
        .unwrap_or_default()
}

fn build_outer_csp(
    connect_domains: &[String],
    resource_domains: &[String],
    frame_domains: &[String],
    base_uri_domains: &[String],
    script_domains: &[String],
    guest_origin: &str,
) -> String {
    let resources = if resource_domains.is_empty() {
        String::new()
    } else {
        format!(" {}", resource_domains.join(" "))
    };

    let scripts = if script_domains.is_empty() {
        String::new()
    } else {
        format!(" {}", script_domains.join(" "))
    };

    let connections = if connect_domains.is_empty() {
        String::new()
    } else {
        format!(" {}", connect_domains.join(" "))
    };

    let frame_src = if frame_domains.is_empty() {
        format!("frame-src 'self' {guest_origin}")
    } else {
        format!(
            "frame-src 'self' {guest_origin} {}",
            frame_domains.join(" ")
        )
    };

    let base_uris = if base_uri_domains.is_empty() {
        String::new()
    } else {
        format!(" {}", base_uri_domains.join(" "))
    };

    format!(
        "default-src 'none'; \
         script-src 'self' 'unsafe-inline'{resources}{scripts}; \
         script-src-elem 'self' 'unsafe-inline'{resources}{scripts}; \
         style-src 'self' 'unsafe-inline'{resources}; \
         style-src-elem 'self' 'unsafe-inline'{resources}; \
         connect-src 'self'{connections}; \
         img-src 'self' data: blob:{resources}; \
         font-src 'self'{resources}; \
         media-src 'self' data: blob:{resources}; \
         {frame_src}; \
         object-src 'none'; \
         base-uri 'self'{base_uris}"
    )
}

async fn mcp_app_proxy(
    State(state): State<AppState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    Query(params): Query<ProxyQuery>,
) -> Response {
    if params.secret != state.secret_key {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    if !peer_addr_is_loopback(&peer_addr) {
        return (
            StatusCode::BAD_REQUEST,
            "MCP app proxy is only available to loopback clients",
        )
            .into_response();
    }

    let html = MCP_APP_PROXY_HTML.replace(
        "{{OUTER_CSP}}",
        &build_outer_csp(
            &parse_domains(params.connect_domains.as_ref()),
            &parse_domains(params.resource_domains.as_ref()),
            &parse_domains(params.frame_domains.as_ref()),
            &parse_domains(params.base_uri_domains.as_ref()),
            &parse_domains(params.script_domains.as_ref()),
            &state.guest_base_url,
        ),
    );

    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (
                header::HeaderName::from_static("referrer-policy"),
                "no-referrer",
            ),
        ],
        Html(html),
    )
        .into_response()
}

async fn store_guest_html(
    State(state): State<AppState>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    Json(body): Json<StoreGuestBody>,
) -> Response {
    if body.secret != state.secret_key {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    if !peer_addr_is_loopback(&peer_addr) {
        return (
            StatusCode::BAD_REQUEST,
            "MCP app guest storage is only available to loopback clients",
        )
            .into_response();
    }

    let nonce = Uuid::new_v4().to_string();
    let csp = body.csp.unwrap_or_default();
    let guest_url = format!("{}/mcp-app-guest?nonce={}", state.guest_base_url, nonce);

    {
        let mut store = state.guest_store.write().await;
        let cutoff = Instant::now() - Duration::from_secs(GUEST_HTML_TTL_SECS);
        store.retain(|_, entry| entry.created > cutoff);

        if store.len() >= GUEST_HTML_MAX_ENTRIES {
            if let Some(oldest_key) = store
                .iter()
                .min_by_key(|(_, entry)| entry.created)
                .map(|(key, _)| key.clone())
            {
                store.remove(&oldest_key);
            }
        }

        store.insert(
            nonce.clone(),
            GuestHtmlEntry {
                html: body.html,
                csp,
                created: Instant::now(),
            },
        );
    }

    (
        StatusCode::OK,
        Json(StoreGuestResponse { nonce, guest_url }),
    )
        .into_response()
}

async fn serve_guest_html(
    State(state): State<GuestState>,
    Query(params): Query<GuestQuery>,
) -> Response {
    let entry = {
        let mut store = state.guest_store.write().await;
        let cutoff = Instant::now() - Duration::from_secs(GUEST_HTML_TTL_SECS);
        store.retain(|_, entry| entry.created > cutoff);
        store.get(&params.nonce).cloned()
    };

    match entry {
        Some(entry) => {
            let mut response = Html(entry.html).into_response();
            let headers = response.headers_mut();
            headers.insert(
                header::HeaderName::from_static("referrer-policy"),
                "strict-origin".parse().unwrap(),
            );
            if !entry.csp.is_empty() {
                match HeaderValue::from_str(&entry.csp) {
                    Ok(csp) => {
                        headers.insert(header::CONTENT_SECURITY_POLICY, csp);
                    }
                    Err(_) => return (StatusCode::BAD_REQUEST, "Invalid CSP").into_response(),
                }
            }
            response
        }
        None => (StatusCode::NOT_FOUND, "Guest content not found").into_response(),
    }
}

fn spawn_guest_server(guest_store: GuestHtmlStore) -> String {
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("failed to bind MCP app guest server");
    let addr = listener
        .local_addr()
        .expect("failed to read MCP app guest server address");
    listener
        .set_nonblocking(true)
        .expect("failed to configure MCP app guest server");
    let listener = tokio::net::TcpListener::from_std(listener)
        .expect("failed to create MCP app guest listener");

    let app = Router::new()
        .route("/mcp-app-guest", get(serve_guest_html))
        .with_state(GuestState { guest_store });

    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            tracing::error!(%error, "MCP app guest server stopped");
        }
    });

    format!("http://{addr}")
}

pub(crate) fn routes(secret_key: String) -> Router {
    let guest_store = Arc::new(RwLock::new(HashMap::new()));
    let guest_base_url = spawn_guest_server(guest_store.clone());
    let state = AppState {
        secret_key,
        guest_store,
        guest_base_url,
    };

    Router::new()
        .route("/mcp-app-proxy", get(mcp_app_proxy))
        .route("/mcp-app-guest", post(store_guest_html))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::{normalize_csp_source, parse_domains, peer_addr_is_loopback};
    use std::net::SocketAddr;

    #[test]
    fn normalizes_url_sources_to_origins() {
        assert_eq!(
            normalize_csp_source("https://cdn.example.com/assets/app.js"),
            Some("https://cdn.example.com".to_string())
        );
        assert_eq!(
            normalize_csp_source("wss://api.example.com/socket"),
            Some("wss://api.example.com".to_string())
        );
    }

    #[test]
    fn accepts_wildcard_and_host_sources() {
        assert_eq!(
            normalize_csp_source("https://*.cloudflare.com"),
            Some("https://*.cloudflare.com".to_string())
        );
        assert_eq!(
            normalize_csp_source("cdn.example.com"),
            Some("cdn.example.com".to_string())
        );
        assert_eq!(
            normalize_csp_source("localhost:3000"),
            Some("localhost:3000".to_string())
        );
    }

    #[test]
    fn rejects_unsafe_csp_sources() {
        assert_eq!(normalize_csp_source("*"), None);
        assert_eq!(normalize_csp_source("'unsafe-inline'"), None);
        assert_eq!(normalize_csp_source("javascript:alert(1)"), None);
        assert_eq!(normalize_csp_source("https://example.com;"), None);
        assert_eq!(normalize_csp_source("https://user@example.com"), None);
    }

    #[test]
    fn parse_domains_filters_invalid_sources() {
        let domains =
            "https://cdn.example.com/app.js, https://*.cloudflare.com, *, cdn.example.com"
                .to_string();

        assert_eq!(
            parse_domains(Some(&domains)),
            vec![
                "https://cdn.example.com".to_string(),
                "https://*.cloudflare.com".to_string(),
                "cdn.example.com".to_string(),
            ]
        );
    }

    #[test]
    fn detects_loopback_peer_addresses() {
        assert!(peer_addr_is_loopback(
            &"127.0.0.1:12345".parse::<SocketAddr>().unwrap()
        ));
        assert!(peer_addr_is_loopback(
            &"[::1]:12345".parse::<SocketAddr>().unwrap()
        ));
        assert!(!peer_addr_is_loopback(
            &"192.168.1.10:12345".parse::<SocketAddr>().unwrap()
        ));
    }
}
