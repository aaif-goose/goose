use anyhow::Result;
use goose_agent::{
    operation::{Emitter, Operation, OperationResult, RunStatus},
    turns::{MaxTurnsOperation, MAX_TURNS_MESSAGE},
};
use goose_provider_types::conversation::{
    message::{Message, MessageErrorKind},
    Conversation,
};
use rmcp::model::{CallToolRequestParams, CallToolResult};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

fn ongoing_turn(turns: u32) -> Conversation {
    let mut messages = vec![Message::user().with_text("request")];
    for turn in 0..turns {
        let id = format!("call-{turn}");
        messages.push(
            Message::assistant()
                .with_tool_request(id.clone(), Ok(CallToolRequestParams::new("work"))),
        );
        messages.push(Message::user().with_tool_response(id, Ok(CallToolResult::success(vec![]))));
    }
    Conversation::new_unvalidated(messages)
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn standalone_limit_preserves_completion_and_resets_on_user_input() -> Result<()> {
    let mut reset = ongoing_turn(2);
    reset.push(Message::user().with_text("new request"));
    let finished = Conversation::new_unvalidated([
        Message::user().with_text("request"),
        Message::assistant().with_text("done"),
    ]);
    for (name, limit, mut conversation, exhausted) in [
        ("below limit", 2, ongoing_turn(1), false),
        ("new prompt", 1, reset, false),
        ("zero limit", 0, ongoing_turn(0), true),
        ("finished at limit", 1, finished, false),
        ("exhausted", 1, ongoing_turn(1), true),
    ] {
        let operation: &dyn Operation<(), Message> = &MaxTurnsOperation::new(limit);
        let (tx, _rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, CancellationToken::new());
        match operation.run(&(), &conversation, &emit).await? {
            OperationResult::NotApplicable => assert!(!exhausted, "{name}"),
            OperationResult::Applied(result) => {
                assert!(exhausted, "{name}");
                assert_eq!(result.status, RunStatus::Yielded);
                let [message]: [Message; 1] = result.effects.try_into().unwrap();
                assert_eq!(
                    message.error_kind(),
                    Some(MessageErrorKind::ResourceExhausted)
                );
                assert_eq!(message.error().unwrap().message, MAX_TURNS_MESSAGE);
                assert!(message.is_user_visible());
                assert!(!message.is_agent_visible());
                let reloaded = serde_json::from_str(&serde_json::to_string(&message)?)?;
                conversation.push(reloaded);
                let rebuilt: &dyn Operation<(), Message> = &MaxTurnsOperation::new(limit);
                let OperationResult::Applied(resumed) =
                    rebuilt.run(&(), &conversation, &emit).await?
                else {
                    panic!("exhaustion must still yield after reconstruction");
                };
                assert_eq!(resumed.status, RunStatus::Yielded);
                assert!(resumed.effects.is_empty());
            }
        }
    }
    Ok(())
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn budget_reminders_start_at_half_the_limit() -> Result<()> {
    for (limit, turns, expected) in [
        (4, 1, None),
        (4, 2, Some("<turn-budget>2/4 used</turn-budget>")),
        (4, 4, Some("<turn-budget>4/4 used</turn-budget>")),
        (3, 1, None),
        (3, 2, Some("<turn-budget>2/3 used</turn-budget>")),
        (0, 0, None),
    ] {
        let operation: &dyn Operation<(), Message> = &MaxTurnsOperation::new(limit);
        let parts = operation.moim_parts(&(), &ongoing_turn(turns)).await?;
        assert_eq!(parts, expected.into_iter().collect::<Vec<_>>());
    }
    Ok(())
}
