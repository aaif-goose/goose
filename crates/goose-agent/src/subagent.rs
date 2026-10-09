use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use futures::{future, FutureExt};
use goose_provider_types::conversation::message::Message;
use goose_provider_types::conversation::Conversation;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::operation::{
    applied, not_applicable, Emitter, Operation, OperationFuture, OperationResult,
};

const OPERATION_NAME: &str = "foreground_subagent";
const DELIVERED: &str = "delivered";
const CANCELLED: &str = "cancelled";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentResult {
    Completed(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentOutcome {
    Finished(SubagentResult),
    Cancelled,
}

pub enum SubagentStart {
    Finished(SubagentResult),
    Started {
        task: Option<String>,
        run: OperationFuture<'static, SubagentOutcome>,
    },
}

#[non_exhaustive]
pub struct SubagentRequest {
    pub subagent_id: String,
    pub cancel: CancellationToken,
}

impl SubagentRequest {
    pub fn new(subagent_id: impl Into<String>, cancel: CancellationToken) -> Self {
        Self {
            subagent_id: subagent_id.into(),
            cancel,
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait SubagentRunner<S>: MaybeSend + MaybeSync {
    fn started_subagent_ids(&self, parent: &S, conversation: &Conversation) -> Vec<String>;

    fn ready(&self, parent: &S, conversation: &Conversation) -> Result<bool>;

    async fn start(&self, parent: &S, request: SubagentRequest) -> SubagentStart;

    async fn on_subagent_started(&self, _subagent_id: &str, _task: Option<&str>, _emit: &Emitter) {}

    async fn on_subagent_finished(
        &self,
        _subagent_id: &str,
        _result: &SubagentResult,
        _remaining: usize,
        _emit: &Emitter,
    ) {
    }
}

fn finished_subagent_ids(message: &Message) -> impl Iterator<Item = &str> {
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
        .flat_map(finished_subagent_ids)
        .map(str::to_owned)
        .collect();
    started
        .into_iter()
        .filter(|subagent_id| seen.insert(subagent_id.clone()))
        .collect()
}

fn subagent_label(subagent_id: &str) -> String {
    format!("Subagent {subagent_id}")
}

fn subagent_result_message(subagent_id: &str, result: &SubagentResult) -> Message {
    let label = subagent_label(subagent_id);
    let text = match result {
        SubagentResult::Completed(output) => format!("{label} completed: {output}"),
        SubagentResult::Failed(reason) => format!("{label} failed: {reason}"),
    };
    let mut message = Message::user().with_text(text).with_visibility(false, true);
    message
        .metadata
        .set_operation_note(OPERATION_NAME, DELIVERED, serde_json::json!(subagent_id));
    message
}

fn subagent_cancelled_message(subagent_ids: &[String]) -> Option<Message> {
    if subagent_ids.is_empty() {
        return None;
    }
    let text = subagent_ids
        .iter()
        .map(|subagent_id| {
            format!(
                "{} was cancelled before it finished and will not run again.",
                subagent_label(subagent_id)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut message = Message::user().with_text(text).with_visibility(false, true);
    message
        .metadata
        .set_operation_note(OPERATION_NAME, CANCELLED, serde_json::json!(subagent_ids));
    Some(message)
}

#[derive(Default)]
struct Running {
    subagent_ids: HashSet<String>,
    subagent_runs: FuturesUnordered<OperationFuture<'static, (String, SubagentOutcome)>>,
}

pub struct SubagentOperation<S> {
    runner: Arc<dyn SubagentRunner<S>>,
    running: Mutex<Running>,
}

impl<S> SubagentOperation<S> {
    pub fn new(runner: Arc<dyn SubagentRunner<S>>) -> Self {
        Self {
            runner,
            running: Mutex::default(),
        }
    }

    async fn deliver<E: From<Message>>(
        &self,
        subagent_id: &str,
        result: &SubagentResult,
        remaining: usize,
        emit: &Emitter,
    ) -> E {
        self.runner
            .on_subagent_finished(subagent_id, result, remaining, emit)
            .await;
        E::from(subagent_result_message(subagent_id, result))
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

    async fn cancel(&self, session: &S, conversation: &Conversation, emit: &Emitter) -> Vec<E> {
        let mut stopped = std::mem::take(&mut *self.running.lock().await);
        let pending = pending_subagents(
            conversation,
            self.runner.started_subagent_ids(session, conversation),
        );
        let mut effects = Vec::new();
        let mut delivered = HashSet::new();
        while let Some(Some((subagent_id, outcome))) = stopped.subagent_runs.next().now_or_never() {
            if let SubagentOutcome::Finished(result) = outcome {
                delivered.insert(subagent_id.clone());
                let remaining = pending.len().saturating_sub(delivered.len());
                effects.push(self.deliver(&subagent_id, &result, remaining, emit).await);
            }
        }
        drop(stopped);

        let cancelled: Vec<String> = pending
            .into_iter()
            .filter(|subagent_id| !delivered.contains(subagent_id))
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
            self.runner.started_subagent_ids(session, conversation),
        );
        if pending.is_empty() || !self.runner.ready(session, conversation)? {
            return not_applicable();
        }
        let remaining = pending.len() - 1;

        let mut running = self.running.lock().await;
        for subagent_id in &pending {
            if running.subagent_ids.contains(subagent_id) {
                continue;
            }
            let request = SubagentRequest::new(subagent_id.clone(), emit.cancel_token().clone());
            let subagent_run: OperationFuture<'static, SubagentOutcome> =
                match self.runner.start(session, request).await {
                    SubagentStart::Finished(result) => {
                        Box::pin(future::ready(SubagentOutcome::Finished(result)))
                    }
                    SubagentStart::Started { task, run } => {
                        self.runner
                            .on_subagent_started(subagent_id, task.as_deref(), emit)
                            .await;
                        run
                    }
                };
            running.subagent_ids.insert(subagent_id.clone());
            let subagent_id = subagent_id.clone();
            running
                .subagent_runs
                .push(Box::pin(async move { (subagent_id, subagent_run.await) }));
        }

        while let Some((subagent_id, outcome)) = running.subagent_runs.next().await {
            running.subagent_ids.remove(&subagent_id);
            if let SubagentOutcome::Finished(result) = outcome {
                return applied([self.deliver(&subagent_id, &result, remaining, emit).await]);
            }
        }
        not_applicable()
    }
}
