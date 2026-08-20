//! Goose-specific inference request preparation.

#[cfg(feature = "code-mode")]
use crate::agents::ExtensionManager;
use crate::agents::PromptManager;
use crate::config::GooseMode;
use crate::hints::load_hints::{HintSnapshot, SubdirectoryHintTracker, HINT_EXTRA_SEPARATOR_BYTES};
use crate::session::Session;
use crate::tool_inspection::ToolInspectionManager;
use anyhow::Result;
use async_trait::async_trait;
use goose_agent::inference::{InferenceRequestPreparer, PreparedInferenceRequest};
use goose_agent::operation::{messages_since_kickoff, InferenceInput};
use goose_providers::conversation::message::{Message, MessageContent};
use goose_providers::conversation::Conversation;
#[cfg(feature = "code-mode")]
use std::sync::Arc;
use tokio::sync::Mutex;

pub(super) fn reconstructed_hint_snapshot(
    conversation: &Conversation,
    working_dir: &std::path::Path,
    goose_mode: GooseMode,
) -> HintSnapshot {
    reconstructed_hint_snapshot_with_hook(conversation, working_dir, goose_mode, || {})
}

fn reconstructed_hint_snapshot_with_hook(
    conversation: &Conversation,
    working_dir: &std::path::Path,
    goose_mode: GooseMode,
    after_top_level_read: impl FnOnce(),
) -> HintSnapshot {
    let mut hints = SubdirectoryHintTracker::new();
    for message in conversation.messages() {
        for content in &message.content {
            if let MessageContent::ToolRequest(request) = content {
                if let Ok(tool_call) = &request.tool_call {
                    hints.record_tool_arguments(&tool_call.arguments, working_dir);
                }
            }
        }
    }
    let reserved_output_bytes = HINT_EXTRA_SEPARATOR_BYTES
        + if goose_mode == GooseMode::Chat {
            HINT_EXTRA_SEPARATOR_BYTES
        } else {
            0
        };
    hints.load_snapshot_with_hook(working_dir, reserved_output_bytes, after_top_level_read)
}

pub struct GooseInferenceRequestPreparer<'a> {
    #[cfg(feature = "code-mode")]
    pub(crate) extension_manager: Arc<ExtensionManager>,
    pub(crate) goose_mode: &'a Mutex<GooseMode>,
    pub(crate) prompt_manager: &'a Mutex<PromptManager>,
    pub(crate) tool_inspection_manager: &'a ToolInspectionManager,
    pub(crate) context_limit: usize,
}

#[async_trait]
impl InferenceRequestPreparer<Session> for GooseInferenceRequestPreparer<'_> {
    async fn prepare(
        &self,
        session: &Session,
        conversation: &Conversation,
        input: InferenceInput,
    ) -> Result<PreparedInferenceRequest> {
        #[cfg(feature = "code-mode")]
        let code_execution_mode = self
            .extension_manager
            .is_extension_enabled(
                crate::agents::platform_extensions::code_execution::EXTENSION_NAME,
            )
            .await;
        #[cfg(not(feature = "code-mode"))]
        let code_execution_mode = false;

        let goose_mode = *self.goose_mode.lock().await;
        if goose_mode == GooseMode::SmartApprove {
            self.tool_inspection_manager
                .apply_tool_annotations(&input.tools);
        }
        let tools =
            crate::agents::reply_parts::prepare_inference_tools(input.tools, code_execution_mode);
        let hint_snapshot =
            reconstructed_hint_snapshot(conversation, &session.working_dir, goose_mode);
        let system_prompt = self
            .prompt_manager
            .lock()
            .await
            .build_system_prompt_from_snapshot(input.prompt_parts, goose_mode, hint_snapshot);
        let turn = messages_since_kickoff(conversation)?;
        let turn_start = turn
            .first()
            .and_then(|message| chrono::DateTime::from_timestamp(message.created, 0))
            .map(|timestamp| timestamp.with_timezone(&chrono::Local))
            .unwrap_or_else(chrono::Local::now);
        let last = turn
            .iter()
            .rev()
            .find(|message| message.is_turn_context())
            .map(Message::as_concat_text);
        let context_limit = Some(self.context_limit);
        let additional_messages = crate::agents::moim::turn_context_event(
            &session.working_dir,
            context_limit,
            input.moim_parts,
            turn_start,
        )
        .filter(|event| Some(event.as_concat_text()) != last)
        .into_iter()
        .collect();
        Ok(PreparedInferenceRequest {
            system_prompt,
            tools,
            additional_messages,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hints::{GOOSE_HINTS_FILENAME, MAX_HINT_OUTPUT_BYTES};
    use rmcp::model::CallToolRequestParams;
    use std::fs;

    #[test]
    fn reconstructed_hint_snapshot_is_stable_across_file_growth() {
        let project = tempfile::tempdir().unwrap();
        let nested = project.path().join("nested");
        fs::create_dir(&nested).unwrap();
        let root_hints = project.path().join(GOOSE_HINTS_FILENAME);
        fs::write(&root_hints, "ROOT_V1").unwrap();
        fs::write(nested.join(GOOSE_HINTS_FILENAME), "NESTED_MARKER").unwrap();

        let arguments = serde_json::json!({ "path": "nested/file.rs" })
            .as_object()
            .unwrap()
            .clone();
        let conversation = Conversation::new_unvalidated([Message::assistant().with_tool_request(
            "read-nested",
            Ok(CallToolRequestParams::new("read_file").with_arguments(arguments)),
        )]);

        let snapshot = reconstructed_hint_snapshot_with_hook(
            &conversation,
            project.path(),
            GooseMode::Auto,
            || fs::write(&root_hints, format!("{}ROOT_V2", "v".repeat(700 * 1024))).unwrap(),
        );
        let hint_bytes = snapshot.top_level.len()
            + snapshot
                .subdirectories
                .iter()
                .map(|(_, content)| content.len())
                .sum::<usize>()
            + snapshot.subdirectories.len() * HINT_EXTRA_SEPARATOR_BYTES;
        fs::write(&root_hints, "ROOT_V3").unwrap();

        let prompt = PromptManager::new().build_system_prompt_from_snapshot(
            Vec::new(),
            GooseMode::Auto,
            snapshot,
        );

        assert!(hint_bytes <= MAX_HINT_OUTPUT_BYTES);
        assert!(prompt.contains("ROOT_V1"));
        assert!(prompt.contains("NESTED_MARKER"));
        assert!(!prompt.contains("ROOT_V2"));
        assert!(!prompt.contains("ROOT_V3"));
    }

    #[test]
    #[serial_test::serial]
    fn state_machine_chat_hints_reserve_trailing_separator_at_exact_limit() {
        const PROJECT_HINTS_HEADER: &str =
            "### Project Hints\nThese are hints for working on the project in this directory.\n";

        let config_root = tempfile::tempdir().unwrap();
        let _guard = env_lock::lock_env([
            (
                "GOOSE_PATH_ROOT",
                Some(config_root.path().to_str().unwrap()),
            ),
            ("CONTEXT_FILE_NAMES", Some(r#"[".goosehints"]"#)),
        ]);
        let project = tempfile::tempdir().unwrap();
        let hints_path = project.path().join(GOOSE_HINTS_FILENAME);
        let reserved_output_bytes = HINT_EXTRA_SEPARATOR_BYTES * 2;
        let exact_content_len =
            MAX_HINT_OUTPUT_BYTES - reserved_output_bytes - PROJECT_HINTS_HEADER.len();
        fs::write(
            &hints_path,
            format!(
                "CHAT_BOUNDARY{}",
                "h".repeat(exact_content_len - "CHAT_BOUNDARY".len())
            ),
        )
        .unwrap();
        let conversation = Conversation::default();

        let snapshot = reconstructed_hint_snapshot(&conversation, project.path(), GooseMode::Chat);
        assert_eq!(
            snapshot.top_level.len() + reserved_output_bytes,
            MAX_HINT_OUTPUT_BYTES
        );
        assert!(snapshot.top_level.contains("CHAT_BOUNDARY"));

        let old_boundary_content_len =
            MAX_HINT_OUTPUT_BYTES - HINT_EXTRA_SEPARATOR_BYTES - PROJECT_HINTS_HEADER.len();
        fs::write(
            hints_path,
            format!(
                "CHAT_OVERFLOW{}",
                "h".repeat(old_boundary_content_len - "CHAT_OVERFLOW".len())
            ),
        )
        .unwrap();
        let snapshot = reconstructed_hint_snapshot(&conversation, project.path(), GooseMode::Chat);
        assert!(snapshot.top_level.is_empty());
    }
}
