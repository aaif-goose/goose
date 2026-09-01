use anyhow::{anyhow, Result};
use async_trait::async_trait;
use goose_providers::live::{
    LiveProtocol, LiveSession, LiveSessionEnd, LiveSessionEvent, LiveTransport,
};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::{mpsc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq)]
enum TestEvent {
    Message(String),
    Closed,
}

struct TestProtocol {
    graceful: bool,
}

impl LiveProtocol for TestProtocol {
    type Command = String;
    type Event = TestEvent;

    fn encode(&self, command: Self::Command) -> Result<Value> {
        Ok(json!({ "text": command }))
    }

    fn decode(&self, message: Value) -> Result<Self::Event> {
        let text = message
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing text"))?;
        Ok(if text == "closed" {
            TestEvent::Closed
        } else {
            TestEvent::Message(text.to_owned())
        })
    }

    fn close_command(&self) -> Option<Self::Command> {
        self.graceful.then(|| "close".to_string())
    }

    fn is_close_acknowledgement(&self, event: &Self::Event) -> bool {
        matches!(event, TestEvent::Closed)
    }
}

struct TestTransport {
    incoming: Mutex<mpsc::Receiver<Result<Option<Value>>>>,
    sent: mpsc::UnboundedSender<Value>,
    closes: Arc<AtomicUsize>,
}

/// Test-side handles for driving and observing a `TestTransport`.
struct Controls {
    incoming: mpsc::Sender<Result<Option<Value>>>,
    sent: mpsc::UnboundedReceiver<Value>,
    closes: Arc<AtomicUsize>,
}

impl Controls {
    fn close_count(&self) -> usize {
        self.closes.load(Ordering::SeqCst)
    }
}

impl TestTransport {
    fn new() -> (Arc<Self>, Controls) {
        let (incoming_tx, incoming_rx) = mpsc::channel(16);
        let (sent_tx, sent_rx) = mpsc::unbounded_channel();
        let closes = Arc::new(AtomicUsize::new(0));
        let transport = Arc::new(Self {
            incoming: Mutex::new(incoming_rx),
            sent: sent_tx,
            closes: closes.clone(),
        });
        let controls = Controls {
            incoming: incoming_tx,
            sent: sent_rx,
            closes,
        };
        (transport, controls)
    }
}

#[async_trait]
impl LiveTransport for TestTransport {
    async fn send(&self, message: Value) -> Result<()> {
        self.sent.send(message)?;
        Ok(())
    }

    async fn receive(&self) -> Result<Option<Value>> {
        match self.incoming.lock().await.recv().await {
            Some(result) => result,
            None => Ok(None),
        }
    }

    async fn close(&self) -> Result<()> {
        self.closes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn session(graceful: bool) -> (Arc<LiveSession<TestProtocol>>, Controls) {
    let (transport, controls) = TestTransport::new();
    let session = LiveSession::connect(Arc::new(TestProtocol { graceful }), transport);
    (session, controls)
}

#[tokio::test]
async fn transport_failure_is_reported_not_swallowed() {
    let (session, controls) = session(true);
    let mut events = session.subscribe();

    controls
        .incoming
        .send(Err(anyhow!("socket exploded")))
        .await
        .unwrap();

    match events.recv().await.unwrap() {
        LiveSessionEvent::Ended { reason, error } => {
            assert_eq!(reason, LiveSessionEnd::TransportFailed);
            assert_eq!(error.unwrap().to_string(), "socket exploded");
        }
        other => panic!("expected Ended, got {other:?}"),
    }
    assert_eq!(session.end_reason(), Some(LiveSessionEnd::TransportFailed));
}

#[tokio::test]
async fn decode_failure_is_reported_not_swallowed() {
    let (session, controls) = session(true);
    let mut events = session.subscribe();

    controls
        .incoming
        .send(Ok(Some(json!({ "nope": 1 }))))
        .await
        .unwrap();

    match events.recv().await.unwrap() {
        LiveSessionEvent::Ended { reason, error } => {
            assert_eq!(reason, LiveSessionEnd::DecodeFailed);
            assert!(error.is_some());
        }
        other => panic!("expected Ended, got {other:?}"),
    }
}

#[tokio::test]
async fn clean_transport_shutdown_reports_closed_without_error() {
    let (session, controls) = session(true);
    let mut events = session.subscribe();

    controls.incoming.send(Ok(None)).await.unwrap();

    match events.recv().await.unwrap() {
        LiveSessionEvent::Ended { reason, error } => {
            assert_eq!(reason, LiveSessionEnd::Closed);
            assert!(error.is_none());
        }
        other => panic!("expected Ended, got {other:?}"),
    }
}

#[tokio::test]
async fn close_sends_protocol_close_command_then_closes_transport() {
    let (session, mut controls) = session(true);

    let incoming = controls.incoming.clone();
    let acknowledge = tokio::spawn(async move {
        incoming
            .send(Ok(Some(json!({ "text": "closed" }))))
            .await
            .unwrap();
    });

    session.close().await.unwrap();
    acknowledge.await.unwrap();

    assert_eq!(
        controls.sent.recv().await.unwrap(),
        json!({ "text": "close" })
    );
    assert_eq!(controls.close_count(), 1);
    assert_eq!(session.end_reason(), Some(LiveSessionEnd::Closed));
}

#[tokio::test]
async fn close_without_protocol_command_only_closes_transport() {
    let (session, mut controls) = session(false);

    session.close().await.unwrap();

    assert!(controls.sent.try_recv().is_err());
    assert_eq!(controls.close_count(), 1);
}

#[tokio::test]
async fn close_is_idempotent() {
    let (session, controls) = session(false);

    session.close().await.unwrap();
    session.close().await.unwrap();

    assert_eq!(controls.close_count(), 1);
    assert_eq!(session.end_reason(), Some(LiveSessionEnd::Closed));
}

#[tokio::test]
async fn concurrent_close_has_one_owner() {
    let (session, mut controls) = session(true);
    let first = tokio::spawn({
        let session = session.clone();
        async move { session.close().await }
    });
    let second = tokio::spawn({
        let session = session.clone();
        async move { session.close().await }
    });

    assert_eq!(
        controls.sent.recv().await.unwrap(),
        json!({ "text": "close" })
    );
    assert!(controls.sent.try_recv().is_err());
    controls
        .incoming
        .send(Ok(Some(json!({ "text": "closed" }))))
        .await
        .unwrap();

    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();
    assert_eq!(controls.close_count(), 1);
}

#[tokio::test]
async fn send_after_end_fails() {
    let (session, controls) = session(true);
    let mut events = session.subscribe();

    controls.incoming.send(Ok(None)).await.unwrap();
    events.recv().await.unwrap();

    assert!(session.send("hello".into()).await.is_err());
}

#[tokio::test]
async fn messages_are_delivered_before_end() {
    let (session, controls) = session(true);
    let mut events = session.subscribe();

    controls
        .incoming
        .send(Ok(Some(json!({ "text": "hi" }))))
        .await
        .unwrap();
    controls.incoming.send(Ok(None)).await.unwrap();

    assert!(matches!(
        events.recv().await.unwrap(),
        LiveSessionEvent::Message(TestEvent::Message(text)) if text == "hi"
    ));
    assert!(matches!(
        events.recv().await.unwrap(),
        LiveSessionEvent::Ended { .. }
    ));
}

#[tokio::test]
async fn dropping_session_stops_receive_task() {
    let (transport, controls) = TestTransport::new();
    let session = LiveSession::connect(Arc::new(TestProtocol { graceful: true }), transport);
    let mut events = session.subscribe();
    drop(session);

    assert!(controls
        .incoming
        .send(Ok(Some(json!({ "text": "hi" }))))
        .await
        .is_ok());
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(events.try_recv().is_err());
}
