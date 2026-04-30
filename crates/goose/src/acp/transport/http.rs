use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::{
    body::Body,
    extract::State,
    http::{HeaderValue, Request, StatusCode},
    response::{IntoResponse, Response, Sse},
};
use http_body_util::BodyExt;
use serde_json::Value;
use tokio::sync::broadcast;
use tracing::{debug, error, info, trace};

use super::connection::{Connection, ConnectionRegistry};
use super::*;

/// POST /acp
///
/// - `initialize`: creates a new connection, forwards the request, waits for
///   the synchronous initialize response from the agent, and returns it as a
///   200 OK JSON body with the `Acp-Connection-Id` header set.
/// - All other messages: require `Acp-Connection-Id` (and `Acp-Session-Id`
///   for session-scoped methods), forward to the agent, return 202 Accepted.
pub(crate) async fn handle_post(
    State(registry): State<Arc<ConnectionRegistry>>,
    request: Request<Body>,
) -> Response {
    if !content_type_is_json(&request) {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Unsupported Media Type: Content-Type must be application/json",
        )
            .into_response();
    }

    let connection_id = header_value(&request, HEADER_CONNECTION_ID);
    let session_id = header_value(&request, HEADER_SESSION_ID);

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
        return handle_initialize(registry, json_message).await;
    }

    let Some(connection_id) = connection_id else {
        return (
            StatusCode::BAD_REQUEST,
            "Bad Request: Acp-Connection-Id header required",
        )
            .into_response();
    };

    let Some(connection) = registry.get(&connection_id).await else {
        return (StatusCode::NOT_FOUND, "Unknown Acp-Connection-Id").into_response();
    };

    if let Some(method) = json_message.get("method").and_then(|m| m.as_str()) {
        if method_requires_session_header(method) && session_id.is_none() {
            return (
                StatusCode::BAD_REQUEST,
                "Bad Request: Acp-Session-Id header required for session-scoped methods",
            )
                .into_response();
        }
    }

    if !is_jsonrpc_request_with_id(&json_message)
        && !is_jsonrpc_notification(&json_message)
        && !is_jsonrpc_response(&json_message)
    {
        return (StatusCode::BAD_REQUEST, "Invalid JSON-RPC message").into_response();
    }

    let message_str = serde_json::to_string(&json_message).unwrap();
    trace!(connection_id = %connection_id, payload = %message_str, "POST → agent");
    if connection.to_agent_tx.send(message_str).await.is_err() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to forward message to agent",
        )
            .into_response();
    }

    StatusCode::ACCEPTED.into_response()
}

async fn handle_initialize(registry: Arc<ConnectionRegistry>, json_message: Value) -> Response {
    let (connection_id, connection) = match registry.create_connection().await {
        Ok(pair) => pair,
        Err(e) => {
            error!("Failed to create connection: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create connection",
            )
                .into_response();
        }
    };

    let message_str = serde_json::to_string(&json_message).unwrap();
    trace!(connection_id = %connection_id, payload = %message_str, "initialize → agent");
    if connection.to_agent_tx.send(message_str).await.is_err() {
        registry.remove(&connection_id).await;
        connection.shutdown().await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to forward initialize to agent",
        )
            .into_response();
    }

    // Read exactly one message from the agent: the initialize response.
    let init_response = {
        let mut guard = connection.init_receiver.lock().await;
        let Some(rx) = guard.as_mut() else {
            registry.remove(&connection_id).await;
            connection.shutdown().await;
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Initialize receiver already consumed",
            )
                .into_response();
        };
        rx.recv().await
    };

    let init_response = match init_response {
        Some(msg) => msg,
        None => {
            registry.remove(&connection_id).await;
            connection.shutdown().await;
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Agent closed before initialize response",
            )
                .into_response();
        }
    };

    connection.start_fanout().await;

    let mut response = (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, JSON_MIME_TYPE)],
        init_response,
    )
        .into_response();
    if let Ok(v) = HeaderValue::from_str(&connection_id) {
        response.headers_mut().insert(HEADER_CONNECTION_ID, v);
    }
    info!(connection_id = %connection_id, "Initialize complete");
    response
}

/// GET /acp (no Upgrade)
///
/// Opens the single long-lived SSE stream for a connection. All server→client
/// messages (responses + notifications + server-initiated requests) are
/// delivered here, correlated by their JSON-RPC body fields.
pub(crate) async fn handle_get(
    registry: Arc<ConnectionRegistry>,
    request: Request<Body>,
) -> Response {
    if !accepts_mime_type(&request, EVENT_STREAM_MIME_TYPE) {
        return (
            StatusCode::NOT_ACCEPTABLE,
            "Not Acceptable: Client must accept text/event-stream",
        )
            .into_response();
    }

    let Some(connection_id) = header_value(&request, HEADER_CONNECTION_ID) else {
        return (
            StatusCode::BAD_REQUEST,
            "Bad Request: Acp-Connection-Id header required",
        )
            .into_response();
    };

    let Some(connection) = registry.get(&connection_id).await else {
        return (StatusCode::NOT_FOUND, "Unknown Acp-Connection-Id").into_response();
    };

    let (replay, receiver) = connection.subscribe_with_replay().await;
    let sse = build_sse_stream(connection.clone(), replay, receiver);

    let mut response = sse.into_response();
    if let Ok(v) = HeaderValue::from_str(&connection_id) {
        response.headers_mut().insert(HEADER_CONNECTION_ID, v);
    }
    response
}

fn build_sse_stream(
    _connection: Arc<Connection>,
    replay: Vec<String>,
    mut receiver: broadcast::Receiver<String>,
) -> Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let stream = async_stream::stream! {
        for msg in replay {
            trace!(payload = %msg, "SSE → client (replay)");
            yield Ok::<_, Infallible>(axum::response::sse::Event::default().data(msg));
        }
        loop {
            match receiver.recv().await {
                Ok(msg) => {
                    trace!(payload = %msg, "SSE → client");
                    yield Ok::<_, Infallible>(axum::response::sse::Event::default().data(msg));
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!("SSE subscriber lagged {} messages", n);
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text(""),
    )
}

/// DELETE /acp
pub(crate) async fn handle_delete(
    State(registry): State<Arc<ConnectionRegistry>>,
    request: Request<Body>,
) -> Response {
    let Some(connection_id) = header_value(&request, HEADER_CONNECTION_ID) else {
        return (
            StatusCode::BAD_REQUEST,
            "Bad Request: Acp-Connection-Id header required",
        )
            .into_response();
    };

    let Some(connection) = registry.remove(&connection_id).await else {
        return (StatusCode::NOT_FOUND, "Unknown Acp-Connection-Id").into_response();
    };
    connection.shutdown().await;
    info!(connection_id = %connection_id, "Connection terminated via DELETE");
    StatusCode::ACCEPTED.into_response()
}
