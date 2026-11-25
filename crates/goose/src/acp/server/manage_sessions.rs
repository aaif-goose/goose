use super::*;
use std::collections::HashSet;

impl AcpConnection {
    pub(super) async fn on_update_working_dir(
        &self,
        req: UpdateWorkingDirRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let working_dir = req.working_dir.trim().to_string();
        if working_dir.is_empty() {
            return Err(agent_client_protocol::Error::invalid_params()
                .data("working directory cannot be empty"));
        }
        let path = std::path::PathBuf::from(&working_dir);
        validate_absolute_cwd(&path)?;
        let session_id = &req.session_id;

        let session = self
            .services
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|_| {
                agent_client_protocol::Error::resource_not_found(Some(session_id.to_string()))
                    .data(format!("Session not found: {}", session_id))
            })?;

        self.open_session(session_id).await?;
        self.services
            .session_manager
            .update(&session.id)
            .working_dir(path)
            .apply()
            .await
            .internal_err_ctx("Failed to update working directory")?;

        let session = self
            .services
            .session_manager
            .get_session(session_id, false)
            .await
            .internal_err_ctx("Failed to reload session")?;
        self.services.providers.release(&session.id);
        self.services
            .restore_provider_from_session(&session)
            .await
            .internal_err_ctx("Failed to refresh provider from session")?;

        Ok(EmptyResponse {})
    }

    pub(super) async fn on_set_session_system_prompt(
        &self,
        req: SetSessionSystemPromptRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("sessionId cannot be empty")
            );
        }

        let text = Some(req.text).filter(|text| !text.trim().is_empty());
        match req.mode {
            SessionSystemPromptMode::Set => self
                .services
                .session_manager
                .update(session_id)
                .system_prompt_override(text)
                .apply()
                .await
                .internal_err()?,
            SessionSystemPromptMode::Append => {
                let key = req
                    .key
                    .as_deref()
                    .map(str::trim)
                    .filter(|key| !key.is_empty())
                    .ok_or_else(|| {
                        agent_client_protocol::Error::invalid_params()
                            .data("key cannot be empty for append mode")
                    })?;
                self.services
                    .session_manager
                    .set_system_prompt_extra(session_id, key, text)
                    .await
                    .internal_err()?
            }
        }

        Ok(EmptyResponse {})
    }

    pub(super) async fn on_delete_session(
        &self,
        req: DeleteSessionRequest,
    ) -> Result<DeleteSessionResponse, agent_client_protocol::Error> {
        let session_id = req.session_id.0.to_string();
        self.runs().cancel_agent_run(&session_id);
        self.live_voice.stop_session_interaction(&session_id).await;
        self.services
            .session_manager
            .delete_session(&session_id)
            .await
            .internal_err()?;
        self.services.release_session(&session_id).await;
        Ok(DeleteSessionResponse::new())
    }

    pub(super) async fn on_delete_sessions(
        &self,
        req: DeleteSessionsRequest,
    ) -> Result<DeleteSessionsResponse, agent_client_protocol::Error> {
        let session_ids = req.session_ids;
        let (tracked, live_stopped) = self.wind_down_before_delete(&session_ids).await;
        // Runs or live interactions still going past the cap keep their rows;
        // deleting under them would let them finish against a missing row.
        // Only the sessions that lose their row are reported as deleted.
        let deletable = Self::deletable_sessions(&session_ids, &tracked, &live_stopped);
        let deleted = self.commit_delete(&session_ids, &deletable).await?;
        Ok(DeleteSessionsResponse {
            deleted_count: deleted,
            deleted_session_ids: deletable,
        })
    }

    pub(super) async fn on_delete_all_sessions(
        &self,
    ) -> Result<DeleteAllSessionsResponse, agent_client_protocol::Error> {
        // The snapshot is taken before the gate closes, so a session created
        // while the wind-down is running keeps its row.
        let pre_sessions = self
            .services
            .session_manager
            .list_all_sessions()
            .await
            .internal_err()?;
        let pre_session_ids: Vec<String> =
            pre_sessions.into_iter().map(|session| session.id).collect();
        let (tracked, live_stopped) = self.wind_down_before_delete(&pre_session_ids).await;
        // Runs or live interactions still going past the cap keep their rows
        // (their prompts stay live); a retry picks them up once they finish.
        let deletable = Self::deletable_sessions(&pre_session_ids, &tracked, &live_stopped);
        let deleted = self.commit_delete(&pre_session_ids, &deletable).await?;
        Ok(DeleteAllSessionsResponse {
            deleted_count: deleted,
            deleted_session_ids: deletable,
        })
    }

    /// Closes prompt admission, cancels the registered runs and stops live
    /// interactions, then waits for the wind-down under one shared cap, so a
    /// stalled delegated stream cannot hang the delete request. Returns the
    /// tracked runs and the sessions whose live interaction stopped in time.
    async fn wind_down_before_delete(
        &self,
        session_ids: &[String],
    ) -> (
        Vec<(String, tokio::sync::watch::Receiver<bool>)>,
        HashSet<String>,
    ) {
        const WIND_DOWN_CAP: std::time::Duration = std::time::Duration::from_secs(10);
        let deadline = tokio::time::Instant::now() + WIND_DOWN_CAP;
        // Close prompt admission and cancel the registered runs as one step:
        // a prompt that passed the gate is tracked, and none can register
        // before the delete commits.
        let tracked = self.runs().begin_session_delete(session_ids);
        let mut live_stopped = HashSet::new();
        for session_id in session_ids {
            let stopped = tokio::time::timeout_at(
                deadline,
                self.live_voice.stop_session_interaction(session_id),
            )
            .await
            .is_ok();
            if stopped {
                live_stopped.insert(session_id.clone());
            }
        }
        // Deleting rows while a prompt task is still winding down lets its
        // remaining effects write into a missing session, so wait for the
        // tasks to signal completion instead of only cancelling them.
        if !tracked.is_empty() {
            let _ = tokio::time::timeout_at(
                deadline,
                futures::future::join_all(
                    tracked
                        .iter()
                        .map(|(_, completion)| Self::wait_for_run_exit(completion.clone())),
                ),
            )
            .await;
        }
        (tracked, live_stopped)
    }

    /// Sessions whose live interaction stopped within the cap and whose run
    /// signalled completion; only these may lose their rows.
    fn deletable_sessions(
        session_ids: &[String],
        tracked: &[(String, tokio::sync::watch::Receiver<bool>)],
        live_stopped: &HashSet<String>,
    ) -> Vec<String> {
        session_ids
            .iter()
            .filter(|session_id| {
                live_stopped.contains(session_id.as_str())
                    && tracked
                        .iter()
                        .find(|(id, _)| id == *session_id)
                        .map(|(_, completion)| *completion.borrow())
                        .unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    /// Deletes the rows, then reopens prompt admission for the sessions that
    /// kept theirs. Deleted IDs stay closed: a prompt that already passed the
    /// admission fast path must not register on a row that is gone. When the
    /// delete fails nothing was removed (single transaction), so every
    /// requested session reopens.
    async fn commit_delete(
        &self,
        session_ids: &[String],
        deletable: &[String],
    ) -> Result<u32, agent_client_protocol::Error> {
        let result = self
            .services
            .session_manager
            .delete_sessions(deletable)
            .await
            .internal_err();
        let deleted: HashSet<&str> = deletable.iter().map(String::as_str).collect();
        let failed = result.is_err();
        for session_id in session_ids {
            if failed || !deleted.contains(session_id.as_str()) {
                self.runs().clear_session_deleting(session_id);
            }
        }
        let count = result? as u32;
        for session_id in deletable {
            self.services.release_session(session_id).await;
        }
        Ok(count)
    }

    /// Resolves once the run signals completion (prompt task fully finished).
    async fn wait_for_run_exit(mut completion: tokio::sync::watch::Receiver<bool>) {
        while !*completion.borrow_and_update() {
            if completion.changed().await.is_err() {
                break;
            }
        }
    }

    pub(super) async fn on_export_session(
        &self,
        req: ExportSessionRequest,
    ) -> Result<ExportSessionResponse, agent_client_protocol::Error> {
        let data = match req.format {
            SessionExportFormat::Json => {
                self.services
                    .session_manager
                    .export_session(&req.session_id)
                    .await
            }
            SessionExportFormat::Markdown => {
                self.services
                    .session_manager
                    .export_session_markdown(&req.session_id)
                    .await
            }
        }
        .internal_err()?;
        Ok(ExportSessionResponse { data })
    }

    pub(super) async fn on_import_session(
        &self,
        req: ImportSessionRequest,
    ) -> Result<ImportSessionResponse, agent_client_protocol::Error> {
        let session = self
            .services
            .session_manager
            .import_session(&req.input, None)
            .await
            .internal_err()?;

        let msg_count = session.message_count as u64;

        Ok(ImportSessionResponse {
            session_id: session.id,
            title: Some(session.name),
            updated_at: Some(session.updated_at.to_rfc3339()),
            message_count: msg_count,
        })
    }

    pub(super) async fn on_get_session_info(
        &self,
        req: GetSessionInfoRequest,
    ) -> Result<GetSessionInfoResponse, agent_client_protocol::Error> {
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("sessionId cannot be empty")
            );
        }

        let session = self
            .services
            .session_manager
            .get_session(session_id, false)
            .await
            .map_err(|_| {
                agent_client_protocol::Error::resource_not_found(Some(session_id.to_string()))
                    .data(format!("Session not found: {}", session_id))
            })?;

        Ok(GetSessionInfoResponse {
            session: build_session_info(session),
        })
    }

    pub(super) async fn on_truncate_session_conversation(
        &self,
        req: TruncateSessionConversationRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let session_id = req.session_id.trim();
        if session_id.is_empty() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("sessionId cannot be empty")
            );
        }

        self.services
            .session_manager
            .truncate_conversation(session_id, req.truncate_from)
            .await
            .internal_err()?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_update_session_project(
        &self,
        req: UpdateSessionProjectRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let session_id = &req.session_id;
        let session_not_found = || {
            agent_client_protocol::Error::resource_not_found(Some(session_id.to_string()))
                .data(format!("Session not found: {session_id}"))
        };
        let updated = self
            .services
            .session_manager
            .update_project_for_session_types(
                session_id,
                req.project_id,
                &ACP_VISIBLE_SESSION_TYPES,
            )
            .await
            .internal_err()?;
        if !updated {
            return Err(session_not_found());
        }
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_rename_session(
        &self,
        req: RenameSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.services
            .session_manager
            .update(&req.session_id)
            .user_provided_name(req.title)
            .apply()
            .await
            .map_err(|e| agent_client_protocol::Error::internal_error().data(e.to_string()))?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_archive_session(
        &self,
        req: ArchiveSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.services
            .session_manager
            .update(&req.session_id)
            .archived_at(Some(chrono::Utc::now()))
            .apply()
            .await
            .internal_err()?;
        self.services.release_session(&req.session_id).await;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_unarchive_session(
        &self,
        req: UnarchiveSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.services
            .session_manager
            .update(&req.session_id)
            .archived_at(None)
            .apply()
            .await
            .internal_err()?;
        Ok(EmptyResponse {})
    }
}
