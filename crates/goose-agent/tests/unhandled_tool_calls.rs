use goose_agent::{
    operation::{ConversationEffect, Emitter, Operation, OperationResult, RunStatus},
    tool_call_state::EXECUTABLE_KEY,
    unhandled_tool_calls::UnhandledToolCallsOperation,
};
use goose_provider_types::conversation::{
    message::{Message, MessageContent, TOOL_META_EXTERNAL_DISPATCH_KEY},
    Conversation,
};
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Outcome {
    status: Option<RunStatus>,
    responses: Vec<(String, String)>,
}

async fn run(messages: Vec<Message>) -> Outcome {
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    let conversation = Conversation::new_unvalidated(messages);
    let result = Operation::<(), ConversationEffect>::run(
        &UnhandledToolCallsOperation::new(),
        &(),
        &conversation,
        &emit,
    )
    .await
    .unwrap();
    let OperationResult::Applied(result) = result else {
        return Outcome::default();
    };
    let mut outcome = Outcome {
        status: Some(result.status),
        ..Outcome::default()
    };
    for effect in result.effects {
        match effect {
            ConversationEffect::AppendMessage(message) => {
                outcome
                    .responses
                    .extend(message.content.into_iter().filter_map(|content| {
                        let MessageContent::ToolResponse(response) = content else {
                            return None;
                        };
                        let result = response.tool_result.unwrap();
                        assert_eq!(result.is_error, Some(true));
                        Some((
                            response.id,
                            result.content[0].as_text().unwrap().text.clone(),
                        ))
                    }));
            }
            _ => panic!("unexpected effect"),
        }
    }
    outcome
}

fn kickoff() -> Message {
    Message::user().with_text("hi")
}

fn call(id: &str, tool_meta: Option<Value>) -> Message {
    Message::assistant().with_tool_request_with_metadata(
        id,
        Ok(CallToolRequestParams::new(id.to_string())),
        None,
        tool_meta,
    )
}

fn marked_call(id: &str, executable: bool) -> Message {
    call(id, Some(json!({ EXECUTABLE_KEY: executable })))
}

fn confirmation_request(id: &str) -> Message {
    Message::assistant()
        .with_action_required(id, id.to_string(), Default::default(), None)
        .user_only()
}

fn tool_responses(ids: &[&str]) -> Message {
    ids.iter().fold(Message::user(), |message, id| {
        message.with_tool_response(
            *id,
            Ok(CallToolResult::success(vec![ContentBlock::text("done")])),
        )
    })
}

fn not_handled(id: &str) -> (String, String) {
    (
        id.to_string(),
        "This tool call was not handled.".to_string(),
    )
}

#[tokio::test]
async fn pauses_when_every_open_call_is_waiting() {
    let outcome = run(vec![
        kickoff(),
        marked_call("a", true),
        tool_responses(&["a"]),
        marked_call("c", false),
        confirmation_request("c"),
    ])
    .await;

    assert_eq!(outcome.status, Some(RunStatus::Yielded));
    assert!(outcome.responses.is_empty());
}

#[tokio::test]
async fn responds_to_open_calls_that_are_not_awaiting_confirmation() {
    let outcome = run(vec![
        kickoff(),
        marked_call("allowed", true),
        call("unchecked", None),
        marked_call("declined", false),
        marked_call("waiting", false),
        confirmation_request("waiting"),
    ])
    .await;

    assert_eq!(outcome.status, Some(RunStatus::Continuing));
    assert_eq!(
        outcome.responses,
        vec![
            not_handled("allowed"),
            not_handled("unchecked"),
            not_handled("declined"),
        ]
    );
}

#[tokio::test]
async fn does_nothing_when_no_call_is_open() {
    let outcome = run(vec![
        kickoff(),
        marked_call("a", true),
        tool_responses(&["a"]),
        call(
            "external",
            Some(json!({ TOOL_META_EXTERNAL_DISPATCH_KEY: true })),
        ),
    ])
    .await;

    assert!(outcome.status.is_none());
}
