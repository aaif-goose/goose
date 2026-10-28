use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::{
    events::AgentEvent,
    operation::{ConversationEffect, Emitter, Operation, OperationResult, RunStatus},
    tool_approval::{
        confirmation_response_message, ApprovalDecision, ToolApproval, ToolApprovalOperation,
        DEFAULT_DECLINED_MESSAGE,
    },
    tool_call_state::{APPROVED_KEY, EXECUTABLE_KEY},
};
use goose_provider_types::conversation::{
    message::{
        ActionRequiredData, Message, MessageContent, ToolRequest, TOOL_META_EXTERNAL_DISPATCH_KEY,
    },
    Conversation,
};
use goose_provider_types::permission::Permission;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, ErrorData};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy)]
enum Reply {
    Allow,
    Deny(&'static str),
    Ask(Option<&'static str>),
    Fail,
}

#[derive(Default)]
struct FakeApproval {
    replies: Vec<(&'static str, Reply)>,
    checked: Mutex<Vec<Vec<String>>>,
    told: Mutex<Vec<(String, Permission)>>,
}

impl FakeApproval {
    fn new(replies: &[(&'static str, Reply)]) -> Arc<Self> {
        Arc::new(Self {
            replies: replies.to_vec(),
            ..Default::default()
        })
    }

    fn checked(&self) -> Vec<Vec<String>> {
        self.checked.lock().unwrap().clone()
    }

    fn told(&self) -> Vec<(String, Permission)> {
        self.told.lock().unwrap().clone()
    }
}

#[async_trait]
impl ToolApproval<()> for FakeApproval {
    async fn check(
        &self,
        _session: &(),
        _conversation: &Conversation,
        requests: &[&ToolRequest],
    ) -> Result<Vec<ApprovalDecision>> {
        self.checked
            .lock()
            .unwrap()
            .push(requests.iter().map(|request| request.id.clone()).collect());
        requests
            .iter()
            .map(|request| {
                let reply = self
                    .replies
                    .iter()
                    .find(|(id, _)| *id == request.id)
                    .map_or(Reply::Allow, |(_, reply)| *reply);
                match reply {
                    Reply::Allow => Ok(ApprovalDecision::Allow),
                    Reply::Deny(reason) => Ok(ApprovalDecision::Deny {
                        reason: reason.to_string(),
                    }),
                    Reply::Ask(prompt) => Ok(ApprovalDecision::Ask {
                        prompt: prompt.map(str::to_string),
                    }),
                    Reply::Fail => anyhow::bail!("approval crashed"),
                }
            })
            .collect()
    }

    async fn on_confirmation(
        &self,
        _session: &(),
        request: &ToolRequest,
        permission: &Permission,
    ) -> Result<()> {
        self.told
            .lock()
            .unwrap()
            .push((request.id.clone(), permission.clone()));
        Ok(())
    }
}

struct Outcome {
    status: Option<RunStatus>,
    marks: Vec<(String, serde_json::Value)>,
    appended: Vec<Message>,
    shown: Vec<Message>,
}

impl Outcome {
    fn responses(&self) -> Vec<(String, String)> {
        self.appended.iter().flat_map(tool_responses).collect()
    }

    fn shown_responses(&self) -> Vec<(String, String)> {
        self.shown.iter().flat_map(tool_responses).collect()
    }

    fn confirmation_requests(&self) -> Vec<&Message> {
        self.appended
            .iter()
            .filter(|message| confirmation_request_of(message).is_some())
            .collect()
    }
}

async fn run(operation: &ToolApprovalOperation<'_, ()>, messages: Vec<Message>) -> Result<Outcome> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    let conversation = Conversation::new_unvalidated(messages);
    let result =
        Operation::<(), ConversationEffect>::run(operation, &(), &conversation, &emit).await?;
    let shown = std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|event| match event {
            AgentEvent::Message(message) => Some(message),
            _ => None,
        })
        .collect();
    let mut outcome = Outcome {
        status: None,
        marks: Vec::new(),
        appended: Vec::new(),
        shown,
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

fn confirmation_request_of(message: &Message) -> Option<(String, String, Option<String>)> {
    message.content.iter().find_map(|content| match content {
        MessageContent::ActionRequired(action) => match &action.data {
            ActionRequiredData::ToolConfirmation {
                id,
                tool_name,
                prompt,
                ..
            } => Some((id.clone(), tool_name.clone(), prompt.clone())),
            _ => None,
        },
        _ => None,
    })
}

fn kickoff() -> Message {
    Message::user().with_text("hi")
}

fn calls(ids: &[&str]) -> Message {
    ids.iter().fold(Message::assistant(), |message, id| {
        message.with_tool_request(*id, Ok(CallToolRequestParams::new(id.to_string())))
    })
}

fn marked_call(id: &str, executable: bool) -> Message {
    Message::assistant().with_tool_request_with_metadata(
        id,
        Ok(CallToolRequestParams::new(id.to_string())),
        None,
        Some(json!({ EXECUTABLE_KEY: executable })),
    )
}

fn confirmation_request(id: &str) -> Message {
    Message::assistant()
        .with_action_required(id, id.to_string(), Default::default(), None)
        .user_only()
}

fn click(id: &str, permission: Permission) -> Message {
    confirmation_response_message(id, permission)
}

fn tool_response(id: &str) -> Message {
    Message::user().with_tool_response(
        id,
        Ok(CallToolResult::success(vec![ContentBlock::text("done")])),
    )
}

fn approved(id: &str) -> (String, serde_json::Value) {
    (
        id.to_string(),
        json!({ APPROVED_KEY: true, EXECUTABLE_KEY: false }),
    )
}

fn waiting(id: &str) -> (String, serde_json::Value) {
    (id.to_string(), json!({ EXECUTABLE_KEY: false }))
}

fn approved_call(id: &str) -> Message {
    Message::assistant().with_tool_request_with_metadata(
        id,
        Ok(CallToolRequestParams::new(id.to_string())),
        None,
        Some(json!({ APPROVED_KEY: true, EXECUTABLE_KEY: false })),
    )
}

#[tokio::test]
async fn allow_marks_the_call_approved_but_not_ready_to_run() -> Result<()> {
    let approval = FakeApproval::new(&[]);

    let outcome = run(
        &ToolApprovalOperation::new(approval.clone()),
        vec![kickoff(), calls(&["a"])],
    )
    .await?;

    assert_eq!(outcome.status, Some(RunStatus::Continuing));
    assert_eq!(outcome.marks, vec![approved("a")]);
    assert!(outcome.appended.is_empty());
    assert_eq!(approval.checked(), vec![vec!["a"]]);
    Ok(())
}

#[tokio::test]
async fn deny_streams_an_error_response_with_the_reason_and_leaves_no_mark() -> Result<()> {
    let operation = ToolApprovalOperation::new(FakeApproval::new(&[("a", Reply::Deny("no"))]));

    let outcome = run(&operation, vec![kickoff(), calls(&["a"])]).await?;

    let expected = vec![("a".to_string(), "no".to_string())];
    assert!(outcome.marks.is_empty());
    assert_eq!(outcome.responses(), expected);
    assert_eq!(outcome.shown_responses(), expected);
    Ok(())
}

#[tokio::test]
async fn ask_marks_false_and_saves_a_confirmation_request_without_streaming_it() -> Result<()> {
    let operation =
        ToolApprovalOperation::new(FakeApproval::new(&[("a", Reply::Ask(Some("careful")))]));

    let outcome = run(&operation, vec![kickoff(), calls(&["a"])]).await?;

    assert_eq!(outcome.marks, vec![waiting("a")]);
    let confirmation_requests = outcome.confirmation_requests();
    assert_eq!(confirmation_requests.len(), 1);
    assert_eq!(
        confirmation_request_of(confirmation_requests[0]),
        Some((
            "a".to_string(),
            "a".to_string(),
            Some("careful".to_string())
        ))
    );
    assert!(confirmation_requests[0].is_user_visible());
    assert!(!confirmation_requests[0].is_agent_visible());
    assert!(confirmation_requests[0].id.is_some());
    assert!(outcome.responses().is_empty());
    assert!(outcome.shown.is_empty());
    Ok(())
}

#[tokio::test]
async fn checks_a_mixed_batch_in_one_call() -> Result<()> {
    let approval = FakeApproval::new(&[
        ("a", Reply::Allow),
        ("b", Reply::Deny("no")),
        ("c", Reply::Ask(None)),
    ]);

    let outcome = run(
        &ToolApprovalOperation::new(approval.clone()),
        vec![kickoff(), calls(&["a", "b", "c"])],
    )
    .await?;

    assert_eq!(outcome.status, Some(RunStatus::Continuing));
    assert_eq!(outcome.marks, vec![approved("a"), waiting("c")]);
    assert_eq!(
        outcome.responses(),
        vec![("b".to_string(), "no".to_string())]
    );
    assert_eq!(outcome.confirmation_requests().len(), 1);
    assert_eq!(approval.checked(), vec![vec!["a", "b", "c"]]);
    Ok(())
}

#[tokio::test]
async fn leaves_waiting_approved_and_ready_calls_alone() -> Result<()> {
    let approval = FakeApproval::new(&[]);

    let outcome = run(
        &ToolApprovalOperation::new(approval.clone()),
        vec![
            kickoff(),
            marked_call("waiting", false),
            confirmation_request("waiting"),
            approved_call("approved"),
            marked_call("ready", true),
        ],
    )
    .await?;

    assert!(outcome.status.is_none());
    assert!(approval.checked().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_user_allow_tells_the_approval_once_and_marks_the_call_approved() -> Result<()> {
    let approval = FakeApproval::new(&[]);
    let operation = ToolApprovalOperation::new(approval.clone());
    let choice = click("c", Permission::AlwaysAllow);
    assert!(!choice.is_user_visible());
    assert!(!choice.is_agent_visible());
    assert!(choice.id.is_some());

    let outcome = run(
        &operation,
        vec![
            kickoff(),
            marked_call("c", false),
            confirmation_request("c"),
            choice,
        ],
    )
    .await?;

    assert_eq!(outcome.marks, vec![approved("c")]);
    assert_eq!(
        approval.told(),
        vec![("c".to_string(), Permission::AlwaysAllow)]
    );
    assert!(approval.checked().is_empty());

    let outcome = run(
        &operation,
        vec![
            kickoff(),
            approved_call("c"),
            confirmation_request("c"),
            click("c", Permission::AlwaysAllow),
        ],
    )
    .await?;

    assert!(outcome.status.is_none());
    assert_eq!(approval.told().len(), 1);
    Ok(())
}

#[tokio::test]
async fn handles_one_user_answer_per_pass() -> Result<()> {
    let approval = FakeApproval::new(&[]);
    let operation = ToolApprovalOperation::new(approval.clone());

    let outcome = run(
        &operation,
        vec![
            kickoff(),
            marked_call("a", false),
            marked_call("b", false),
            confirmation_request("a"),
            confirmation_request("b"),
            click("a", Permission::AllowOnce),
            click("b", Permission::DenyOnce),
        ],
    )
    .await?;

    assert_eq!(outcome.marks, vec![approved("a")]);
    assert!(outcome.responses().is_empty());
    assert_eq!(
        approval.told(),
        vec![("a".to_string(), Permission::AllowOnce)]
    );

    let outcome = run(
        &operation,
        vec![
            kickoff(),
            approved_call("a"),
            marked_call("b", false),
            confirmation_request("a"),
            confirmation_request("b"),
            click("a", Permission::AllowOnce),
            click("b", Permission::DenyOnce),
        ],
    )
    .await?;

    assert!(outcome.marks.is_empty());
    assert_eq!(
        outcome.responses(),
        vec![("b".to_string(), DEFAULT_DECLINED_MESSAGE.to_string())]
    );
    assert_eq!(
        approval.told(),
        vec![
            ("a".to_string(), Permission::AllowOnce),
            ("b".to_string(), Permission::DenyOnce)
        ]
    );
    Ok(())
}

#[tokio::test]
async fn applies_a_user_answer_before_checking_new_calls() -> Result<()> {
    let approval = FakeApproval::new(&[]);

    let outcome = run(
        &ToolApprovalOperation::new(approval.clone()),
        vec![
            kickoff(),
            marked_call("answered", false),
            confirmation_request("answered"),
            click("answered", Permission::AllowOnce),
            calls(&["new"]),
        ],
    )
    .await?;

    assert_eq!(outcome.marks, vec![approved("answered")]);
    assert!(approval.checked().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_user_deny_responds_with_the_declined_message() -> Result<()> {
    let conversation = || {
        vec![
            kickoff(),
            marked_call("c", false),
            confirmation_request("c"),
            click("c", Permission::DenyOnce),
        ]
    };

    let approval = FakeApproval::new(&[]);
    let outcome = run(
        &ToolApprovalOperation::new(approval.clone()),
        conversation(),
    )
    .await?;
    assert!(outcome.marks.is_empty());
    assert_eq!(
        outcome.responses(),
        vec![("c".to_string(), DEFAULT_DECLINED_MESSAGE.to_string())]
    );
    assert_eq!(
        approval.told(),
        vec![("c".to_string(), Permission::DenyOnce)]
    );

    let custom = ToolApprovalOperation::new(FakeApproval::new(&[])).with_declined_message("nope");
    let outcome = run(&custom, conversation()).await?;
    assert_eq!(
        outcome.responses(),
        vec![("c".to_string(), "nope".to_string())]
    );
    Ok(())
}

#[tokio::test]
async fn skips_unreadable_external_and_responded_calls() -> Result<()> {
    let approval = FakeApproval::new(&[]);

    let outcome = run(
        &ToolApprovalOperation::new(approval.clone()),
        vec![
            kickoff(),
            Message::assistant()
                .with_tool_request(
                    "bad",
                    Err(ErrorData::invalid_params("unreadable arguments", None)),
                )
                .with_tool_request_with_metadata(
                    "external",
                    Ok(CallToolRequestParams::new("external")),
                    None,
                    Some(json!({ TOOL_META_EXTERNAL_DISPATCH_KEY: true })),
                ),
            calls(&["done"]),
            tool_response("done"),
        ],
    )
    .await?;

    assert!(outcome.status.is_none());
    assert!(approval.checked().is_empty());
    Ok(())
}

#[tokio::test]
async fn an_approval_error_ends_the_run() {
    let operation = ToolApprovalOperation::new(FakeApproval::new(&[("a", Reply::Fail)]));

    let error = run(&operation, vec![kickoff(), calls(&["a"])])
        .await
        .err()
        .unwrap();

    assert_eq!(error.to_string(), "approval crashed");
}

struct TooFewDecisions;

#[async_trait]
impl ToolApproval<()> for TooFewDecisions {
    async fn check(
        &self,
        _session: &(),
        _conversation: &Conversation,
        _requests: &[&ToolRequest],
    ) -> Result<Vec<ApprovalDecision>> {
        Ok(vec![ApprovalDecision::Allow])
    }
}

#[tokio::test]
async fn a_decision_count_that_does_not_match_the_calls_ends_the_run() {
    let operation = ToolApprovalOperation::new(Arc::new(TooFewDecisions));

    let error = run(&operation, vec![kickoff(), calls(&["a", "b"])])
        .await
        .err()
        .unwrap();

    assert_eq!(
        error.to_string(),
        "tool approval returned 1 decisions for 2 tool calls"
    );
}
