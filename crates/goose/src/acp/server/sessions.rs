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
        let session_id = &req.session_id;
        self.session_manager
            .update(session_id)
            .working_dir(path.clone())
            .apply()
            .await
            .internal_err()?;

        if let Some(session) = self.sessions.lock().await.get_mut(session_id) {
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
        self.session_manager
            .delete_session(&req.session_id)
            .await
            .internal_err()?;
        self.sessions.lock().await.remove(&req.session_id);
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_export_session(
        &self,
        req: ExportSessionRequest,
    ) -> Result<ExportSessionResponse, agent_client_protocol::Error> {
        let data = self
            .session_manager
            .export_session(&req.session_id)
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
            .import_session(&req.data, None)
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

    pub(super) async fn on_update_session_project(
        &self,
        req: UpdateSessionProjectRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.session_manager
            .update(&req.session_id)
            .project_id(req.project_id)
            .apply()
            .await
            .internal_err()?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_rename_session(
        &self,
        req: RenameSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.session_manager
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
        self.session_manager
            .update(&req.session_id)
            .archived_at(Some(chrono::Utc::now()))
            .apply()
            .await
            .internal_err()?;
        self.sessions.lock().await.remove(&req.session_id);
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_unarchive_session(
        &self,
        req: UnarchiveSessionRequest,
    ) -> Result<EmptyResponse, agent_client_protocol::Error> {
        self.session_manager
            .update(&req.session_id)
            .archived_at(None)
            .apply()
            .await
            .internal_err()?;
        Ok(EmptyResponse {})
    }
}
