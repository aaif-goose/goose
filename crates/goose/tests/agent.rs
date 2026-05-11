use std::sync::Arc;

use anyhow::Result;
use futures::StreamExt;
use goose::agents::{Agent, AgentEvent, GoosePlatform};
use goose::config::extensions::{set_extension, ExtensionEntry};

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(test)]
    mod schedule_tool_tests {
        use super::*;
        use async_trait::async_trait;
        use chrono::{DateTime, Utc};
        use goose::agents::platform_tools::PLATFORM_MANAGE_SCHEDULE_TOOL_NAME;
        use goose::agents::AgentConfig;
        use goose::config::permission::PermissionManager;
        use goose::config::GooseMode;
        use goose::scheduler::{ScheduledJob, SchedulerError};
        use goose::scheduler_trait::SchedulerTrait;
        use goose::session::{Session, SessionManager};
        use std::path::PathBuf;
        use std::sync::Arc;
        use tempfile::TempDir;

        struct MockScheduler {
            jobs: tokio::sync::Mutex<Vec<ScheduledJob>>,
        }

        impl MockScheduler {
            fn new() -> Self {
                Self {
                    jobs: tokio::sync::Mutex::new(Vec::new()),
                }
            }
        }

        #[async_trait]
        impl SchedulerTrait for MockScheduler {
            async fn add_scheduled_job(
                &self,
                job: ScheduledJob,
                _copy: bool,
            ) -> Result<(), SchedulerError> {
                let mut jobs = self.jobs.lock().await;
                jobs.push(job);
                Ok(())
            }

            async fn schedule_recipe(
                &self,
                _recipe_path: PathBuf,
                _cron_schedule: Option<String>,
            ) -> Result<(), SchedulerError> {
                Ok(())
            }

            async fn list_scheduled_jobs(&self) -> Vec<ScheduledJob> {
                let jobs = self.jobs.lock().await;
                jobs.clone()
            }

            async fn remove_scheduled_job(
                &self,
                id: &str,
                _remove: bool,
            ) -> Result<(), SchedulerError> {
                let mut jobs = self.jobs.lock().await;
                if let Some(pos) = jobs.iter().position(|job| job.id == id) {
                    jobs.remove(pos);
                    Ok(())
                } else {
                    Err(SchedulerError::JobNotFound(id.to_string()))
                }
            }

            async fn pause_schedule(&self, _id: &str) -> Result<(), SchedulerError> {
                Ok(())
            }

            async fn unpause_schedule(&self, _id: &str) -> Result<(), SchedulerError> {
                Ok(())
            }

            async fn run_now(&self, _id: &str) -> Result<String, SchedulerError> {
                Ok("test_session_123".to_string())
            }

            async fn sessions(
                &self,
                _sched_id: &str,
                _limit: usize,
            ) -> Result<Vec<(String, Session)>, SchedulerError> {
                Ok(vec![])
            }

            async fn update_schedule(
                &self,
                _sched_id: &str,
                _new_cron: String,
            ) -> Result<(), SchedulerError> {
                Ok(())
            }

            async fn kill_running_job(&self, _sched_id: &str) -> Result<(), SchedulerError> {
                Ok(())
            }

            async fn get_running_job_info(
                &self,
                _sched_id: &str,
            ) -> Result<Option<(String, DateTime<Utc>)>, SchedulerError> {
                Ok(None)
            }
        }

        #[tokio::test]
        async fn test_schedule_management_tool_list() {
            let temp_dir = TempDir::new().unwrap();
            let data_dir = temp_dir.path().to_path_buf();
            let session_manager = Arc::new(SessionManager::new(data_dir.clone()));
            let permission_manager = Arc::new(PermissionManager::new(data_dir));
            let mock_scheduler = Arc::new(MockScheduler::new());
            let config = AgentConfig::new(
                session_manager,
                permission_manager,
                Some(mock_scheduler),
                GooseMode::Auto,
                false,
                GoosePlatform::GooseCli,
            );
            let agent = Agent::with_config(config);

            let tools = agent.list_tools("test-session-id", None).await;
            let schedule_tool = tools
                .iter()
                .find(|tool| tool.name == PLATFORM_MANAGE_SCHEDULE_TOOL_NAME);
            assert!(schedule_tool.is_some());

            let tool = schedule_tool.unwrap();
            assert!(tool
                .description
                .clone()
                .unwrap_or_default()
                .contains("Manage goose's internal scheduled recipe execution"));
        }

        #[tokio::test]
        async fn test_no_schedule_management_tool_without_scheduler() {
            let agent = Agent::new();

            let tools = agent.list_tools("test-session-id", None).await;
            let schedule_tool = tools
                .iter()
                .find(|tool| tool.name == PLATFORM_MANAGE_SCHEDULE_TOOL_NAME);
            assert!(schedule_tool.is_none());
        }

        #[tokio::test]
        async fn test_schedule_management_tool_in_platform_tools() {
            let temp_dir = TempDir::new().unwrap();
            let data_dir = temp_dir.path().to_path_buf();
            let session_manager = Arc::new(SessionManager::new(data_dir.clone()));
            let permission_manager = Arc::new(PermissionManager::new(data_dir));
            let mock_scheduler = Arc::new(MockScheduler::new());
            let config = AgentConfig::new(
                session_manager,
                permission_manager,
                Some(mock_scheduler),
                GooseMode::Auto,
                false,
                GoosePlatform::GooseCli,
            );
            let agent = Agent::with_config(config);

            let tools = agent
                .list_tools("test-session-id", Some("platform".to_string()))
                .await;

            // Check that the schedule management tool is included in platform tools
            let schedule_tool = tools
                .iter()
                .find(|tool| tool.name == PLATFORM_MANAGE_SCHEDULE_TOOL_NAME);
            assert!(schedule_tool.is_some());

            let tool = schedule_tool.unwrap();
            assert!(tool
                .description
                .clone()
                .unwrap_or_default()
                .contains("Manage goose's internal scheduled recipe execution"));

            // Verify the tool has the expected actions in its schema
            if let Some(properties) = tool.input_schema.get("properties") {
                if let Some(action_prop) = properties.get("action") {
                    if let Some(enum_values) = action_prop.get("enum") {
                        let actions: Vec<String> = enum_values
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_str().unwrap().to_string())
                            .collect();

                        // Check that our session_content action is included
                        assert!(actions.contains(&"session_content".to_string()));
                        assert!(actions.contains(&"list".to_string()));
                        assert!(actions.contains(&"create".to_string()));
                        assert!(actions.contains(&"sessions".to_string()));
                    }
                }
            }
        }

        #[tokio::test]
        async fn test_schedule_management_tool_schema_validation() {
            let temp_dir = TempDir::new().unwrap();
            let data_dir = temp_dir.path().to_path_buf();
            let session_manager = Arc::new(SessionManager::new(data_dir.clone()));
            let permission_manager = Arc::new(PermissionManager::new(data_dir));
            let mock_scheduler = Arc::new(MockScheduler::new());
            let config = AgentConfig::new(
                session_manager,
                permission_manager,
                Some(mock_scheduler),
                GooseMode::Auto,
                false,
                GoosePlatform::GooseCli,
            );
            let agent = Agent::with_config(config);

            let tools = agent.list_tools("test-session-id", None).await;
            let schedule_tool = tools
                .iter()
                .find(|tool| tool.name == PLATFORM_MANAGE_SCHEDULE_TOOL_NAME);
            assert!(schedule_tool.is_some());

            let tool = schedule_tool.unwrap();

            // Verify the tool schema has the session_id parameter for session_content action
            if let Some(properties) = tool.input_schema.get("properties") {
                assert!(properties.get("session_id").is_some());

                if let Some(session_id_prop) = properties.get("session_id") {
                    assert_eq!(
                        session_id_prop.get("type").unwrap().as_str().unwrap(),
                        "string"
                    );
                    assert!(session_id_prop
                        .get("description")
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .contains("Session identifier for session_content action"));
                }
            }
        }
    }

    #[cfg(test)]
    mod retry_tests {
        use super::*;
        use goose::agents::types::{RetryConfig, SuccessCheck};

        #[tokio::test]
        async fn test_retry_success_check_execution() -> Result<()> {
            use goose::agents::retry::execute_success_checks;

            let retry_config = RetryConfig {
                max_retries: 3,
                checks: vec![],
                on_failure: None,
                timeout_seconds: Some(30),
                on_failure_timeout_seconds: Some(60),
            };

            let success_checks = vec![SuccessCheck::Shell {
                command: "echo 'test'".to_string(),
            }];

            let result = execute_success_checks(&success_checks, &retry_config).await;
            assert!(result.is_ok(), "Success check should pass");
            assert!(result.unwrap(), "Command should succeed");

            let fail_checks = vec![SuccessCheck::Shell {
                command: "false".to_string(),
            }];

            let result = execute_success_checks(&fail_checks, &retry_config).await;
            assert!(result.is_ok(), "Success check execution should not error");
            assert!(!result.unwrap(), "Command should fail");

            Ok(())
        }

        #[tokio::test]
        async fn test_retry_logic_with_validation_errors() -> Result<()> {
            let invalid_retry_config = RetryConfig {
                max_retries: 0,
                checks: vec![],
                on_failure: None,
                timeout_seconds: Some(0),
                on_failure_timeout_seconds: None,
            };

            let validation_result = invalid_retry_config.validate();
            assert!(
                validation_result.is_err(),
                "Should validate max_retries > 0"
            );
            assert!(validation_result
                .unwrap_err()
                .contains("max_retries must be greater than 0"));

            Ok(())
        }

        #[tokio::test]
        async fn test_retry_attempts_counter_reset() -> Result<()> {
            let agent = Agent::new();

            agent.reset_retry_attempts().await;
            let initial_attempts = agent.get_retry_attempts().await;
            assert_eq!(initial_attempts, 0);

            let new_attempts = agent.increment_retry_attempts().await;
            assert_eq!(new_attempts, 1);

            agent.reset_retry_attempts().await;
            let reset_attempts = agent.get_retry_attempts().await;
            assert_eq!(reset_attempts, 0);

            Ok(())
        }
    }

    #[cfg(test)]
    mod max_turns_tests {
        use super::*;
        use async_trait::async_trait;
        use goose::agents::SessionConfig;
        use goose::config::GooseMode;
        use goose::conversation::message::{Message, MessageContent};
        use goose::model::ModelConfig;
        use goose::providers::base::{
            stream_from_single_message, MessageStream, Provider, ProviderDef, ProviderMetadata,
            ProviderUsage, Usage,
        };
        use goose::providers::errors::ProviderError;
        use goose::session::session_manager::SessionType;
        use rmcp::model::{CallToolRequestParams, Tool};
        use rmcp::object;
        use std::path::PathBuf;

        struct MockToolProvider {}

        impl MockToolProvider {
            fn new() -> Self {
                Self {}
            }
        }

        impl ProviderDef for MockToolProvider {
            type Provider = Self;

            fn metadata() -> ProviderMetadata {
                ProviderMetadata {
                    name: "mock".to_string(),
                    display_name: "Mock Provider".to_string(),
                    description: "Mock provider for testing".to_string(),
                    default_model: "mock-model".to_string(),
                    known_models: vec![],
                    model_doc_link: "".to_string(),
                    config_keys: vec![],
                    setup_steps: vec![],
                    model_selection_hint: None,
                }
            }

            fn from_env(
                _model: ModelConfig,
                _extensions: Vec<goose::config::ExtensionConfig>,
            ) -> futures::future::BoxFuture<'static, anyhow::Result<Self>> {
                Box::pin(async { Ok(Self::new()) })
            }
        }

        #[async_trait]
        impl Provider for MockToolProvider {
            async fn stream(
                &self,
                _model_config: &ModelConfig,
                _session_id: &str,
                _system_prompt: &str,
                _messages: &[Message],
                _tools: &[Tool],
            ) -> Result<MessageStream, ProviderError> {
                let tool_call = CallToolRequestParams::new("test_tool")
                    .with_arguments(object!({"param": "value"}));
                let message = Message::assistant().with_tool_request("call_123", Ok(tool_call));

                let usage = ProviderUsage::new(
                    "mock-model".to_string(),
                    Usage::new(Some(10), Some(5), Some(15)),
                );

                Ok(stream_from_single_message(message, usage))
            }

            fn get_model_config(&self) -> ModelConfig {
                ModelConfig::new("mock-model").unwrap()
            }

            fn get_name(&self) -> &str {
                "mock-test"
            }
        }

        #[tokio::test]
        async fn test_max_turns_limit() -> Result<()> {
            let agent = Agent::new();
            let provider = Arc::new(MockToolProvider::new());
            let user_message = Message::user().with_text("Hello");

            let session = agent
                .config
                .session_manager
                .create_session(
                    PathBuf::default(),
                    "max-turn-test".to_string(),
                    SessionType::Hidden,
                    GooseMode::default(),
                )
                .await?;

            agent.update_provider(provider, &session.id).await?;

            let session_config = SessionConfig {
                id: session.id,
                schedule_id: None,
                max_turns: Some(1),
                retry_config: None,
            };

            let reply_stream = agent.reply(user_message, session_config, None).await?;
            tokio::pin!(reply_stream);

            let mut responses = Vec::new();
            while let Some(response_result) = reply_stream.next().await {
                match response_result {
                    Ok(AgentEvent::Message(response)) => {
                        if let Some(MessageContent::ActionRequired(action)) =
                            response.content.first()
                        {
                            if let goose::conversation::message::ActionRequiredData::ToolConfirmation { id, .. } = &action.data {
                                agent.handle_confirmation(
                                    id.clone(),
                                    goose::permission::PermissionConfirmation {
                                        principal_type: goose::permission::permission_confirmation::PrincipalType::Tool,
                                        permission: goose::permission::Permission::AllowOnce,
                                    }
                                ).await;
                            }
                        }
                        responses.push(response);
                    }
                    Ok(AgentEvent::McpNotification(_)) => {}
                    Ok(AgentEvent::HistoryReplaced(_updated_conversation)) => {
                        // We should update the conversation here, but we're not reading it
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }

            assert!(
                !responses.is_empty(),
                "Expected at least 1 response, got {}",
                responses.len()
            );

            // Look for the max turns message as the last response
            let last_response = responses.last().unwrap();
            let last_content = last_response.content.first().unwrap();
            if let MessageContent::Text(text_content) = last_content {
                assert!(text_content.text.contains(
                    "I've reached the maximum number of actions I can do without user input"
                ));
            } else {
                panic!("Expected text content in last message");
            }
            Ok(())
        }
    }

    #[cfg(test)]
    mod tool_pair_summarization_tests {
        use super::*;
        use async_trait::async_trait;
        use goose::agents::SessionConfig;
        use goose::config::base::Config;
        use goose::config::GooseMode;
        use goose::conversation::message::Message;
        use goose::model::ModelConfig;
        use goose::providers::base::{
            stream_from_single_message, MessageStream, Provider, ProviderDef, ProviderMetadata,
            ProviderUsage, Usage,
        };
        use goose::providers::errors::ProviderError;
        use goose::session::session_manager::SessionType;
        use rmcp::model::{AnnotateAble, CallToolRequestParams, CallToolResult, RawContent, Tool};
        use std::path::PathBuf;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        /// Mock provider that returns text for the main reply and summaries for
        /// summarization calls. Distinguishes by checking if tools are empty
        /// (summarization calls pass no tools).
        struct SummarizationTestProvider {
            summary_count: AtomicUsize,
        }

        impl SummarizationTestProvider {
            fn new() -> Self {
                Self {
                    summary_count: AtomicUsize::new(0),
                }
            }
        }

        impl ProviderDef for SummarizationTestProvider {
            type Provider = Self;

            fn metadata() -> ProviderMetadata {
                ProviderMetadata {
                    name: "mock-summarization".to_string(),
                    display_name: "Mock Summarization Provider".to_string(),
                    description: "Mock provider for summarization tests".to_string(),
                    default_model: "mock-model".to_string(),
                    known_models: vec![],
                    model_doc_link: "".to_string(),
                    config_keys: vec![],
                    setup_steps: vec![],
                    model_selection_hint: None,
                }
            }

            fn from_env(
                _model: ModelConfig,
                _extensions: Vec<goose::config::ExtensionConfig>,
            ) -> futures::future::BoxFuture<'static, anyhow::Result<Self>> {
                Box::pin(async { Ok(Self::new()) })
            }
        }

        #[async_trait]
        impl Provider for SummarizationTestProvider {
            async fn stream(
                &self,
                _model_config: &ModelConfig,
                _session_id: &str,
                system_prompt: &str,
                _messages: &[Message],
                _tools: &[Tool],
            ) -> Result<MessageStream, ProviderError> {
                let message = if system_prompt.contains("summarize a tool call") {
                    // Summarization call — return a unique summary
                    let n = self.summary_count.fetch_add(1, Ordering::SeqCst);
                    Message::assistant().with_text(format!("Summary of tool call #{}", n))
                } else {
                    // Main agent reply — return plain text so the loop exits
                    Message::assistant().with_text("Done processing.")
                };

                let usage = ProviderUsage::new(
                    "mock-model".to_string(),
                    Usage::new(Some(10), Some(5), Some(15)),
                );
                Ok(stream_from_single_message(message, usage))
            }

            fn get_model_config(&self) -> ModelConfig {
                ModelConfig::new("mock-model").unwrap()
            }

            fn get_name(&self) -> &str {
                "mock-summarization"
            }
        }

        /// Test that batch tool pair summarization preserves all summaries.
        ///
        /// Pre-populates a session with enough tool call/response pairs to trigger
        /// batch summarization, runs agent.reply(), then verifies:
        /// - All 10 summaries are present in the final conversation
        /// - The original tool pairs are marked invisible
        #[tokio::test]
        async fn test_batch_summarization_preserves_all_summaries() -> Result<()> {
            // Set a low cutoff so we don't need hundreds of tool pairs.
            // cutoff=2 means we need >2+10=12 visible tool pairs to trigger.
            Config::global()
                .set_param("GOOSE_TOOL_CALL_CUTOFF", 2)
                .unwrap();

            let agent = Agent::new();
            let session_manager = agent.config.session_manager.clone();
            let provider = Arc::new(SummarizationTestProvider::new());

            let session = session_manager
                .create_session(
                    PathBuf::from("."),
                    "summarization-test".to_string(),
                    SessionType::Hidden,
                    GooseMode::Auto,
                )
                .await?;

            agent.update_provider(provider, &session.id).await?;

            // Pre-populate 13 tool pairs (need > cutoff + batch_size = 12 to trigger).
            // Timestamps in the past so DB ordering places summaries before current turn.
            let base_ts = chrono::Utc::now().timestamp() - 100;

            let mut initial_msg = Message::user().with_text("help me read some files");
            initial_msg.created = base_ts;
            session_manager
                .add_message(&session.id, &initial_msg)
                .await?;

            for i in 0..13 {
                let call_id = format!("precall_{}", i);
                let mut req_msg = Message::assistant()
                    .with_tool_request(&call_id, Ok(CallToolRequestParams::new("read_file")))
                    .with_generated_id();
                req_msg.created = base_ts + i as i64 + 1;
                session_manager.add_message(&session.id, &req_msg).await?;

                let mut resp_msg = Message::user()
                    .with_tool_response(
                        &call_id,
                        Ok(CallToolResult::success(vec![RawContent::text(format!(
                            "content of file {}",
                            i
                        ))
                        .no_annotation()])),
                    )
                    .with_generated_id();
                resp_msg.created = base_ts + i as i64 + 1;
                session_manager.add_message(&session.id, &resp_msg).await?;
            }

            // Send a user message to trigger the reply loop
            let user_message = Message::user().with_text("summarize what you found");

            let session_config = SessionConfig {
                id: session.id.clone(),
                schedule_id: None,
                max_turns: Some(1),
                retry_config: None,
            };

            let reply_stream = agent.reply(user_message, session_config, None).await?;
            tokio::pin!(reply_stream);

            // Drain the stream
            while let Some(event) = reply_stream.next().await {
                match event {
                    Ok(AgentEvent::Message(_)) => {}
                    Ok(_) => {}
                    Err(e) => return Err(e),
                }
            }

            // Load the final session and inspect the conversation
            let final_session = session_manager.get_session(&session.id, true).await?;
            let conversation = final_session
                .conversation
                .expect("Session should have a conversation");
            let messages = conversation.messages();

            // Count summaries: messages that are agent-visible, not user-visible,
            // and contain our summary text pattern
            let summaries: Vec<&Message> = messages
                .iter()
                .filter(|m| {
                    m.metadata.agent_visible
                        && !m.metadata.user_visible
                        && m.as_concat_text().starts_with("Summary of tool call #")
                })
                .collect();

            assert_eq!(
                summaries.len(),
                10,
                "Expected 10 summaries (one full batch), got {}. Summary texts: {:?}",
                summaries.len(),
                summaries
                    .iter()
                    .map(|m| m.as_concat_text())
                    .collect::<Vec<_>>()
            );

            // Verify each summary is unique
            let summary_texts: std::collections::HashSet<String> =
                summaries.iter().map(|m| m.as_concat_text()).collect();
            assert_eq!(summary_texts.len(), 10, "All 10 summaries should be unique");

            // Count invisible tool pairs: original pairs that were summarized
            // should have agent_visible=false
            let invisible_tool_msgs: Vec<&Message> = messages
                .iter()
                .filter(|m| !m.metadata.agent_visible && (m.is_tool_call() || m.is_tool_response()))
                .collect();

            // Each summarized pair = 2 invisible messages (request + response)
            assert_eq!(
                invisible_tool_msgs.len(),
                20, // 10 pairs × 2 messages
                "Expected 20 invisible tool messages (10 summarized pairs), got {}",
                invisible_tool_msgs.len()
            );

            // Summaries must appear before the current turn's reply, not after it
            let agent_visible: Vec<&Message> = messages
                .iter()
                .filter(|m| m.metadata.agent_visible)
                .collect();

            let last_summary_pos = agent_visible
                .iter()
                .rposition(|m| m.as_concat_text().starts_with("Summary of tool call #"))
                .expect("Should have at least one summary");
            let agent_reply_pos = agent_visible
                .iter()
                .position(|m| m.as_concat_text().contains("Done processing."))
                .expect("Should have the agent reply");

            assert!(
                last_summary_pos < agent_reply_pos,
                "Summaries appeared after the current turn's reply: last_summary={}, reply={}",
                last_summary_pos,
                agent_reply_pos,
            );

            // Clean up the config override
            Config::global().delete("GOOSE_TOOL_CALL_CUTOFF").unwrap();

            Ok(())
        }
    }

    #[cfg(test)]
    mod extension_manager_tests {
        use super::*;
        use goose::agents::extension::ExtensionConfig;
        use goose::agents::platform_extensions::{
            MANAGE_EXTENSIONS_TOOL_NAME, SEARCH_AVAILABLE_EXTENSIONS_TOOL_NAME,
        };
        use goose::agents::AgentConfig;
        use goose::config::permission::PermissionManager;
        use goose::config::GooseMode;
        use goose::session::SessionManager;

        async fn setup_agent_with_extension_manager() -> (Agent, String) {
            use goose::session::session_manager::SessionType;

            // Add the TODO extension to the config so it can be discovered by search_available_extensions
            // Set it as disabled initially so tests can enable it
            let todo_extension_entry = ExtensionEntry {
                enabled: false,
                config: ExtensionConfig::Platform {
                    name: "todo".to_string(),
                    description:
                        "Enable a todo list for goose so it can keep track of what it is doing"
                            .to_string(),
                    display_name: Some("Todo".to_string()),
                    bundled: Some(true),
                    available_tools: vec![],
                },
            };
            set_extension(todo_extension_entry);

            // Create agent with session_id from the start
            let temp_dir = tempfile::tempdir().unwrap();
            let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
            let config = AgentConfig::new(
                session_manager.clone(),
                PermissionManager::instance(),
                None,
                GooseMode::default(),
                false,
                GoosePlatform::GooseCli,
            );

            let agent = Agent::with_config(config);

            let session = session_manager
                .create_session(
                    std::path::PathBuf::from("."),
                    "Test Session".to_string(),
                    SessionType::Hidden,
                    GooseMode::default(),
                )
                .await
                .expect("Failed to create session");
            let session_id = session.id;

            // Now add the extension manager platform extension
            let ext_config = ExtensionConfig::Platform {
                name: "extensionmanager".to_string(),
                description: "Extension Manager".to_string(),
                display_name: Some("Extension Manager".to_string()),
                bundled: Some(true),
                available_tools: vec![],
            };

            agent
                .add_extension(ext_config, &session_id)
                .await
                .expect("Failed to add extension manager");
            (agent, session_id)
        }

        #[tokio::test]
        async fn test_extension_manager_tools_available() {
            let (agent, session_id) = setup_agent_with_extension_manager().await;
            let tools = agent.list_tools(&session_id, None).await;

            // Note: Tool names are prefixed with the normalized extension name "extensionmanager"
            // not the display name "Extension Manager"
            let search_tool = tools.iter().find(|tool| {
                tool.name == format!("extensionmanager__{SEARCH_AVAILABLE_EXTENSIONS_TOOL_NAME}")
            });
            assert!(
                search_tool.is_some(),
                "search_available_extensions tool should be available"
            );

            let manage_tool = tools.iter().find(|tool| {
                tool.name == format!("extensionmanager__{MANAGE_EXTENSIONS_TOOL_NAME}")
            });
            assert!(
                manage_tool.is_some(),
                "manage_extensions tool should be available"
            );
        }
    }

    #[cfg(test)]
    mod streaming_persistence_tests {
        use super::*;
        use async_trait::async_trait;
        use goose::agents::{AgentConfig, SessionConfig};
        use goose::config::permission::PermissionManager;
        use goose::config::GooseMode;
        use goose::conversation::message::Message;
        use goose::model::ModelConfig;
        use goose::providers::base::{
            MessageStream, Provider, ProviderDef, ProviderMetadata, ProviderUsage, Usage,
        };
        use goose::providers::errors::ProviderError;
        use goose::session::session_manager::SessionType;
        use goose::session::SessionManager;
        use rmcp::model::{CallToolRequestParams, Role, Tool};
        use rmcp::object;
        use std::path::PathBuf;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio_util::sync::CancellationToken;

        struct MultiStepProvider {
            call_count: AtomicUsize,
            cancel_token: CancellationToken,
        }

        impl MultiStepProvider {
            fn new(cancel_token: CancellationToken) -> Self {
                Self {
                    call_count: AtomicUsize::new(0),
                    cancel_token,
                }
            }
        }

        impl ProviderDef for MultiStepProvider {
            type Provider = Self;

            fn metadata() -> ProviderMetadata {
                ProviderMetadata {
                    name: "multi-step-mock".to_string(),
                    display_name: "Multi-Step Mock".to_string(),
                    description: "Mock provider for streaming persistence tests".to_string(),
                    default_model: "mock-model".to_string(),
                    known_models: vec![],
                    model_doc_link: "".to_string(),
                    config_keys: vec![],
                    setup_steps: vec![],
                    model_selection_hint: None,
                }
            }

            fn from_env(
                _model: ModelConfig,
                _extensions: Vec<goose::config::ExtensionConfig>,
            ) -> futures::future::BoxFuture<'static, anyhow::Result<Self>> {
                unimplemented!()
            }
        }

        #[async_trait]
        impl Provider for MultiStepProvider {
            async fn stream(
                &self,
                _model_config: &ModelConfig,
                _session_id: &str,
                _system_prompt: &str,
                _messages: &[Message],
                _tools: &[Tool],
            ) -> Result<MessageStream, ProviderError> {
                let call = self.call_count.fetch_add(1, Ordering::SeqCst);
                let usage = ProviderUsage::new(
                    "mock-model".to_string(),
                    Usage::new(Some(10), Some(5), Some(15)),
                );

                match call {
                    0 => {
                        let tool_call = CallToolRequestParams::new("test_tool")
                            .with_arguments(object!({"param": "value"}));
                        let message =
                            Message::assistant().with_tool_request("call_1", Ok(tool_call));
                        let stream =
                            futures::stream::once(async move { Ok((Some(message), Some(usage))) });
                        Ok(Box::pin(stream))
                    }
                    1 => {
                        let msg_id = format!("msg_{}", uuid::Uuid::new_v4());
                        let tokens = vec!["Hello", " world", ", how", " are", " you?"];
                        let stream = futures::stream::iter(tokens.into_iter().enumerate().map(
                            move |(i, token)| {
                                let msg = Message::assistant()
                                    .with_text(token)
                                    .with_id(msg_id.clone());
                                let u = if i == 4 { Some(usage.clone()) } else { None };
                                Ok((Some(msg), u))
                            },
                        ));
                        Ok(Box::pin(stream))
                    }
                    _ => {
                        let cancel = self.cancel_token.clone();
                        let msg_id = format!("msg_{}", uuid::Uuid::new_v4());
                        let tokens = vec!["This ", "should ", "be ", "cancelled ", "soon."];
                        let stream = futures::stream::iter(tokens.into_iter().enumerate().map(
                            move |(i, token)| {
                                if i == 1 {
                                    cancel.cancel();
                                }
                                let msg = Message::assistant()
                                    .with_text(token)
                                    .with_id(msg_id.clone());
                                let u = if i == 4 { Some(usage.clone()) } else { None };
                                Ok((Some(msg), u))
                            },
                        ));
                        Ok(Box::pin(stream))
                    }
                }
            }

            fn get_model_config(&self) -> ModelConfig {
                ModelConfig::new("mock-model").unwrap()
            }

            fn get_name(&self) -> &str {
                "multi-step-mock"
            }
        }

        #[tokio::test]
        async fn test_streaming_text_not_persisted_per_token() -> Result<()> {
            let cancel_token = CancellationToken::new();
            let temp_dir = tempfile::tempdir()?;
            let session_manager = Arc::new(SessionManager::new(temp_dir.path().to_path_buf()));
            let config = AgentConfig::new(
                session_manager.clone(),
                PermissionManager::instance(),
                None,
                GooseMode::Auto,
                true, // disable session naming so it doesn't consume a provider call
                GoosePlatform::GooseCli,
            );
            let agent = Agent::with_config(config);
            let provider = Arc::new(MultiStepProvider::new(cancel_token.clone()));

            let session = session_manager
                .create_session(
                    PathBuf::default(),
                    "streaming-test".to_string(),
                    SessionType::Hidden,
                    GooseMode::default(),
                )
                .await?;

            let session_id = session.id.clone();
            agent.update_provider(provider, &session_id).await?;

            // ── Single reply: tool call (call 0) → text stream (call 1) → cancelled text (call 2)
            // max_turns=3 allows all three provider calls within one reply().
            //   call 0: tool call → agent executes tool, loops
            //   call 1: 5 text deltas → no tools called, agent exits loop
            //   call 2: 5 text deltas, cancel token fired after 1st → agent interrupted
            //
            // Because call 1 ends the agent loop (no_tools_called=true → exit),
            // call 2 is NOT reached in the same reply. We issue a second reply()
            // with the cancel token so the provider triggers cancellation.
            let session_config = SessionConfig {
                id: session_id.clone(),
                schedule_id: None,
                max_turns: Some(2),
                retry_config: None,
            };

            let reply_stream = agent
                .reply(
                    Message::user().with_text("Do something then say hello"),
                    session_config,
                    None,
                )
                .await?;
            tokio::pin!(reply_stream);

            while let Some(event) = reply_stream.next().await {
                match event {
                    Ok(AgentEvent::Message(_)) => {}
                    Ok(_) => {}
                    Err(e) => return Err(e),
                }
            }

            // ── Check persisted state after reply 1 ─────────────────
            let reloaded = session_manager.get_session(&session_id, true).await?;
            let messages = reloaded
                .conversation
                .expect("should have conversation")
                .messages()
                .to_vec();

            let user_count = messages.iter().filter(|m| m.role == Role::User).count();
            let asst_count = messages
                .iter()
                .filter(|m| m.role == Role::Assistant)
                .count();

            // Expected: user(prompt) + assistant(tool-req) + user(tool-resp) + assistant(text)
            assert_eq!(
                user_count, 2,
                "Expected 2 user messages (prompt + tool response), got {user_count}",
            );
            assert_eq!(
                asst_count, 2,
                "Expected 2 assistant messages (tool request + text reply), got {asst_count} \
                 — streaming text deltas are being persisted as separate messages",
            );

            // ── Reply 2: text stream with provider-triggered cancellation (call 2)
            let session_config2 = SessionConfig {
                id: session_id.clone(),
                schedule_id: None,
                max_turns: Some(2),
                retry_config: None,
            };

            let reply_stream2 = agent
                .reply(
                    Message::user().with_text("Tell me more"),
                    session_config2,
                    Some(cancel_token),
                )
                .await?;
            tokio::pin!(reply_stream2);

            while let Some(event) = reply_stream2.next().await {
                match event {
                    Ok(_) => {}
                    Err(e) => return Err(e),
                }
            }

            // ── Check persisted state after cancellation ────────────
            let reloaded2 = session_manager.get_session(&session_id, true).await?;
            let messages2 = reloaded2
                .conversation
                .expect("should have conversation")
                .messages()
                .to_vec();

            let user_count2 = messages2.iter().filter(|m| m.role == Role::User).count();
            let asst_count2 = messages2
                .iter()
                .filter(|m| m.role == Role::Assistant)
                .count();

            // Reply 2 added 1 user message. The cancelled stream should
            // have persisted at most 1 (partial) assistant message.
            assert_eq!(
                user_count2, 3,
                "Expected 3 user messages (2 from reply 1 + follow-up), got {user_count2}",
            );
            assert!(
                asst_count2 <= 3,
                "Expected at most 3 assistant messages (2 from reply 1 + at most 1 partial \
                 from cancelled reply 2), got {asst_count2} \
                 — streaming deltas are leaking into persistence",
            );

            Ok(())
        }
    }
}
