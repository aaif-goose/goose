use super::{
    config_secret_value, default_inventory_identity, default_inventory_identity_resolver,
    serialize_string_map, InventoryIdentityInput, InventoryRegistration,
};
use crate::config::{self, Config};
use crate::providers::acp_tooling::{acp_adapter_installed, resolved_acp_command};
use crate::providers::amp_acp::{AMP_ACP_BINARY, AMP_ACP_PROVIDER_NAME};
use crate::providers::base::ProviderDescriptor;
use crate::providers::chatgpt_codex::TokenCache as ChatGptCodexTokenCache;
use crate::providers::claude_acp::{CLAUDE_ACP_BINARY, CLAUDE_ACP_PROVIDER_NAME};
use crate::providers::codex_acp::CODEX_ACP_PROVIDER_NAME;
use crate::providers::copilot_acp::{COPILOT_ACP_BINARY, COPILOT_ACP_PROVIDER_NAME};
use crate::providers::formats::anthropic::ANTHROPIC_PROVIDER_NAME;
use crate::providers::gemini_oauth::TokenCache as GeminiOAuthTokenCache;
use crate::providers::google::{GOOGLE_API_HOST, GOOGLE_PROVIDER_NAME};
use crate::providers::huggingface::HuggingFaceProvider;
use crate::providers::huggingface_auth;
use crate::providers::kimicode;
use crate::providers::ollama::OLLAMA_PROVIDER_NAME;
use crate::providers::openai::{OPEN_AI_DEFAULT_BASE_PATH, OPEN_AI_PROVIDER_NAME};
use crate::providers::pi_acp::{PI_ACP_BINARY, PI_ACP_PROVIDER_NAME};
use crate::providers::xai_oauth::TokenCache as XaiOAuthTokenCache;
use goose_providers::azure_foundry::{endpoint_kind, EndpointKind, AZURE_FOUNDRY_PROVIDER_NAME};

pub fn openai_inventory() -> InventoryRegistration {
    InventoryRegistration::new(true, || {
        let config = Config::global();
        let mut identity =
            InventoryIdentityInput::new(OPEN_AI_PROVIDER_NAME, OPEN_AI_PROVIDER_NAME)
                .with_public(
                    "host",
                    config
                        .get_param::<String>("OPENAI_HOST")
                        .unwrap_or_else(|_| "https://api.openai.com".to_string()),
                )
                .with_public(
                    "base_path",
                    config
                        .get_param::<String>("OPENAI_BASE_PATH")
                        .unwrap_or_else(|_| OPEN_AI_DEFAULT_BASE_PATH.to_string()),
                );

        if let Ok(organization) = config.get_param::<String>("OPENAI_ORGANIZATION") {
            identity = identity.with_public("organization", organization);
        }
        if let Ok(project) = config.get_param::<String>("OPENAI_PROJECT") {
            identity = identity.with_public("project", project);
        }
        if let Some(api_key) = config_secret_value(config, "OPENAI_API_KEY") {
            identity = identity.with_secret("api_key", api_key);
        }
        if let Some(custom_headers) = config_secret_value(config, "OPENAI_CUSTOM_HEADERS") {
            identity = identity.with_secret("custom_headers", custom_headers);
        }

        Ok(identity)
    })
    .with_configured(|| {
        let config = Config::global();
        if let Ok(host) = config.get_param::<String>("OPENAI_HOST") {
            if host != "https://api.openai.com" {
                return true;
            }
        }
        config
            .get_secret::<serde_json::Value>("OPENAI_API_KEY")
            .is_ok()
    })
}

pub fn azure_foundry_inventory() -> InventoryRegistration {
    InventoryRegistration::new(true, || {
        let config = Config::global();
        let mut identity =
            InventoryIdentityInput::new(AZURE_FOUNDRY_PROVIDER_NAME, AZURE_FOUNDRY_PROVIDER_NAME);
        if let Ok(endpoint) = config.get_param::<String>("AZURE_FOUNDRY_ENDPOINT") {
            identity = identity.with_public("endpoint", endpoint);
        }
        if let Ok(api_version) = config.get_param::<String>("AZURE_FOUNDRY_API_VERSION") {
            identity = identity.with_public("api_version", api_version);
        }
        if let Ok(model) = config.get_param::<String>("AZURE_FOUNDRY_MODEL") {
            identity = identity.with_public("model", model);
        }
        if let Some(api_key) = config_secret_value(config, "AZURE_FOUNDRY_API_KEY") {
            identity = identity.with_secret("api_key", api_key);
        }
        if let Some(ad_token) = config_secret_value(config, "AZURE_FOUNDRY_AD_TOKEN") {
            identity = identity.with_secret("ad_token", ad_token);
        }
        Ok(identity)
    })
    .with_configured(|| azure_foundry_configured(Config::global()))
}

fn azure_foundry_configured(config: &Config) -> bool {
    azure_foundry_configured_values(
        config
            .get_param::<String>("AZURE_FOUNDRY_ENDPOINT")
            .ok()
            .as_deref(),
        config
            .get_param::<String>("AZURE_FOUNDRY_MODEL")
            .ok()
            .as_deref(),
    )
}

fn azure_foundry_configured_values(endpoint: Option<&str>, model: Option<&str>) -> bool {
    let Some(endpoint) = endpoint.filter(|endpoint| !endpoint.trim().is_empty()) else {
        return false;
    };
    endpoint_kind(endpoint) != EndpointKind::Maas
        || model.is_some_and(|model| !model.trim().is_empty())
}

pub fn anthropic_inventory() -> InventoryRegistration {
    InventoryRegistration::new(true, || {
        let config = Config::global();
        let mut identity =
            InventoryIdentityInput::new(ANTHROPIC_PROVIDER_NAME, ANTHROPIC_PROVIDER_NAME)
                .with_public(
                    "host",
                    config
                        .get_param::<String>("ANTHROPIC_HOST")
                        .unwrap_or_else(|_| "https://api.anthropic.com".to_string()),
                );

        if let Some(api_key) = config_secret_value(config, "ANTHROPIC_API_KEY") {
            identity = identity.with_secret("api_key", api_key);
        }
        if let Ok(headers) = config
            .get_secret::<std::collections::HashMap<String, String>>("ANTHROPIC_CUSTOM_HEADERS")
        {
            identity = identity.with_secret("headers", serialize_string_map(&headers)?);
        }
        Ok(identity)
    })
}

pub fn google_inventory() -> InventoryRegistration {
    InventoryRegistration::new(true, || {
        let config = Config::global();
        let mut identity = InventoryIdentityInput::new(GOOGLE_PROVIDER_NAME, GOOGLE_PROVIDER_NAME)
            .with_public(
                "host",
                config
                    .get_param::<String>("GOOGLE_HOST")
                    .unwrap_or_else(|_| GOOGLE_API_HOST.to_string()),
            );
        if let Some(api_key) = config_secret_value(config, "GOOGLE_API_KEY") {
            identity = identity.with_secret("api_key", api_key);
        }
        Ok(identity)
    })
}

pub fn ollama_inventory() -> InventoryRegistration {
    InventoryRegistration::new(true, || {
        let config = Config::global();
        Ok(
            InventoryIdentityInput::new(OLLAMA_PROVIDER_NAME, OLLAMA_PROVIDER_NAME).with_public(
                "host",
                config
                    .get_param::<String>("OLLAMA_HOST")
                    .unwrap_or_else(|_| "http://localhost:11434".to_string()),
            ),
        )
    })
    .with_configured(|| {
        config::Config::global()
            .get_param::<String>("OLLAMA_HOST")
            .is_ok()
    })
}

pub fn huggingface_inventory() -> InventoryRegistration {
    InventoryRegistration::new(false, || {
        let metadata = HuggingFaceProvider::metadata();
        Ok(default_inventory_identity(
            &metadata.name,
            &metadata.name,
            &metadata.config_keys,
            Config::global(),
        ))
    })
    .with_configured(|| huggingface_auth::has_configured_token().unwrap_or(false))
}

pub fn refresh_only() -> InventoryRegistration {
    InventoryRegistration {
        supports_refresh: true,
        identity: default_inventory_identity_resolver(),
        configured: None,
    }
}

pub fn kimi_code_inventory() -> InventoryRegistration {
    refresh_only().with_configured(kimicode::has_configured_token)
}

pub fn chatgpt_codex_inventory() -> InventoryRegistration {
    InventoryRegistration {
        supports_refresh: false,
        identity: default_inventory_identity_resolver(),
        configured: None,
    }
    .with_configured(|| ChatGptCodexTokenCache::new().has_token())
}

pub fn gemini_oauth_inventory() -> InventoryRegistration {
    InventoryRegistration {
        supports_refresh: false,
        identity: default_inventory_identity_resolver(),
        configured: None,
    }
    .with_configured(|| GeminiOAuthTokenCache::new().has_token())
}

pub fn xai_oauth_inventory() -> InventoryRegistration {
    InventoryRegistration {
        supports_refresh: true,
        identity: default_inventory_identity_resolver(),
        configured: None,
    }
    .with_configured(|| XaiOAuthTokenCache::new().has_token())
}

pub fn acp_inventory(
    provider_id: &'static str,
    command: &'static str,
    supports_refresh: bool,
) -> InventoryRegistration {
    InventoryRegistration::new(supports_refresh, move || {
        let resolved_command = resolved_acp_command(command)?;
        Ok(InventoryIdentityInput::new(provider_id, provider_id)
            .with_public("command", resolved_command.display().to_string()))
    })
    .with_configured(move || acp_adapter_installed(command))
}

pub fn amp_acp_inventory() -> InventoryRegistration {
    acp_inventory(AMP_ACP_PROVIDER_NAME, AMP_ACP_BINARY, false)
}

pub fn claude_acp_inventory() -> InventoryRegistration {
    acp_inventory(CLAUDE_ACP_PROVIDER_NAME, CLAUDE_ACP_BINARY, true)
}

pub fn codex_acp_inventory() -> InventoryRegistration {
    acp_inventory(CODEX_ACP_PROVIDER_NAME, CODEX_ACP_PROVIDER_NAME, true)
}

pub fn copilot_acp_inventory() -> InventoryRegistration {
    acp_inventory(COPILOT_ACP_PROVIDER_NAME, COPILOT_ACP_BINARY, true)
}

pub fn pi_acp_inventory() -> InventoryRegistration {
    acp_inventory(PI_ACP_PROVIDER_NAME, PI_ACP_BINARY, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::paths::Paths;
    use chrono::Utc;

    #[test]
    fn azure_foundry_maas_requires_a_model_to_be_configured() {
        assert!(!azure_foundry_configured_values(
            Some("https://deployment.models.ai.azure.com"),
            None,
        ));
        assert!(!azure_foundry_configured_values(
            Some("https://deployment.models.ai.azure.com"),
            Some("  "),
        ));
        assert!(azure_foundry_configured_values(
            Some("https://deployment.models.ai.azure.com"),
            Some("Phi-4"),
        ));
        assert!(azure_foundry_configured_values(
            Some("https://hub.services.ai.azure.com/api/projects/project"),
            None,
        ));
        assert!(azure_foundry_configured_values(
            Some("https://hub.services.ai.azure.com"),
            None,
        ));
    }

    #[test]
    #[serial_test::serial]
    fn gemini_oauth_inventory_configured_uses_token_cache() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_string_lossy().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path.as_str()))]);

        let registration = gemini_oauth_inventory();
        let configured = registration
            .configured
            .expect("Gemini OAuth should define configured resolver");

        assert!(!configured());

        let cache_path = Paths::in_config_dir("gemini_oauth/tokens.json");
        std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
        std::fs::write(
            cache_path,
            serde_json::to_string(&serde_json::json!({
                "project_id": "test-project",
                "token": {
                    "access_token": "access",
                    "refresh_token": "refresh",
                    "expires_at": (Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
                },
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(configured());
    }

    #[test]
    #[serial_test::serial]
    fn kimi_code_inventory_configured_uses_token_cache() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_string_lossy().to_string();
        let _guard = env_lock::lock_env([("GOOSE_PATH_ROOT", Some(root_path.as_str()))]);

        let registration = kimi_code_inventory();
        let configured = registration
            .configured
            .expect("Kimi Code should define configured resolver");

        assert!(!configured());

        let cache_path = Paths::in_config_dir("kimicode/token.json");
        std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
        std::fs::write(
            cache_path,
            serde_json::to_string(&serde_json::json!({
                "access_token": "access",
                "refresh_token": "refresh",
                "expires_at": (Utc::now() + chrono::Duration::hours(1)).to_rfc3339(),
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(configured());
    }
}
