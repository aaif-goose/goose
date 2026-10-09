use super::*;
use crate::session::{generate_diagnostics, DiagnosticsLevel};

impl AcpConnection {
    pub(super) async fn on_get_diagnostics(
        &self,
        req: DiagnosticsGetRequest,
    ) -> Result<DiagnosticsGetResponse, agent_client_protocol::Error> {
        let level = match req.level {
            DiagnosticsReportLevel::Summary => DiagnosticsLevel::Summary,
            DiagnosticsReportLevel::Full => DiagnosticsLevel::Full,
        };
        let report = generate_diagnostics(&self.session_manager, &req.session_id, level)
            .await
            .internal_err()?;
        let report = serde_json::to_value(report).internal_err()?;

        Ok(DiagnosticsGetResponse { report })
    }
}
