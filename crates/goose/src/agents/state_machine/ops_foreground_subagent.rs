use anyhow::Result;
use async_trait::async_trait;
use goose_agent::subagent::{SubagentRequest, SubagentResult, SubagentRunner, SubagentStart};

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

fn finished_notice(subagent_id: &str, result: &SubagentResult, show_output: bool) -> String {
    match result {
        SubagentResult::Completed(output) if show_output => {
            format!(
                "Subagent {subagent_id} completed\n\n{}",
                readable_output(output)
            )
        }
        SubagentResult::Completed(_) => format!("Subagent {subagent_id} completed"),
        SubagentResult::Failed(reason) => format!(
            "Subagent {subagent_id} failed: {}",
            reason.lines().next().unwrap_or_default()
        ),
    }
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
    fn started_subagent_ids(&self, parent: &Session, conversation: &Conversation) -> Vec<String> {
        if parent.session_type == SessionType::SubAgent {
            return Vec::new();
        }
        conversation
            .messages()
            .iter()
            .flat_map(|message| delegated_subagent_ids(&message.content))
            .map(str::to_owned)
            .collect()
    }

    fn ready(&self, _parent: &Session, conversation: &Conversation) -> Result<bool> {
        Ok(!awaits_tool_responses(messages_since_kickoff(
            conversation,
        )?))
    }

    async fn start(&self, parent: &Session, request: SubagentRequest) -> SubagentStart {
        self.start_subagent(&parent.id, request).await
    }

    async fn on_subagent_started(&self, subagent_id: &str, task: Option<&str>, emit: &Emitter) {
        emit.message(inline_notice(start_notice(subagent_id, task)))
            .await;
    }

    async fn on_subagent_finished(
        &self,
        subagent_id: &str,
        result: &SubagentResult,
        remaining: usize,
        emit: &Emitter,
    ) {
        let show_output = remaining > 0 || emit.cancel_token().is_cancelled();
        emit.message(inline_notice(finished_notice(
            subagent_id,
            result,
            show_output,
        )))
        .await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use goose_agent::events::AgentEvent;
    use goose_agent::subagent::{SubagentOperation, SubagentOutcome};
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
    const CANCELLED: &str = "cancelled";

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

        fn operation(&self) -> SubagentOperation<Session> {
            SubagentOperation::new(Arc::new(self.runner()))
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
        operation: &SubagentOperation<Session>,
        fixture: &Fixture,
        emit: &Emitter,
    ) -> Result<(Session, OperationResult<GooseEffect>)> {
        let parent = fixture
            .manager
            .get_session(&fixture.parent_id, true)
            .await?;
        let conversation = parent.conversation.as_ref().unwrap();
        let result =
            Operation::<Session, GooseEffect>::run(operation, &parent, conversation, emit).await?;
        Ok((parent, result))
    }

    fn delivery_effects(result: OperationResult<GooseEffect>) -> Vec<GooseEffect> {
        let OperationResult::Applied(step) = result else {
            panic!("expected the operation to apply");
        };
        assert!(!step.yield_to_client);
        step.effects
    }

    fn delivered(effect: &GooseEffect) -> (&str, &Message) {
        let GooseEffect::Conversation(ConversationEffect::AppendMessage(message)) = effect else {
            panic!("expected a foreground subagent delivery");
        };
        let subagent_id = message
            .metadata
            .operation_note(OPERATION_NAME, DELIVERED)
            .and_then(serde_json::Value::as_str)
            .expect("the result message records its subagent");
        (subagent_id, message)
    }

    #[tokio::test]
    async fn successful_foreground_delegations_are_the_started_subagents() -> Result<()> {
        let fixture = fixture().await?;
        let mut failed_delegate = delegate_result("failed", true);
        failed_delegate.is_error = Some(true);
        let conversation = Conversation::new_unvalidated(vec![
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
            runner.started_subagent_ids(&parent, &conversation),
            ["first", "second"]
        );
        parent.session_type = SessionType::SubAgent;
        assert!(runner
            .started_subagent_ids(&parent, &conversation)
            .is_empty());
        Ok(())
    }

    #[test]
    fn waits_for_all_tool_responses_before_running_subagents() {
        let requests = Message::assistant()
            .with_tool_request("delegate-call", Ok(CallToolRequestParams::new("delegate")))
            .with_tool_request("other-call", Ok(CallToolRequestParams::new("other")));
        let response = Message::user().with_tool_response(
            "delegate-call",
            Ok(CallToolResult::success(vec![ContentBlock::text(
                "delegated",
            )])),
        );
        let mut messages = vec![requests, response];
        assert!(awaits_tool_responses(&messages));

        messages.push(Message::user().with_tool_response(
            "other-call",
            Ok(CallToolResult::success(vec![ContentBlock::text("done")])),
        ));
        assert!(!awaits_tool_responses(&messages));
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
            let (parent, result) = run_step(&operation, &fixture, &emit).await?;
            let mut effects = delivery_effects(result);
            assert_eq!(effects.len(), 1);
            let (subagent_id, message) = delivered(&effects[0]);
            assert_eq!(subagent_id, expected_id);
            assert!(!message.is_user_visible());
            assert!(message.is_agent_visible());
            assert_eq!(
                message.as_concat_text(),
                format!("Subagent {subagent_id} completed: {{\"summary\":\"done\"}}")
            );
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
        let (_, result) = run_step(&operation, &fixture, &emit).await?;
        let effects = delivery_effects(result);
        assert_eq!(effects.len(), 1);
        let (subagent_id, message) = delivered(&effects[0]);
        assert_eq!(subagent_id, fixture.subagent_id);
        assert!(!message.is_user_visible());
        assert!(message.is_agent_visible());

        let reason = format!("Subagent {} has no saved recipe", fixture.subagent_id);
        assert_eq!(
            message.as_concat_text(),
            format!("Subagent {} failed: {reason}", fixture.subagent_id)
        );
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
    async fn delivers_each_subagent_in_its_own_step() -> Result<()> {
        let fixture = fixture().await?;
        let second_subagent_id =
            add_delegated_subagent(&fixture.manager, &fixture.temp_dir, &fixture.parent_id).await?;
        let cancel = CancellationToken::new();
        let (tx, _rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, cancel.clone());
        let operation = fixture.operation();

        let mut delivered_ids = Vec::new();
        for _ in 0..2 {
            let (parent, result) = run_step(&operation, &fixture, &emit).await?;
            let mut effects = delivery_effects(result);
            assert_eq!(effects.len(), 1);
            delivered_ids.push(delivered(&effects[0]).0.to_string());
            fixture
                .manager
                .apply_effects(&parent, &mut effects, &emit)
                .await?;
        }
        delivered_ids.sort();
        let mut expected = vec![fixture.subagent_id.clone(), second_subagent_id];
        expected.sort();
        assert_eq!(delivered_ids, expected);

        let (parent, result) = run_step(&operation, &fixture, &emit).await?;
        assert!(matches!(result, OperationResult::NotApplicable));
        let hidden_deliveries = parent
            .conversation
            .unwrap()
            .messages()
            .iter()
            .filter(|message| !message.is_user_visible())
            .count();
        assert_eq!(hidden_deliveries, 2);
        Ok(())
    }

    async fn cancel_step(
        operation: &SubagentOperation<Session>,
        fixture: &Fixture,
        emit: &Emitter,
    ) -> Result<(Session, Vec<GooseEffect>)> {
        let parent = fixture
            .manager
            .get_session(&fixture.parent_id, true)
            .await?;
        let effects = Operation::<Session, GooseEffect>::cancel(
            operation,
            &parent,
            parent.conversation.as_ref().unwrap(),
            emit,
        )
        .await;
        Ok((parent, effects))
    }

    fn cancelled_ids(effect: &GooseEffect) -> Vec<String> {
        let GooseEffect::Conversation(ConversationEffect::AppendMessage(message)) = effect else {
            panic!("expected a cancelled note");
        };
        assert!(!message.is_user_visible());
        assert!(message.is_agent_visible());
        serde_json::from_value(
            message
                .metadata
                .operation_note(OPERATION_NAME, CANCELLED)
                .cloned()
                .expect("the note records the cancelled subagents"),
        )
        .unwrap()
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
                &SubagentResult::Completed(r#"{"summary":"done"}"#.to_string()),
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
        let SubagentStart::Started { run, .. } = fixture
            .runner()
            .start_subagent(
                &fixture.parent_id,
                SubagentRequest::new(fixture.subagent_id.clone(), cancel),
            )
            .await
        else {
            panic!("expected the subagent to start");
        };
        assert!(matches!(run.await, SubagentOutcome::Cancelled));
        Ok(())
    }

    #[tokio::test]
    async fn cancel_marks_only_undelivered_subagents_once() -> Result<()> {
        let fixture = fixture().await?;
        save_final_output(&fixture.manager, &fixture.subagent_id).await?;
        let unfinished_subagent_id =
            add_delegated_subagent(&fixture.manager, &fixture.temp_dir, &fixture.parent_id).await?;
        let cancel = CancellationToken::new();
        cancel.cancel();
        let (tx, _rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, cancel);
        let operation = fixture.operation();
        let (parent, result) = run_step(&operation, &fixture, &emit).await?;
        fixture
            .manager
            .apply_effects(&parent, &mut delivery_effects(result), &emit)
            .await?;

        let (parent, mut effects) = cancel_step(&operation, &fixture, &emit).await?;
        assert_eq!(effects.len(), 1);
        assert_eq!(cancelled_ids(&effects[0]), [unfinished_subagent_id]);
        fixture
            .manager
            .apply_effects(&parent, &mut effects, &emit)
            .await?;
        let (_, effects) = cancel_step(&fixture.operation(), &fixture, &emit).await?;
        assert!(effects.is_empty());
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

        let run_copy = |copy: Session| async {
            let result = Operation::<Session, GooseEffect>::run(
                &operation,
                &copy,
                copy.conversation.as_ref().unwrap(),
                &emit,
            )
            .await?;
            anyhow::Ok((copy, result))
        };
        for _ in 0..2 {
            let (copy, result) =
                run_copy(fixture.manager.get_session(&copy.id, true).await?).await?;
            fixture
                .manager
                .apply_effects(&copy, &mut delivery_effects(result), &emit)
                .await?;
        }

        let (copy, result) = run_copy(fixture.manager.get_session(&copy.id, true).await?).await?;
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
