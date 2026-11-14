use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::command::{
    normalize_command_name, parse_slash_command, Command, CommandAction, CommandInvocation,
};
use goose_agent::machine::{MachineSession, StateMachine, Step};
use goose_agent::operation::{applied, ConversationEffect, Emitter, Operation, OperationResult};
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

impl MachineSession for TestSession {
    fn id(&self) -> &str {
        "test"
    }

    fn conversation(&self) -> Option<&Conversation> {
        Some(&self.conversation)
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
        Ok(vec![
            Command::new("Do-It", self.description, self).with_aliases(["alias"])
        ])
    }
}

#[async_trait]
impl CommandAction<TestSession, ConversationEffect> for TestCommandOperation {
    async fn execute(
        &self,
        command: &CommandInvocation<'_>,
        _session: &TestSession,
        _conversation: &Conversation,
        _emit: &Emitter,
    ) -> Result<OperationResult> {
        applied([Message::assistant()
            .with_text(format!(
                "{}:{}:{}",
                self.description, command.name, command.arguments
            ))
            .into()])
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

#[test]
fn parsing_and_normalization_are_shared() {
    let command = parse_slash_command("  /Do-It   some input  ").unwrap();

    assert_eq!(normalize_command_name(command.name), "do-it");
    assert_eq!(command.arguments, "some input");
    assert!(parse_slash_command("Do-It").is_none());
}

#[tokio::test]
async fn state_machine_returns_duplicate_commands_in_step_order() -> Result<()> {
    let first = Arc::new(TestCommandOperation::new("first"));
    let second = Arc::new(TestCommandOperation::new("second"));
    let machine = StateMachine::new(
        vec![Step::Operation(first), Step::Operation(second)],
        CancellationToken::new(),
    );
    let session = TestSession::with_messages(vec![Message::user().with_text("hello")]);

    let commands = machine.commands(&session).await?;

    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].description, "first");
    assert_eq!(commands[1].description, "second");
    Ok(())
}

#[tokio::test]
async fn slash_dispatch_uses_aliases_arguments_and_the_first_match() -> Result<()> {
    let first = Arc::new(TestCommandOperation::new("first"));
    let second = Arc::new(TestCommandOperation::new("second"));
    let operation = SlashCommandOperation::new(vec![first, second]);
    let session = TestSession::with_messages(vec![Message::user().with_text("/ALIAS hello")]);

    let result = operation
        .run(&session, &session.conversation, &emitter())
        .await?;

    assert_eq!(command_result(result), "first:do-it:hello");
    Ok(())
}

#[tokio::test]
async fn slash_dispatch_accepts_another_collision_policy() -> Result<()> {
    let first = Arc::new(TestCommandOperation::new("first"));
    let second = Arc::new(TestCommandOperation::new("second"));
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
