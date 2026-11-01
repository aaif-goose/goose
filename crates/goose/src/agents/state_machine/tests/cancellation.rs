use std::{sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::{
    inference::{is_cancelled_response, InferenceRunner},
    tool::ToolOperation,
};
use goose_providers::{
    base::{MessageStream, Provider},
    conversation::token_usage::{ProviderUsage, Usage},
    errors::ProviderError,
    model::ModelConfig,
};
use rmcp::model::{CallToolRequestParams, Tool};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use super::{pipeline::MessageKind, test_pipeline};
use crate::agents::gen_ai_telemetry::test_support::SpanFieldCapture;
use crate::agents::state_machine::{Emitter, GooseEffect, StateMachine, Step};
use crate::agents::AgentEvent;
use crate::conversation::message::Message;
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

#[tokio::test]
async fn interrupted_inference_saves_streamed_output_then_answers_its_request() -> Result<()> {
    let (pipeline, _) = test_pipeline().await?;
    pipeline
        .seed([Message::user().with_text("kickoff")])
        .await?;
    let cancel = CancellationToken::new();
    let machine: StateMachine<Session, GooseEffect> = StateMachine::new(
        vec![
            Step::Operation(Arc::new(ToolOperation::new())),
            Step::Inference(Arc::new(InferenceRunner::new(
                Arc::new(InterruptedProvider {
                    cancel: cancel.clone(),
                }),
                ModelConfig::new("model"),
            ))),
        ],
        cancel.clone(),
    );
    let (tx, mut rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, cancel);
    let capture = SpanFieldCapture::new("turn");
    let _subscriber = capture.clone().set_default();
    let turn_span = tracing::info_span!(
        "turn",
        gen_ai.usage.input_tokens = tracing::field::Empty,
        gen_ai.usage.output_tokens = tracing::field::Empty,
    );
    let session = tokio::time::timeout(
        Duration::from_secs(5),
        crate::agents::state_machine::session::run(
            &machine,
            pipeline.session_manager.as_ref(),
            &pipeline.services().await.hook_manager,
            &pipeline.session_id,
            &emit,
        )
        .instrument(turn_span),
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
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1].as_concat_text(), "partial \noutput");
    assert!(is_cancelled_response(&messages[1]));
    assert!(messages[1].get_tool_request_ids().contains("pending"));
    assert!(messages[2].get_tool_response_ids().contains("pending"));
    assert_eq!(session.usage.total_tokens, Some(15));
    let turn_fields = capture.fields();
    assert_eq!(turn_fields["gen_ai.usage.input_tokens"], 10);
    assert_eq!(turn_fields["gen_ai.usage.output_tokens"], 5);
    let emitted_ids: Vec<_> = emitted.iter().map(|message| &message.id).collect();
    assert_eq!(
        emitted_ids,
        [&messages[1].id, &messages[1].id, &messages[2].id]
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_compaction_is_discarded_before_the_next_message() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    let pipeline = pipeline.with_cache_friendly_compaction();
    pipeline
        .set_total_tokens((pipeline.context_limit() as f64 * 0.81) as i32)
        .await;
    let held_response = api.on("## Task Context").hold_reply("unused");
    let cancel = CancellationToken::new();
    let run = pipeline.run_with_cancel("cancelled request", cancel.clone());
    tokio::pin!(run);
    tokio::select! {
        () = held_response.entered() => {}
        _ = &mut run => panic!("compaction request was not held"),
    }
    cancel.cancel();
    let cancelled = tokio::time::timeout(Duration::from_secs(5), &mut run).await??;
    held_response.release();

    let messages = cancelled.conversation().messages();
    let trigger = messages
        .iter()
        .find(|message| message.as_concat_text() == "cancelled request")
        .unwrap();
    assert!(trigger.is_user_visible());
    assert!(!trigger.is_agent_visible());
    assert!(messages.iter().any(|message| {
        message.as_concat_text().contains("## Task Context")
            && !message.is_user_visible()
            && !message.is_agent_visible()
    }));

    api.on("## Task Context").reply("summary");
    api.on("Your context was compacted").reply("continued");
    let resumed = pipeline.run(["replacement request"]).await?;

    resumed.assert_message(-1, MessageKind::Agent, "continued");
    let calls = api.calls();
    assert_eq!(calls.len(), 3);
    assert!(!calls[1].input_contains("cancelled request"));
    assert!(calls[2].input_contains("replacement request"));
    assert!(!calls[2].input_contains("cancelled request"));

    Ok(())
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(unix)]
#[tokio::test]
async fn stop_kills_a_running_shell_command() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    let pipeline = pipeline
        .with_goose_mode(crate::config::GooseMode::Auto)
        .await;
    pipeline.add_extension("developer").await?;
    let pid_file = pipeline.working_dir().join("shell.pid");
    api.on("run the tests").calls([(
        "call_tests",
        "shell",
        serde_json::json!({ "command": format!("echo $$ > {}; exec sleep 30", pid_file.display()) }),
    )]);
    let cancel = CancellationToken::new();
    let stop_once_started = async {
        let pid = loop {
            if let Some(pid) = std::fs::read_to_string(&pid_file)
                .ok()
                .and_then(|pid| pid.trim().parse::<u32>().ok())
            {
                break pid;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        cancel.cancel();
        pid
    };

    let (result, pid) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(
            pipeline.run_with_cancel("run the tests", cancel.clone()),
            stop_once_started
        )
    })
    .await?;
    result?;

    tokio::time::timeout(Duration::from_secs(5), async {
        while process_is_alive(pid) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    Ok(())
}
