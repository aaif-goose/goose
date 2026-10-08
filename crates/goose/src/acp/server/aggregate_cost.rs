use super::{GooseAcpAgent, ResultExt, ACP_VISIBLE_SESSION_TYPES};
use crate::acp::custom_requests::{
    SessionCostAggregateGroup, SessionCostAggregateRequest, SessionCostAggregateResponse,
};

impl GooseAcpAgent {
    pub(super) async fn on_aggregate_session_costs(
        &self,
        _req: SessionCostAggregateRequest,
    ) -> Result<SessionCostAggregateResponse, agent_client_protocol::Error> {
        let aggregates = self
            .session_manager
            .aggregate_session_costs(&ACP_VISIBLE_SESSION_TYPES)
            .await
            .internal_err()?;

        let groups = aggregates
            .into_iter()
            .map(|a| SessionCostAggregateGroup {
                group_key: a.working_dir,
                total_cost: a.total_cost,
                session_count: a.session_count,
                sessions_with_cost: a.sessions_with_cost,
            })
            .collect();

        Ok(SessionCostAggregateResponse { groups })
    }
}
