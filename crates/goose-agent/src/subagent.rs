use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures::stream::{self, SelectAll, StreamExt};
use futures::FutureExt;
use goose_provider_types::conversation::message::Message;
use goose_provider_types::conversation::Conversation;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::operation::{
    applied, not_applicable, Emitter, Operation, OperationResult, OperationStream,
};

const OPERATION_NAME: &str = "foreground_subagent";
const DELIVERED: &str = "delivered";
const CANCELLED: &str = "cancelled";
const ENDED_WITHOUT_OUTCOME: &str = "it ended without an outcome";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentOutcome {
    Completed(String),
    Failed(String),
    Cancelled,
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum SubagentUpdate {
    ToParent(Message),
    Finished(SubagentOutcome),
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
        cancel: CancellationToken,
    ) -> OperationStream<'static, SubagentUpdate>;

    fn on_subagent_finished(
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

fn tracked_updates(
    subagent_session_id: String,
    updates: OperationStream<'static, SubagentUpdate>,
) -> OperationStream<'static, (String, SubagentUpdate)> {
    Box::pin(stream::unfold(Some(updates), move |updates| {
        let subagent_session_id = subagent_session_id.clone();
        async move {
            let mut updates = updates?;
            let update = updates.next().await.unwrap_or_else(|| {
                SubagentUpdate::Finished(SubagentOutcome::Failed(ENDED_WITHOUT_OUTCOME.to_string()))
            });
            let rest = match update {
                SubagentUpdate::Finished(_) => None,
                SubagentUpdate::ToParent(_) => Some(updates),
            };
            Some(((subagent_session_id, update), rest))
        }
    }))
}

#[derive(Default)]
struct Running {
    subagent_session_ids: HashSet<String>,
    subagent_runs: SelectAll<OperationStream<'static, (String, SubagentUpdate)>>,
}

pub struct SubagentOperation<S> {
    runner: Arc<dyn SubagentRunner<S>>,
    running: Mutex<Running>,
    max_concurrency: NonZeroUsize,
}

impl<S> SubagentOperation<S> {
    pub fn new(runner: Arc<dyn SubagentRunner<S>>) -> Self {
        Self {
            runner,
            running: Mutex::default(),
            max_concurrency: NonZeroUsize::MAX,
        }
    }

    /// Limits how many subagents run at once; additional subagents remain pending.
    pub fn with_max_concurrency(mut self, max_concurrency: NonZeroUsize) -> Self {
        self.max_concurrency = max_concurrency;
        self
    }

    fn deliver<E: From<Message>>(
        &self,
        subagent_session_id: &str,
        outcome: &SubagentOutcome,
        remaining: usize,
        emit: &Emitter,
    ) -> Option<E> {
        let message = subagent_result_message(subagent_session_id, outcome)?;
        self.runner
            .on_subagent_finished(subagent_session_id, outcome, remaining, emit);
        Some(E::from(message))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for SubagentOperation<S>
where
    S: MaybeSend + MaybeSync + 'static,
    E: From<Message> + MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        OPERATION_NAME
    }

    async fn finalize_cancellation(
        &self,
        session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Vec<E> {
        let mut stopped = std::mem::take(&mut *self.running.lock().await);
        let pending = pending_subagents(
            conversation,
            self.runner
                .started_subagent_session_ids(session, conversation),
        );
        let mut effects = Vec::new();
        let mut delivered = HashSet::new();
        let finished = std::iter::from_fn(|| loop {
            match stopped.subagent_runs.next().now_or_never().flatten()? {
                (subagent_session_id, SubagentUpdate::Finished(outcome)) => {
                    return Some((subagent_session_id, outcome))
                }
                (_, SubagentUpdate::ToParent(_)) => {}
            }
        });
        for (subagent_session_id, outcome) in finished {
            if outcome == SubagentOutcome::Cancelled {
                continue;
            }
            let remaining = pending.len().saturating_sub(delivered.len() + 1);
            if let Some(effect) = self.deliver(&subagent_session_id, &outcome, remaining, emit) {
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
            if running.subagent_session_ids.len() >= self.max_concurrency.get() {
                break;
            }
            if running.subagent_session_ids.contains(subagent_session_id) {
                continue;
            }
            let updates = self
                .runner
                .start(session, subagent_session_id, emit.cancel_token().clone())
                .await;
            running
                .subagent_session_ids
                .insert(subagent_session_id.clone());
            running
                .subagent_runs
                .push(tracked_updates(subagent_session_id.clone(), updates));
        }

        while let Some((subagent_session_id, update)) = running.subagent_runs.next().await {
            let outcome = match update {
                SubagentUpdate::ToParent(message) => {
                    emit.message(message);
                    continue;
                }
                SubagentUpdate::Finished(outcome) => outcome,
            };
            running.subagent_session_ids.remove(&subagent_session_id);
            if let Some(effect) = self.deliver(&subagent_session_id, &outcome, remaining, emit) {
                return applied([effect]);
            }
        }
        not_applicable()
    }
}
