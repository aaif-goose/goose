use super::*;

impl GooseAcpAgent {
    pub(super) async fn on_check_secret(
        &self,
        req: CheckSecretRequest,
    ) -> Result<CheckSecretResponse, sacp::Error> {
        let config = self.config()?;
        let exists = config.get_secret::<serde_json::Value>(&req.key).is_ok();
        Ok(CheckSecretResponse { exists })
    }

    pub(super) async fn on_upsert_secret(
        &self,
        req: UpsertSecretRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let config = self.config()?;
        config.set_secret(&req.key, &req.value).internal_err()?;
        Config::global().invalidate_secrets_cache();
        Ok(EmptyResponse {})
    }

    pub(super) async fn on_remove_secret(
        &self,
        req: RemoveSecretRequest,
    ) -> Result<EmptyResponse, sacp::Error> {
        let config = self.config()?;
        config.delete_secret(&req.key).internal_err()?;
        Config::global().invalidate_secrets_cache();
        Ok(EmptyResponse {})
    }
}
