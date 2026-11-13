use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::{
    exit_on_error::ExitOnErrorOperation,
    machine::{MachineSession, StateMachine, Step},
    operation::{yielded, ConversationEffect, Emitter, Operation, OperationResult},
};
use goose_provider_types::{
    conversation::{
        message::{Message, MessageErrorKind},
        Conversation,
    },
    maybe_send::{MaybeSend, MaybeSync},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct Session {
    conversation: Conversation,
}

impl MachineSession for Session {
    fn id(&self) -> &str {
        "session"
    }

    fn conversation(&self) -> Option<&Conversation> {
        Some(&self.conversation)
    }
}

struct RecordingOperation {
    ran: Arc<AtomicBool>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for RecordingOperation
where
    S: MaybeSend + MaybeSync + 'static,
    E: MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        "recording"
    }

    async fn run(
        &self,
        _session: &S,
        _conversation: &Conversation,
        _emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        self.ran.store(true, Ordering::SeqCst);
        yielded()
    }
}

fn session(last: Message) -> Session {
    Session {
        conversation: Conversation::new_unvalidated([Message::user().with_text("request"), last]),
    }
}

fn recording(ran: &Arc<AtomicBool>) -> Step<'static, Session> {
    Step::Operation(Arc::new(RecordingOperation {
        ran: Arc::clone(ran),
    }))
}

async fn applied_step(
    steps: Vec<Step<'static, Session>>,
    session: &Session,
) -> Result<Option<&'static str>> {
    let machine = StateMachine::<Session, ConversationEffect>::new(steps, CancellationToken::new());
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, CancellationToken::new());
    Ok(machine.step(session, &emit).await?.applied_step)
}

#[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
async fn marks_the_error_boundary_in_an_ordered_pipeline() -> Result<()> {
    let error =
        || session(Message::assistant().with_error(MessageErrorKind::Other, "provider failed"));

    let after = Arc::new(AtomicBool::new(false));
    let step = applied_step(
        vec![
            Step::Operation(Arc::new(ExitOnErrorOperation)),
            recording(&after),
        ],
        &error(),
    )
    .await?;
    assert_eq!(step, Some("exit_on_error"));
    assert!(!after.load(Ordering::SeqCst));

    let before = Arc::new(AtomicBool::new(false));
    let step = applied_step(
        vec![
            recording(&before),
            Step::Operation(Arc::new(ExitOnErrorOperation)),
        ],
        &error(),
    )
    .await?;
    assert_eq!(step, Some("recording"));
    assert!(before.load(Ordering::SeqCst));

    let after = Arc::new(AtomicBool::new(false));
    let step = applied_step(
        vec![
            Step::Operation(Arc::new(ExitOnErrorOperation)),
            recording(&after),
        ],
        &session(Message::assistant().with_text("done")),
    )
    .await?;
    assert_eq!(step, Some("recording"));
    assert!(after.load(Ordering::SeqCst));

    Ok(())
}
