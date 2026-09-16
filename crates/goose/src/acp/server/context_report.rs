use super::*;
use crate::token_counter::create_token_counter;

impl GooseAcpAgent {
    pub(super) async fn on_get_context_report(
        &self,
        req: ContextReportRequest,
    ) -> Result<ContextReportResponse, agent_client_protocol::Error> {
        let agent = self.get_session_agent(&req.session_id).await?;
        let token_counter = create_token_counter().await.internal_err()?;

        let unrolled = req
            .unrolled_agent_loop
            .unwrap_or_else(crate::agents::state_machine::enabled);

        agent
            .build_context_report(&req.session_id, &token_counter, unrolled)
            .await
            .internal_err()
    }
}
