use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::command::Command;
use goose_agent::operation::{applied, Emitter, Operation, OperationResult};
use goose_agent::slash_command::SlashCommandOperation;
use goose_provider_types::conversation::message::Message;
use goose_provider_types::conversation::Conversation;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct Echo {
    label: &'static str,
    collected: AtomicUsize,
}

impl Echo {
    fn new(label: &'static str) -> Arc<Self> {
        Arc::new(Self {
            label,
            collected: AtomicUsize::new(0),
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Operation<(), Message> for Echo {
    fn name(&self) -> &'static str {
        "echo"
    }

    async fn commands(&self, _session: &()) -> Result<Vec<Command<'_, (), Message>>> {
        self.collected.fetch_add(1, Ordering::SeqCst);
        let mut echo = Command::new("echo", "Echo the arguments", move |call| {
            Box::pin(async move {
                applied([
                    Message::assistant().with_text(format!("{}: {}", self.label, call.arguments))
                ])
            })
        });
        echo.aliases.push("say".to_string());
        Ok(vec![echo])
    }
}

async fn dispatch(
    operation: &SlashCommandOperation<'_, (), Message>,
    messages: Vec<Message>,
) -> Result<Option<String>> {
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    let conversation = Conversation::new_unvalidated(messages);
    Ok(match operation.run(&(), &conversation, &emit).await? {
        OperationResult::Applied(result) => Some(result.effects[0].as_concat_text()),
        OperationResult::NotApplicable => None,
    })
}

fn kickoff(text: &str) -> Vec<Message> {
    vec![Message::user().with_text(text)]
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn runs_the_first_command_that_answers_to_a_fresh_kickoff() -> Result<()> {
    let first = Echo::new("first");
    let second = Echo::new("second");
    let operation = SlashCommandOperation::new(vec![first.clone(), second.clone()]);

    assert_eq!(
        dispatch(&operation, kickoff("/SAY hello  world ")).await?,
        Some("first: hello  world".to_string())
    );
    assert_eq!(dispatch(&operation, kickoff("/unknown")).await?, None);
    assert_eq!(first.collected.load(Ordering::SeqCst), 2);

    let mut continuing = kickoff("/echo");
    continuing.push(Message::assistant().with_text("already answered"));
    assert_eq!(dispatch(&operation, continuing).await?, None);
    assert_eq!(
        dispatch(&operation, kickoff("echo plain text")).await?,
        None
    );
    assert_eq!(first.collected.load(Ordering::SeqCst), 2);

    let reversed = SlashCommandOperation::new(vec![first, second]).with_resolver(|mut commands| {
        commands.reverse();
        commands
    });
    assert_eq!(
        dispatch(&reversed, kickoff("/echo")).await?,
        Some("second: ".to_string())
    );
    Ok(())
}
