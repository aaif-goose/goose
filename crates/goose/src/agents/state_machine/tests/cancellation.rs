use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::{
    inference::{InferenceRequestPreparer, InferenceRunner, PreparedInferenceRequest},
    operation::InferenceInput,
    tool::ToolOperation,
};
use goose_providers::{
    base::{MessageStream, Provider},
    conversation::token_usage::{ProviderUsage, Usage},
    errors::ProviderError,
    model::ModelConfig,
};
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, Tool};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::hooks_lifecycle::{HookTestEnv, LOG_AND_ALLOW_SCRIPT};
use super::test_pipeline;
use crate::agents::state_machine::{
    Emitter, GooseEffect, Operation, StateMachine, Step, ToolPairCompactionOperation,
};
use crate::agents::AgentEvent;
use crate::conversation::{message::Message, Conversation};
use crate::session::Session;

struct InterruptedProvider {
    cancel: CancellationToken,
}

#[async_trait]
impl Provider for InterruptedProvider {
    fn get_name(&self) -> &str {
        "interrupted-stream"
    }

    async fn stream(
        &self,
        _model_config: &ModelConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let first = Message::assistant()
            .with_generated_id()
            .with_text("partial ");
        let second = Message::assistant()
            .with_id(first.id.clone().unwrap())
            .with_text("output")
            .with_tool_request("pending", Ok(CallToolRequestParams::new("missing")));
        let cancel = self.cancel.clone();
        let mut chunks = [first, second].into_iter().enumerate();
        Ok(Box::pin(futures::stream::poll_fn(move |_| {
            if let Some((index, message)) = chunks.next() {
                let total = if index == 0 { 12 } else { 15 };
                let usage = ProviderUsage::new(
                    "model".into(),
                    Usage::new(Some(10), Some(total - 10), Some(total)),
                );
                std::task::Poll::Ready(Some(Ok((Some(message), Some(usage)))))
            } else {
                cancel.cancel();
                std::task::Poll::Pending
            }
        })))
    }
}

struct AdditionalContext;

#[async_trait]
impl InferenceRequestPreparer<Session> for AdditionalContext {
    async fn prepare(
        &self,
        _session: &Session,
        _conversation: &Conversation,
        input: InferenceInput,
    ) -> Result<PreparedInferenceRequest> {
        Ok(PreparedInferenceRequest {
            system_prompt: String::new(),
            tools: input.tools,
            additional_messages: vec![Message::user().with_text("prepared context").agent_only()],
        })
    }
}

#[tokio::test]
async fn interrupted_inference_saves_streamed_output_then_answers_its_request() -> Result<()> {
    let prompt_submit = HookTestEnv::new("UserPromptSubmit", LOG_AND_ALLOW_SCRIPT);
    let (pipeline, _) = test_pipeline().await?;
    let pipeline = pipeline.with_hook_manager(prompt_submit.hook_manager());
    pipeline
        .seed([Message::user().with_text("kickoff")])
        .await?;
    let cancel = CancellationToken::new();
    let machine: StateMachine<Session, GooseEffect> = StateMachine::new(
        vec![
            Step::Operation(Arc::new(ToolOperation::new())),
            Step::Inference(Arc::new(
                InferenceRunner::new(
                    Arc::new(InterruptedProvider {
                        cancel: cancel.clone(),
                    }),
                    ModelConfig::new("model"),
                )
                .with_request_preparer(Arc::new(AdditionalContext)),
            )),
        ],
        cancel.clone(),
    );
    let (tx, mut rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, cancel);
    let session = tokio::time::timeout(
        Duration::from_secs(5),
        crate::agents::state_machine::session::run(
            &machine,
            pipeline.session_manager.as_ref(),
            &pipeline.hook_manager,
            &pipeline.session_id,
            &emit,
        ),
    )
    .await??;
    drop(emit);
    let mut emitted = Vec::new();
    while let Some(event) = rx.recv().await {
        if let AgentEvent::Message(message) = event {
            emitted.push(message);
        }
    }

    let messages = session.conversation.as_ref().unwrap().messages();
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[1].as_concat_text(), "prepared context");
    assert_eq!(messages[2].as_concat_text(), "partial \noutput");
    assert!(messages[2].get_tool_request_ids().contains("pending"));
    assert!(messages[3].get_tool_response_ids().contains("pending"));
    assert_eq!(session.usage.total_tokens, Some(15));
    assert_eq!(prompt_submit.invocations(), 1);
    let emitted_ids: Vec<_> = emitted.iter().map(|message| &message.id).collect();
    assert_eq!(
        emitted_ids,
        [&messages[2].id, &messages[2].id, &messages[3].id]
    );
    Ok(())
}

struct InterruptedSummaryProvider {
    cancel: CancellationToken,
    calls: AtomicUsize,
}

#[async_trait]
impl Provider for InterruptedSummaryProvider {
    fn get_name(&self) -> &str {
        "interrupted-summary"
    }

    async fn stream(
        &self,
        _model: &ModelConfig,
        _system: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) > 0 {
            self.cancel.cancel();
            return std::future::pending().await;
        }
        Ok(Box::pin(futures::stream::iter([Ok((
            Some(Message::assistant().with_text("completed summary")),
            Some(ProviderUsage::new("model".to_string(), Usage::default())),
        ))])))
    }
}

#[tokio::test]
async fn stop_during_next_summary_saves_the_completed_pair_and_leaves_other_pairs_visible(
) -> Result<()> {
    let (pipeline, _) = test_pipeline().await?;
    let mut messages = vec![Message::user().with_text("old work")];
    let pairs = crate::context_mgmt::TOOLCALL_SUMMARIZATION_BATCH_SIZE + 1;
    for index in 0..pairs {
        let id = format!("call-{index}");
        messages.push(
            Message::assistant()
                .with_tool_request(id.clone(), Ok(CallToolRequestParams::new("tool"))),
        );
        messages.push(Message::user().with_tool_response(
            id,
            Ok(CallToolResult::success(vec![ContentBlock::text("output")])),
        ));
    }
    messages.push(Message::user().with_text("next turn"));
    for message in messages {
        pipeline.seed([message]).await?;
    }
    let cancel = CancellationToken::new();
    let provider = Arc::new(InterruptedSummaryProvider {
        cancel: cancel.clone(),
        calls: AtomicUsize::new(0),
    });
    let operation = Arc::new(ToolPairCompactionOperation::new(
        provider.clone(),
        ModelConfig::new("model"),
        0,
        true,
    ));
    let machine = StateMachine::new(vec![Step::Operation(operation.clone())], cancel.clone());
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, cancel);
    let session = tokio::time::timeout(
        Duration::from_secs(5),
        machine.run(
            pipeline.session_manager.as_ref(),
            &pipeline.session_id,
            &emit,
        ),
    )
    .await??;
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    let messages = session.conversation.as_ref().unwrap().messages();
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.as_concat_text() == "completed summary")
            .count(),
        1
    );
    for message in messages
        .iter()
        .filter(|message| message.is_tool_call() || message.is_tool_response())
    {
        let first_pair = message.get_tool_request_ids().contains("call-0")
            || message.get_tool_response_ids().contains("call-0");
        assert_eq!(message.is_agent_visible(), !first_pair);
    }
    assert!(operation.cancel(&emit).await.is_empty());
    Ok(())
}
