use super::TunnelInfo;
use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use reqwest;
use serde::{Deserialize, Serialize};
use socket2::{SockRef, TcpKeepalive};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};
use url::Url;

/// Shared state for proxying tunnel requests to the local goosed server.
#[derive(Clone)]
struct ProxyContext {
    port: u16,
    tunnel_secret: String,
    server_secret: String,
    http_client: reqwest::Client,
}

/// Constant-time comparison using hash to prevent timing attacks
fn secure_compare(a: &str, b: &str) -> bool {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher_a = DefaultHasher::new();
    a.hash(&mut hasher_a);
    let hash_a = hasher_a.finish();

    let mut hasher_b = DefaultHasher::new();
    b.hash(&mut hasher_b);
    let hash_b = hasher_b.finish();

    hash_a == hash_b
}

const WORKER_URL: &str = "https://cloudflare-tunnel-proxy.michael-neale.workers.dev";
const IDLE_TIMEOUT_SECS: u64 = 300;
const CONNECTION_TIMEOUT_SECS: u64 = 30;
const MAX_WS_SIZE: usize = 900_000;

fn get_worker_url() -> String {
    std::env::var("GOOSE_TUNNEL_WORKER_URL")
        .ok()
        .unwrap_or_else(|| WORKER_URL.to_string())
}

type WebSocketSender = Arc<
    RwLock<
        Option<
            futures::stream::SplitSink<
                tokio_tungstenite::WebSocketStream<
                    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                >,
                Message,
            >,
        >,
    >,
>;

#[derive(Debug, Serialize, Deserialize)]
struct TunnelMessage {
    #[serde(rename = "requestId")]
    request_id: String,
    method: String,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
}

#[derive(Debug, Serialize)]
struct TunnelResponse {
    #[serde(rename = "requestId")]
    request_id: String,
    status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "chunkIndex")]
    chunk_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "totalChunks")]
    total_chunks: Option<usize>,
    #[serde(rename = "isChunked")]
    is_chunked: bool,
    #[serde(rename = "isStreaming")]
    is_streaming: bool,
    #[serde(rename = "isFirstChunk")]
    is_first_chunk: bool,
    #[serde(rename = "isLastChunk")]
    is_last_chunk: bool,
}

fn validate_and_build_request(
    client: &reqwest::Client,
    url: &str,
    message: &TunnelMessage,
    tunnel_secret: &str,
    server_secret: &str,
) -> Result<reqwest::RequestBuilder> {
    let incoming_secret = message
        .headers
        .as_ref()
        .and_then(|h| {
            h.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("x-secret-key"))
                .map(|(_, v)| v)
        })
        .ok_or_else(|| anyhow::anyhow!("Missing tunnel secret header"))?;

    if !secure_compare(incoming_secret, tunnel_secret) {
        anyhow::bail!("Invalid tunnel secret");
    }

    let mut request_builder = match message.method.as_str() {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "PATCH" => client.patch(url),
        _ => client.get(url),
    };

    if let Some(headers) = &message.headers {
        for (key, value) in headers {
            if key.eq_ignore_ascii_case("x-secret-key") {
                continue;
            }
            request_builder = request_builder.header(key, value);
        }
    }

    request_builder = request_builder.header("X-Secret-Key", server_secret);

    if let Some(body) = &message.body {
        if message.method != "GET" && message.method != "HEAD" {
            request_builder = request_builder.body(body.clone());
        }
    }

    Ok(request_builder)
}

async fn handle_streaming_response(
    response: reqwest::Response,
    status: u16,
    headers_map: HashMap<String, String>,
    request_id: String,
    message_path: String,
    ws_tx: WebSocketSender,
) -> Result<()> {
    info!("← {} {} [{}] (streaming)", status, message_path, request_id);

    let mut stream = response.bytes_stream();
    let mut chunk_index = 0;
    let mut is_first_chunk = true;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                let chunk_str = String::from_utf8_lossy(&chunk).to_string();
                let tunnel_response = TunnelResponse {
                    request_id: request_id.clone(),
                    status,
                    headers: if is_first_chunk {
                        Some(headers_map.clone())
                    } else {
                        None
                    },
                    body: Some(chunk_str),
                    error: None,
                    chunk_index: Some(chunk_index),
                    total_chunks: None,
                    is_chunked: false,
                    is_streaming: true,
                    is_first_chunk,
                    is_last_chunk: false,
                };
                send_response(ws_tx.clone(), tunnel_response).await?;
                chunk_index += 1;
                is_first_chunk = false;
            }
            Err(e) => {
                error!("Error reading stream chunk: {}", e);
                break;
            }
        }
    }

    let tunnel_response = TunnelResponse {
        request_id: request_id.clone(),
        status,
        headers: None,
        body: Some(String::new()),
        error: None,
        chunk_index: Some(chunk_index),
        total_chunks: None,
        is_chunked: false,
        is_streaming: true,
        is_first_chunk: false,
        is_last_chunk: true,
    };
    send_response(ws_tx, tunnel_response).await?;
    info!(
        "← {} {} [{}] (complete, {} chunks)",
        status, message_path, request_id, chunk_index
    );
    Ok(())
}

async fn handle_chunked_response(
    body: String,
    status: u16,
    headers_map: HashMap<String, String>,
    request_id: String,
    message_path: String,
    ws_tx: WebSocketSender,
) -> Result<()> {
    let total_chunks = body.len().div_ceil(MAX_WS_SIZE);
    info!(
        "← {} {} [{}] ({} bytes, {} chunks)",
        status,
        message_path,
        request_id,
        body.len(),
        total_chunks
    );

    for (i, chunk) in body.as_bytes().chunks(MAX_WS_SIZE).enumerate() {
        let chunk_str = String::from_utf8_lossy(chunk).to_string();
        let tunnel_response = TunnelResponse {
            request_id: request_id.clone(),
            status,
            headers: if i == 0 {
                Some(headers_map.clone())
            } else {
                None
            },
            body: Some(chunk_str),
            error: None,
            chunk_index: Some(i),
            total_chunks: Some(total_chunks),
            is_chunked: true,
            is_streaming: false,
            is_first_chunk: false,
            is_last_chunk: false,
        };
        send_response(ws_tx.clone(), tunnel_response).await?;
    }
    Ok(())
}

async fn handle_request(
    message: TunnelMessage,
    ctx: ProxyContext,
    ws_tx: WebSocketSender,
    scheme: &str,
) -> Result<()> {
    let request_id = message.request_id.clone();
    let client = &ctx.http_client;

    let url = format!("{}://127.0.0.1:{}{}", scheme, ctx.port, message.path);

    let request_builder = match validate_and_build_request(
        client,
        &url,
        &message,
        &ctx.tunnel_secret,
        &ctx.server_secret,
    ) {
        Ok(builder) => builder,
        Err(e) => {
            error!("✗ Authentication error [{}]: {}", request_id, e);
            let error_response = TunnelResponse {
                request_id,
                status: 401,
                headers: None,
                body: None,
                error: Some(e.to_string()),
                chunk_index: None,
                total_chunks: None,
                is_chunked: false,
                is_streaming: false,
                is_first_chunk: false,
                is_last_chunk: false,
            };
            send_response(ws_tx, error_response).await?;
            return Ok(());
        }
    };

    let response = match request_builder.send().await {
        Ok(resp) => resp,
        Err(e) => {
            error!("✗ Request error [{}]: {}", request_id, e);
            let error_response = TunnelResponse {
                request_id,
                status: 500,
                headers: None,
                body: None,
                error: Some(e.to_string()),
                chunk_index: None,
                total_chunks: None,
                is_chunked: false,
                is_streaming: false,
                is_first_chunk: false,
                is_last_chunk: false,
            };
            send_response(ws_tx, error_response).await?;
            return Ok(());
        }
    };

    let status = response.status().as_u16();
    // Normalize header names to lowercase per RFC 7230 (HTTP headers are case-insensitive)
    let headers_map: HashMap<String, String> = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_lowercase(),
                v.to_str().unwrap_or("").to_string(),
            )
        })
        .collect();

    let is_streaming = headers_map
        .get("content-type")
        .map(|ct| ct.contains("text/event-stream"))
        .unwrap_or(false);

    if is_streaming {
        handle_streaming_response(
            response,
            status,
            headers_map,
            request_id,
            message.path,
            ws_tx,
        )
        .await?;
    } else {
        let body = response.text().await.unwrap_or_default();

        if body.len() > MAX_WS_SIZE {
            handle_chunked_response(body, status, headers_map, request_id, message.path, ws_tx)
                .await?;
        } else {
            let tunnel_response = TunnelResponse {
                request_id: request_id.clone(),
                status,
                headers: Some(headers_map),
                body: Some(body),
                error: None,
                chunk_index: None,
                total_chunks: None,
                is_chunked: false,
                is_streaming: false,
                is_first_chunk: false,
                is_last_chunk: false,
            };
            send_response(ws_tx, tunnel_response).await?;
        }
    }

    Ok(())
}

async fn send_response(ws_tx: WebSocketSender, response: TunnelResponse) -> Result<()> {
    let json = serde_json::to_string(&response)?;
    if let Some(tx) = ws_tx.write().await.as_mut() {
        tx.send(Message::Text(json.into()))
            .await
            .context("Failed to send response")?;
    }
    Ok(())
}

fn configure_tcp_keepalive(
    stream: &tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    let tcp_stream = stream.get_ref().get_ref();
    let socket_ref = SockRef::from(tcp_stream);

    let keepalive = TcpKeepalive::new()
        .with_time(Duration::from_secs(30))
        .with_interval(Duration::from_secs(30));

    if let Err(e) = socket_ref.set_tcp_keepalive(&keepalive) {
        warn!("Failed to set TCP keep-alive: {}", e);
    } else {
        info!("✓ TCP keep-alive enabled (30s interval)");
    }
}

async fn handle_websocket_messages(
    mut read: futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    ws_tx: WebSocketSender,
    ctx: ProxyContext,
    last_activity: Arc<RwLock<Instant>>,
    active_tasks: Arc<RwLock<Vec<JoinHandle<()>>>>,
    scheme: String,
) {
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                *last_activity.write().await = Instant::now();

                match serde_json::from_str::<TunnelMessage>(&text) {
                    Ok(tunnel_msg) => {
                        let ws_tx_clone = ws_tx.clone();
                        let ctx_clone = ctx.clone();
                        let scheme_clone = scheme.clone();
                        let task = tokio::spawn(async move {
                            if let Err(e) =
                                handle_request(tunnel_msg, ctx_clone, ws_tx_clone, &scheme_clone)
                                    .await
                            {
                                error!("Error handling request: {}", e);
                            }
                        });
                        {
                            let mut tasks = active_tasks.write().await;
                            tasks.retain(|t| !t.is_finished());
                            tasks.push(task);
                        }
                    }
                    Err(e) => {
                        error!("Error parsing tunnel message: {}", e);
                    }
                }
            }
            Ok(Message::Close(_)) => {
                info!("✗ Connection closed by server");
                break;
            }
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {
                *last_activity.write().await = Instant::now();
            }
            Err(e) => {
                error!("✗ WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }
}

async fn cleanup_connection(
    ws_tx: WebSocketSender,
    active_tasks: Arc<RwLock<Vec<JoinHandle<()>>>>,
) {
    if let Some(mut tx) = ws_tx.write().await.take() {
        let _ = tx.close().await;
    }

    let tasks = active_tasks.write().await.drain(..).collect::<Vec<_>>();
    info!("Aborting {} active request tasks", tasks.len());
    for task in tasks {
        task.abort();
    }
}

async fn run_single_connection(
    port: u16,
    agent_id: String,
    tunnel_secret: String,
    server_secret: String,
    scheme: String,
    restart_tx: mpsc::Sender<()>,
) {
    #[cfg(feature = "rustls-tls")]
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let worker_url = get_worker_url();
    let ws_url = worker_url
        .replace("https://", "wss://")
        .replace("http://", "ws://");

    let url = format!("{}/connect?agent_id={}", ws_url, agent_id);

    info!("Connecting to {}...", url);

    let ws_stream = match tokio::time::timeout(
        Duration::from_secs(CONNECTION_TIMEOUT_SECS),
        connect_async(url.clone()),
    )
    .await
    {
        Ok(Ok((stream, _))) => {
            configure_tcp_keepalive(&stream);
            stream
        }
        Ok(Err(e)) => {
            error!("✗ WebSocket connection error: {}", e);
            let _ = restart_tx.send(()).await;
            return;
        }
        Err(_) => {
            error!(
                "✗ WebSocket connection timeout after {}s",
                CONNECTION_TIMEOUT_SECS
            );
            let _ = restart_tx.send(()).await;
            return;
        }
    };

    info!("✓ Connected as agent: {}", agent_id);
    info!("✓ Proxying to: {}://127.0.0.1:{}", scheme, port);
    let public_url = format!("{}/tunnel/{}", worker_url, agent_id);
    info!("✓ Public URL: {}", public_url);

    let mut client_builder = reqwest::Client::builder();
    if scheme == "https" {
        client_builder = client_builder.danger_accept_invalid_certs(true);
    }
    let http_client = client_builder
        .build()
        .expect("failed to build reqwest client");

    let ctx = ProxyContext {
        port,
        tunnel_secret,
        server_secret,
        http_client,
    };

    let (write, read) = ws_stream.split();
    let ws_tx: WebSocketSender = Arc::new(RwLock::new(Some(write)));
    let last_activity = Arc::new(RwLock::new(Instant::now()));
    let active_tasks: Arc<RwLock<Vec<JoinHandle<()>>>> = Arc::new(RwLock::new(Vec::new()));

    let last_activity_clone = last_activity.clone();
    let idle_task = async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let elapsed = last_activity_clone.read().await.elapsed();
            if elapsed > Duration::from_secs(IDLE_TIMEOUT_SECS) {
                warn!(
                    "No activity for {} minutes, forcing reconnect",
                    IDLE_TIMEOUT_SECS / 60
                );
                break;
            }
        }
    };

    tokio::select! {
        _ = idle_task => {
            info!("✗ Idle timeout triggered");
        }
        _ = handle_websocket_messages(
            read,
            ws_tx.clone(),
            ctx,
            last_activity,
            active_tasks.clone(),
            scheme,
        ) => {
            info!("✗ Connection ended");
        }
    }

    cleanup_connection(ws_tx, active_tasks).await;

    let _ = restart_tx.send(()).await;
}

pub async fn start(
    port: u16,
    tunnel_secret: String,
    server_secret: String,
    agent_id: String,
    scheme: &str,
    handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    restart_tx: mpsc::Sender<()>,
) -> Result<TunnelInfo> {
    let worker_url = get_worker_url();

    let agent_id_clone = agent_id.clone();
    let tunnel_secret_clone = tunnel_secret.clone();
    let server_secret_clone = server_secret;
    let scheme = scheme.to_string();

    let task = tokio::spawn(async move {
        run_single_connection(
            port,
            agent_id_clone,
            tunnel_secret_clone,
            server_secret_clone,
            scheme,
            restart_tx,
        )
        .await;
    });

    *handle.write().await = Some(task);

    let public_url = format!("{}/tunnel/{}", worker_url, agent_id);
    let hostname = Url::parse(&worker_url)?
        .host_str()
        .unwrap_or("")
        .to_string();

    Ok(TunnelInfo {
        state: super::TunnelState::Running,
        url: public_url,
        hostname,
        secret: tunnel_secret,
    })
}

pub async fn stop(handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>) {
    if let Some(task) = handle.write().await.take() {
        task.abort();
        info!("Lapstone tunnel stopped");
    }
}
