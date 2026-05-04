use super::*;

impl GooseAcpAgent {
    pub(super) async fn on_read_config(
        &self,
        req: ReadConfigRequest,
    ) -> Result<ReadConfigResponse, sacp::Error> {
        let config = self.config()?;
        let response = match config.get_param::<serde_json::Value>(&req.key) {
            Ok(value) => ReadConfigResponse { value },
            Err(crate::config::ConfigError::NotFound(_)) => ReadConfigResponse {
                value: serde_json::Value::Null,
            },
            Err(e) => return Err(sacp::Error::internal_error().data(e.to_string())),
        };
        Ok(response)
    }

    pub(super) async fn on_upsert_config(
        &self,
        req: UpsertConfigRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let config = self.config()?;
        config.set_param(&req.key, &req.value).internal_err()?;
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_remove_config(
        &self,
        req: RemoveConfigRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let config = self.config()?;
        config.delete(&req.key).internal_err()?;
        Ok(EmptyResponse {})
    }
}
