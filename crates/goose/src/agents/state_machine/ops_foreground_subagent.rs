use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::StreamExt;
use rmcp::model::Role;
use tokio::sync::Mutex;
use tokio::task::{self, JoinSet};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use crate::agents::final_output_tool::FinalOutputTool;
use crate::agents::state_machine::ops_maxturns::MAX_TURNS_MESSAGE;
use crate::agents::state_machine::{
    applied, awaits_tool_responses, messages_since_kickoff, not_applicable, trailing_error,
    yielded, Emitter, GooseEffect, Operation, OperationResult,
};
use crate::agents::subagent_handler::from_foreground_subagent_session;
use crate::agents::SessionConfig;
use crate::conversation::message::{Message, MessageContent, SystemNotificationType};
use crate::conversation::Conversation;
use crate::session::{Session, SessionManager, SessionType};
use crate::utils::safe_truncate;

const OPERATION_NAME: &str = "foreground_subagent";
const TASK_SNIPPET_CHARS: usize = 160;

enum ChildOutcome {
    Completed(String),
    Failed(String),
}

fn inline_notice(text: String) -> Message {
    Message::assistant().with_system_notification(SystemNotificationType::InlineMessage, text)
}

fn start_notice(child: &Session) -> String {
    let snippet = child
        .recipe
        .as_ref()
        .and_then(|recipe| recipe.prompt.as_deref())
        .map(|prompt| {
            let prompt = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
            format!(" ({})", safe_truncate(&prompt, TASK_SNIPPET_CHARS))
        })
        .unwrap_or_default();
    format!("Running subagent {}{snippet}", child.id)
}

pub(super) fn foreground_child_ids_in_message(content: &[MessageContent]) -> Vec<String> {
    content
        .iter()
        .filter_map(|content| {
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
            meta.0
                .get("subagent_session_id")?
                .as_str()
                .map(str::to_owned)
        })
        .collect()
}

async fn advance_child(
    session_manager: Arc<SessionManager>,
    child: &Session,
    use_login_shell_path: bool,
    cancel: CancellationToken,
) -> Result<()> {
    let agent =
        from_foreground_subagent_session(session_manager, &child.id, use_login_shell_path).await?;
    let recipe = child
        .recipe
        .as_ref()
        .ok_or_else(|| anyhow!("Subagent {} has no saved recipe", child.id))?;
    let max_turns = recipe
        .settings
        .as_ref()
        .and_then(|settings| settings.max_turns)
        .ok_or_else(|| anyhow!("Subagent {} has no saved turn limit", child.id))?;
    let session_config = SessionConfig {
        id: child.id.clone(),
        schedule_id: None,
        max_turns: Some(max_turns as u32),
        retry_config: recipe.retry.clone(),
    };
    let mut events = agent
        .stream_state_machine_session(session_config, cancel)
        .await?;
    while let Some(event) = events.next().await {
        event?;
    }
    Ok(())
}

async fn run_child(
    session_manager: Arc<SessionManager>,
    child: Session,
    use_login_shell_path: bool,
    cancel: CancellationToken,
) -> ChildOutcome {
    let run_result = advance_child(
        session_manager.clone(),
        &child,
        use_login_shell_path,
        cancel,
    )
    .await;
    let child = match session_manager.get_session(&child.id, true).await {
        Ok(child) => child,
        Err(error) => return ChildOutcome::Failed(error.to_string()),
    };
    let messages = child.conversation.as_ref().map(Conversation::messages);
    if let Some(output) = messages.and_then(|messages| FinalOutputTool::successful_output(messages))
    {
        return ChildOutcome::Completed(output);
    }
    if let Err(error) = run_result {
        return ChildOutcome::Failed(error.to_string());
    }
    if let Some(error) = child.conversation.as_ref().and_then(trailing_error) {
        return ChildOutcome::Failed(format!("{error:?}"));
    }

    let reason = messages
        .and_then(|messages| {
            if messages
                .last()
                .is_some_and(|message| message.as_concat_text() == MAX_TURNS_MESSAGE)
            {
                let last_assistant_text = messages
                    .iter()
                    .rev()
                    .filter(|message| message.role == Role::Assistant)
                    .map(Message::as_concat_text)
                    .find(|text| !text.is_empty() && text != MAX_TURNS_MESSAGE);
                Some(format!(
                    "max turns reached{}",
                    last_assistant_text
                        .map(|text| format!("; last response: {text}"))
                        .unwrap_or_default()
                ))
            } else {
                messages
                    .last()
                    .map(Message::as_concat_text)
                    .filter(|text| !text.is_empty())
            }
        })
        .unwrap_or_else(|| "stopped without final output".to_string());
    ChildOutcome::Failed(reason)
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

async fn delivery(
    child_id: &str,
    outcome: ChildOutcome,
    others_waiting: bool,
    emit: &Emitter,
) -> GooseEffect {
    let (notice, text) = match outcome {
        ChildOutcome::Completed(output) => (
            if others_waiting {
                format!(
                    "Subagent {child_id} completed\n\n{}",
                    readable_output(&output)
                )
            } else {
                format!("Subagent {child_id} completed")
            },
            format!("Subagent {child_id} completed: {output}"),
        ),
        ChildOutcome::Failed(reason) => (
            format!(
                "Subagent {child_id} failed: {}",
                reason.lines().next().unwrap_or_default()
            ),
            format!("Subagent {child_id} failed: {reason}"),
        ),
    };
    emit.message(inline_notice(notice)).await;
    GooseEffect::DeliverForegroundSubagent {
        message: Message::user().with_text(text).with_visibility(false, true),
        child_id: child_id.to_string(),
    }
}

#[derive(Default)]
struct RunningChildren {
    tasks: JoinSet<ChildOutcome>,
    child_ids: HashMap<task::Id, String>,
}

impl RunningChildren {
    fn contains(&self, child_id: &str) -> bool {
        self.child_ids.values().any(|running| running == child_id)
    }
}

pub struct ForegroundSubagentOperation {
    session_manager: Arc<SessionManager>,
    use_login_shell_path: bool,
    cancel: CancellationToken,
    running: Mutex<RunningChildren>,
}

impl ForegroundSubagentOperation {
    pub fn new(
        session_manager: Arc<SessionManager>,
        use_login_shell_path: bool,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            session_manager,
            use_login_shell_path,
            cancel,
            running: Mutex::default(),
        }
    }

    async fn settle_or_start(
        &self,
        parent_id: &str,
        child_id: &str,
        running: &mut RunningChildren,
        emit: &Emitter,
    ) -> Option<ChildOutcome> {
        let child = match self.session_manager.get_session(child_id, true).await {
            Ok(child) => child,
            Err(error) => return Some(ChildOutcome::Failed(error.to_string())),
        };
        if child.session_type != SessionType::SubAgent
            || child.parent_session_id.as_deref() != Some(parent_id)
        {
            return Some(ChildOutcome::Failed(
                "it does not belong to this session".to_string(),
            ));
        }
        if let Some(output) = child
            .conversation
            .as_ref()
            .and_then(|conversation| FinalOutputTool::successful_output(conversation.messages()))
        {
            return Some(ChildOutcome::Completed(output));
        }

        emit.message(inline_notice(start_notice(&child))).await;
        let task = running.tasks.spawn(
            run_child(
                self.session_manager.clone(),
                child,
                self.use_login_shell_path,
                self.cancel.clone(),
            )
            .in_current_span(),
        );
        running.child_ids.insert(task.id(), child_id.to_string());
        None
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for ForegroundSubagentOperation {
    fn name(&self) -> &'static str {
        OPERATION_NAME
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        if session.session_type == SessionType::SubAgent {
            return not_applicable();
        }
        if awaits_tool_responses(messages_since_kickoff(conversation)?) {
            return not_applicable();
        }
        let waiting = self
            .session_manager
            .pending_foreground_subagents(&session.id)
            .await?;
        if waiting.is_empty() {
            return not_applicable();
        }

        let mut running = self.running.lock().await;
        for child_id in &waiting {
            if running.contains(child_id) {
                continue;
            }
            if let Some(outcome) = self
                .settle_or_start(&session.id, child_id, &mut running, emit)
                .await
            {
                if self.cancel.is_cancelled() {
                    return yielded();
                }
                return applied([delivery(child_id, outcome, waiting.len() > 1, emit).await]);
            }
        }

        let finished = tokio::select! {
            biased;
            _ = self.cancel.cancelled() => return yielded(),
            finished = running.tasks.join_next_with_id() => {
                finished.ok_or_else(|| anyhow!("No foreground subagent is running"))?
            }
        };
        let (task_id, outcome) = match finished {
            Ok(finished) => finished,
            Err(error) => (error.id(), ChildOutcome::Failed(error.to_string())),
        };
        let child_id = running
            .child_ids
            .remove(&task_id)
            .ok_or_else(|| anyhow!("Unknown foreground subagent task {task_id}"))?;
        applied([delivery(&child_id, outcome, waiting.len() > 1, emit).await])
    }
}

#[cfg(test)]
mod tests {
    use goose_agent::events::AgentEvent;
    use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, MetaObject};
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    use super::*;
    use crate::agents::final_output_tool::{FINAL_OUTPUT_SUCCESS_MESSAGE, FINAL_OUTPUT_TOOL_NAME};
    use crate::agents::state_machine::subagent_stop::cancellation_note;
    use crate::config::GooseMode;
    use goose_agent::machine::EffectHandler;

    struct Fixture {
        temp_dir: TempDir,
        manager: Arc<SessionManager>,
        parent_id: String,
        child_id: String,
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
        let child_id = add_scheduled_child(&manager, &temp_dir, &parent.id).await?;
        Ok(Fixture {
            temp_dir,
            manager,
            parent_id: parent.id,
            child_id,
        })
    }

    async fn add_scheduled_child(
        manager: &SessionManager,
        temp_dir: &TempDir,
        parent_id: &str,
    ) -> Result<String> {
        let child = manager
            .create_session(
                temp_dir.path().to_path_buf(),
                "child".to_string(),
                SessionType::SubAgent,
                GooseMode::Auto,
            )
            .await?;
        manager
            .update(&child.id)
            .parent_session_id(Some(parent_id.to_string()))
            .apply()
            .await?;
        manager
            .save_foreground_delegation_message(
                parent_id,
                &Message::user().with_text(format!("Scheduled foreground subagent {}", child.id)),
                std::slice::from_ref(&child.id),
            )
            .await?;
        Ok(child.id)
    }

    async fn run_step(
        operation: &ForegroundSubagentOperation,
        fixture: &Fixture,
        emit: &Emitter,
    ) -> Result<(Session, OperationResult<GooseEffect>)> {
        let parent = fixture
            .manager
            .get_session(&fixture.parent_id, true)
            .await?;
        let conversation = parent.conversation.as_ref().unwrap();
        let result = operation.run(&parent, conversation, emit).await?;
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
        let GooseEffect::DeliverForegroundSubagent { message, child_id } = effect else {
            panic!("expected a foreground subagent delivery");
        };
        (child_id, message)
    }

    async fn deliver_child_with_notices(fixture: &Fixture) -> Result<(String, Vec<String>)> {
        let cancel = CancellationToken::new();
        let (tx, mut rx) = mpsc::channel(16);
        let emit = Emitter::new(tx, cancel.clone());
        let operation = ForegroundSubagentOperation::new(fixture.manager.clone(), false, cancel);
        let (_, result) = run_step(&operation, fixture, &emit).await?;
        let effects = delivery_effects(result);
        assert_eq!(effects.len(), 1);
        let (child_id, message) = delivered(&effects[0]);
        assert_eq!(child_id, fixture.child_id);
        assert!(!message.is_user_visible());
        assert!(message.is_agent_visible());
        Ok((message.as_concat_text(), notices(&mut rx)))
    }

    #[test]
    fn identifies_foreground_children_in_tool_responses() {
        let mut meta = MetaObject::new();
        meta.0.insert(
            "foreground_subagent".to_string(),
            serde_json::Value::Bool(true),
        );
        meta.0.insert(
            "subagent_session_id".to_string(),
            serde_json::Value::String("child-1".to_string()),
        );
        let message = Message::user()
            .with_tool_response(
                "delegate-call",
                Ok(
                    CallToolResult::success(vec![ContentBlock::text("scheduled")])
                        .with_meta(Some(meta)),
                ),
            )
            .with_tool_response(
                "other-call",
                Ok(CallToolResult::success(vec![ContentBlock::text("done")])),
            );

        assert_eq!(
            foreground_child_ids_in_message(&message.content),
            vec!["child-1"]
        );
    }

    #[test]
    fn waits_for_all_tool_responses_before_running_children() {
        let requests = Message::assistant()
            .with_tool_request("delegate-call", Ok(CallToolRequestParams::new("delegate")))
            .with_tool_request("other-call", Ok(CallToolRequestParams::new("other")));
        let response = Message::user().with_tool_response(
            "delegate-call",
            Ok(CallToolResult::success(vec![ContentBlock::text(
                "scheduled",
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

    async fn save_final_output(manager: &SessionManager, child_id: &str) -> Result<()> {
        let arguments = serde_json::json!({"summary": "done"})
            .as_object()
            .unwrap()
            .clone();
        manager
            .add_message(
                child_id,
                &Message::assistant().with_tool_request(
                    "final-output-call",
                    Ok(CallToolRequestParams::new(FINAL_OUTPUT_TOOL_NAME)
                        .with_arguments(arguments)),
                ),
            )
            .await?;
        manager
            .add_message(
                child_id,
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

    fn notices(rx: &mut mpsc::Receiver<AgentEvent>) -> Vec<String> {
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
    async fn saved_outputs_show_results_only_while_other_children_wait() -> Result<()> {
        let fixture = fixture().await?;
        save_final_output(&fixture.manager, &fixture.child_id).await?;
        let second_child_id =
            add_scheduled_child(&fixture.manager, &fixture.temp_dir, &fixture.parent_id).await?;
        save_final_output(&fixture.manager, &second_child_id).await?;
        let cancel = CancellationToken::new();
        let (tx, mut rx) = mpsc::channel(16);
        let emit = Emitter::new(tx, cancel.clone());
        let operation = ForegroundSubagentOperation::new(fixture.manager.clone(), false, cancel);

        for (expected_id, expected_notice) in [
            (
                &fixture.child_id,
                format!("Subagent {} completed\n\ndone", fixture.child_id),
            ),
            (
                &second_child_id,
                format!("Subagent {second_child_id} completed"),
            ),
        ] {
            let (parent, result) = run_step(&operation, &fixture, &emit).await?;
            let mut effects = delivery_effects(result);
            assert_eq!(effects.len(), 1);
            let (child_id, message) = delivered(&effects[0]);
            assert_eq!(child_id, expected_id);
            assert!(!message.is_user_visible());
            assert!(message.is_agent_visible());
            assert_eq!(
                message.as_concat_text(),
                format!("Subagent {child_id} completed: {{\"summary\":\"done\"}}")
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
    async fn failed_child_emits_start_and_failure_notices() -> Result<()> {
        let fixture = fixture().await?;

        let (delivery, notices) = deliver_child_with_notices(&fixture).await?;
        let reason = format!("Subagent {} has no saved recipe", fixture.child_id);
        assert_eq!(
            delivery,
            format!("Subagent {} failed: {reason}", fixture.child_id)
        );
        assert_eq!(
            notices,
            vec![
                format!("Running subagent {}", fixture.child_id),
                format!("Subagent {} failed: {reason}", fixture.child_id),
            ]
        );
        Ok(())
    }

    #[tokio::test]
    async fn delivers_each_child_in_its_own_step() -> Result<()> {
        let fixture = fixture().await?;
        let second_child_id =
            add_scheduled_child(&fixture.manager, &fixture.temp_dir, &fixture.parent_id).await?;
        let cancel = CancellationToken::new();
        let (tx, _rx) = mpsc::channel(16);
        let emit = Emitter::new(tx, cancel.clone());
        let operation = ForegroundSubagentOperation::new(fixture.manager.clone(), false, cancel);

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
        let mut expected = vec![fixture.child_id.clone(), second_child_id];
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

    #[tokio::test]
    async fn stop_yields_without_delivering() -> Result<()> {
        let fixture = fixture().await?;
        let cancel = CancellationToken::new();
        cancel.cancel();
        let (tx, _rx) = mpsc::channel(16);
        let emit = Emitter::new(tx, cancel.clone());
        let operation = ForegroundSubagentOperation::new(fixture.manager.clone(), false, cancel);

        let (_, result) = run_step(&operation, &fixture, &emit).await?;
        let OperationResult::Applied(step) = result else {
            panic!("expected the operation to yield");
        };
        assert!(step.yield_to_client);
        assert!(step.effects.is_empty());
        assert_eq!(
            fixture
                .manager
                .pending_foreground_subagents(&fixture.parent_id)
                .await?,
            vec![fixture.child_id.clone()]
        );
        Ok(())
    }

    #[tokio::test]
    async fn cancelled_children_do_not_run_on_next_turn() -> Result<()> {
        let fixture = fixture().await?;
        let cancel = CancellationToken::new();
        let (tx, _rx) = mpsc::channel(16);
        let emit = Emitter::new(tx, cancel.clone());
        fixture
            .manager
            .cancel_foreground_subagents(&fixture.parent_id, cancellation_note)
            .await?;

        let messages = fixture
            .manager
            .get_session(&fixture.parent_id, true)
            .await?
            .conversation
            .unwrap();
        let hidden: Vec<String> = messages
            .messages()
            .iter()
            .filter(|message| !message.is_user_visible())
            .inspect(|message| assert!(message.is_agent_visible()))
            .map(Message::as_concat_text)
            .collect();
        assert_eq!(
            hidden,
            vec![format!(
                "Subagent {} was cancelled before it finished and will not run again.",
                fixture.child_id
            )]
        );

        let next_turn = ForegroundSubagentOperation::new(fixture.manager.clone(), false, cancel);
        let (_, result) = run_step(&next_turn, &fixture, &emit).await?;
        assert!(matches!(result, OperationResult::NotApplicable));

        let redelegated_child_id =
            add_scheduled_child(&fixture.manager, &fixture.temp_dir, &fixture.parent_id).await?;
        assert_eq!(
            fixture
                .manager
                .pending_foreground_subagents(&fixture.parent_id)
                .await?,
            vec![redelegated_child_id]
        );
        Ok(())
    }

    #[test]
    fn start_notice_includes_a_single_line_task_snippet() {
        let recipe = crate::recipe::Recipe::builder()
            .title("Delegated task")
            .description("Delegated task")
            .prompt("Review the auth\nchanges in the login flow and report back")
            .build()
            .unwrap();
        let child = Session {
            id: "child".to_string(),
            recipe: Some(recipe),
            ..Default::default()
        };

        assert_eq!(
            start_notice(&child),
            "Running subagent child (Review the auth changes in the login flow and report back)"
        );
    }
}
