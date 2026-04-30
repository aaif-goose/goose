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

const GUEST_HTML_TTL_SECS: u64 = 300;
const GUEST_HTML_MAX_ENTRIES: usize = 64;
const MCP_APP_PROXY_HTML: &str = include_str!("templates/mcp_app_proxy.html");

type GuestHtmlStore = Arc<RwLock<HashMap<String, (String, String, Instant)>>>;

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
    secret: String,
    nonce: String,
}

#[derive(Deserialize)]
struct StoreGuestBody {
    secret: String,
    html: String,
    csp: Option<String>,
}

#[derive(Clone)]
struct AppState {
    secret_key: String,
    guest_store: GuestHtmlStore,
}

fn parse_domains(domains: Option<&String>) -> Vec<String> {
    domains
        .map(|domains| {
            domains
                .split(',')
                .map(|domain| domain.trim().to_string())
                .filter(|domain| !domain.is_empty())
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

async fn mcp_app_proxy(
    axum::extract::State(state): axum::extract::State<AppState>,
    Query(params): Query<ProxyQuery>,
) -> Response {
    if params.secret != state.secret_key {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let html = MCP_APP_PROXY_HTML.replace(
        "{{OUTER_CSP}}",
        &build_outer_csp(
            &parse_domains(params.connect_domains.as_ref()),
            &parse_domains(params.resource_domains.as_ref()),
            &parse_domains(params.frame_domains.as_ref()),
            &parse_domains(params.base_uri_domains.as_ref()),
            &parse_domains(params.script_domains.as_ref()),
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
        let cutoff = Instant::now() - std::time::Duration::from_secs(GUEST_HTML_TTL_SECS);
        store.retain(|_, (_, _, created)| *created > cutoff);

        if store.len() >= GUEST_HTML_MAX_ENTRIES {
            if let Some(oldest_key) = store
                .iter()
                .min_by_key(|(_, (_, _, created))| *created)
                .map(|(key, _)| key.clone())
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

async fn serve_guest_html(
    axum::extract::State(state): axum::extract::State<AppState>,
    Query(params): Query<GuestQuery>,
) -> Response {
    if params.secret != state.secret_key {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let entry = {
        let mut store = state.guest_store.write().await;
        store.remove(&params.nonce)
    };

    match entry {
        Some((html, csp, _created)) => {
            let mut response = Html(html).into_response();
            let headers = response.headers_mut();
            headers.insert(
                header::HeaderName::from_static("referrer-policy"),
                "strict-origin".parse().unwrap(),
            );
            if !csp.is_empty() {
                headers.insert(header::CONTENT_SECURITY_POLICY, csp.parse().unwrap());
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

pub(crate) fn routes(secret_key: String) -> Router {
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
