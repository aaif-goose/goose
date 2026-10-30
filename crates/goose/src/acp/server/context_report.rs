use super::*;
use crate::token_counter::create_token_counter;

impl AcpConnection {
    pub(super) async fn on_get_context_report(
        &self,
        req: ContextReportRequest,
    ) -> Result<ContextReportResponse, agent_client_protocol::Error> {
        let token_counter = create_token_counter().await.internal_err()?;

        self.services
            .build_context_report(&req.session_id, &token_counter)
            .await
            .internal_err()
    }
}
