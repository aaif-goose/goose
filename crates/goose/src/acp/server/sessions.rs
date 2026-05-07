use super::*;

impl GooseAcpAgent {
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
        if !path.exists() || !path.is_dir() {
            return Err(
                agent_client_protocol::Error::invalid_params().data("invalid directory path")
            );
        }
        let internal_id = self.internal_session_id(&req.session_id).await?;
        self.session_manager
            .update(&internal_id)
            .working_dir(path.clone())
            .apply()
            .await
            .internal_err()?;

        self.thread_manager
            .update_working_dir(&req.session_id, &working_dir)
            .await
            .internal_err()?;

        if let Some(session) = self.sessions.lock().await.get_mut(&req.session_id) {
            match &session.agent {
                AgentHandle::Ready(agent) => {
                    agent.extension_manager.update_working_dir(&path).await;
                }
                AgentHandle::Loading(_) => {
                    session.pending_working_dir = Some(path);
                }
            }
        }

        Ok(EmptyResponse {})
    }

    pub(super) async fn on_delete_session(
        &self,
        req: DeleteSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        // Delete the thread and all its internal sessions + messages.
        self.thread_manager
            .delete_thread(&req.session_id)
            .await
            .internal_err()?;
        self.sessions.lock().await.remove(&req.session_id);
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_export_session(
        &self,
        req: ExportSessionRequest,
    ) -> Result<ExportSessionResponse, agent_client_protocol::Error> {
        let thread = self
            .thread_manager
            .get_thread(&req.session_id)
            .await
            .internal_err()?;
        let internal_id = thread.current_session_id.ok_or_else(|| {
            agent_client_protocol::Error::internal_error().data("Thread has no internal session")
        })?;
        let data = self
            .session_manager
            .export_session(&internal_id)
            .await
            .internal_err()?;
        Ok(ExportSessionResponse { data })
    }

    pub(super) async fn on_import_session(
        &self,
        req: ImportSessionRequest,
    ) -> Result<ImportSessionResponse, agent_client_protocol::Error> {
        let session = self
            .session_manager
            .import_session(&req.data, Some(SessionType::Acp))
            .await
            .internal_err()?;

        // Create a thread for the imported session.
        let thread = self
            .thread_manager
            .create_thread(
                Some(session.name.clone()),
                None,
                Some(session.working_dir.display().to_string()),
            )
            .await
            .internal_err()?;

        // Link the internal session to the thread.
        self.session_manager
            .update(&session.id)
            .thread_id(Some(thread.id.clone()))
            .apply()
            .await
            .internal_err()?;

        // Copy conversation messages into thread_messages so they appear in the thread.
        if let Some(ref conversation) = session.conversation {
            for msg in conversation.messages() {
                self.thread_manager
                    .append_message(&thread.id, Some(&session.id), msg)
                    .await
                    .internal_err()?;
            }
        }

        // Re-fetch thread to get accurate message_count.
        let thread = self
            .thread_manager
            .get_thread(&thread.id)
            .await
            .internal_err()?;

        Ok(ImportSessionResponse {
            session_id: thread.id,
            title: Some(thread.name),
            updated_at: Some(thread.updated_at.to_rfc3339()),
            message_count: thread.message_count as u64,
        })
    }

    pub(super) async fn on_update_session_project(
        &self,
        req: UpdateSessionProjectRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let project_id = req.project_id;
        self.update_thread_metadata(&req.session_id, move |meta| {
            meta.project_id = project_id;
        })
        .await?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_rename_session(
        &self,
        req: RenameSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        let title = req.title;
        let thread = self
            .thread_manager
            .update_thread(&req.session_id, Some(title.clone()), Some(true), None)
            .await
            .map_err(|e| agent_client_protocol::Error::internal_error().data(e.to_string()))?;
        if let Some(internal_session_id) = thread.current_session_id {
            self.session_manager
                .update(&internal_session_id)
                .user_provided_name(title)
                .apply()
                .await
                .map_err(|e| agent_client_protocol::Error::internal_error().data(e.to_string()))?;
        }
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_archive_session(
        &self,
        req: ArchiveSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.thread_manager
            .archive_thread(&req.session_id)
            .await
            .internal_err()?;
        self.sessions.lock().await.remove(&req.session_id);
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_unarchive_session(
        &self,
        req: UnarchiveSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.thread_manager
            .unarchive_thread(&req.session_id)
            .await
            .internal_err()?;
        Ok(EmptyResponse {})
    }
}
