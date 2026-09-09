use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex, OwnedMutexGuard, RwLock};
use tokio::time::timeout;
use tracing::warn;
use uuid::Uuid;

use crate::conversation::message::{Message, MessageContent};

const ACTION_REQUIRED_STREAM_CAPACITY: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ElicitationOutcome {
    Accept(Value),
    Decline,
    Cancel,
}

struct PendingRequest {
    session_id: String,
    response_tx: Option<tokio::sync::oneshot::Sender<ElicitationOutcome>>,
}

pub(crate) struct PendingResponseClaim {
    request_id: String,
    pending: OwnedMutexGuard<PendingRequest>,
}

impl PendingResponseClaim {
    pub(crate) fn submit(mut self, response: ElicitationOutcome) -> Result<()> {
        let tx = self
            .pending
            .response_tx
            .take()
            .ok_or_else(|| anyhow::anyhow!("Request already completed: {}", self.request_id))?;
        drop(self.pending);

        if tx.send(response).is_err() {
            return Err(anyhow::anyhow!("Response channel closed"));
        }

        Ok(())
    }
}

pub(crate) struct ActionRequiredManager {
    pending: Arc<RwLock<HashMap<String, Arc<Mutex<PendingRequest>>>>>,
    action_required_senders: Mutex<HashMap<(String, String), mpsc::Sender<Message>>>,
}

impl ActionRequiredManager {
    pub(crate) fn new() -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            action_required_senders: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) async fn request_and_wait(
        &self,
        session_id: String,
        tool_call_request_id: String,
        message: String,
        schema: Value,
        timeout_duration: Duration,
    ) -> Result<ElicitationOutcome> {
        let sender = self
            .action_required_senders
            .lock()
            .await
            .get(&(session_id.clone(), tool_call_request_id.clone()))
            .cloned();

        let Some(sender) = sender else {
            return Err(anyhow::anyhow!(
                "Tool call request not found for elicitation: {}",
                tool_call_request_id
            ));
        };

        let id = Uuid::new_v4().to_string();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let pending_request = PendingRequest {
            session_id: session_id.clone(),
            response_tx: Some(tx),
        };
        let pending_request = Arc::new(Mutex::new(pending_request));

        self.pending
            .write()
            .await
            .insert(id.clone(), Arc::clone(&pending_request));

        let action_required_message = Message::assistant().with_content(
            MessageContent::action_required_elicitation(id.clone(), message, schema),
        );
        if let Err(error) = sender.try_send(action_required_message) {
            self.pending.write().await.remove(&id);
            let message = match error {
                mpsc::error::TrySendError::Full(_) => "stream is full",
                mpsc::error::TrySendError::Closed(_) => "stream closed",
            };
            return Err(anyhow::anyhow!(
                "Tool call action-required {message}: {tool_call_request_id}"
            ));
        }

        let result = self
            .wait_for_response(&id, pending_request, rx, timeout_duration)
            .await;

        self.pending.write().await.remove(&id);

        result
    }

    pub(crate) async fn claim_response(
        &self,
        session_id: &str,
        request_id: &str,
    ) -> Result<PendingResponseClaim> {
        let pending_arc = self.pending_request(request_id).await?;
        let mut pending = pending_arc.lock_owned().await;

        if pending.session_id != session_id {
            return Err(anyhow::anyhow!(
                "Request {} belongs to session {}, not {}",
                request_id,
                pending.session_id,
                session_id
            ));
        }

        let tx = pending
            .response_tx
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Request already completed: {}", request_id))?;
        if tx.is_closed() {
            pending.response_tx.take();
            return Err(anyhow::anyhow!("Response channel closed"));
        }

        Ok(PendingResponseClaim {
            request_id: request_id.to_string(),
            pending,
        })
    }

    async fn pending_request(&self, request_id: &str) -> Result<Arc<Mutex<PendingRequest>>> {
        let pending = self.pending.read().await;
        pending
            .get(request_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Request not found: {}", request_id))
    }

    async fn wait_for_response(
        &self,
        request_id: &str,
        pending_request: Arc<Mutex<PendingRequest>>,
        mut rx: tokio::sync::oneshot::Receiver<ElicitationOutcome>,
        timeout_duration: Duration,
    ) -> Result<ElicitationOutcome> {
        match timeout(timeout_duration, &mut rx).await {
            Ok(response) => Self::finish_waiting(request_id, response),
            Err(_) => {
                let mut pending = pending_request.lock().await;
                if pending.response_tx.is_some() {
                    pending.response_tx.take();
                    warn!("Timeout waiting for response: {}", request_id);
                    return Err(anyhow::anyhow!("Timeout waiting for user response"));
                }
                drop(pending);

                Self::finish_waiting(request_id, rx.await)
            }
        }
    }

    fn finish_waiting(
        request_id: &str,
        response: Result<ElicitationOutcome, tokio::sync::oneshot::error::RecvError>,
    ) -> Result<ElicitationOutcome> {
        match response {
            Ok(user_data) => Ok(user_data),
            Err(_) => {
                warn!("Response channel closed for request: {}", request_id);
                Err(anyhow::anyhow!("Response channel closed"))
            }
        }
    }

    pub(crate) async fn register_action_required_stream(
        &self,
        session_id: String,
        tool_call_request_id: String,
    ) -> mpsc::Receiver<Message> {
        let (tx, rx) = mpsc::channel(ACTION_REQUIRED_STREAM_CAPACITY);
        self.action_required_senders
            .lock()
            .await
            .insert((session_id, tool_call_request_id), tx);
        rx
    }

    pub(crate) async fn has_action_required_stream(
        &self,
        session_id: &str,
        tool_call_request_id: &str,
    ) -> bool {
        self.action_required_senders
            .lock()
            .await
            .contains_key(&(session_id.to_string(), tool_call_request_id.to_string()))
    }

    pub(crate) async fn unregister_action_required_stream(
        &self,
        session_id: &str,
        tool_call_request_id: &str,
    ) {
        self.action_required_senders
            .lock()
            .await
            .remove(&(session_id.to_string(), tool_call_request_id.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::ActionRequiredData;
    use serde_json::json;

    fn elicitation_id(message: &Message) -> String {
        match &message.content[0] {
            MessageContent::ActionRequired(action_required) => match &action_required.data {
                ActionRequiredData::Elicitation { id, .. } => id.clone(),
                _ => panic!("expected elicitation action-required message"),
            },
            _ => panic!("expected action-required message"),
        }
    }

    async fn recv_elicitation_message(rx: &mut mpsc::Receiver<Message>) -> Message {
        tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out waiting for elicitation message")
            .expect("action-required stream closed")
    }

    #[tokio::test]
    async fn wrong_session_does_not_consume_pending_response() {
        let manager = Arc::new(ActionRequiredManager::new());
        let mut action_required_rx = manager
            .register_action_required_stream("session-a".to_string(), "tool-call-a".to_string())
            .await;
        let waiter = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager
                    .request_and_wait(
                        "session-a".to_string(),
                        "tool-call-a".to_string(),
                        "Need input".to_string(),
                        json!({ "type": "object" }),
                        Duration::from_secs(5),
                    )
                    .await
            })
        };

        let message = recv_elicitation_message(&mut action_required_rx).await;
        let request_id = elicitation_id(&message);

        let err = match manager.claim_response("session-b", &request_id).await {
            Ok(_) => panic!("wrong session should not claim pending response"),
            Err(error) => error,
        };
        assert!(err.to_string().contains("belongs to session session-a"));

        manager
            .claim_response("session-a", &request_id)
            .await
            .unwrap()
            .submit(ElicitationOutcome::Accept(json!({ "answer": "right" })))
            .unwrap();

        let response = waiter.await.unwrap().unwrap();
        assert_eq!(
            response,
            ElicitationOutcome::Accept(json!({ "answer": "right" }))
        );
    }

    #[tokio::test]
    async fn streams_only_requested_tool_call() {
        let manager = Arc::new(ActionRequiredManager::new());
        let mut stream_a = manager
            .register_action_required_stream("session-a".to_string(), "tool-call-a".to_string())
            .await;
        let mut stream_b = manager
            .register_action_required_stream("session-b".to_string(), "tool-call-b".to_string())
            .await;
        let waiter_a = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager
                    .request_and_wait(
                        "session-a".to_string(),
                        "tool-call-a".to_string(),
                        "Need input A".to_string(),
                        json!({ "type": "object" }),
                        Duration::from_secs(5),
                    )
                    .await
            })
        };
        let waiter_b = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager
                    .request_and_wait(
                        "session-b".to_string(),
                        "tool-call-b".to_string(),
                        "Need input B".to_string(),
                        json!({ "type": "object" }),
                        Duration::from_secs(5),
                    )
                    .await
            })
        };

        let message_a = recv_elicitation_message(&mut stream_a).await;
        let request_id_a = elicitation_id(&message_a);
        assert!(stream_a.try_recv().is_err());

        let message_b = recv_elicitation_message(&mut stream_b).await;
        let request_id_b = elicitation_id(&message_b);

        manager
            .claim_response("session-a", &request_id_a)
            .await
            .unwrap()
            .submit(ElicitationOutcome::Accept(json!({ "answer": "a" })))
            .unwrap();
        manager
            .claim_response("session-b", &request_id_b)
            .await
            .unwrap()
            .submit(ElicitationOutcome::Accept(json!({ "answer": "b" })))
            .unwrap();

        assert_eq!(
            waiter_a.await.unwrap().unwrap(),
            ElicitationOutcome::Accept(json!({ "answer": "a" }))
        );
        assert_eq!(
            waiter_b.await.unwrap().unwrap(),
            ElicitationOutcome::Accept(json!({ "answer": "b" }))
        );
    }

    #[tokio::test]
    async fn streams_are_namespaced_by_session() {
        let manager = Arc::new(ActionRequiredManager::new());
        let mut stream_a = manager
            .register_action_required_stream("session-a".to_string(), "tool-call-a".to_string())
            .await;
        let mut stream_b = manager
            .register_action_required_stream("session-b".to_string(), "tool-call-a".to_string())
            .await;
        let waiter_a = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager
                    .request_and_wait(
                        "session-a".to_string(),
                        "tool-call-a".to_string(),
                        "Need input A".to_string(),
                        json!({ "type": "object" }),
                        Duration::from_secs(5),
                    )
                    .await
            })
        };
        let waiter_b = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager
                    .request_and_wait(
                        "session-b".to_string(),
                        "tool-call-a".to_string(),
                        "Need input B".to_string(),
                        json!({ "type": "object" }),
                        Duration::from_secs(5),
                    )
                    .await
            })
        };

        let message_a = recv_elicitation_message(&mut stream_a).await;
        let request_id_a = elicitation_id(&message_a);
        let message_b = recv_elicitation_message(&mut stream_b).await;
        let request_id_b = elicitation_id(&message_b);

        manager
            .claim_response("session-a", &request_id_a)
            .await
            .unwrap()
            .submit(ElicitationOutcome::Accept(json!({ "answer": "a" })))
            .unwrap();
        manager
            .claim_response("session-b", &request_id_b)
            .await
            .unwrap()
            .submit(ElicitationOutcome::Accept(json!({ "answer": "b" })))
            .unwrap();

        assert_eq!(
            waiter_a.await.unwrap().unwrap(),
            ElicitationOutcome::Accept(json!({ "answer": "a" }))
        );
        assert_eq!(
            waiter_b.await.unwrap().unwrap(),
            ElicitationOutcome::Accept(json!({ "answer": "b" }))
        );
    }

    #[tokio::test]
    async fn claimed_response_can_complete_after_timeout_deadline() {
        let manager = Arc::new(ActionRequiredManager::new());
        let mut action_required_rx = manager
            .register_action_required_stream("session-a".to_string(), "tool-call-a".to_string())
            .await;
        let waiter = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager
                    .request_and_wait(
                        "session-a".to_string(),
                        "tool-call-a".to_string(),
                        "Need input".to_string(),
                        json!({ "type": "object" }),
                        Duration::from_millis(25),
                    )
                    .await
            })
        };

        let message = recv_elicitation_message(&mut action_required_rx).await;
        let request_id = elicitation_id(&message);

        let claim = manager
            .claim_response("session-a", &request_id)
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        claim
            .submit(ElicitationOutcome::Accept(json!({ "answer": "late" })))
            .unwrap();

        assert_eq!(
            waiter.await.unwrap().unwrap(),
            ElicitationOutcome::Accept(json!({ "answer": "late" }))
        );
    }

    #[tokio::test]
    async fn request_and_wait_returns_decline_and_cancel_actions() {
        let manager = Arc::new(ActionRequiredManager::new());
        let mut decline_rx = manager
            .register_action_required_stream("session-a".to_string(), "tool-call-a".to_string())
            .await;
        let mut cancel_rx = manager
            .register_action_required_stream("session-b".to_string(), "tool-call-b".to_string())
            .await;
        let decline_waiter = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager
                    .request_and_wait(
                        "session-a".to_string(),
                        "tool-call-a".to_string(),
                        "Need input A".to_string(),
                        json!({ "type": "object" }),
                        Duration::from_secs(5),
                    )
                    .await
            })
        };
        let cancel_waiter = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager
                    .request_and_wait(
                        "session-b".to_string(),
                        "tool-call-b".to_string(),
                        "Need input B".to_string(),
                        json!({ "type": "object" }),
                        Duration::from_secs(5),
                    )
                    .await
            })
        };

        let decline_message = recv_elicitation_message(&mut decline_rx).await;
        let decline_request_id = elicitation_id(&decline_message);
        let cancel_message = recv_elicitation_message(&mut cancel_rx).await;
        let cancel_request_id = elicitation_id(&cancel_message);

        manager
            .claim_response("session-a", &decline_request_id)
            .await
            .unwrap()
            .submit(ElicitationOutcome::Decline)
            .unwrap();
        manager
            .claim_response("session-b", &cancel_request_id)
            .await
            .unwrap()
            .submit(ElicitationOutcome::Cancel)
            .unwrap();

        assert_eq!(
            decline_waiter.await.unwrap().unwrap(),
            ElicitationOutcome::Decline
        );
        assert_eq!(
            cancel_waiter.await.unwrap().unwrap(),
            ElicitationOutcome::Cancel
        );
    }

    #[tokio::test]
    async fn missing_tool_call_stream_errors() {
        let manager = Arc::new(ActionRequiredManager::new());

        let result = manager
            .request_and_wait(
                "session-a".to_string(),
                "missing-tool-call".to_string(),
                "Need input".to_string(),
                json!({ "type": "object" }),
                Duration::from_secs(5),
            )
            .await;

        let err = result.expect_err("request should fail without a registered stream");
        assert!(err
            .to_string()
            .contains("Tool call request not found for elicitation"));
    }

    #[tokio::test]
    async fn closed_tool_call_stream_errors() {
        let manager = Arc::new(ActionRequiredManager::new());
        let rx = manager
            .register_action_required_stream("session-a".to_string(), "tool-call-a".to_string())
            .await;
        drop(rx);

        let result = manager
            .request_and_wait(
                "session-a".to_string(),
                "tool-call-a".to_string(),
                "Need input".to_string(),
                json!({ "type": "object" }),
                Duration::from_secs(5),
            )
            .await;

        let err = result.expect_err("request should fail when stream is closed");
        assert!(err
            .to_string()
            .contains("Tool call action-required stream closed"));
    }

    #[tokio::test]
    async fn full_tool_call_stream_rejects_without_pending_state() {
        let manager = Arc::new(ActionRequiredManager::new());
        let mut action_required_rx = manager
            .register_action_required_stream("session-a".to_string(), "tool-call-a".to_string())
            .await;

        for index in 0..ACTION_REQUIRED_STREAM_CAPACITY {
            let result = manager
                .request_and_wait(
                    "session-a".to_string(),
                    "tool-call-a".to_string(),
                    format!("Fill queue {index}"),
                    json!({ "type": "object" }),
                    Duration::ZERO,
                )
                .await;
            assert!(result.is_err());
        }

        assert!(manager.pending.read().await.is_empty());

        let result = tokio::time::timeout(
            Duration::from_millis(100),
            manager.request_and_wait(
                "session-a".to_string(),
                "tool-call-a".to_string(),
                "Rejected while queue is full".to_string(),
                json!({ "type": "object" }),
                Duration::from_secs(5),
            ),
        )
        .await
        .expect("full-stream rejection must not wait for response timeout");

        assert!(result
            .expect_err("full stream should reject another elicitation")
            .to_string()
            .contains("Tool call action-required stream is full"));
        assert!(manager.pending.read().await.is_empty());

        recv_elicitation_message(&mut action_required_rx).await;
        let waiter = {
            let manager = manager.clone();
            tokio::spawn(async move {
                manager
                    .request_and_wait(
                        "session-a".to_string(),
                        "tool-call-a".to_string(),
                        "Admitted after capacity is available".to_string(),
                        json!({ "type": "object" }),
                        Duration::from_secs(5),
                    )
                    .await
            })
        };

        for _ in 1..ACTION_REQUIRED_STREAM_CAPACITY {
            recv_elicitation_message(&mut action_required_rx).await;
        }
        let admitted = recv_elicitation_message(&mut action_required_rx).await;
        manager
            .claim_response("session-a", &elicitation_id(&admitted))
            .await
            .unwrap()
            .submit(ElicitationOutcome::Accept(json!({ "answer": "accepted" })))
            .unwrap();

        assert_eq!(
            waiter.await.unwrap().unwrap(),
            ElicitationOutcome::Accept(json!({ "answer": "accepted" }))
        );
    }
}
