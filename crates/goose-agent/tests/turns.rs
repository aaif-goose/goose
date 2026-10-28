use anyhow::Result;
use goose_agent::{
    operation::{Emitter, Operation, OperationResult, RunStatus},
    turns::{turn_limit_reached, MaxTurnsOperation},
};
use goose_provider_types::conversation::{message::Message, Conversation};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn exhaustion_marker_survives_serialization_and_display_text_changes() -> Result<()> {
    let operation: &dyn Operation<(), Message> = &MaxTurnsOperation::new(1, "host exhaustion text");
    let conversation = Conversation::new_unvalidated(vec![
        Message::user().with_text("request"),
        Message::assistant().with_text("partial progress"),
    ]);
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());

    let OperationResult::Applied(result) = operation.run(&(), &conversation, &emit).await? else {
        panic!("expected exhaustion");
    };
    assert_eq!(result.status, RunStatus::Yielded);
    let [message]: [Message; 1] = result.effects.try_into().unwrap();
    assert_eq!(message.as_concat_text(), "host exhaustion text");

    let mut reloaded: Message = serde_json::from_str(&serde_json::to_string(&message)?)?;
    reloaded.content.clear();
    let reloaded = reloaded.with_text("different display text");
    assert!(turn_limit_reached(&reloaded));
    Ok(())
}
