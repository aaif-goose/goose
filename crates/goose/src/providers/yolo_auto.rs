use super::api_client::{ApiClient, AuthMethod};
use super::base::{ConfigKey, ProviderDef, ProviderMetadata};
use super::openai_compatible::OpenAiCompatibleProvider;
use anyhow::Result;
use futures::future::BoxFuture;

const YOLO_AUTO_PROVIDER_NAME: &str = "yolo-auto";
pub const YOLO_AUTO_API_HOST: &str = "https://yolo-auto.com/v1";
pub const YOLO_AUTO_DEFAULT_MODEL: &str = "yolo";
// The catalog changes often, so the static seed lists only the two evergreen aliases that
// always resolve regardless of the backing models. The full, current model list is discovered
// live from GET /v1/models once a key is configured (OpenAiCompatibleProvider::fetch_supported_models).
pub const YOLO_AUTO_KNOWN_MODELS: &[&str] = &["yolo", "yolo-small"];
pub const YOLO_AUTO_DOC_URL: &str = "https://yolo-auto.com/docs";

pub struct YoloAutoProvider;

impl goose_providers::base::ProviderDescriptor for YoloAutoProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            YOLO_AUTO_PROVIDER_NAME,
            "Yolo-Auto",
            "Flat-rate inference gateway. OpenAI-compatible with streaming, function calling, and live model discovery via /v1/models.",
            YOLO_AUTO_DEFAULT_MODEL,
            YOLO_AUTO_KNOWN_MODELS.to_vec(),
            YOLO_AUTO_DOC_URL,
            vec![
                ConfigKey::new("YOLO_AUTO_API_KEY", true, true, None, true),
                ConfigKey::new(
                    "YOLO_AUTO_HOST",
                    false,
                    false,
                    Some(YOLO_AUTO_API_HOST),
                    false,
                ),
            ],
        )
    }
}

impl ProviderDef for YoloAutoProvider {
    type Provider = OpenAiCompatibleProvider;

    fn from_env(
        _extensions: Vec<crate::config::ExtensionConfig>,
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<OpenAiCompatibleProvider>> {
        Box::pin(async move {
            let config = crate::config::Config::global();
            let api_key: String = config.get_secret("YOLO_AUTO_API_KEY")?;
            let host: String = config
                .get_param("YOLO_AUTO_HOST")
                .unwrap_or_else(|_| YOLO_AUTO_API_HOST.to_string());

            let api_client =
                ApiClient::new_with_tls(host, AuthMethod::BearerToken(api_key), tls_config)?
                    .with_request_builder(crate::session_context::session_id_request_builder());

            Ok(OpenAiCompatibleProvider::new(
                YOLO_AUTO_PROVIDER_NAME.to_string(),
                api_client,
                String::new(),
            ))
        })
    }
}
