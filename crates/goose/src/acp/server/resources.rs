use super::*;

impl GooseAcpAgent {
    pub(super) async fn on_read_resource(
        &self,
        req: ReadResourceRequest,
    ) -> Result<ReadResourceResponse, agent_client_protocol::Error> {
        let session_id = &req.session_id;
        let agent = self.get_session_agent(&req.session_id, None).await?;
        let cancel_token = CancellationToken::new();
        let result = agent
            .extension_manager
            .read_resource(session_id, &req.uri, &req.extension_name, cancel_token)
            .await
            .internal_err()?;
        let result_json = serde_json::to_value(&result).internal_err()?;
        Ok(ReadResourceResponse {
            result: result_json,
        })
    }
}
