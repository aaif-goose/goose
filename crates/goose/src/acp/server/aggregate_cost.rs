use super::{GooseAcpAgent, ResultExt, ACP_VISIBLE_SESSION_TYPES};
use crate::acp::custom_requests::{
    ProjectCostAggregateRequest, ProjectCostAggregateResponse, ProjectCostEntry,
};

impl GooseAcpAgent {
    pub(super) async fn on_aggregate_project_costs(
        &self,
        _req: ProjectCostAggregateRequest,
    ) -> Result<ProjectCostAggregateResponse, agent_client_protocol::Error> {
        let aggregates = self
            .session_manager
            .aggregate_project_costs(&ACP_VISIBLE_SESSION_TYPES)
            .await
            .internal_err()?;

        let projects = aggregates
            .into_iter()
            .map(|a| ProjectCostEntry {
                working_dir: a.working_dir,
                total_cost: a.total_cost,
                session_count: a.session_count,
                sessions_with_cost: a.sessions_with_cost,
            })
            .collect();

        Ok(ProjectCostAggregateResponse { projects })
    }
}
