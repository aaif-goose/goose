//! Shared adapter classes for converting mpsc channels to AsyncRead/AsyncWrite streams
//! Used by both HTTP and WebSocket transports

use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::mpsc;

/// Converts an mpsc::Receiver<String> to AsyncRead
/// Each message is terminated with a newline for JSON-RPC framing
pub(crate) struct ReceiverToAsyncRead {
    rx: mpsc::Receiver<String>,
    buffer: Vec<u8>,
    pos: usize,
}

impl ReceiverToAsyncRead {
    pub(crate) fn new(rx: mpsc::Receiver<String>) -> Self {
        Self {
            rx,
            buffer: Vec::new(),
            pos: 0,
        }
    }
}

impl tokio::io::AsyncRead for ReceiverToAsyncRead {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.pos < self.buffer.len() {
            let remaining = &self.buffer[self.pos..];
            let to_copy = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..to_copy]);
            self.pos += to_copy;
            if self.pos >= self.buffer.len() {
                self.buffer.clear();
                self.pos = 0;
            }
            return Poll::Ready(Ok(()));
        }

        match Pin::new(&mut self.rx).poll_recv(cx) {
            Poll::Ready(Some(msg)) => {
                let bytes = format!("{}\n", msg).into_bytes();
                let to_copy = bytes.len().min(buf.remaining());
                buf.put_slice(&bytes[..to_copy]);
                if to_copy < bytes.len() {
                    self.buffer = bytes[to_copy..].to_vec();
                    self.pos = 0;
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => Poll::Ready(Ok(())),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Converts an unbounded mpsc::Sender<String> to AsyncWrite.
/// Splits incoming data on newlines for JSON-RPC framing.
///
/// Uses an unbounded sender so that bursts of outgoing messages (e.g. replaying
/// a long session history) are never silently dropped due to backpressure.
pub(crate) struct SenderToAsyncWrite {
    tx: mpsc::UnboundedSender<String>,
    buffer: Vec<u8>,
}

impl SenderToAsyncWrite {
    pub(crate) fn new(tx: mpsc::UnboundedSender<String>) -> Self {
        Self {
            tx,
            buffer: Vec::new(),
        }
    }
}

impl tokio::io::AsyncWrite for SenderToAsyncWrite {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.buffer.extend_from_slice(buf);

        while let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
            let line = String::from_utf8_lossy(&self.buffer[..pos]).to_string();
            self.buffer.drain(..=pos);

            if !line.is_empty() && self.tx.send(line).is_err() {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "Channel closed",
                )));
            }
        }

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
