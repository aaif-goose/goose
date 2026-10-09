use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio::task::{self, JoinSet};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use crate::agents::state_machine::{
    applied, awaits_tool_responses, messages_since_kickoff, not_applicable, Emitter, GooseEffect,
    Operation, OperationResult,
};
use crate::agents::subagent_handler::{ForegroundSubagentRunner, SubagentOutcome, SubagentStart};
use crate::conversation::message::{Message, MessageContent, SystemNotificationType};
use crate::conversation::Conversation;
use crate::session::{Session, SessionType};
use crate::utils::safe_truncate;

const OPERATION_NAME: &str = "foreground_subagent";
const DELIVERED: &str = "delivered";
const CANCELLED: &str = "cancelled";
const TASK_SNIPPET_CHARS: usize = 160;

fn inline_notice(text: String) -> Message {
    Message::assistant().with_system_notification(SystemNotificationType::InlineMessage, text)
}

fn subagent_label(subagent_id: &str) -> String {
    format!("Subagent {subagent_id}")
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

fn delivered_or_cancelled_ids(message: &Message) -> impl Iterator<Item = &str> {
    let delivered = message
        .metadata
        .operation_note(OPERATION_NAME, DELIVERED)
        .and_then(serde_json::Value::as_str);
    let cancelled = message
        .metadata
        .operation_note(OPERATION_NAME, CANCELLED)
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str);
    delivered.into_iter().chain(cancelled)
}

fn pending_subagents(messages: &[Message]) -> Vec<String> {
    let mut seen: HashSet<&str> = messages
        .iter()
        .flat_map(delivered_or_cancelled_ids)
        .collect();
    let mut pending: Vec<String> = Vec::new();
    for subagent_id in messages
        .iter()
        .flat_map(|message| delegated_subagent_ids(&message.content))
    {
        if seen.insert(subagent_id) {
            pending.push(subagent_id.to_owned());
        }
    }
    pending
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

async fn subagent_result_message(
    subagent_id: &str,
    outcome: SubagentOutcome,
    others_waiting: bool,
    emit: &Emitter,
) -> Option<GooseEffect> {
    let label = subagent_label(subagent_id);
    let (notice, text) = match outcome {
        SubagentOutcome::Completed(output) => (
            if others_waiting {
                format!("{label} completed\n\n{}", readable_output(&output))
            } else {
                format!("{label} completed")
            },
            format!("{label} completed: {output}"),
        ),
        SubagentOutcome::Failed(reason) => (
            format!(
                "{label} failed: {}",
                reason.lines().next().unwrap_or_default()
            ),
            format!("{label} failed: {reason}"),
        ),
        SubagentOutcome::Cancelled => return None,
    };
    emit.message(inline_notice(notice)).await;
    let mut message = Message::user().with_text(text).with_visibility(false, true);
    message
        .metadata
        .set_operation_note(OPERATION_NAME, DELIVERED, serde_json::json!(subagent_id));
    Some(message.into())
}

fn subagent_cancelled_message(pending: Vec<String>) -> Option<Message> {
    if pending.is_empty() {
        return None;
    }
    let text = pending
        .iter()
        .map(|subagent_id| {
            format!(
                "{} was cancelled before it finished and will not run again.",
                subagent_label(subagent_id)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut message = Message::user().with_text(text).with_visibility(false, true);
    message
        .metadata
        .set_operation_note(OPERATION_NAME, CANCELLED, serde_json::json!(pending));
    Some(message)
}

#[derive(Default)]
struct RunningSubagents {
    tasks: JoinSet<SubagentOutcome>,
    subagent_ids: HashMap<task::Id, String>,
}

impl RunningSubagents {
    fn contains(&self, subagent_id: &str) -> bool {
        self.subagent_ids
            .values()
            .any(|running| running == subagent_id)
    }
}

pub struct ForegroundSubagentOperation {
    runner: ForegroundSubagentRunner,
    cancel: CancellationToken,
    running: Mutex<RunningSubagents>,
}

impl ForegroundSubagentOperation {
    pub fn new(runner: ForegroundSubagentRunner, cancel: CancellationToken) -> Self {
        Self {
            runner,
            cancel,
            running: Mutex::default(),
        }
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for ForegroundSubagentOperation {
    fn name(&self) -> &'static str {
        OPERATION_NAME
    }

    async fn cancel(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Vec<GooseEffect> {
        if session.session_type == SessionType::SubAgent {
            return Vec::new();
        }
        let mut stopped = std::mem::take(&mut *self.running.lock().await);
        let mut effects = Vec::new();
        let mut delivered = HashSet::new();
        while let Some(finished) = stopped.tasks.try_join_next_with_id() {
            let Ok((task_id, outcome)) = finished else {
                continue;
            };
            let Some(subagent_id) = stopped.subagent_ids.remove(&task_id) else {
                continue;
            };
            if let Some(message) = subagent_result_message(&subagent_id, outcome, true, emit).await
            {
                effects.push(message);
                delivered.insert(subagent_id);
            }
        }
        stopped.tasks.abort_all();

        let pending = pending_subagents(conversation.messages())
            .into_iter()
            .filter(|subagent_id| !delivered.contains(subagent_id))
            .collect();
        effects.extend(subagent_cancelled_message(pending).map(GooseEffect::from));
        effects
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
        let pending = pending_subagents(conversation.messages());
        if pending.is_empty() {
            return not_applicable();
        }

        let mut running = self.running.lock().await;
        for subagent_id in &pending {
            if running.contains(subagent_id) {
                continue;
            }
            match self
                .runner
                .start(&session.id, subagent_id, self.cancel.clone())
                .await
            {
                SubagentStart::HasOutcome(outcome) => {
                    if let Some(message) =
                        subagent_result_message(subagent_id, outcome, pending.len() > 1, emit).await
                    {
                        return applied([message]);
                    }
                }
                SubagentStart::Started { task, run } => {
                    emit.message(inline_notice(start_notice(subagent_id, task.as_deref())))
                        .await;
                    let handle = running.tasks.spawn(run.in_current_span());
                    running
                        .subagent_ids
                        .insert(handle.id(), subagent_id.to_string());
                }
            }
        }

        let finished = running
            .tasks
            .join_next_with_id()
            .await
            .ok_or_else(|| anyhow!("No foreground subagent is running"))?;
        let (task_id, outcome) = match finished {
            Ok(finished) => finished,
            Err(error) => (error.id(), SubagentOutcome::Failed(error.to_string())),
        };
        let subagent_id = running
            .subagent_ids
            .remove(&task_id)
            .ok_or_else(|| anyhow!("Unknown foreground subagent task {task_id}"))?;
        match subagent_result_message(&subagent_id, outcome, pending.len() > 1, emit).await {
            Some(message) => applied([message]),
            None => not_applicable(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use goose_agent::events::AgentEvent;
    use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, MetaObject};
    use tempfile::TempDir;
    use tokio::sync::mpsc;

    use super::*;
    use crate::agents::final_output_tool::{FINAL_OUTPUT_SUCCESS_MESSAGE, FINAL_OUTPUT_TOOL_NAME};
    use crate::agents::state_machine::ConversationEffect;
    use crate::config::GooseMode;
    use crate::session::SessionManager;
    use goose_agent::machine::EffectHandler;

    struct Fixture {
        temp_dir: TempDir,
        manager: Arc<SessionManager>,
        parent_id: String,
        subagent_id: String,
    }

    impl Fixture {
        fn operation(&self, cancel: CancellationToken) -> ForegroundSubagentOperation {
            ForegroundSubagentOperation::new(
                ForegroundSubagentRunner::new(self.manager.clone()),
                cancel,
            )
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

    #[test]
    fn pending_subagents_are_delegated_until_delivered_or_cancelled() {
        let mut failed_delegate = delegate_result("failed", true);
        failed_delegate.is_error = Some(true);
        let mut messages = vec![
            Message::user()
                .with_tool_response("a", Ok(delegate_result("delivered", true)))
                .with_tool_response("b", Ok(delegate_result("background", false)))
                .with_tool_response("c", Ok(failed_delegate))
                .with_tool_response(
                    "d",
                    Ok(CallToolResult::success(vec![ContentBlock::text("done")])),
                ),
            delegate_message("cancelled"),
            delegate_message("waiting"),
            delegate_message("waiting"),
        ];
        assert_eq!(
            pending_subagents(&messages),
            ["delivered", "cancelled", "waiting"]
        );

        for (key, id) in [
            (DELIVERED, serde_json::json!("delivered")),
            (CANCELLED, serde_json::json!(["cancelled"])),
        ] {
            let mut message = Message::user().with_text(key);
            message.metadata.set_operation_note(OPERATION_NAME, key, id);
            messages.push(message);
        }
        assert_eq!(pending_subagents(&messages), ["waiting"]);
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
        let operation = fixture.operation(cancel);

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
        let operation = fixture.operation(cancel);
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
        let operation = fixture.operation(cancel);

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

    struct DropFlag(Arc<std::sync::atomic::AtomicBool>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    async fn cancel_step(
        operation: &ForegroundSubagentOperation,
        fixture: &Fixture,
        emit: &Emitter,
    ) -> Result<(Session, Vec<GooseEffect>)> {
        let parent = fixture
            .manager
            .get_session(&fixture.parent_id, true)
            .await?;
        let effects = operation
            .cancel(&parent, parent.conversation.as_ref().unwrap(), emit)
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
    async fn stop_delivers_finished_subagents_and_aborts_running_ones_without_waiting() -> Result<()>
    {
        let fixture = fixture().await?;
        let stopped_subagent_id =
            add_delegated_subagent(&fixture.manager, &fixture.temp_dir, &fixture.parent_id).await?;
        let running_subagent_id =
            add_delegated_subagent(&fixture.manager, &fixture.temp_dir, &fixture.parent_id).await?;
        let cancel = CancellationToken::new();
        let (tx, _rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, cancel.clone());
        let operation = fixture.operation(cancel.clone());
        let aborted = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let finished = {
            let mut running = operation.running.lock().await;
            let mut finished = Vec::new();
            for (subagent_id, outcome) in [
                (
                    &fixture.subagent_id,
                    SubagentOutcome::Completed("done".to_string()),
                ),
                (&stopped_subagent_id, SubagentOutcome::Cancelled),
            ] {
                let handle = running.tasks.spawn(async { outcome });
                running
                    .subagent_ids
                    .insert(handle.id(), subagent_id.clone());
                finished.push(handle);
            }
            let flag = DropFlag(aborted.clone());
            let stuck = running.tasks.spawn(async move {
                let _flag = flag;
                std::future::pending::<SubagentOutcome>().await
            });
            running
                .subagent_ids
                .insert(stuck.id(), running_subagent_id.clone());
            finished
        };
        while !finished.iter().all(|handle| handle.is_finished()) {
            tokio::task::yield_now().await;
        }
        cancel.cancel();

        let (_, effects) = tokio::time::timeout(
            Duration::from_secs(5),
            cancel_step(&operation, &fixture, &emit),
        )
        .await??;

        assert_eq!(effects.len(), 2);
        let (subagent_id, message) = delivered(&effects[0]);
        assert_eq!(subagent_id, fixture.subagent_id);
        assert_eq!(
            message.as_concat_text(),
            format!("Subagent {} completed: done", fixture.subagent_id)
        );
        assert_eq!(
            cancelled_ids(&effects[1]),
            [stopped_subagent_id, running_subagent_id]
        );
        tokio::time::timeout(Duration::from_secs(5), async {
            while !aborted.load(std::sync::atomic::Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        Ok(())
    }

    #[tokio::test]
    async fn a_subagent_ended_by_stop_is_cancelled_not_failed() -> Result<()> {
        let fixture = fixture().await?;
        let cancel = CancellationToken::new();
        cancel.cancel();
        let runner = ForegroundSubagentRunner::new(fixture.manager.clone());
        let SubagentStart::Started { run, .. } = runner
            .start(&fixture.parent_id, &fixture.subagent_id, cancel)
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
        let (tx, _rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, cancel.clone());
        let operation = fixture.operation(cancel.clone());
        let (parent, result) = run_step(&operation, &fixture, &emit).await?;
        fixture
            .manager
            .apply_effects(&parent, &mut delivery_effects(result), &emit)
            .await?;

        cancel.cancel();
        let (parent, mut effects) = cancel_step(&operation, &fixture, &emit).await?;
        assert_eq!(effects.len(), 1);
        assert_eq!(cancelled_ids(&effects[0]), [unfinished_subagent_id]);
        fixture
            .manager
            .apply_effects(&parent, &mut effects, &emit)
            .await?;
        let (_, effects) = cancel_step(&fixture.operation(cancel), &fixture, &emit).await?;
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
        let operation = fixture.operation(cancel);

        for _ in 0..2 {
            let copy = fixture.manager.get_session(&copy.id, true).await?;
            let result = operation
                .run(&copy, copy.conversation.as_ref().unwrap(), &emit)
                .await?;
            fixture
                .manager
                .apply_effects(&copy, &mut delivery_effects(result), &emit)
                .await?;
        }

        let copy = fixture.manager.get_session(&copy.id, true).await?;
        let hidden = hidden_texts(copy.conversation.as_ref().unwrap());
        assert_eq!(
            hidden[0],
            format!(
                "Subagent {} failed: it does not belong to this session",
                fixture.subagent_id
            )
        );
        assert!(hidden[1].starts_with("Subagent missing-subagent failed: "));
        assert!(pending_subagents(copy.conversation.as_ref().unwrap().messages()).is_empty());
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
