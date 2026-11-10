use anyhow::Result;
use rmcp::model::Role;

use super::calculator_extension::{value, ADD};
use super::pipeline::test_pipeline;
use super::pipeline::MessageKind::Agent;
use crate::agents::state_machine::ops_llm::{ADVERTISED_TOOLS_NOTE, LLM_OPERATION_NAME};
use crate::agents::state_machine::ops_toolcalling::request_was_advertised;
use crate::config::GooseMode;
use crate::conversation::message::{Message, MessageContent};
use crate::permission::Permission;

fn advertised_tool_names(message: &Message) -> Option<&Vec<serde_json::Value>> {
    message
        .metadata
        .operation_note(LLM_OPERATION_NAME, ADVERTISED_TOOLS_NOTE)
        .and_then(serde_json::Value::as_array)
}

fn persisted_advertised_tools_bytes(messages: &[Message]) -> usize {
    messages
        .iter()
        .filter(|message| message.role == Role::Assistant)
        .map(|message| {
            advertised_tool_names(message)
                .and_then(|names| serde_json::to_string(names).ok())
                .map(|names| names.len())
                .unwrap_or(0)
        })
        .sum()
}

#[tokio::test]
async fn completed_turns_do_not_persist_repeated_advertised_tools() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    for turn in ["one", "two", "three"] {
        api.on(turn).reply(turn);
    }

    let result = pipeline.run(["one", "two", "three"]).await?;
    let messages = result.conversation().messages();
    let copies = messages
        .iter()
        .filter(|message| advertised_tool_names(message).is_some())
        .count();
    let bytes = persisted_advertised_tools_bytes(messages);

    assert_eq!(
        copies, 0,
        "completed assistant messages persisted advertised_tools {copies} times ({bytes} bytes); metadata grows with tools×turns"
    );
    Ok(())
}

#[tokio::test]
async fn pending_approval_keeps_advertised_tools_until_the_tool_is_resolved() -> Result<()> {
    let (pipeline, api) = test_pipeline().await?;
    let pipeline = pipeline.with_goose_mode(GooseMode::Approve).await;
    api.on("add one").call(ADD, value(1));
    api.on("result: 1").reply("done");

    let awaiting = pipeline.run(["add one"]).await?;
    let pending = tool_request(awaiting.conversation().messages());
    assert!(
        request_was_advertised(awaiting.conversation().messages(), pending.request),
        "the current turn must still treat the pending call as advertised"
    );
    assert!(
        advertised_tool_names(pending.message)
            .is_some_and(|names| names.iter().any(|name| name.as_str() == Some(ADD))),
        "a tool request awaiting approval must keep advertised_tools so a resume can validate it"
    );

    let reloaded = pipeline
        .session_manager
        .get_session(&pipeline.session_id, true)
        .await?;
    let reloaded_messages = reloaded
        .conversation
        .as_ref()
        .expect("reloaded session")
        .messages();
    let reloaded_pending = tool_request(reloaded_messages);
    assert!(request_was_advertised(
        reloaded_messages,
        reloaded_pending.request
    ));

    pipeline
        .confirm(&pending.request.id, Permission::AllowOnce)
        .await?;
    let result = pipeline.resume().await?;
    result.assert_message(-1, Agent, "done");
    assert_eq!(pipeline.calculator_total(), 1);
    assert_eq!(
        persisted_advertised_tools_bytes(result.conversation().messages()),
        0,
        "resolved turns must drop advertised_tools from durable metadata"
    );
    Ok(())
}

#[tokio::test]
async fn reused_id_pending_approval_keeps_advertised_tools_after_copy_and_import() -> Result<()> {
    use crate::conversation::Conversation;
    use rmcp::model::{CallToolRequestParams, CallToolResult};

    let (pipeline, api) = test_pipeline().await?;
    let pipeline = pipeline.with_goose_mode(GooseMode::Approve).await;
    api.on("add one").calls([("reused-call", ADD, value(1))]);
    api.on("result: 1").reply("done");

    let awaiting = pipeline.run(["add one"]).await?;
    let mut prior = tool_request(awaiting.conversation().messages())
        .message
        .clone();
    prior.content.clear();
    prior = prior.with_tool_request("reused-call", Ok(CallToolRequestParams::new(ADD)));
    let mut history = vec![
        prior,
        Message::user().with_tool_response("reused-call", Ok(CallToolResult::success(vec![]))),
    ];
    history.extend_from_slice(awaiting.conversation().messages());
    for message in &mut history {
        message.created = 100;
    }
    pipeline
        .session_manager
        .replace_conversation(
            &pipeline.session_id,
            &Conversation::new_unvalidated(history),
        )
        .await?;
    let copied = pipeline
        .session_manager
        .copy_session(&pipeline.session_id, "copy".into())
        .await?;
    let exported = pipeline.session_manager.export_session(&copied.id).await?;
    let imported = pipeline
        .session_manager
        .import_session(&exported, None)
        .await?;
    pipeline
        .session_manager
        .replace_conversation(
            &pipeline.session_id,
            imported.conversation.as_ref().unwrap(),
        )
        .await?;

    let reloaded = pipeline
        .session_manager
        .get_session(&pipeline.session_id, true)
        .await?;
    let messages = reloaded.conversation.as_ref().unwrap().messages();
    assert!(advertised_tool_names(&messages[0]).is_none());
    assert!(request_was_advertised(
        &messages[2..],
        tool_request(messages).request
    ));
    pipeline
        .confirm("reused-call", Permission::AllowOnce)
        .await?;
    let result = pipeline.resume().await?;
    result.assert_message(-1, Agent, "done");
    assert_eq!(pipeline.calculator_total(), 1);
    assert_eq!(
        persisted_advertised_tools_bytes(result.conversation().messages()),
        0
    );
    Ok(())
}

struct PendingToolRequest<'a> {
    message: &'a Message,
    request: &'a crate::conversation::message::ToolRequest,
}

fn tool_request(messages: &[Message]) -> PendingToolRequest<'_> {
    let message = messages
        .iter()
        .rev()
        .find(|message| {
            message
                .content
                .iter()
                .any(|content| content.as_tool_request().is_some())
        })
        .expect("assistant tool request");
    let request = message
        .content
        .iter()
        .find_map(MessageContent::as_tool_request)
        .expect("tool request");
    PendingToolRequest { message, request }
}
