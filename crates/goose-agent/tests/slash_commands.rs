use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::command::Command;
use goose_agent::operation::{
    applied, ConversationEffect, Emitter, Operation, OperationFuture, OperationResult,
};
use goose_agent::slash_command::SlashCommandOperation;
use goose_provider_types::conversation::message::Message;
use goose_provider_types::conversation::Conversation;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct TestSession {
    conversation: Conversation,
}

impl TestSession {
    fn with_messages(messages: Vec<Message>) -> Self {
        Self {
            conversation: Conversation::new_unvalidated(messages),
        }
    }
}

struct TestCommandOperation {
    description: &'static str,
    collections: AtomicUsize,
}

impl TestCommandOperation {
    fn new(description: &'static str) -> Self {
        Self {
            description,
            collections: AtomicUsize::new(0),
        }
    }

    fn command<'a>(
        &'a self,
        arguments: &'a str,
        _session: &'a TestSession,
        _conversation: &'a Conversation,
        _emit: &'a Emitter,
    ) -> OperationFuture<'a, Result<OperationResult>> {
        Box::pin(async move {
            applied([Message::assistant()
                .with_text(format!("{}:do-it:{arguments}", self.description))
                .into()])
        })
    }
}

#[async_trait]
impl Operation<TestSession> for TestCommandOperation {
    fn name(&self) -> &'static str {
        "test_command"
    }

    async fn commands<'a>(
        &'a self,
        _session: &'a TestSession,
    ) -> Result<Vec<Command<'a, TestSession, ConversationEffect>>> {
        self.collections.fetch_add(1, Ordering::SeqCst);
        Ok(vec![Command::new(
            "Do-It",
            self.description,
            self,
            Self::command,
        )
        .with_aliases(["alias"])])
    }
}

fn reverse<'a>(
    mut commands: Vec<Command<'a, TestSession, ConversationEffect>>,
) -> Vec<Command<'a, TestSession, ConversationEffect>> {
    commands.reverse();
    commands
}

fn emitter() -> Emitter {
    let (tx, _rx) = mpsc::unbounded_channel();
    Emitter::new(tx, CancellationToken::new())
}

fn command_result(result: OperationResult) -> String {
    let OperationResult::Applied(result) = result else {
        panic!("command was not applied");
    };
    let ConversationEffect::AppendMessage(message) = &result.effects[0] else {
        panic!("command did not append a message");
    };
    message.as_concat_text()
}

#[tokio::test]
async fn slash_dispatch_uses_aliases_arguments_and_the_first_match() -> Result<()> {
    let first = Arc::new(TestCommandOperation::new("first"));
    let second = Arc::new(TestCommandOperation::new("second"));
    let operation = SlashCommandOperation::new(vec![first.clone(), second.clone()]);
    let session = TestSession::with_messages(vec![Message::user().with_text("/ALIAS hello")]);

    let result = operation
        .run(&session, &session.conversation, &emitter())
        .await?;

    assert_eq!(command_result(result), "first:do-it:hello");
    let operation = SlashCommandOperation::new(vec![first, second]).with_resolver(reverse);
    let session = TestSession::with_messages(vec![Message::user().with_text("/do-it")]);

    let result = operation
        .run(&session, &session.conversation, &emitter())
        .await?;

    assert_eq!(command_result(result), "second:do-it:");
    Ok(())
}

#[tokio::test]
async fn ordinary_and_continuing_messages_do_not_collect_commands() -> Result<()> {
    let command = Arc::new(TestCommandOperation::new("first"));
    let operation = SlashCommandOperation::new(vec![command.clone()]);
    let ordinary = TestSession::with_messages(vec![Message::user().with_text("hello")]);

    assert!(matches!(
        operation
            .run(&ordinary, &ordinary.conversation, &emitter())
            .await?,
        OperationResult::NotApplicable
    ));
    assert_eq!(command.collections.load(Ordering::SeqCst), 0);

    let continuing = TestSession::with_messages(vec![
        Message::user().with_text("/do-it"),
        Message::assistant().with_text("already running"),
    ]);
    assert!(matches!(
        operation
            .run(&continuing, &continuing.conversation, &emitter())
            .await?,
        OperationResult::NotApplicable
    ));
    assert_eq!(command.collections.load(Ordering::SeqCst), 0);

    let unknown = TestSession::with_messages(vec![Message::user().with_text("/unknown")]);
    assert!(matches!(
        operation
            .run(&unknown, &unknown.conversation, &emitter())
            .await?,
        OperationResult::NotApplicable
    ));
    assert_eq!(command.collections.load(Ordering::SeqCst), 1);
    Ok(())
}
