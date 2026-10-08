use std::{sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::{inference::InferenceRunner, tool::ToolOperation};
use goose_providers::{
    base::{MessageStream, Provider},
    conversation::token_usage::{ProviderUsage, Usage},
    errors::ProviderError,
    model::ModelConfig,
};
use rmcp::model::{CallToolRequestParams, Tool};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::test_pipeline;
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
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1].as_concat_text(), "partial \noutput");
    assert!(messages[1].get_tool_request_ids().contains("pending"));
    assert!(messages[2].get_tool_response_ids().contains("pending"));
    assert_eq!(session.usage.total_tokens, Some(15));
    let emitted_ids: Vec<_> = emitted.iter().map(|message| &message.id).collect();
    assert_eq!(
        emitted_ids,
        [&messages[1].id, &messages[1].id, &messages[2].id]
    );
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
