use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::operation::{
    messages_since_kickoff, ConversationEffect, Emitter, Inference, InferenceInput, MachineEffect,
    Operation, OperationResult, RunStatus, StepResult,
};
use goose_provider_types::conversation::message::{Message, MessageContent};
use goose_provider_types::conversation::Conversation;
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};

pub trait MachineSession: MaybeSend + MaybeSync {
    fn id(&self) -> &str;
    fn conversation(&self) -> Option<&Conversation>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait SessionLoader<S>: MaybeSend + MaybeSync {
    async fn load(&self, session_id: &str) -> Result<S>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait EffectHandler<S, E>: MaybeSend + MaybeSync {
    async fn apply_effects(&self, session: &S, effects: &mut [E], emit: &Emitter) -> Result<()>;
}

pub trait EffectUsage<E>: MaybeSend + MaybeSync {
    fn usage(&self, _effect: &E) -> Option<goose_provider_types::conversation::token_usage::Usage> {
        None
    }
}

pub enum Step<'a, S, E = ConversationEffect> {
    Operation(Arc<dyn Operation<S, E> + 'a>),
    Inference(Arc<dyn Inference<S, E> + 'a>),
}

impl<S, E: MaybeSend> Step<'_, S, E> {
    fn operation(&self) -> &dyn Operation<S, E> {
        match self {
            Step::Operation(operation) => operation.as_ref(),
            Step::Inference(inference) => inference.as_ref(),
        }
    }
}

pub struct StateMachine<'a, S, E = ConversationEffect> {
    steps: Vec<Step<'a, S, E>>,
    cancel: CancellationToken,
    interrupted_step: Mutex<Option<usize>>,
}

fn empty_result<E>(status: RunStatus) -> StepResult<E> {
    StepResult {
        effects: Vec::new(),
        applied_step: None,
        status,
    }
}

fn interrupted_response(messages: &[Message]) -> Option<Message> {
    let answered = messages
        .iter()
        .flat_map(Message::get_tool_response_ids)
        .collect::<HashSet<_>>();
    let mut request_ids = HashSet::new();
    let mut response = Message::user();
    for request in messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(MessageContent::as_tool_request)
    {
        if request_ids.insert(request.id.as_str()) && !answered.contains(request.id.as_str()) {
            response.add_tool_response_with_metadata(
                request.id.clone(),
                Ok(rmcp::model::CallToolResult::error(vec![
                    rmcp::model::ContentBlock::text("Tool call was interrupted before completing"),
                ])),
                request.metadata.as_ref(),
            );
        }
    }
    (!response.get_tool_response_ids().is_empty()).then_some(response)
}

fn add_tools_to_inference_input(
    input: &mut InferenceInput,
    tool_names: &mut HashSet<String>,
    tools: Vec<rmcp::model::Tool>,
) -> Result<()> {
    for tool in tools {
        if !tool_names.insert(tool.name.to_string()) {
            anyhow::bail!("multiple operations registered tool '{}'", tool.name);
        }
        input.tools.push(tool);
    }
    Ok(())
}

impl<'a, S, E> StateMachine<'a, S, E>
where
    S: MachineSession,
    E: MachineEffect + From<Message> + MaybeSend + 'static,
{
    pub fn new(steps: Vec<Step<'a, S, E>>, cancel: CancellationToken) -> Self {
        Self {
            steps,
            cancel,
            interrupted_step: Mutex::new(None),
        }
    }

    pub async fn step(&self, session: &S, emit: &Emitter) -> Result<StepResult<E>> {
        let conversation = session
            .conversation()
            .ok_or_else(|| anyhow!("state-machine session loaded without conversation"))?;

        for (index, step) in self.steps.iter().enumerate() {
            let name = step.operation().name();
            if self.cancel.is_cancelled() {
                return Ok(empty_result(RunStatus::Cancelled));
            }
            let execution = async {
                match step {
                    Step::Operation(operation) => operation.run(session, conversation, emit).await,
                    Step::Inference(inference) => {
                        let prepared_session = inference.prepare_session(session).await?;
                        let session = prepared_session.as_ref().unwrap_or(session);
                        let conversation = session.conversation().ok_or_else(|| {
                            anyhow!("state-machine session loaded without conversation")
                        })?;
                        if !inference.applies(conversation) {
                            return Ok(OperationResult::NotApplicable);
                        }
                        let mut input = InferenceInput::default();
                        let mut tool_names = HashSet::new();
                        for operation in self.steps.iter().map(|step| step.operation()) {
                            let tools = operation.inference_tools(session).await?;
                            add_tools_to_inference_input(&mut input, &mut tool_names, tools)?;
                            input
                                .prompt_parts
                                .extend(operation.prompt_parts(session, conversation).await?);
                            input
                                .moim_parts
                                .extend(operation.moim_parts(session, conversation).await?);
                        }
                        inference.infer(session, conversation, input, emit).await
                    }
                }
            };
            let result = tokio::select! {
                biased;
                result = execution => Some(result),
                _ = self.cancel.cancelled() => None,
            };
            let result = match result {
                Some(Ok(result)) => result,
                Some(Err(error)) if !self.cancel.is_cancelled() => return Err(error),
                _ => {
                    *self.interrupted_step.lock().unwrap() = Some(index);
                    return Ok(empty_result(RunStatus::Cancelled));
                }
            };

            match result {
                OperationResult::NotApplicable => {}
                OperationResult::Applied(mut result) => {
                    tracing::debug!(step = name, "applied step");
                    result.applied_step = Some(name);
                    for effect in &mut result.effects {
                        effect.ensure_message_ids();
                    }
                    if self.cancel.is_cancelled() {
                        result.status = RunStatus::Cancelled;
                    }
                    return Ok(result);
                }
            }
        }

        Ok(empty_result(RunStatus::Yielded))
    }

    pub async fn apply<R>(
        &self,
        runtime: &R,
        session: &S,
        effects: &mut [E],
        emit: &Emitter,
    ) -> Result<()>
    where
        R: EffectHandler<S, E>,
    {
        if effects.is_empty() {
            return Ok(());
        }
        for effect in effects.iter_mut() {
            effect.ensure_message_ids();
        }
        runtime.apply_effects(session, effects, emit).await
    }

    pub async fn run<R>(&self, runtime: &R, session_id: &str, emit: &Emitter) -> Result<S>
    where
        R: SessionLoader<S> + EffectHandler<S, E>,
    {
        loop {
            let session = runtime.load(session_id).await?;
            let mut result = self.step(&session, emit).await?;
            self.apply(runtime, &session, &mut result.effects, emit)
                .await?;
            match result.status {
                RunStatus::Continuing => {}
                RunStatus::Yielded | RunStatus::Cancelled => break,
            }
        }
        self.finalize(runtime, session_id, emit).await
    }

    pub async fn finalize<R>(&self, runtime: &R, session_id: &str, emit: &Emitter) -> Result<S>
    where
        R: SessionLoader<S> + EffectHandler<S, E>,
    {
        let mut session = runtime.load(session_id).await?;
        if !self.cancel.is_cancelled() {
            return Ok(session);
        }

        let interrupted_step = self.interrupted_step.lock().unwrap().take();
        if let Some(index) = interrupted_step {
            session = self
                .collect_cancelled(runtime, session_id, session, &self.steps[index], emit)
                .await?;
        }

        let unanswered = session
            .conversation()
            .and_then(|conversation| messages_since_kickoff(conversation).ok())
            .and_then(interrupted_response);
        if let Some(response) = unanswered {
            let effects = vec![E::from(emit.message(response).await)];
            session = self
                .save(runtime, session_id, session, effects, emit)
                .await?;
        }

        for (index, step) in self.steps.iter().enumerate() {
            if Some(index) != interrupted_step {
                session = self
                    .collect_cancelled(runtime, session_id, session, step, emit)
                    .await?;
            }
        }
        Ok(session)
    }

    async fn collect_cancelled<R>(
        &self,
        runtime: &R,
        session_id: &str,
        session: S,
        step: &Step<'a, S, E>,
        emit: &Emitter,
    ) -> Result<S>
    where
        R: SessionLoader<S> + EffectHandler<S, E>,
    {
        let conversation = session
            .conversation()
            .ok_or_else(|| anyhow!("state-machine session loaded without conversation"))?;
        let effects = step.operation().cancel(&session, conversation, emit).await;
        self.save(runtime, session_id, session, effects, emit).await
    }

    async fn save<R>(
        &self,
        runtime: &R,
        session_id: &str,
        session: S,
        mut effects: Vec<E>,
        emit: &Emitter,
    ) -> Result<S>
    where
        R: SessionLoader<S> + EffectHandler<S, E>,
    {
        if effects.is_empty() {
            return Ok(session);
        }
        self.apply(runtime, &session, &mut effects, emit).await?;
        runtime.load(session_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_tools_across_operations() {
        let schema = Arc::new(serde_json::Map::new());
        let mut input = InferenceInput::default();
        let mut names = HashSet::new();

        add_tools_to_inference_input(
            &mut input,
            &mut names,
            vec![rmcp::model::Tool::new("duplicate", "first", schema.clone())],
        )
        .unwrap();
        let error = add_tools_to_inference_input(
            &mut input,
            &mut names,
            vec![rmcp::model::Tool::new("duplicate", "second", schema)],
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "multiple operations registered tool 'duplicate'"
        );
    }
}
