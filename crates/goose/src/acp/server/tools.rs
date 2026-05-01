use super::*;

impl GooseAcpAgent {
    pub(super) async fn on_get_tools(
        &self,
        req: GetToolsRequest,
    ) -> Result<GetToolsResponse, sacp::Error> {
        let internal_id = self.internal_session_id(&req.session_id).await?;
        let agent = self.get_session_agent(&req.session_id, None).await?;
        let tools = agent.list_tools(&internal_id, None).await;
        let tools_json = tools
            .into_iter()
            .map(|t| serde_json::to_value(&t))
            .collect::<Result<Vec<_>, _>>()
            .internal_err()?;
        Ok(GetToolsResponse { tools: tools_json })
    }
}
