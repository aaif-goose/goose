use anyhow::Result;
use goose_agent::{
    max_turns::{MaxTurnsOperation, MAX_TURNS_MESSAGE},
    operation::{Emitter, Operation, OperationResult, RunStatus},
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

async fn run(max_turns: u32, conversation: &Conversation) -> Result<OperationResult<Message>> {
    let operation: &dyn Operation<(), Message> = &MaxTurnsOperation::new(max_turns);
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    operation.run(&(), conversation, &emit).await
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn enforces_max_turns_from_persisted_history() -> Result<()> {
    let mut reset = ongoing_turn(2);
    reset.push(Message::user().with_text("new request"));

    for (name, limit, conversation, exhausted) in [
        ("below limit", 2, ongoing_turn(1), false),
        ("new prompt", 1, reset, false),
        ("zero limit", 0, ongoing_turn(0), true),
        ("exhausted", 1, ongoing_turn(1), true),
    ] {
        match run(limit, &conversation).await? {
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
            }
        }
    }
    Ok(())
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn preserves_a_completed_answer_at_the_limit() -> Result<()> {
    let conversation = Conversation::new_unvalidated([
        Message::user().with_text("request"),
        Message::assistant().with_text("done"),
    ]);

    assert!(matches!(
        run(1, &conversation).await?,
        OperationResult::NotApplicable
    ));
    Ok(())
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn reconstructed_exhaustion_does_not_append_another_error() -> Result<()> {
    let mut conversation = ongoing_turn(1);
    let OperationResult::Applied(exhausted) = run(1, &conversation).await? else {
        panic!("expected exhaustion");
    };
    let [message]: [Message; 1] = exhausted.effects.try_into().unwrap();
    let reloaded = serde_json::from_str(&serde_json::to_string(&message)?)?;
    conversation.push(reloaded);

    let OperationResult::Applied(reconstructed) = run(1, &conversation).await? else {
        panic!("expected the reconstructed operation to yield");
    };
    assert_eq!(reconstructed.status, RunStatus::Yielded);
    assert!(reconstructed.effects.is_empty());
    Ok(())
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn adds_budget_reminder_halfway_through() -> Result<()> {
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
