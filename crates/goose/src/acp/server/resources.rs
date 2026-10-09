use super::*;

impl GooseAcpAgent {
    pub(super) async fn on_read_resource(
        &self,
        req: ReadResourceRequest,
    ) -> Result<ReadResourceResponse, agent_client_protocol::Error> {
        let session_id = &req.session_id;
        self.open_session(&req.session_id).await?;
        let cancel_token = CancellationToken::new();
        let result = self
            .services
            .extension_manager
            .current_lease(session_id)
            .await
            .internal_err()?
            .read_resource(&req.uri, &req.extension_name, cancel_token)
            .await
            .internal_err()?;
        let result_json = serde_json::to_value(&result).internal_err()?;
        Ok(ReadResourceResponse {
            result: result_json,
        })
    }
}
