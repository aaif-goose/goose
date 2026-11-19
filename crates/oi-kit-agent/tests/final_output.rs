use std::sync::Arc;

use oi_kit_agent::final_output::{
    FinalOutputOperation, FinalOutputSchemaSource, FinalOutputTool,
    FINAL_OUTPUT_CONTINUATION_MESSAGE, FINAL_OUTPUT_NOT_ALONE_MESSAGE,
    FINAL_OUTPUT_SUCCESS_MESSAGE, FINAL_OUTPUT_TOOL_NAME,
};
use oi_kit_agent::machine::MachineSession;
use oi_kit_agent::operation::{
    ConversationEffect, Emitter, Operation, OperationResult, RunStatus, StepResult,
};
use oi_kit_provider_types::conversation::message::{Message, MessageContent};
use oi_kit_provider_types::conversation::Conversation;
use rmcp::model::{CallToolRequestParams, CallToolResult, ErrorCode, ErrorData, Role, Tool};
use rmcp::object;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

struct Session {
    schema: Option<Value>,
    conversation: Conversation,
}

impl MachineSession for Session {
    fn id(&self) -> &str {
        "final-output-test"
    }

    fn conversation(&self) -> Option<&Conversation> {
        Some(&self.conversation)
    }
}

struct SessionSchemaSource;

impl FinalOutputSchemaSource<Session> for SessionSchemaSource {
    fn json_schema(&self, session: &Session) -> Option<Value> {
        session.schema.clone()
    }
}

fn answer_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "answer": { "type": "string" } },
        "required": ["answer"]
    })
}

fn operation(supports_builtin_tools: bool) -> FinalOutputOperation<Session> {
    FinalOutputOperation::new(Arc::new(SessionSchemaSource), supports_builtin_tools)
}

fn session(schema: Option<Value>, messages: Vec<Message>) -> Session {
    let mut conversation = vec![Message::user().with_text("go").with_id("kickoff")];
    conversation.extend(messages);
    Session {
        schema,
        conversation: Conversation::new_unvalidated(conversation),
    }
}

fn final_output_call(id: &str, arguments: Value) -> Message {
    Message::assistant().with_tool_request(
        id,
        Ok(CallToolRequestParams::new(FINAL_OUTPUT_TOOL_NAME)
            .with_arguments(arguments.as_object().unwrap().clone())),
    )
}

async fn tools(operation: &FinalOutputOperation<Session>, session: &Session) -> Vec<Tool> {
    Operation::<Session, ConversationEffect>::inference_tools(operation, session)
        .await
        .unwrap()
}

async fn prompt(
    operation: &FinalOutputOperation<Session>,
    session: &Session,
) -> Vec<(String, String)> {
    Operation::<Session, ConversationEffect>::prompt_parts(
        operation,
        session,
        &session.conversation,
    )
    .await
    .unwrap()
}

async fn run(
    operation: &FinalOutputOperation<Session>,
    session: &Session,
) -> Option<StepResult<ConversationEffect>> {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    match Operation::<Session, ConversationEffect>::run(
        operation,
        session,
        &session.conversation,
        &emit,
    )
    .await
    .unwrap()
    {
        OperationResult::NotApplicable => None,
        OperationResult::Applied(result) => Some(result),
    }
}

fn appended(result: &StepResult<ConversationEffect>) -> Vec<&Message> {
    result
        .effects
        .iter()
        .filter_map(|effect| match effect {
            ConversationEffect::AppendMessage(message) => Some(message),
            _ => None,
        })
        .collect()
}

fn tool_result(message: &Message) -> &Result<CallToolResult, ErrorData> {
    match &message.content[0] {
        MessageContent::ToolResponse(response) => &response.tool_result,
        other => panic!("expected a tool response, got {other:?}"),
    }
}

#[test]
fn rejects_an_empty_schema() {
    assert_eq!(
        FinalOutputTool::try_new(json!({})).err().unwrap(),
        "empty json_schema is not allowed"
    );
}

#[test]
fn rejects_an_invalid_schema() {
    assert!(FinalOutputTool::try_new(json!({
        "type": "invalid_type",
        "properties": { "message": { "type": "unknown_type" } }
    }))
    .is_err());
}

#[test]
fn rejects_an_invalid_pattern() {
    assert!(FinalOutputTool::try_new(json!({
        "type": "object",
        "properties": { "message": { "type": "string", "pattern": "[" } }
    }))
    .is_err());
}

#[test]
fn reports_validation_failures() {
    let tool = FinalOutputTool::try_new(json!({
        "type": "object",
        "properties": {
            "message": { "type": "string" },
            "count": { "type": "number" }
        },
        "required": ["message", "count"]
    }))
    .unwrap();

    let error = tool
        .validate(Some(object!({ "message": "Hello" })))
        .unwrap_err();

    assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
    assert!(error.message.contains("Validation failed"));
}

#[test]
fn accepts_valid_output_as_a_single_line() {
    let tool = FinalOutputTool::try_new(json!({
        "type": "object",
        "properties": {
            "user": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "age": { "type": "number" }
                },
                "required": ["name", "age"]
            },
            "tags": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["user", "tags"]
    }))
    .unwrap();

    let output = tool
        .validate(Some(object!({
            "user": { "name": "John", "age": 30 },
            "tags": ["developer", "rust"]
        })))
        .unwrap();

    assert!(serde_json::from_str::<Value>(&output).is_ok());
    assert!(!output.contains('\n'));
}

#[tokio::test]
async fn offers_the_tool_only_when_the_session_has_a_schema() {
    let operation = operation(true);
    let with_schema = session(Some(answer_schema()), vec![]);
    let without_schema = session(None, vec![]);

    let offered_tools = tools(&operation, &with_schema).await;
    let prompt_parts = prompt(&operation, &with_schema).await;

    assert_eq!(offered_tools.len(), 1);
    assert_eq!(offered_tools[0].name, FINAL_OUTPUT_TOOL_NAME);
    assert_eq!(prompt_parts.len(), 1);
    assert_eq!(prompt_parts[0].0, "final_output");
    assert!(tools(&operation, &without_schema).await.is_empty());
    assert!(run(&operation, &without_schema).await.is_none());
}

#[tokio::test]
async fn accepts_a_valid_call() {
    let session = session(
        Some(answer_schema()),
        vec![final_output_call("call", json!({ "answer": "42" }))],
    );

    let result = run(&operation(true), &session).await.unwrap();
    let messages = appended(&result);

    assert_eq!(result.status, RunStatus::Continuing);
    assert_eq!(messages.len(), 2);
    let tool_result = tool_result(messages[0]).as_ref().unwrap();
    assert_eq!(
        tool_result.content[0].as_text().unwrap().text,
        FINAL_OUTPUT_SUCCESS_MESSAGE
    );
    assert_eq!(messages[1].role, Role::Assistant);
    assert_eq!(messages[1].as_concat_text(), r#"{"answer":"42"}"#);
}

#[tokio::test]
async fn answers_an_invalid_call_with_the_validation_error() {
    let session = session(
        Some(answer_schema()),
        vec![final_output_call("call", json!({ "wrong": "field" }))],
    );

    let result = run(&operation(true), &session).await.unwrap();
    let messages = appended(&result);

    assert_eq!(messages.len(), 1);
    let error = tool_result(messages[0]).as_ref().unwrap_err();
    assert!(error.message.contains("Validation failed"));
}

#[tokio::test]
async fn refuses_a_call_made_together_with_other_tools() {
    let call = final_output_call("call", json!({ "answer": "42" }))
        .with_tool_request("other", Ok(CallToolRequestParams::new("shell")));
    let session = session(Some(answer_schema()), vec![call]);

    let result = run(&operation(true), &session).await.unwrap();
    let messages = appended(&result);

    assert_eq!(messages.len(), 1);
    let error = tool_result(messages[0]).as_ref().unwrap_err();
    assert_eq!(error.message, FINAL_OUTPUT_NOT_ALONE_MESSAGE);
}

#[tokio::test]
async fn nudges_the_model_when_the_turn_ends_without_a_call() {
    let session = session(
        Some(answer_schema()),
        vec![Message::assistant().with_text("done")],
    );

    let result = run(&operation(true), &session).await.unwrap();
    let messages = appended(&result);

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].as_concat_text(),
        FINAL_OUTPUT_CONTINUATION_MESSAGE
    );
    assert!(!messages[0].is_user_visible());
}

#[tokio::test]
async fn stops_with_an_error_when_the_provider_cannot_offer_the_tool() {
    let session = session(Some(answer_schema()), vec![]);

    let result = run(&operation(false), &session).await.unwrap();
    let messages = appended(&result);

    assert_eq!(result.status, RunStatus::Yielded);
    assert_eq!(messages.len(), 1);
    assert!(messages[0]
        .as_concat_text()
        .contains("the provider can't receive the `final_output` tool"));
    assert!(result.effects.iter().any(|effect| matches!(
        effect,
        ConversationEffect::SetMessageVisibility {
            message_id,
            user_visible: true,
            agent_visible: false,
        } if message_id == "kickoff"
    )));
}

#[tokio::test]
async fn uses_the_unsupported_message_it_is_given() {
    let session = session(Some(answer_schema()), vec![]);
    let operation = operation(false).with_unsupported_message("use another provider");

    let result = run(&operation, &session).await.unwrap();

    assert_eq!(
        appended(&result)[0].as_concat_text(),
        "use another provider"
    );
}
