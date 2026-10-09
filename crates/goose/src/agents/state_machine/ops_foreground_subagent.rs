use async_trait::async_trait;
use futures::future::{self, BoxFuture};
use goose_agent::subagent::{SubagentOutcome, SubagentRunner};

use crate::agents::final_output_tool::FinalOutputTool;
use crate::agents::state_machine::{awaits_tool_responses, messages_since_kickoff, Emitter};
use crate::agents::subagent_handler::ForegroundSubagentRunner;
use crate::conversation::message::{Message, MessageContent, SystemNotificationType};
use crate::conversation::Conversation;
use crate::session::{Session, SessionType};
use crate::utils::safe_truncate;

const TASK_SNIPPET_CHARS: usize = 160;

fn inline_notice(text: String) -> Message {
    Message::assistant().with_system_notification(SystemNotificationType::InlineMessage, text)
}

fn start_notice(subagent_id: &str, task: Option<&str>) -> String {
    let snippet = task
        .map(|task| {
            let task = task.split_whitespace().collect::<Vec<_>>().join(" ");
            format!(" ({})", safe_truncate(&task, TASK_SNIPPET_CHARS))
        })
        .unwrap_or_default();
    format!("Running subagent {subagent_id}{snippet}")
}

fn finished_notice(
    subagent_id: &str,
    outcome: &SubagentOutcome,
    show_output: bool,
) -> Option<String> {
    let notice = match outcome {
        SubagentOutcome::Completed(output) if show_output => {
            format!(
                "Subagent {subagent_id} completed\n\n{}",
                readable_output(output)
            )
        }
        SubagentOutcome::Completed(_) => format!("Subagent {subagent_id} completed"),
        SubagentOutcome::Failed(reason) => format!(
            "Subagent {subagent_id} failed: {}",
            reason.lines().next().unwrap_or_default()
        ),
        SubagentOutcome::Cancelled => return None,
    };
    Some(notice)
}

fn ended(outcome: SubagentOutcome) -> BoxFuture<'static, SubagentOutcome> {
    Box::pin(future::ready(outcome))
}

fn delegated_subagent_ids(content: &[MessageContent]) -> impl Iterator<Item = &str> {
    content.iter().filter_map(|content| {
        let MessageContent::ToolResponse(response) = content else {
            return None;
        };
        let result = response.tool_result.as_ref().ok()?;
        let meta = result.meta.as_ref()?;
        if result.is_error == Some(true)
            || meta.0.get("foreground_subagent") != Some(&serde_json::Value::Bool(true))
        {
            return None;
        }
        meta.0.get("subagent_session_id")?.as_str()
    })
}

fn readable_output(output: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(output) {
        Ok(serde_json::Value::Object(fields)) if fields.len() == 1 => fields
            .get("summary")
            .and_then(serde_json::Value::as_str)
            .map_or_else(|| output.to_string(), str::to_owned),
        _ => output.to_string(),
    }
}

#[async_trait]
impl SubagentRunner<Session> for ForegroundSubagentRunner {
    fn started_subagent_session_ids(
        &self,
        parent_session: &Session,
        conversation: &Conversation,
    ) -> Vec<String> {
        let all_tool_calls_answered = messages_since_kickoff(conversation)
            .is_ok_and(|messages| !awaits_tool_responses(messages));
        if parent_session.session_type == SessionType::SubAgent || !all_tool_calls_answered {
            return Vec::new();
        }
        conversation
            .messages()
            .iter()
            .flat_map(|message| delegated_subagent_ids(&message.content))
            .map(str::to_owned)
            .collect()
    }

    async fn start(
        &self,
        parent_session: &Session,
        subagent_session_id: &str,
        emit: &Emitter,
    ) -> BoxFuture<'static, SubagentOutcome> {
        let subagent = match self.load(subagent_session_id).await {
            Ok(subagent) => subagent,
            Err(error) => return ended(SubagentOutcome::Failed(error.to_string())),
        };
        if subagent.session_type != SessionType::SubAgent
            || subagent.parent_session_id.as_deref() != Some(parent_session.id.as_str())
        {
            return ended(SubagentOutcome::Failed(
                "it does not belong to this session".to_string(),
            ));
        }
        if let Some(output) = subagent
            .conversation
            .as_ref()
            .and_then(|conversation| FinalOutputTool::successful_output(conversation.messages()))
        {
            return ended(SubagentOutcome::Completed(output));
        }
        let task = subagent
            .recipe
            .as_ref()
            .and_then(|recipe| recipe.prompt.as_deref());
        emit.message(inline_notice(start_notice(subagent_session_id, task)))
            .await;
        self.spawn(subagent, emit.cancel_token().clone())
    }

    async fn on_subagent_finished(
        &self,
        subagent_session_id: &str,
        outcome: &SubagentOutcome,
        remaining: usize,
        emit: &Emitter,
    ) {
        let show_output = remaining > 0 || emit.cancel_token().is_cancelled();
        if let Some(notice) = finished_notice(subagent_session_id, outcome, show_output) {
            emit.message(inline_notice(notice)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::Result;
    use goose_agent::events::AgentEvent;
    use goose_agent::subagent::ForegroundSubagentOperation;
    use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, MetaObject};
    use tempfile::TempDir;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::agents::final_output_tool::{FINAL_OUTPUT_SUCCESS_MESSAGE, FINAL_OUTPUT_TOOL_NAME};
    use crate::agents::state_machine::{
        ConversationEffect, GooseEffect, Operation, OperationResult,
    };
    use crate::config::GooseMode;
    use crate::session::SessionManager;
    use goose_agent::machine::EffectHandler;

    const OPERATION_NAME: &str = "foreground_subagent";
    const DELIVERED: &str = "delivered";

    struct Fixture {
        temp_dir: TempDir,
        manager: Arc<SessionManager>,
        parent_id: String,
        subagent_id: String,
    }

    impl Fixture {
        fn runner(&self) -> ForegroundSubagentRunner {
            ForegroundSubagentRunner::new(self.manager.clone(), false)
        }

        fn operation(&self) -> ForegroundSubagentOperation<Session> {
            ForegroundSubagentOperation::new(Arc::new(self.runner()))
        }
    }

    async fn fixture() -> Result<Fixture> {
        let temp_dir = TempDir::new()?;
        let manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
        let parent = manager
            .create_session(
                temp_dir.path().to_path_buf(),
                "parent".to_string(),
                SessionType::User,
                GooseMode::Auto,
            )
            .await?;
        manager
            .add_message(&parent.id, &Message::user().with_text("delegate"))
            .await?;
        let subagent_id = add_delegated_subagent(&manager, &temp_dir, &parent.id).await?;
        Ok(Fixture {
            temp_dir,
            manager,
            parent_id: parent.id,
            subagent_id,
        })
    }

    async fn add_delegated_subagent(
        manager: &SessionManager,
        temp_dir: &TempDir,
        parent_id: &str,
    ) -> Result<String> {
        let subagent = manager
            .create_session(
                temp_dir.path().to_path_buf(),
                "subagent".to_string(),
                SessionType::SubAgent,
                GooseMode::Auto,
            )
            .await?;
        manager
            .update(&subagent.id)
            .parent_session_id(Some(parent_id.to_string()))
            .apply()
            .await?;
        manager
            .add_message(parent_id, &delegate_message(&subagent.id))
            .await?;
        Ok(subagent.id)
    }

    fn delegate_result(subagent_id: &str, foreground: bool) -> CallToolResult {
        let mut meta = MetaObject::new();
        meta.0.insert(
            "foreground_subagent".to_string(),
            serde_json::Value::Bool(foreground),
        );
        meta.0.insert(
            "subagent_session_id".to_string(),
            serde_json::Value::String(subagent_id.to_string()),
        );
        CallToolResult::success(vec![ContentBlock::text(format!(
            "Delegated to foreground subagent {subagent_id}"
        ))])
        .with_meta(Some(meta))
    }

    fn delegate_message(subagent_id: &str) -> Message {
        Message::user().with_tool_response(
            format!("delegate-{subagent_id}"),
            Ok(delegate_result(subagent_id, true)),
        )
    }

    fn hidden_texts(conversation: &Conversation) -> Vec<String> {
        conversation
            .messages()
            .iter()
            .filter(|message| !message.is_user_visible())
            .inspect(|message| assert!(message.is_agent_visible()))
            .map(Message::as_concat_text)
            .collect()
    }

    async fn run_step(
        operation: &ForegroundSubagentOperation<Session>,
        manager: &SessionManager,
        session_id: &str,
        emit: &Emitter,
    ) -> Result<(Session, OperationResult<GooseEffect>)> {
        let session = manager.get_session(session_id, true).await?;
        let conversation = session.conversation.as_ref().unwrap();
        let result =
            Operation::<Session, GooseEffect>::run(operation, &session, conversation, emit).await?;
        Ok((session, result))
    }

    fn delivery_effects(result: OperationResult<GooseEffect>) -> Vec<GooseEffect> {
        let OperationResult::Applied(step) = result else {
            panic!("expected the operation to apply");
        };
        assert!(!step.yield_to_client);
        step.effects
    }

    fn delivered(effect: &GooseEffect) -> &str {
        let GooseEffect::Conversation(ConversationEffect::AppendMessage(message)) = effect else {
            panic!("expected a foreground subagent delivery");
        };
        message
            .metadata
            .operation_note(OPERATION_NAME, DELIVERED)
            .and_then(serde_json::Value::as_str)
            .expect("the result message records its subagent")
    }

    #[tokio::test]
    async fn successful_foreground_delegations_are_the_started_subagents() -> Result<()> {
        let fixture = fixture().await?;
        let mut failed_delegate = delegate_result("failed", true);
        failed_delegate.is_error = Some(true);
        let conversation = Conversation::new_unvalidated(vec![
            Message::user().with_text("delegate"),
            Message::user()
                .with_tool_response("a", Ok(delegate_result("first", true)))
                .with_tool_response("b", Ok(delegate_result("background", false)))
                .with_tool_response("c", Ok(failed_delegate))
                .with_tool_response(
                    "d",
                    Ok(CallToolResult::success(vec![ContentBlock::text("done")])),
                ),
            delegate_message("second"),
        ]);
        let mut parent = fixture
            .manager
            .get_session(&fixture.parent_id, false)
            .await?;
        let runner = fixture.runner();

        assert_eq!(
            runner.started_subagent_session_ids(&parent, &conversation),
            ["first", "second"]
        );
        parent.session_type = SessionType::SubAgent;
        assert!(runner
            .started_subagent_session_ids(&parent, &conversation)
            .is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn waits_for_all_tool_responses_before_running_subagents() -> Result<()> {
        let fixture = fixture().await?;
        let parent = fixture
            .manager
            .get_session(&fixture.parent_id, false)
            .await?;
        let runner = fixture.runner();
        let requests = Message::assistant()
            .with_tool_request("delegate-s1", Ok(CallToolRequestParams::new("delegate")))
            .with_tool_request("other-call", Ok(CallToolRequestParams::new("other")));
        let mut messages = vec![
            Message::user().with_text("delegate"),
            requests,
            delegate_message("s1"),
        ];
        assert!(runner
            .started_subagent_session_ids(&parent, &Conversation::new_unvalidated(messages.clone()))
            .is_empty());

        messages.push(Message::user().with_tool_response(
            "other-call",
            Ok(CallToolResult::success(vec![ContentBlock::text("done")])),
        ));
        assert_eq!(
            runner.started_subagent_session_ids(&parent, &Conversation::new_unvalidated(messages)),
            ["s1"]
        );
        Ok(())
    }

    async fn save_final_output(manager: &SessionManager, subagent_id: &str) -> Result<()> {
        let arguments = serde_json::json!({"summary": "done"})
            .as_object()
            .unwrap()
            .clone();
        manager
            .add_message(
                subagent_id,
                &Message::assistant().with_tool_request(
                    "final-output-call",
                    Ok(CallToolRequestParams::new(FINAL_OUTPUT_TOOL_NAME)
                        .with_arguments(arguments)),
                ),
            )
            .await?;
        manager
            .add_message(
                subagent_id,
                &Message::user().with_tool_response(
                    "final-output-call",
                    Ok(CallToolResult::success(vec![ContentBlock::text(
                        FINAL_OUTPUT_SUCCESS_MESSAGE,
                    )])),
                ),
            )
            .await?;
        Ok(())
    }

    fn notices(rx: &mut mpsc::UnboundedReceiver<AgentEvent>) -> Vec<String> {
        let mut notices = Vec::new();
        while let Ok(event) = rx.try_recv() {
            let AgentEvent::Message(message) = event else {
                continue;
            };
            for content in message.content {
                if let MessageContent::SystemNotification(notification) = content {
                    assert_eq!(
                        notification.notification_type,
                        SystemNotificationType::InlineMessage
                    );
                    notices.push(notification.msg);
                }
            }
        }
        notices
    }

    #[tokio::test]
    async fn saved_outputs_show_results_only_while_other_subagents_wait() -> Result<()> {
        let fixture = fixture().await?;
        save_final_output(&fixture.manager, &fixture.subagent_id).await?;
        let second_subagent_id =
            add_delegated_subagent(&fixture.manager, &fixture.temp_dir, &fixture.parent_id).await?;
        save_final_output(&fixture.manager, &second_subagent_id).await?;
        let cancel = CancellationToken::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, cancel.clone());
        let operation = fixture.operation();

        for (expected_id, expected_notice) in [
            (
                &fixture.subagent_id,
                format!("Subagent {} completed\n\ndone", fixture.subagent_id),
            ),
            (
                &second_subagent_id,
                format!("Subagent {second_subagent_id} completed"),
            ),
        ] {
            let (parent, result) =
                run_step(&operation, &fixture.manager, &fixture.parent_id, &emit).await?;
            let mut effects = delivery_effects(result);
            assert_eq!(effects.len(), 1);
            assert_eq!(delivered(&effects[0]), expected_id);
            assert_eq!(notices(&mut rx), vec![expected_notice]);
            fixture
                .manager
                .apply_effects(&parent, &mut effects, &emit)
                .await?;
        }
        Ok(())
    }

    #[test]
    fn readable_output_unwraps_only_the_default_summary() {
        assert_eq!(readable_output(r#"{"summary":"a poem"}"#), "a poem");
        assert_eq!(
            readable_output(r#"{"summary":"a poem","score":3}"#),
            r#"{"summary":"a poem","score":3}"#
        );
        assert_eq!(
            readable_output(r#"{"result":"done"}"#),
            r#"{"result":"done"}"#
        );
    }

    #[tokio::test]
    async fn failed_subagent_emits_start_and_failure_notices() -> Result<()> {
        let fixture = fixture().await?;
        let cancel = CancellationToken::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, cancel.clone());
        let operation = fixture.operation();
        let (_, result) = run_step(&operation, &fixture.manager, &fixture.parent_id, &emit).await?;
        let effects = delivery_effects(result);
        assert_eq!(effects.len(), 1);
        assert_eq!(delivered(&effects[0]), fixture.subagent_id);

        let reason = format!("Subagent {} has no saved recipe", fixture.subagent_id);
        assert_eq!(
            notices(&mut rx),
            vec![
                format!("Running subagent {}", fixture.subagent_id),
                format!("Subagent {} failed: {reason}", fixture.subagent_id),
            ]
        );
        Ok(())
    }

    #[tokio::test]
    async fn stop_shows_the_output_of_the_last_finished_subagent() -> Result<()> {
        let fixture = fixture().await?;
        let cancel = CancellationToken::new();
        cancel.cancel();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, cancel);

        fixture
            .runner()
            .on_subagent_finished(
                &fixture.subagent_id,
                &SubagentOutcome::Completed(r#"{"summary":"done"}"#.to_string()),
                0,
                &emit,
            )
            .await;

        assert_eq!(
            notices(&mut rx),
            [format!(
                "Subagent {} completed\n\ndone",
                fixture.subagent_id
            )]
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_subagent_ended_by_stop_is_cancelled_not_failed() -> Result<()> {
        let fixture = fixture().await?;
        let cancel = CancellationToken::new();
        cancel.cancel();
        let (tx, _rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, cancel);
        let parent = fixture
            .manager
            .get_session(&fixture.parent_id, false)
            .await?;
        let run = fixture
            .runner()
            .start(&parent, &fixture.subagent_id, &emit)
            .await;
        assert_eq!(run.await, SubagentOutcome::Cancelled);
        Ok(())
    }

    #[tokio::test]
    async fn subagents_of_another_session_are_reported_without_running() -> Result<()> {
        let fixture = fixture().await?;
        let copy = fixture
            .manager
            .copy_session(&fixture.parent_id, "copy".to_string())
            .await?;
        fixture
            .manager
            .add_message(&copy.id, &delegate_message("missing-subagent"))
            .await?;
        let cancel = CancellationToken::new();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, cancel.clone());
        let operation = fixture.operation();

        for _ in 0..2 {
            let (copy, result) = run_step(&operation, &fixture.manager, &copy.id, &emit).await?;
            fixture
                .manager
                .apply_effects(&copy, &mut delivery_effects(result), &emit)
                .await?;
        }

        let (copy, result) = run_step(&operation, &fixture.manager, &copy.id, &emit).await?;
        assert!(matches!(result, OperationResult::NotApplicable));
        let hidden = hidden_texts(copy.conversation.as_ref().unwrap());
        assert_eq!(
            hidden[0],
            format!(
                "Subagent {} failed: it does not belong to this session",
                fixture.subagent_id
            )
        );
        assert!(hidden[1].starts_with("Subagent missing-subagent failed: "));
        assert!(notices(&mut rx)
            .iter()
            .all(|notice| !notice.starts_with("Running subagent")));
        Ok(())
    }

    #[test]
    fn start_notice_includes_a_single_line_task_snippet() {
        assert_eq!(
            start_notice(
                "s1",
                Some("Review the auth\nchanges in the login flow and report back")
            ),
            "Running subagent s1 (Review the auth changes in the login flow and report back)"
        );
    }
}
