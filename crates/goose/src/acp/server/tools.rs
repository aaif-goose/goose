use super::*;
use crate::agents::reply_parts::is_tool_visible_to_app;
use rmcp::model::CallToolRequestParams;

impl GooseAcpAgent {
    pub(super) async fn on_get_tools(
        &self,
        req: GetToolsRequest,
    ) -> Result<GetToolsResponse, agent_client_protocol::Error> {
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

    pub(super) async fn on_call_tool(
        &self,
        req: GooseToolCallRequest,
    ) -> Result<GooseToolCallResponse, agent_client_protocol::Error> {
        let internal_id = self.internal_session_id(&req.session_id).await?;
        let agent = self.get_session_agent(&req.session_id, None).await?;
        let tools = agent.list_tools(&internal_id, None).await;

        let Some(tool) = tools.iter().find(|t| *t.name == req.name) else {
            return Err(agent_client_protocol::Error::invalid_params().data("tool not found"));
        };

        if !is_tool_visible_to_app(tool) {
            return Err(agent_client_protocol::Error::invalid_params()
                .data("tool is not visible to app clients"));
        }

        let arguments = match req.arguments {
            serde_json::Value::Object(map) => Some(map),
            serde_json::Value::Null => None,
            _ => {
                return Err(agent_client_protocol::Error::invalid_params()
                    .data("tool arguments must be an object"));
            }
        };

        let tool_call = {
            let mut params = CallToolRequestParams::new(req.name);
            if let Some(args) = arguments {
                params = params.with_arguments(args);
            }
            params
        };

        let ctx = crate::agents::ToolCallContext::new(internal_id, None, None);
        let tool_result = agent
            .extension_manager
            .dispatch_tool_call(&ctx, tool_call, CancellationToken::new())
            .await
            .map_err(|e| agent_client_protocol::Error::internal_error().data(e.to_string()))?;

        let result = tool_result
            .result
            .await
            .map_err(|e| agent_client_protocol::Error::internal_error().data(e.to_string()))?;

        let content = result
            .content
            .into_iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| agent_client_protocol::Error::internal_error().data(e.to_string()))?;

        Ok(GooseToolCallResponse {
            content,
            structured_content: result.structured_content,
            is_error: result.is_error.unwrap_or(false),
            meta: result.meta.and_then(|m| serde_json::to_value(m).ok()),
        })
    }
}
