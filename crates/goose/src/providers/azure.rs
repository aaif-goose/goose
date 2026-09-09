use anyhow::Result;
use async_trait::async_trait;

use super::api_client::{ApiClient, AuthMethod, AuthProvider};
use super::azureauth::{AuthError, AzureAuth};
use super::base::{ConfigKey, ProviderDef, ProviderMetadata};
use super::openai_compatible::OpenAiCompatibleProvider;
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
            super::azureauth::AzureCredentials::BearerToken(_)
            | super::azureauth::AzureCredentials::DefaultCredential => Ok((
                "Authorization".to_string(),
                format!("Bearer {}", auth_token.token_value),
            )),
        }
    }
}

impl goose_providers::base::ProviderDescriptor for AzureProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            AZURE_PROVIDER_NAME,
            "Azure OpenAI",
            "Models through Azure OpenAI Service (supports API key, Entra ID bearer token, and Azure credential chain)",
            "gpt-4o",
            AZURE_OPENAI_KNOWN_MODELS.to_vec(),
            AZURE_DOC_URL,
            vec![
                ConfigKey::new("AZURE_OPENAI_ENDPOINT", true, false, None, true),
                ConfigKey::new("AZURE_OPENAI_DEPLOYMENT_NAME", true, false, None, true),
                ConfigKey::new("AZURE_OPENAI_API_VERSION", false, false, None, false),
                ConfigKey::new("AZURE_OPENAI_API_KEY", false, true, Some(""), true),
                ConfigKey::new("AZURE_OPENAI_AD_TOKEN", false, true, Some(""), true),
            ],
        )
        .with_setup(
            crate::providers::catalog::ProviderSetupMetadata::new(
                crate::providers::catalog::ProviderSetupCategory::Model,
                crate::providers::catalog::ProviderSetupMethod::ConfigFields,
                crate::providers::catalog::ProviderSetupGroup::Additional,
            )
            .with_field("AZURE_OPENAI_ENDPOINT", "Endpoint", Some("https://your-resource.openai.azure.com"), None)
            .with_field("AZURE_OPENAI_DEPLOYMENT_NAME", "Deployment", Some("gpt-4o"), None)
            .with_field("AZURE_OPENAI_API_KEY", "API Key", Some("Paste your API key"), None)
            .with_field("AZURE_OPENAI_AD_TOKEN", "Entra ID Token", Some("Optional: short-lived Microsoft Entra access token"), None),
        )
    }
}

impl ProviderDef for AzureProvider {
    type Provider = OpenAiCompatibleProvider;

    fn from_env(
        _extensions: Vec<crate::config::ExtensionConfig>,
        tls_config: Option<crate::providers::api_client::TlsConfig>,
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
            let ad_token = config
                .get_secret("AZURE_OPENAI_AD_TOKEN")
                .ok()
                .filter(|token: &String| !token.is_empty());
            let auth = AzureAuth::new(api_key, ad_token).map_err(|e| match e {
                AuthError::Credentials(msg) => anyhow::anyhow!("Credentials error: {}", msg),
                AuthError::TokenExchange(msg) => anyhow::anyhow!("Token exchange error: {}", msg),
            })?;

            let auth_provider = AzureAuthProvider { auth };
            let host = format!("{}/openai", endpoint.trim_end_matches('/'));
            let mut api_client = ApiClient::new_with_tls(
                host,
                AuthMethod::Custom(Box::new(auth_provider)),
                tls_config,
            )?
            .with_request_builder(crate::session_context::session_id_request_builder());
            if let Some(version) = api_version {
                api_client = api_client.with_query(vec![("api-version".to_string(), version)]);
            }

            Ok(OpenAiCompatibleProvider::new(
                AZURE_PROVIDER_NAME.to_string(),
                api_client,
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

    #[tokio::test]
    async fn test_auth_header_bearer_token() {
        let auth = AzureAuth::new(None, Some("my-token".to_string())).unwrap();
        let provider = AzureAuthProvider { auth };
        let (header, value) = provider.get_auth_header().await.unwrap();
        assert_eq!(header, "Authorization");
        assert_eq!(value, "Bearer my-token");
    }

    #[tokio::test]
    async fn test_auth_header_api_key() {
        let auth = AzureAuth::new(Some("my-key".to_string()), None).unwrap();
        let provider = AzureAuthProvider { auth };
        let (header, value) = provider.get_auth_header().await.unwrap();
        assert_eq!(header, "api-key");
        assert_eq!(value, "my-key");
    }
}
