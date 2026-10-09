use super::*;
use std::path::PathBuf;

impl AcpConnection {
    pub(super) async fn on_list_slash_commands(
        &self,
        req: ListSlashCommandsRequest,
    ) -> Result<ListSlashCommandsResponse, agent_client_protocol::Error> {
        let cwd = if let Some(cwd) = req
            .cwd
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            Some(PathBuf::from(cwd))
        } else if let Some(session_id) = req
            .session_id
            .as_deref()
            .map(str::trim)
            .filter(|session_id| !session_id.is_empty())
        {
            Some(
                self.session_manager
                    .get_session(session_id, false)
                    .await
                    .map_err(|_| {
                        agent_client_protocol::Error::resource_not_found(Some(
                            session_id.to_string(),
                        ))
                        .data(format!("Session not found: {}", session_id))
                    })?
                    .working_dir,
            )
        } else {
            None
        };

        Ok(ListSlashCommandsResponse {
            available_commands:
                crate::acp::response_builder::available_commands_for_optional_working_dir(
                    cwd.as_deref(),
                ),
        })
    }
}
