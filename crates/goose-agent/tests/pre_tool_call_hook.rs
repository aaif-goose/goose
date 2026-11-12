use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::{
    operation::{ConversationEffect, Emitter, Operation, OperationResult, RunStatus},
    pre_tool_call_hook::{
        PreToolCallDecision, PreToolCallHook, PreToolCallHookOperation, PRE_HOOK_ALLOWED_KEY,
    },
    tool_call_state::EXECUTABLE_KEY,
};
use goose_provider_types::conversation::{
    message::{Message, MessageContent, ToolRequest, TOOL_META_EXTERNAL_DISPATCH_KEY},
    Conversation,
};
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, ErrorData};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy)]
enum Reply {
    Allow,
    Deny(&'static str),
    Fail,
}

#[derive(Default)]
struct FakeHook {
    replies: Vec<(&'static str, Reply)>,
    called: Mutex<Vec<String>>,
}

impl FakeHook {
    fn new(replies: &[(&'static str, Reply)]) -> Arc<Self> {
        Arc::new(Self {
            replies: replies.to_vec(),
            ..Default::default()
        })
    }

    fn called(&self) -> Vec<String> {
        self.called.lock().unwrap().clone()
    }
}

#[async_trait]
impl PreToolCallHook<()> for FakeHook {
    async fn before_call(
        &self,
        _session: &(),
        _conversation: &Conversation,
        request: &ToolRequest,
    ) -> Result<PreToolCallDecision> {
        self.called.lock().unwrap().push(request.id.clone());
        let reply = self
            .replies
            .iter()
            .find(|(id, _)| *id == request.id)
            .map_or(Reply::Allow, |(_, reply)| *reply);
        match reply {
            Reply::Allow => Ok(PreToolCallDecision::Allow),
            Reply::Deny(reason) => Ok(PreToolCallDecision::Deny {
                reason: reason.to_string(),
            }),
            Reply::Fail => anyhow::bail!("hook crashed"),
        }
    }
}

struct Outcome {
    status: Option<RunStatus>,
    marks: Vec<(String, serde_json::Value)>,
    appended: Vec<Message>,
}

impl Outcome {
    fn responses(&self) -> Vec<(String, String)> {
        self.appended.iter().flat_map(tool_responses).collect()
    }
}

async fn run(
    operation: &PreToolCallHookOperation<'_, ()>,
    messages: Vec<Message>,
) -> Result<Outcome> {
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    let conversation = Conversation::new_unvalidated(messages);
    let result =
        Operation::<(), ConversationEffect>::run(operation, &(), &conversation, &emit).await?;
    let mut outcome = Outcome {
        status: None,
        marks: Vec::new(),
        appended: Vec::new(),
    };
    if let OperationResult::Applied(result) = result {
        outcome.status = Some(result.status);
        for effect in result.effects {
            match effect {
                ConversationEffect::AppendMessage(message) => outcome.appended.push(message),
                ConversationEffect::PatchToolRequestMeta {
                    tool_call_id,
                    patch,
                } => outcome.marks.push((tool_call_id, patch)),
                _ => panic!("unexpected effect"),
            }
        }
    }
    Ok(outcome)
}

fn tool_responses(message: &Message) -> Vec<(String, String)> {
    message
        .content
        .iter()
        .filter_map(|content| match content {
            MessageContent::ToolResponse(response) => {
                let result = response.tool_result.as_ref().unwrap();
                assert_eq!(result.is_error, Some(true));
                let text = result.content[0].as_text().unwrap().text.clone();
                Some((response.id.clone(), text))
            }
            _ => None,
        })
        .collect()
}

fn kickoff() -> Message {
    Message::user().with_text("hi")
}

fn ready_calls(ids: &[&str]) -> Message {
    ids.iter().fold(Message::assistant(), |message, id| {
        message.with_tool_request_with_metadata(
            *id,
            Ok(CallToolRequestParams::new(id.to_string())),
            None,
            Some(json!({ EXECUTABLE_KEY: true })),
        )
    })
}

fn marked_call(id: &str, meta: serde_json::Value) -> Message {
    Message::assistant().with_tool_request_with_metadata(
        id,
        Ok(CallToolRequestParams::new(id.to_string())),
        None,
        Some(meta),
    )
}

fn tool_response(id: &str) -> Message {
    Message::user().with_tool_response(
        id,
        Ok(CallToolResult::success(vec![ContentBlock::text("done")])),
    )
}

fn allowed(id: &str) -> (String, serde_json::Value) {
    (id.to_string(), json!({ PRE_HOOK_ALLOWED_KEY: true }))
}

#[tokio::test]
async fn allow_marks_one_ready_call_allowed_per_pass() -> Result<()> {
    let hook = FakeHook::new(&[]);
    let operation = PreToolCallHookOperation::new(hook.clone());

    let outcome = run(&operation, vec![kickoff(), ready_calls(&["a", "b"])]).await?;

    assert_eq!(outcome.status, Some(RunStatus::Continuing));
    assert_eq!(outcome.marks, vec![allowed("a")]);
    assert_eq!(hook.called(), vec!["a"]);

    let outcome = run(
        &operation,
        vec![
            kickoff(),
            marked_call(
                "a",
                json!({ EXECUTABLE_KEY: true, PRE_HOOK_ALLOWED_KEY: true }),
            ),
            ready_calls(&["b"]),
        ],
    )
    .await?;

    assert_eq!(outcome.marks, vec![allowed("b")]);
    assert_eq!(hook.called(), vec!["a", "b"]);
    Ok(())
}

#[tokio::test]
async fn deny_responds_with_the_reason_and_leaves_no_mark() -> Result<()> {
    let operation = PreToolCallHookOperation::new(FakeHook::new(&[("a", Reply::Deny("no"))]));

    let outcome = run(&operation, vec![kickoff(), ready_calls(&["a"])]).await?;

    assert!(outcome.marks.is_empty());
    assert_eq!(
        outcome.responses(),
        vec![("a".to_string(), "no".to_string())]
    );
    Ok(())
}

#[tokio::test]
async fn skips_calls_that_cannot_run_or_were_already_allowed() -> Result<()> {
    let hook = FakeHook::new(&[]);

    let outcome = run(
        &PreToolCallHookOperation::new(hook.clone()),
        vec![
            kickoff(),
            Message::assistant()
                .with_tool_request("unmarked", Ok(CallToolRequestParams::new("unmarked"))),
            marked_call("waiting", json!({ EXECUTABLE_KEY: false })),
            marked_call(
                "allowed",
                json!({ EXECUTABLE_KEY: true, PRE_HOOK_ALLOWED_KEY: true }),
            ),
            marked_call(
                "external",
                json!({ EXECUTABLE_KEY: true, TOOL_META_EXTERNAL_DISPATCH_KEY: true }),
            ),
            Message::assistant().with_tool_request_with_metadata(
                "bad",
                Err(ErrorData::invalid_params("unreadable arguments", None)),
                None,
                Some(json!({ EXECUTABLE_KEY: true })),
            ),
            ready_calls(&["done"]),
            tool_response("done"),
        ],
    )
    .await?;

    assert!(outcome.status.is_none());
    assert!(hook.called().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_hook_error_ends_the_run() {
    let operation = PreToolCallHookOperation::new(FakeHook::new(&[("a", Reply::Fail)]));

    let error = run(&operation, vec![kickoff(), ready_calls(&["a"])])
        .await
        .err()
        .unwrap();

    assert_eq!(error.to_string(), "hook crashed");
}
