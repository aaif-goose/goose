use axum::{
    body::Body,
    extract::{Path, Query, Request, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use serde::Deserialize;
use std::str::FromStr;

use crate::acp::transport::auth::token_matches;
use crate::config::paths::Paths;
use crate::plugins::formats::goose_native::{http_port_for_plugin, read_manifest};
use crate::plugins::is_plugin_enabled;

#[derive(Clone)]
struct ProxyState {
    secret_key: String,
    http_client: reqwest::Client,
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

async fn proxy_to_plugin(
    State(state): State<ProxyState>,
    Path((plugin_id, tail)): Path<(String, String)>,
    Query(query): Query<TokenQuery>,
    req: Request,
) -> Result<Response, StatusCode> {
    let header_token = req
        .headers()
        .get("X-Secret-Key")
        .and_then(|v| v.to_str().ok());
    if !token_matches(header_token, &state.secret_key)
        && !token_matches(query.token.as_deref(), &state.secret_key)
    {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if plugin_id.contains("..") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let plugins_dir = Paths::plugins_dir();
    let plugin_dir = plugins_dir.join(&plugin_id);
    if !plugin_dir.is_dir() {
        return Err(StatusCode::NOT_FOUND);
    }

    let Ok(Some(manifest)) = read_manifest(&plugin_dir) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let Some(http_surface) = manifest.http else {
        return Err(StatusCode::NOT_FOUND);
    };
    if http_surface.routes.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    if !is_plugin_enabled(&plugin_dir) {
        return Err(StatusCode::FORBIDDEN);
    }

    let port = http_port_for_plugin(&manifest.id);
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");

    let prefix = format!("/plugin-http/{plugin_id}/");
    let tail_path = path_and_query
        .strip_prefix(&prefix)
        .unwrap_or(tail.as_str());

    let target_url = format!("http://127.0.0.1:{port}/{tail_path}");

    let method = Method::from_str(req.method().as_str()).unwrap_or(Method::GET);

    let mut upstream_req = state.http_client.request(method, &target_url);

    for (name, value) in req.headers() {
        if name == header::HOST || name.as_str().eq_ignore_ascii_case("x-secret-key") {
            continue;
        }
        upstream_req = upstream_req.header(name, value);
    }

    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    upstream_req = upstream_req.body(body_bytes);

    let upstream_resp = upstream_req.send().await.map_err(|e| {
        tracing::warn!(plugin = plugin_id, error = %e, "plugin HTTP proxy error");
        StatusCode::BAD_GATEWAY
    })?;

    let status = StatusCode::from_u16(upstream_resp.status().as_u16())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    let mut resp_headers = HeaderMap::new();
    for (name, value) in upstream_resp.headers() {
        if let Ok(n) = HeaderName::from_str(name.as_str()) {
            if let Ok(v) = HeaderValue::from_bytes(value.as_bytes()) {
                resp_headers.insert(n, v);
            }
        }
    }

    let resp_body = Body::from_stream(upstream_resp.bytes_stream());

    Ok((status, resp_headers, resp_body).into_response())
}

pub fn routes(secret_key: String) -> Router {
    let state = ProxyState {
        secret_key,
        http_client: reqwest::Client::new(),
    };
    Router::new()
        .route("/plugin-http/{plugin_id}/{*tail}", any(proxy_to_plugin))
        .with_state(state)
}
