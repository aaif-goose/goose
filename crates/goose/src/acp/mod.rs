mod common;
pub(crate) mod fs;
mod handoff;
mod mcp_app_proxy;
mod provider;
mod response_builder;
pub mod server;
pub mod server_factory;
pub(crate) mod tool_call_notifier;
pub(crate) mod tools;
pub mod transport;

pub use common::{map_permission_response, PermissionDecision};
pub use goose_sdk_types::{custom_notifications, custom_requests};
pub use provider::{
    extension_configs_to_mcp_servers, AcpProvider, AcpProviderConfig, ACP_CURRENT_MODEL,
};

/// `data.reason` on a prompt error raised because the agent's account is out of credits.
/// Set by the ACP server, read by the provider to tell a spent account apart from a
/// prompt the agent could not accept.
pub(crate) const CREDITS_EXHAUSTED_REASON: &str = "credits_exhausted";

pub(crate) fn configured_model_for_provider(
    config: &crate::config::Config,
    provider_name: &str,
) -> String {
    if config.get_goose_provider().ok().as_deref() == Some(provider_name) {
        config
            .get_goose_model()
            .unwrap_or_else(|_| ACP_CURRENT_MODEL.to_string())
    } else {
        ACP_CURRENT_MODEL.to_string()
    }
}

pub(crate) fn is_auth_required(error: &anyhow::Error) -> bool {
    error.chain().any(|source| {
        source
            .downcast_ref::<agent_client_protocol::Error>()
            .is_some_and(|error| {
                error.code == agent_client_protocol::schema::v1::ErrorCode::AuthRequired
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_model_is_not_reused_for_another_provider() {
        let directory = tempfile::tempdir().unwrap();
        let config =
            crate::config::Config::new(directory.path().join("config.yaml"), "test").unwrap();
        config.set_goose_provider("openai").unwrap();
        config.set_goose_model("gpt-5").unwrap();

        assert_eq!(
            configured_model_for_provider(&config, "copilot-acp"),
            ACP_CURRENT_MODEL
        );
    }

    #[test]
    fn configured_model_is_used_for_the_active_provider() {
        let directory = tempfile::tempdir().unwrap();
        let config =
            crate::config::Config::new(directory.path().join("config.yaml"), "test").unwrap();
        config.set_goose_provider("pi-acp").unwrap();
        config.set_goose_model("anthropic/claude-sonnet-4").unwrap();

        assert_eq!(
            configured_model_for_provider(&config, "pi-acp"),
            "anthropic/claude-sonnet-4"
        );
    }

    #[test]
    fn identifies_typed_auth_required_errors() {
        let error = anyhow::Error::new(agent_client_protocol::Error::auth_required());

        assert!(is_auth_required(&error));
    }

    #[test]
    fn does_not_classify_other_acp_errors_as_authentication() {
        let error = anyhow::Error::new(agent_client_protocol::Error::internal_error());

        assert!(!is_auth_required(&error));
    }
}
