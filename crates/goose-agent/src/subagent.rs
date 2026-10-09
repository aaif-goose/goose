use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use futures::FutureExt;
use goose_provider_types::conversation::message::Message;
use goose_provider_types::conversation::Conversation;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};
use tokio::sync::Mutex;

use crate::operation::{
    applied, not_applicable, Emitter, Operation, OperationFuture, OperationResult,
};

const OPERATION_NAME: &str = "foreground_subagent";
const DELIVERED: &str = "delivered";
const CANCELLED: &str = "cancelled";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentOutcome {
    Completed(String),
    Failed(String),
    Cancelled,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait SubagentRunner<S>: MaybeSend + MaybeSync {
    fn started_subagent_session_ids(
        &self,
        parent_session: &S,
        conversation: &Conversation,
    ) -> Vec<String>;

    async fn start(
        &self,
        parent_session: &S,
        subagent_session_id: &str,
        emit: &Emitter,
    ) -> OperationFuture<'static, SubagentOutcome>;

    async fn on_subagent_finished(
        &self,
        _subagent_session_id: &str,
        _outcome: &SubagentOutcome,
        _remaining: usize,
        _emit: &Emitter,
    ) {
    }
}

fn finished_subagent_session_ids(message: &Message) -> impl Iterator<Item = &str> {
    let delivered = message
        .metadata
        .operation_note(OPERATION_NAME, DELIVERED)
        .and_then(serde_json::Value::as_str);
    let cancelled = message
        .metadata
        .operation_note(OPERATION_NAME, CANCELLED)
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str);
    delivered.into_iter().chain(cancelled)
}

fn pending_subagents(conversation: &Conversation, started: Vec<String>) -> Vec<String> {
    let mut seen: HashSet<String> = conversation
        .messages()
        .iter()
        .flat_map(finished_subagent_session_ids)
        .map(str::to_owned)
        .collect();
    started
        .into_iter()
        .filter(|subagent_session_id| seen.insert(subagent_session_id.clone()))
        .collect()
}

fn subagent_label(subagent_session_id: &str) -> String {
    format!("Subagent {subagent_session_id}")
}

fn subagent_result_message(
    subagent_session_id: &str,
    outcome: &SubagentOutcome,
) -> Option<Message> {
    let label = subagent_label(subagent_session_id);
    let text = match outcome {
        SubagentOutcome::Completed(output) => format!("{label} completed: {output}"),
        SubagentOutcome::Failed(reason) => format!("{label} failed: {reason}"),
        SubagentOutcome::Cancelled => {
            return subagent_cancelled_message(&[subagent_session_id.to_string()])
        }
    };
    let mut message = Message::user().with_text(text).with_visibility(false, true);
    message.metadata.set_operation_note(
        OPERATION_NAME,
        DELIVERED,
        serde_json::json!(subagent_session_id),
    );
    Some(message)
}

fn subagent_cancelled_message(subagent_session_ids: &[String]) -> Option<Message> {
    if subagent_session_ids.is_empty() {
        return None;
    }
    let text = subagent_session_ids
        .iter()
        .map(|subagent_session_id| {
            format!(
                "{} was cancelled before it finished and will not run again.",
                subagent_label(subagent_session_id)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut message = Message::user().with_text(text).with_visibility(false, true);
    message.metadata.set_operation_note(
        OPERATION_NAME,
        CANCELLED,
        serde_json::json!(subagent_session_ids),
    );
    Some(message)
}

#[derive(Default)]
struct Running {
    subagent_session_ids: HashSet<String>,
    subagent_runs: FuturesUnordered<OperationFuture<'static, (String, SubagentOutcome)>>,
    delivering: Option<(String, SubagentOutcome)>,
}

pub struct ForegroundSubagentOperation<S> {
    runner: Arc<dyn SubagentRunner<S>>,
    running: Mutex<Running>,
}

impl<S> ForegroundSubagentOperation<S> {
    pub fn new(runner: Arc<dyn SubagentRunner<S>>) -> Self {
        Self {
            runner,
            running: Mutex::default(),
        }
    }

    async fn deliver<E: From<Message>>(
        &self,
        subagent_session_id: &str,
        outcome: &SubagentOutcome,
        remaining: usize,
        emit: &Emitter,
    ) -> Option<E> {
        let message = subagent_result_message(subagent_session_id, outcome)?;
        self.runner
            .on_subagent_finished(subagent_session_id, outcome, remaining, emit)
            .await;
        Some(E::from(message))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for ForegroundSubagentOperation<S>
where
    S: MaybeSend + MaybeSync + 'static,
    E: From<Message> + MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        OPERATION_NAME
    }

    async fn cancel(&self, session: &S, conversation: &Conversation, emit: &Emitter) -> Vec<E> {
        let mut stopped = std::mem::take(&mut *self.running.lock().await);
        let pending = pending_subagents(
            conversation,
            self.runner
                .started_subagent_session_ids(session, conversation),
        );
        let mut effects = Vec::new();
        let mut delivered = HashSet::new();
        let interrupted = stopped.delivering.take();
        let finished = std::iter::from_fn(|| stopped.subagent_runs.next().now_or_never().flatten());
        for (subagent_session_id, outcome) in interrupted.into_iter().chain(finished) {
            if outcome == SubagentOutcome::Cancelled {
                continue;
            }
            let remaining = pending.len().saturating_sub(delivered.len() + 1);
            if let Some(effect) = self
                .deliver(&subagent_session_id, &outcome, remaining, emit)
                .await
            {
                delivered.insert(subagent_session_id);
                effects.push(effect);
            }
        }
        drop(stopped);

        let cancelled: Vec<String> = pending
            .into_iter()
            .filter(|subagent_session_id| !delivered.contains(subagent_session_id))
            .collect();
        effects.extend(subagent_cancelled_message(&cancelled).map(E::from));
        effects
    }

    async fn run(
        &self,
        session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let pending = pending_subagents(
            conversation,
            self.runner
                .started_subagent_session_ids(session, conversation),
        );
        let remaining = pending.len().saturating_sub(1);

        let mut running = self.running.lock().await;
        for subagent_session_id in &pending {
            if running.subagent_session_ids.contains(subagent_session_id) {
                continue;
            }
            let subagent_run = self.runner.start(session, subagent_session_id, emit).await;
            running
                .subagent_session_ids
                .insert(subagent_session_id.clone());
            let subagent_session_id = subagent_session_id.clone();
            running.subagent_runs.push(Box::pin(async move {
                (subagent_session_id, subagent_run.await)
            }));
        }

        while let Some((subagent_session_id, outcome)) = running.subagent_runs.next().await {
            running.subagent_session_ids.remove(&subagent_session_id);
            running.delivering = Some((subagent_session_id.clone(), outcome.clone()));
            let effect = self
                .deliver(&subagent_session_id, &outcome, remaining, emit)
                .await;
            running.delivering = None;
            if let Some(effect) = effect {
                return applied([effect]);
            }
        }
        not_applicable()
    }
}
