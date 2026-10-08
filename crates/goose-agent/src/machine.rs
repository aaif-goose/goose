use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::operation::{
    ConversationEffect, Emitter, Inference, MachineEffect, Operation, OperationResult, StepResult,
};
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
}

impl<'a, S, E> StateMachine<'a, S, E>
where
    S: MachineSession,
    E: MachineEffect + MaybeSend + 'static,
{
    pub fn new(steps: Vec<Step<'a, S, E>>, cancel: CancellationToken) -> Self {
        Self { steps, cancel }
    }

    pub async fn step(&self, session: &S, emit: &Emitter) -> Result<Option<StepResult<E>>> {
        let conversation = session
            .conversation()
            .ok_or_else(|| anyhow!("state-machine session loaded without conversation"))?;

        for step in &self.steps {
            let name = step.operation().name();
            let result = if self.cancel.is_cancelled() {
                OperationResult::NotApplicable
            } else {
                match step {
                    Step::Operation(operation) => {
                        operation.run(session, conversation, emit).await?
                    }
                    Step::Inference(inference) => {
                        let prepared_session = inference.prepare_session(session).await?;
                        let session = prepared_session.as_ref().unwrap_or(session);
                        let conversation = session.conversation().ok_or_else(|| {
                            anyhow!("state-machine session loaded without conversation")
                        })?;
                        if !inference.applies(conversation) {
                            continue;
                        }
                        let operations = self.steps.iter().map(Step::operation).collect::<Vec<_>>();
                        let input = inference
                            .prepare_input(session, conversation, &operations, &self.cancel)
                            .await?;
                        if self.cancel.is_cancelled() {
                            return Ok(None);
                        }
                        inference.infer(session, conversation, input, emit).await?
                    }
                }
            };
            let cancelled = self.cancel.is_cancelled();
            let result = if cancelled {
                step.operation()
                    .cancel(session, conversation, result, emit)
                    .await?
            } else {
                result
            };

            match result {
                OperationResult::NotApplicable => {}
                OperationResult::Applied(mut result) => {
                    result.applied_step = Some(name);
                    for effect in &mut result.effects {
                        effect.ensure_message_ids();
                    }
                    if cancelled {
                        result.yield_to_client = true;
                    }
                    return Ok(Some(result));
                }
            }
        }

        Ok(None)
    }

    pub async fn apply<R>(
        &self,
        runtime: &R,
        session: &S,
        result: &mut StepResult<E>,
        emit: &Emitter,
    ) -> Result<()>
    where
        R: EffectHandler<S, E>,
    {
        for effect in &mut result.effects {
            effect.ensure_message_ids();
        }
        runtime
            .apply_effects(session, &mut result.effects, emit)
            .await
    }

    pub async fn run<R>(&self, runtime: &R, session_id: &str, emit: &Emitter) -> Result<S>
    where
        R: SessionLoader<S> + EffectHandler<S, E>,
    {
        loop {
            let session = runtime.load(session_id).await?;
            let Some(mut result) = self.step(&session, emit).await? else {
                break;
            };
            self.apply(runtime, &session, &mut result, emit).await?;
            if result.yield_to_client {
                break;
            }
        }
        runtime.load(session_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation::{not_applicable, InferenceInput};

    struct Contributor;

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl Operation<(), ConversationEffect> for Contributor {
        fn name(&self) -> &'static str {
            "contributor"
        }
        async fn inference_tools(&self, _: &()) -> Result<Vec<rmcp::model::Tool>> {
            Ok(vec![rmcp::model::Tool::new(
                "duplicate",
                "test",
                serde_json::Map::new(),
            )])
        }
        async fn prompt_parts(&self, _: &(), _: &Conversation) -> Result<Vec<(String, String)>> {
            Ok(vec![("part".into(), "prompt".into())])
        }
        async fn moim_parts(&self, _: &(), _: &Conversation) -> Result<Vec<String>> {
            Ok(vec!["context".into()])
        }
    }

    struct CompletionInference;

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl Operation<(), ConversationEffect> for CompletionInference {
        fn name(&self) -> &'static str {
            "inference"
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl Inference<(), ConversationEffect> for CompletionInference {
        fn applies(&self, _: &Conversation) -> bool {
            true
        }
        async fn infer(
            &self,
            _: &(),
            _: &Conversation,
            _: InferenceInput,
            _: &Emitter,
        ) -> Result<OperationResult<ConversationEffect>> {
            not_applicable()
        }
    }

    #[tokio::test]
    async fn default_inference_preparation_preserves_operation_contributions() {
        let input = CompletionInference
            .prepare_input(
                &(),
                &Conversation::empty(),
                &[&Contributor],
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(input.tools[0].name, "duplicate");
        assert_eq!(input.prompt_parts, [("part".into(), "prompt".into())]);
        assert_eq!(input.moim_parts, ["context"]);
    }

    #[tokio::test]
    async fn rejects_duplicate_tools_across_operations() {
        let error = CompletionInference
            .prepare_input(
                &(),
                &Conversation::empty(),
                &[&Contributor, &Contributor],
                &CancellationToken::new(),
            )
            .await
            .err()
            .unwrap();
        assert_eq!(
            error.to_string(),
            "multiple operations registered tool 'duplicate'"
        );
    }
}
