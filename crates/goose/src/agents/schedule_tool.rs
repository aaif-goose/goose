//! Schedule tool handlers for the goose agent
//!
//! This module contains all the handlers for the schedule management platform tool,
//! including job creation, execution, monitoring, and session management.

use std::sync::Arc;

use crate::mcp_utils::ToolResult;
use chrono::Utc;
use rmcp::model::{Content, ErrorCode, ErrorData};

use super::Agent;
use crate::recipe::Recipe;
use crate::scheduler_trait::SchedulerTrait;

impl Agent {
    /// Handle schedule management tool calls
    pub async fn handle_schedule_management(
        &self,
        arguments: serde_json::Value,
        _request_id: String,
    ) -> ToolResult<Vec<Content>> {
        let scheduler = self.config.scheduler_service.clone().ok_or_else(|| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                "Scheduler not available".to_string(),
                None,
            )
        })?;

        let action = arguments
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    "Missing 'action' parameter".to_string(),
                    None,
                )
            })?;

        match action {
            "list" => self.handle_list_jobs(scheduler).await,
            "create" => self.handle_create_job(scheduler, arguments).await,
            "run_now" => self.handle_run_now(scheduler, arguments).await,
            "pause" => self.handle_pause_job(scheduler, arguments).await,
            "unpause" => self.handle_unpause_job(scheduler, arguments).await,
            "delete" => self.handle_delete_job(scheduler, arguments).await,
            "kill" => self.handle_kill_job(scheduler, arguments).await,
            "inspect" => self.handle_inspect_job(scheduler, arguments).await,
            "sessions" => self.handle_list_sessions(scheduler, arguments).await,
            "session_content" => self.handle_session_content(arguments).await,
            _ => Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Unknown action: {}", action),
                None,
            )),
        }
    }

    async fn handle_list_jobs(
        &self,
        scheduler: Arc<dyn SchedulerTrait>,
    ) -> ToolResult<Vec<Content>> {
        let jobs = scheduler.list_scheduled_jobs().await;
        let jobs_json = serde_json::to_string_pretty(&jobs).map_err(|e| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to serialize jobs: {}", e),
                None,
            )
        })?;
        Ok(vec![Content::text(format!(
            "Scheduled Jobs:\n{}",
            jobs_json
        ))])
    }

    async fn handle_create_job(
        &self,
        scheduler: Arc<dyn SchedulerTrait>,
        arguments: serde_json::Value,
    ) -> ToolResult<Vec<Content>> {
        let recipe_path = arguments
            .get("recipe_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    "Missing 'recipe_path' parameter".to_string(),
                    None,
                )
            })?;

        let cron_expression = arguments
            .get("cron_expression")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    "Missing 'cron_expression' parameter".to_string(),
                    None,
                )
            })?;

        // Get the execution_mode parameter, defaulting to "background" if not provided
        let execution_mode = arguments
            .get("execution_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("background");

        if !std::path::Path::new(recipe_path).exists() {
            return Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Recipe file not found: {}", recipe_path),
                None,
            ));
        }

        // Validate it's a valid recipe by trying to parse it
        match std::fs::read_to_string(recipe_path) {
            Ok(content) => {
                if recipe_path.ends_with(".json") {
                    serde_json::from_str::<Recipe>(&content).map_err(|e| {
                        ErrorData::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Invalid JSON recipe: {}", e),
                            None,
                        )
                    })?;
                } else {
                    serde_yaml::from_str::<Recipe>(&content).map_err(|e| {
                        ErrorData::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Invalid YAML recipe: {}", e),
                            None,
                        )
                    })?;
                }
            }
            Err(e) => {
                return Err(ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Cannot read recipe file: {}", e),
                    None,
                ))
            }
        }

        // Generate unique job ID
        let job_id = format!("agent_created_{}", Utc::now().timestamp());

        let job = crate::scheduler::ScheduledJob {
            id: job_id.clone(),
            source: recipe_path.to_string(),
            cron: cron_expression.to_string(),
            last_run: None,
            currently_running: false,
            paused: false,
            current_session_id: None,
            process_start_time: None,
        };

        match scheduler.add_scheduled_job(job, true).await {
            Ok(()) => Ok(vec![Content::text(format!(
                "Successfully created scheduled job '{}' for recipe '{}' with cron expression '{}' in {} mode",
                job_id, recipe_path, cron_expression, execution_mode
            ))]),
            Err(e) => Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to create job: {}", e),
                None,
            )),
        }
    }

    /// Run a scheduled job immediately
    async fn handle_run_now(
        &self,
        scheduler: Arc<dyn SchedulerTrait>,
        arguments: serde_json::Value,
    ) -> ToolResult<Vec<Content>> {
        let job_id = arguments
            .get("job_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    "Missing 'job_id' parameter".to_string(),
                    None,
                )
            })?;

        match scheduler.run_now(job_id).await {
            Ok(session_id) => Ok(vec![Content::text(format!(
                "Successfully started job '{}'. Session ID: {}",
                job_id, session_id
            ))]),
            Err(e) => Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to run job: {}", e),
                None,
            )),
        }
    }

    /// Pause a scheduled job
    async fn handle_pause_job(
        &self,
        scheduler: Arc<dyn SchedulerTrait>,
        arguments: serde_json::Value,
    ) -> ToolResult<Vec<Content>> {
        let job_id = arguments
            .get("job_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Missing 'job_id' parameter".to_string(),
                    None,
                )
            })?;

        match scheduler.pause_schedule(job_id).await {
            Ok(()) => Ok(vec![Content::text(format!(
                "Successfully paused job '{}'",
                job_id
            ))]),
            Err(e) => Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to pause job: {}", e),
                None,
            )),
        }
    }

    /// Resume a paused scheduled job
    async fn handle_unpause_job(
        &self,
        scheduler: Arc<dyn SchedulerTrait>,
        arguments: serde_json::Value,
    ) -> ToolResult<Vec<Content>> {
        let job_id = arguments
            .get("job_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Missing 'job_id' parameter".to_string(),
                    None,
                )
            })?;

        match scheduler.unpause_schedule(job_id).await {
            Ok(()) => Ok(vec![Content::text(format!(
                "Successfully unpaused job '{}'",
                job_id
            ))]),
            Err(e) => Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to unpause job: {}", e),
                None,
            )),
        }
    }

    /// Delete a scheduled job
    async fn handle_delete_job(
        &self,
        scheduler: Arc<dyn SchedulerTrait>,
        arguments: serde_json::Value,
    ) -> ToolResult<Vec<Content>> {
        let job_id = arguments
            .get("job_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Missing 'job_id' parameter".to_string(),
                    None,
                )
            })?;

        match scheduler.remove_scheduled_job(job_id, false).await {
            Ok(()) => Ok(vec![Content::text(format!(
                "Successfully removed schedule for job '{}'",
                job_id
            ))]),
            Err(e) => Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to remove schedule: {}", e),
                None,
            )),
        }
    }

    /// Terminate a currently running job
    async fn handle_kill_job(
        &self,
        scheduler: Arc<dyn SchedulerTrait>,
        arguments: serde_json::Value,
    ) -> ToolResult<Vec<Content>> {
        let job_id = arguments
            .get("job_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Missing 'job_id' parameter".to_string(),
                    None,
                )
            })?;

        match scheduler.kill_running_job(job_id).await {
            Ok(()) => Ok(vec![Content::text(format!(
                "Successfully killed running job '{}'",
                job_id
            ))]),
            Err(e) => Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to kill job: {}", e),
                None,
            )),
        }
    }

    /// Get information about a running job
    async fn handle_inspect_job(
        &self,
        scheduler: Arc<dyn SchedulerTrait>,
        arguments: serde_json::Value,
    ) -> ToolResult<Vec<Content>> {
        let job_id = arguments
            .get("job_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Missing 'job_id' parameter".to_string(),
                    None,
                )
            })?;

        match scheduler.get_running_job_info(job_id).await {
            Ok(Some((session_id, start_time))) => {
                let duration = Utc::now().signed_duration_since(start_time);
                Ok(vec![Content::text(format!(
                    "Job '{}' is currently running:\n- Session ID: {}\n- Started: {}\n- Duration: {} seconds",
                    job_id, session_id, start_time.to_rfc3339(), duration.num_seconds()
                ))])
            }
            Ok(None) => Ok(vec![Content::text(format!(
                "Job '{}' is not currently running",
                job_id
            ))]),
            Err(e) => Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to inspect job: {}", e),
                None,
            )),
        }
    }

    /// List execution sessions for a job
    async fn handle_list_sessions(
        &self,
        scheduler: Arc<dyn SchedulerTrait>,
        arguments: serde_json::Value,
    ) -> ToolResult<Vec<Content>> {
        let job_id = arguments
            .get("job_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    "Missing 'job_id' parameter".to_string(),
                    None,
                )
            })?;

        let limit = arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        match scheduler.sessions(job_id, limit).await {
            Ok(sessions) => {
                if sessions.is_empty() {
                    Ok(vec![Content::text(format!(
                        "No sessions found for job '{}'",
                        job_id
                    ))])
                } else {
                    let sessions_info: Vec<String> = sessions
                        .into_iter()
                        .map(|(session_name, session)| {
                            format!(
                                "- Session: {} (Messages: {}, Working Dir: {})",
                                session_name,
                                session.conversation.unwrap_or_default().len(),
                                session.working_dir.display()
                            )
                        })
                        .collect();

                    Ok(vec![Content::text(format!(
                        "Sessions for job '{}':\n{}",
                        job_id,
                        sessions_info.join("\n")
                    ))])
                }
            }
            Err(e) => Err(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("Failed to list sessions: {}", e),
                None,
            )),
        }
    }

    /// Get the full content (metadata and messages) of a specific session
    async fn handle_session_content(
        &self,
        arguments: serde_json::Value,
    ) -> ToolResult<Vec<Content>> {
        let session_id = arguments
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Missing 'session_id' parameter".to_string(),
                    None,
                )
            })?;

        let session = match self
            .config
            .session_manager
            .get_session(session_id, true)
            .await
        {
            Ok(metadata) => metadata,
            Err(e) => {
                return Err(ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to read session for '{}': {}", session_id, e),
                    None,
                ));
            }
        };

        // Format the response with metadata and messages
        let metadata_json = match serde_json::to_string_pretty(&session) {
            Ok(json) => json,
            Err(e) => {
                return Err(ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Failed to serialize metadata: {}", e),
                    None,
                ));
            }
        };

        Ok(vec![Content::text(format!(
            "Session '{}' Content:\n\nSession:\n{}",
            session_id, metadata_json
        ))])
    }
}
