//! Test harness for exercising SSE streaming over a raw TCP server, where
//! keepalive comment frames can be replayed exactly as a wedged provider
//! sends them (which mock HTTP servers cannot express).

use std::net::SocketAddr;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_stream::StreamExt;

use crate::base::MessageStream;
use crate::errors::ProviderError;

/// A scripted SSE chunk: wait `after`, then write `body` as one HTTP chunk.
#[derive(Clone)]
pub(crate) struct Chunk {
    pub(crate) after: Duration,
    pub(crate) body: String,
}

impl Chunk {
    pub(crate) fn immediate(body: impl Into<String>) -> Self {
        Self {
            after: Duration::ZERO,
            body: body.into(),
        }
    }
}

/// What the server does after the scripted chunks.
#[derive(Clone, Copy)]
pub(crate) enum Tail {
    /// Emit `: ping` frames forever without terminating the response: the
    /// connection stays byte-alive while no data ever arrives.
    KeepaliveForever,
    /// Emit payload-bearing keepalive events forever without terminating the
    /// response: byte-alive like `KeepaliveForever`, but with `data:` frames
    /// a naive watchdog would count as progress.
    KeepaliveDataForever(&'static str),
    /// Terminate the chunked body and close the connection.
    Close,
}

pub(crate) async fn spawn_server(script: Vec<Chunk>, tail: Tail) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((sock, _)) = listener.accept().await {
            let script = script.clone();
            tokio::spawn(serve(sock, script, tail));
        }
    });
    addr
}

async fn serve(mut sock: TcpStream, script: Vec<Chunk>, tail: Tail) {
    let mut buf = [0u8; 8192];
    let _ = sock.read(&mut buf).await;
    let headers = concat!(
        "HTTP/1.1 200 OK\r\n",
        "content-type: text/event-stream\r\n",
        "transfer-encoding: chunked\r\n\r\n"
    );
    if sock.write_all(headers.as_bytes()).await.is_err() {
        return;
    }
    let chunk = |data: &str| format!("{:x}\r\n{data}\r\n", data.len());
    for step in script {
        tokio::time::sleep(step.after).await;
        if sock.write_all(chunk(&step.body).as_bytes()).await.is_err() {
            return;
        }
    }
    match tail {
        Tail::KeepaliveForever => loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if sock
                .write_all(chunk(": ping\n\n").as_bytes())
                .await
                .is_err()
            {
                return;
            }
        },
        Tail::KeepaliveDataForever(frame) => loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if sock.write_all(chunk(frame).as_bytes()).await.is_err() {
                return;
            }
        },
        Tail::Close => {
            let _ = sock.write_all(b"0\r\n\r\n").await;
        }
    }
}

pub(crate) async fn post_stream(addr: SocketAddr, path: &str, body: Value) -> reqwest::Response {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .post(format!("http://{addr}{path}"))
        .json(&body)
        .send()
        .await
        .unwrap()
}

/// Drains `stream` within `limit`, returning the item count and the error the
/// stream terminated with, if any.
pub(crate) async fn drain_within(
    mut stream: MessageStream,
    limit: Duration,
) -> (usize, Option<ProviderError>) {
    tokio::time::timeout(limit, async {
        let mut items = 0;
        while let Some(item) = stream.next().await {
            match item {
                Ok(_) => items += 1,
                Err(e) => return (items, Some(e)),
            }
        }
        (items, None)
    })
    .await
    .expect("stream did not terminate within the deadline")
}
