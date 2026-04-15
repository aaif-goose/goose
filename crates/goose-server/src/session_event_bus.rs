use crate::routes::reply::MessageEvent;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{broadcast, Mutex};
use tokio_util::sync::CancellationToken;

const BROADCAST_CAPACITY: usize = 256;
const REPLAY_BUFFER_CAPACITY: usize = 512;

/// Error returned by [`SessionEventBus::subscribe`].
#[derive(Debug)]
pub enum SubscribeError {
    /// The client's `Last-Event-ID` has been evicted from the replay buffer,
    /// so events have been irrecoverably lost.
    ClientTooFarBehind,
}

#[derive(Clone, Debug)]
pub struct SessionEvent {
    /// Monotonic sequence number, written as SSE `id:` frame (not in JSON payload).
    pub seq: u64,
    /// None for Ping events, Some for events associated with a specific request.
    pub request_id: Option<String>,
    /// The event payload.
    pub event: MessageEvent,
}

pub struct SessionEventBus {
    tx: broadcast::Sender<SessionEvent>,
    buffer: Mutex<VecDeque<SessionEvent>>,
    next_seq: AtomicU64,
    active_requests: Mutex<HashMap<String, CancellationToken>>,
}

impl SessionEventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            tx,
            buffer: Mutex::new(VecDeque::with_capacity(REPLAY_BUFFER_CAPACITY)),
            next_seq: AtomicU64::new(1),
            active_requests: Mutex::new(HashMap::new()),
        }
    }

    /// Publish an event to the bus. Assigns a monotonic sequence number.
    ///
    /// The sequence ID is assigned under the buffer lock so that concurrent
    /// callers cannot reorder events (i.e. seq=2 published before seq=1).
    pub async fn publish(&self, request_id: Option<String>, event: MessageEvent) -> u64 {
        let session_event = {
            let mut buf = self.buffer.lock().await;
            let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
            let session_event = SessionEvent {
                seq,
                request_id,
                event,
            };
            buf.push_back(session_event.clone());
            while buf.len() > REPLAY_BUFFER_CAPACITY {
                buf.pop_front();
            }
            session_event
        };

        // Send on broadcast channel (ignore error if no subscribers)
        let _ = self.tx.send(session_event.clone());

        session_event.seq
    }

    /// Subscribe to live events. If `last_event_id` is provided, replay buffered
    /// events with seq > last_event_id. Returns (replay_events, replay_max_seq, live_receiver).
    ///
    /// Returns `Err(SubscribeError::ClientTooFarBehind)` when `last_event_id`
    /// refers to an event that has already been evicted from the replay buffer,
    /// meaning the client has irrecoverably missed events.
    ///
    /// The live receiver is created *before* snapshotting the buffer so that
    /// no event can fall into the gap between the two steps. The caller must
    /// skip live events with `seq <= replay_max_seq` to deduplicate.
    pub async fn subscribe(
        &self,
        last_event_id: Option<u64>,
    ) -> Result<(Vec<SessionEvent>, u64, broadcast::Receiver<SessionEvent>), SubscribeError> {
        // Subscribe first so that any event published while we hold the
        // buffer lock is guaranteed to appear in `rx` (possibly duplicating
        // a replay entry). The caller deduplicates via replay_max_seq.
        let rx = self.tx.subscribe();

        let (replay, replay_max_seq) = {
            let buf = self.buffer.lock().await;
            let buf_max = buf.back().map(|e| e.seq).unwrap_or(0);
            let buf_min = buf.front().map(|e| e.seq).unwrap_or(0);
            let last_id = last_event_id.unwrap_or(0);

            // If the client sent a Last-Event-ID that has been evicted from
            // the buffer, they have irrecoverably missed events.
            if last_id > 0 && buf_min > 0 && last_id < buf_min {
                return Err(SubscribeError::ClientTooFarBehind);
            }

            // Clamp to the actual buffer max so a stale Last-Event-ID
            // (e.g. from before a server restart) doesn't suppress live events.
            let events: Vec<_> = buf.iter().filter(|e| e.seq > last_id).cloned().collect();
            let max_seq = events.last().map(|e| e.seq).unwrap_or(last_id.min(buf_max));
            (events, max_seq)
        };

        Ok((replay, replay_max_seq, rx))
    }

    /// Return the IDs of all currently active (in-flight) requests.
    pub async fn active_request_ids(&self) -> Vec<String> {
        let requests = self.active_requests.lock().await;
        requests.keys().cloned().collect()
    }

    #[cfg(test)]
    pub async fn register_request(&self, request_id: String) -> CancellationToken {
        let token = CancellationToken::new();
        let mut requests = self.active_requests.lock().await;
        requests.insert(request_id, token.clone());
        token
    }

    /// Atomically check no requests are active and register one. Returns Err if busy.
    pub async fn try_register_request(
        &self,
        request_id: String,
    ) -> Result<CancellationToken, String> {
        let mut requests = self.active_requests.lock().await;
        requests.retain(|_id, token| !token.is_cancelled());
        if !requests.is_empty() {
            return Err("Session already has an active request".into());
        }
        let token = CancellationToken::new();
        requests.insert(request_id, token.clone());
        Ok(token)
    }

    /// Cancel a specific request by request_id.
    pub async fn cancel_request(&self, request_id: &str) -> bool {
        let requests = self.active_requests.lock().await;
        if let Some(token) = requests.get(request_id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    /// Cancel all active requests (e.g. when deleting a session).
    pub async fn cancel_all_requests(&self) {
        let requests = self.active_requests.lock().await;
        for token in requests.values() {
            token.cancel();
        }
    }

    /// Remove the cancellation token for a completed request.
    pub async fn cleanup_request(&self, request_id: &str) {
        let mut requests = self.active_requests.lock().await;
        requests.remove(request_id);
    }
}

impl Default for SessionEventBus {
    fn default() -> Self {
        Self::new()
    }
}

pub struct RequestGuard {
    bus: std::sync::Arc<SessionEventBus>,
    request_id: String,
    disarmed: bool,
}

impl RequestGuard {
    pub fn new(bus: std::sync::Arc<SessionEventBus>, request_id: String) -> Self {
        Self {
            bus,
            request_id,
            disarmed: false,
        }
    }

    pub fn disarm(&mut self) {
        self.disarmed = true;
    }
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        if !self.disarmed {
            let bus = self.bus.clone();
            let request_id = self.request_id.clone();
            tokio::spawn(async move {
                bus.cleanup_request(&request_id).await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose::conversation::message::TokenState;

    #[tokio::test]
    async fn test_publish_and_subscribe() {
        let bus = SessionEventBus::new();

        // Publish some events
        bus.publish(Some("req-1".to_string()), MessageEvent::Ping)
            .await;
        bus.publish(
            Some("req-1".to_string()),
            MessageEvent::Finish {
                reason: "stop".to_string(),
                token_state: TokenState::default(),
            },
        )
        .await;

        // Subscribe with replay
        let (replay, replay_max_seq, _rx) = bus.subscribe(Some(0)).await.unwrap();
        assert_eq!(replay.len(), 2);
        assert_eq!(replay[0].seq, 1);
        assert_eq!(replay[1].seq, 2);
        assert_eq!(replay_max_seq, 2);
    }

    #[tokio::test]
    async fn test_subscribe_with_last_event_id() {
        let bus = SessionEventBus::new();

        bus.publish(None, MessageEvent::Ping).await;
        bus.publish(None, MessageEvent::Ping).await;
        bus.publish(None, MessageEvent::Ping).await;

        // Only get events after seq 2
        let (replay, replay_max_seq, _rx) = bus.subscribe(Some(2)).await.unwrap();
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].seq, 3);
        assert_eq!(replay_max_seq, 3);
    }

    #[tokio::test]
    async fn test_subscribe_without_last_event_id_replays_all() {
        let bus = SessionEventBus::new();

        bus.publish(None, MessageEvent::Ping).await;
        bus.publish(None, MessageEvent::Ping).await;

        // First connect (no Last-Event-ID) should replay all buffered events
        let (replay, replay_max_seq, _rx) = bus.subscribe(None).await.unwrap();
        assert_eq!(replay.len(), 2);
        assert_eq!(replay_max_seq, 2);
    }

    #[tokio::test]
    async fn test_subscribe_with_stale_last_event_id() {
        let bus = SessionEventBus::new();

        // Buffer has seq 1..3, but client sends Last-Event-ID: 9999
        bus.publish(None, MessageEvent::Ping).await;
        bus.publish(None, MessageEvent::Ping).await;
        bus.publish(None, MessageEvent::Ping).await;

        let (replay, replay_max_seq, _rx) = bus.subscribe(Some(9999)).await.unwrap();
        // No replay events (all are below 9999)
        assert_eq!(replay.len(), 0);
        // replay_max_seq should be clamped to buf_max (3), not 9999
        assert_eq!(replay_max_seq, 3);
    }

    #[tokio::test]
    async fn test_cancel_request() {
        let bus = SessionEventBus::new();

        let token = bus.register_request("req-1".to_string()).await;
        assert!(!token.is_cancelled());

        let cancelled = bus.cancel_request("req-1").await;
        assert!(cancelled);
        assert!(token.is_cancelled());

        // Non-existent request
        let cancelled = bus.cancel_request("req-999").await;
        assert!(!cancelled);
    }

    #[tokio::test]
    async fn test_cleanup_request() {
        let bus = SessionEventBus::new();

        bus.register_request("req-1".to_string()).await;
        bus.cleanup_request("req-1").await;

        // Should return false since it was cleaned up
        let cancelled = bus.cancel_request("req-1").await;
        assert!(!cancelled);
    }
}
