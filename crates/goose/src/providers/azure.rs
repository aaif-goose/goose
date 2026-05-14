use anyhow::Result;
use async_trait::async_trait;

use super::api_client::{ApiClient, AuthMethod, AuthProvider};
use super::azureauth::{AuthError, AzureAuth};
use super::base::{ConfigKey, ProviderDef, ProviderMetadata};
use super::openai_compatible::OpenAiCompatibleProvider;
use crate::model::ModelConfig;
use futures::future::BoxFuture;

const AZURE_PROVIDER_NAME: &str = "azure_openai";
pub const AZURE_DEFAULT_MODEL: &str = "gpt-4o";
pub const AZURE_DOC_URL: &str =
    "https://learn.microsoft.com/en-us/azure/ai-services/openai/concepts/models";
const AZURE_DEFAULT_API_VERSION: &str = "2024-10-21";
pub const AZURE_OPENAI_KNOWN_MODELS: &[&str] = &["gpt-4o", "gpt-4o-mini", "gpt-4"];

/// New-style Azure AI endpoints use `/v1/` paths and reject the `api-version` query param.
fn is_v1_endpoint(endpoint: &str) -> bool {
    let normalized = endpoint.trim_end_matches('/');
    normalized.ends_with("/v1") || endpoint.contains("/v1/")
}

pub struct AzureProvider;

// Custom auth provider that wraps AzureAuth
struct AzureAuthProvider {
    auth: AzureAuth,
}

#[async_trait]
impl AuthProvider for AzureAuthProvider {
    async fn get_auth_header(&self) -> Result<(String, String)> {
        let auth_token = self
            .auth
            .get_token()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get authentication token: {}", e))?;

        match self.auth.credential_type() {
            super::azureauth::AzureCredentials::ApiKey(_) => {
                Ok(("api-key".to_string(), auth_token.token_value))
            }
            super::azureauth::AzureCredentials::DefaultCredential => Ok((
                "Authorization".to_string(),
                format!("Bearer {}", auth_token.token_value),
            )),
        }
    }
}

impl ProviderDef for AzureProvider {
    type Provider = OpenAiCompatibleProvider;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            AZURE_PROVIDER_NAME,
            "Azure OpenAI",
            "Models through Azure OpenAI Service (uses Azure credential chain by default)",
            "gpt-4o",
            AZURE_OPENAI_KNOWN_MODELS.to_vec(),
            AZURE_DOC_URL,
            vec![
                ConfigKey::new("AZURE_OPENAI_ENDPOINT", true, false, None, true),
                ConfigKey::new("AZURE_OPENAI_DEPLOYMENT_NAME", true, false, None, true),
                ConfigKey::new("AZURE_OPENAI_API_VERSION", false, false, None, false),
                ConfigKey::new("AZURE_OPENAI_API_KEY", false, true, Some(""), true),
            ],
        )
    }

    fn from_env(
        model: ModelConfig,
        _extensions: Vec<crate::config::ExtensionConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(async move {
            let config = crate::config::Config::global();
            let endpoint: String = config.get_param("AZURE_OPENAI_ENDPOINT")?;
            let deployment_name: String = config.get_param("AZURE_OPENAI_DEPLOYMENT_NAME")?;
            let api_version: Option<String> = config
                .get_param("AZURE_OPENAI_API_VERSION")
                .ok()
                .or_else(|| {
                    if is_v1_endpoint(&endpoint) {
                        None
                    } else {
                        Some(AZURE_DEFAULT_API_VERSION.to_string())
                    }
                });

            let api_key = config
                .get_secret("AZURE_OPENAI_API_KEY")
                .ok()
                .filter(|key: &String| !key.is_empty());
            let auth = AzureAuth::new(api_key).map_err(|e| match e {
                AuthError::Credentials(msg) => anyhow::anyhow!("Credentials error: {}", msg),
                AuthError::TokenExchange(msg) => anyhow::anyhow!("Token exchange error: {}", msg),
            })?;

            let auth_provider = AzureAuthProvider { auth };
            let host = format!("{}/openai", endpoint.trim_end_matches('/'));
            let mut api_client = ApiClient::new(host, AuthMethod::Custom(Box::new(auth_provider)))?;
            if let Some(version) = api_version {
                api_client = api_client.with_query(vec![("api-version".to_string(), version)]);
            }

            Ok(OpenAiCompatibleProvider::new(
                AZURE_PROVIDER_NAME.to_string(),
                api_client,
                model,
                format!("deployments/{}/", deployment_name),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_v1_endpoint() {
        assert!(is_v1_endpoint(
            "https://my-resource.services.ai.azure.com/api/projects/my-proj/openai/v1"
        ));
        assert!(is_v1_endpoint(
            "https://my-resource.services.ai.azure.com/api/projects/my-proj/openai/v1/"
        ));
        assert!(is_v1_endpoint(
            "https://my-resource.services.ai.azure.com/v1/some/path"
        ));

        assert!(!is_v1_endpoint("https://my-resource.openai.azure.com"));
        assert!(!is_v1_endpoint("https://my-resource.openai.azure.com/"));
        assert!(!is_v1_endpoint(
            "https://my-resource.openai.azure.com/openai"
        ));
    }
}
