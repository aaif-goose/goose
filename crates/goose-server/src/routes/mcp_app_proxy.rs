use axum::{
    extract::Query,
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use uuid::Uuid;

const GUEST_HTML_TTL_SECS: u64 = 300; // 5 minutes
const GUEST_HTML_MAX_ENTRIES: usize = 64;

/// In-memory store for guest HTML content.
/// Maps nonce -> (html_content, csp_string, created_at)
/// Entries are consumed on first read and evicted after TTL.
type GuestHtmlStore = Arc<RwLock<HashMap<String, (String, String, Instant)>>>;

#[derive(Deserialize)]
struct ProxyQuery {
    secret: String,
    /// Comma-separated list of domains for connect-src (fetch, XHR, WebSocket)
    connect_domains: Option<String>,
    /// Comma-separated list of domains for resource loading (scripts, styles, images, fonts, media)
    resource_domains: Option<String>,
    /// Comma-separated list of origins for nested iframes (frame-src)
    frame_domains: Option<String>,
    /// Comma-separated list of allowed base URIs (base-uri)
    base_uri_domains: Option<String>,
    /// Comma-separated list of domains for script-src (external scripts like SDKs)
    script_domains: Option<String>,
}

#[derive(Deserialize)]
struct GuestQuery {
    secret: String,
    nonce: String,
}

#[derive(Deserialize)]
struct StoreGuestBody {
    secret: String,
    html: String,
    /// CSP string to apply to the guest page
    csp: Option<String>,
}

const MCP_APP_PROXY_HTML: &str = include_str!("templates/mcp_app_proxy.html");

/// Build the outer sandbox CSP based on declared domains.
///
/// This CSP acts as a ceiling - the inner guest UI iframe cannot exceed these
/// permissions, even if it tried. This is the single source of truth for
/// security policy enforcement.
///
/// Based on the MCP Apps specification (ext-apps SEP):
/// <https://github.com/modelcontextprotocol/ext-apps/blob/main/specification/draft/apps.mdx>
fn build_outer_csp(
    connect_domains: &[String],
    resource_domains: &[String],
    frame_domains: &[String],
    base_uri_domains: &[String],
    script_domains: &[String],
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

    // frame-src needs 'self' so the proxy can load the guest iframe from /mcp-app-guest
    let frame_src = if frame_domains.is_empty() {
        "frame-src 'self'".to_string()
    } else {
        format!("frame-src 'self' {}", frame_domains.join(" "))
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

/// Parse comma-separated domains, filtering out empty strings
fn parse_domains(domains: Option<&String>) -> Vec<String> {
    domains
        .map(|d| {
            d.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Clone)]
struct AppState {
    secret_key: String,
    guest_store: GuestHtmlStore,
}

#[utoipa::path(
    get,
    path = "/mcp-app-proxy",
    params(
        ("secret" = String, Query, description = "Secret key for authentication"),
        ("connect_domains" = Option<String>, Query, description = "Comma-separated domains for connect-src"),
        ("resource_domains" = Option<String>, Query, description = "Comma-separated domains for resource loading"),
        ("frame_domains" = Option<String>, Query, description = "Comma-separated origins for nested iframes (frame-src)"),
        ("base_uri_domains" = Option<String>, Query, description = "Comma-separated allowed base URIs (base-uri)"),
        ("script_domains" = Option<String>, Query, description = "Comma-separated domains for script-src")
    ),
    responses(
        (status = 200, description = "MCP App proxy HTML page", content_type = "text/html"),
        (status = 401, description = "Unauthorized - invalid or missing secret"),
    )
)]
async fn mcp_app_proxy(
    axum::extract::State(state): axum::extract::State<AppState>,
    Query(params): Query<ProxyQuery>,
) -> Response {
    if params.secret != state.secret_key {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    // Parse domains from query params
    let connect_domains = parse_domains(params.connect_domains.as_ref());
    let resource_domains = parse_domains(params.resource_domains.as_ref());
    let frame_domains = parse_domains(params.frame_domains.as_ref());
    let base_uri_domains = parse_domains(params.base_uri_domains.as_ref());
    let script_domains = parse_domains(params.script_domains.as_ref());

    // Build the outer CSP based on declared domains
    let csp = build_outer_csp(
        &connect_domains,
        &resource_domains,
        &frame_domains,
        &base_uri_domains,
        &script_domains,
    );

    // Replace the CSP placeholder in the HTML template
    let html = MCP_APP_PROXY_HTML.replace("{{OUTER_CSP}}", &csp);

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

/// Store guest HTML and return a nonce for retrieval.
/// The proxy page calls this via fetch, then sets the guest iframe src to /mcp-app-guest?nonce=...
async fn store_guest_html(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(body): Json<StoreGuestBody>,
) -> Response {
    if body.secret != state.secret_key {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let nonce = Uuid::new_v4().to_string();
    let csp = body.csp.unwrap_or_default();

    {
        let mut store = state.guest_store.write().await;

        // Evict expired entries
        let cutoff = Instant::now() - std::time::Duration::from_secs(GUEST_HTML_TTL_SECS);
        store.retain(|_, (_, _, created)| *created > cutoff);

        // If still at capacity, drop the oldest entry
        if store.len() >= GUEST_HTML_MAX_ENTRIES {
            if let Some(oldest_key) = store
                .iter()
                .min_by_key(|(_, (_, _, created))| *created)
                .map(|(k, _)| k.clone())
            {
                store.remove(&oldest_key);
            }
        }

        store.insert(nonce.clone(), (body.html, csp, Instant::now()));
    }

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        format!(r#"{{"nonce":"{}"}}"#, nonce),
    )
        .into_response()
}

/// Serve stored guest HTML with a real HTTPS URL.
/// This gives the guest iframe `window.location.protocol === "https:"`,
/// which is required by SDKs like Square Web Payments that check for secure context.
async fn serve_guest_html(
    axum::extract::State(state): axum::extract::State<AppState>,
    Query(params): Query<GuestQuery>,
) -> Response {
    if params.secret != state.secret_key {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    // Consume the entry (one-time use)
    let entry = {
        let mut store = state.guest_store.write().await;
        store.remove(&params.nonce)
    };

    match entry {
        Some((html, csp, _created)) => {
            let mut response = Html(html).into_response();
            let headers = response.headers_mut();
            // Use strict-origin so third-party SDKs (e.g. Square Web Payments)
            // receive the origin in their requests, which they need for auth.
            // no-referrer would cause 401s from SDK servers.
            headers.insert(
                header::HeaderName::from_static("referrer-policy"),
                header::HeaderValue::from_static("strict-origin"),
            );
            if !csp.is_empty() {
                match csp.parse::<header::HeaderValue>() {
                    Ok(csp_value) => {
                        headers.insert(header::CONTENT_SECURITY_POLICY, csp_value);
                    }
                    Err(_) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            "Invalid characters in Content-Security-Policy value",
                        )
                            .into_response();
                    }
                }
            }
            response
        }
        None => (
            StatusCode::NOT_FOUND,
            "Guest content not found or already consumed",
        )
            .into_response(),
    }
}

pub fn routes(secret_key: String) -> Router {
    let state = AppState {
        secret_key,
        guest_store: Arc::new(RwLock::new(HashMap::new())),
    };

    Router::new()
        .route("/mcp-app-proxy", get(mcp_app_proxy))
        .route("/mcp-app-guest", get(serve_guest_html))
        .route("/mcp-app-guest", post(store_guest_html))
        .with_state(state)
}
