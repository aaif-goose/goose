use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use futures::future::BoxFuture;
use reqwest::{Request, Response};
use serde_json::Value;
use tinfoil::SecureClient;
use tokio::sync::Mutex;

use super::api_client::{ApiClient, AuthMethod, RequestExecutor, TlsConfig};
use super::base::{ConfigKey, ProviderDef, ProviderMetadata, DEFAULT_PROVIDER_TIMEOUT_SECS};
use super::openai_compatible::OpenAiCompatibleProvider;

const TINFOIL_HOST: &str = "inference.tinfoil.sh";
const TINFOIL_REPO: &str = "tinfoilsh/confidential-model-router";
const TINFOIL_API_URL: &str = "https://inference.tinfoil.sh/v1";

pub struct TinfoilProvider;

impl goose_providers::base::ProviderDescriptor for TinfoilProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            "tinfoil",
            "Tinfoil",
            "Private inference with hardware attestation verification and pinned TLS",
            "gpt-oss-120b",
            vec!["gpt-oss-120b"],
            "https://docs.tinfoil.sh/models/catalog",
            vec![ConfigKey::new("TINFOIL_API_KEY", true, true, None, true)],
        )
    }
}

impl ProviderDef for TinfoilProvider {
    type Provider = OpenAiCompatibleProvider;

    fn from_env(
        _extensions: Vec<crate::config::ExtensionConfig>,
        tls_config: Option<TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(async move {
            if tls_config.as_ref().is_some_and(TlsConfig::is_configured) {
                bail!("Tinfoil uses attested TLS and does not support custom CA or client certificates");
            }
            let api_key: String = crate::config::Config::global().get_secret("TINFOIL_API_KEY")?;
            let timeout = Duration::from_secs(DEFAULT_PROVIDER_TIMEOUT_SECS);
            let mut enclave = SecureClient::new(TINFOIL_HOST, TINFOIL_REPO, "");
            tokio::time::timeout(timeout, enclave.verify())
                .await
                .context("Tinfoil attestation verification timed out")?
                .context(
                    "Tinfoil attestation verification failed; no inference request was sent",
                )?;

            let executor = TinfoilTransport {
                enclave: Mutex::new(enclave),
                cache_secret: uuid::Uuid::new_v4().to_string(),
            };
            let client = ApiClient::new_with_tls(
                TINFOIL_API_URL.to_string(),
                AuthMethod::BearerToken(api_key),
                None,
            )?
            .with_request_builder(crate::session_context::session_id_request_builder())
            .with_request_executor(Arc::new(executor));

            Ok(OpenAiCompatibleProvider::new(
                "tinfoil".to_string(),
                client,
                String::new(),
            ))
        })
    }
}

#[async_trait]
trait AttestedEnclave: Send + Sync {
    fn is_verified(&self) -> bool;
    async fn verify(&mut self) -> Result<()>;
    async fn execute(&self, request: Request) -> Result<Response>;
}

#[async_trait]
impl AttestedEnclave for SecureClient {
    fn is_verified(&self) -> bool {
        self.is_verified()
    }

    async fn verify(&mut self) -> Result<()> {
        self.verify().await?;
        Ok(())
    }

    async fn execute(&self, request: Request) -> Result<Response> {
        Ok(self.http_client()?.execute(request).await?)
    }
}

struct TinfoilTransport<C> {
    enclave: Mutex<C>,
    cache_secret: String,
}

#[async_trait]
impl<C: AttestedEnclave> RequestExecutor for TinfoilTransport<C> {
    async fn execute(&self, mut request: Request) -> Result<Response> {
        if request.url().origin() != reqwest::Url::parse(TINFOIL_API_URL)?.origin() {
            bail!("Tinfoil request is outside the verified enclave origin");
        }
        if request.headers().contains_key(reqwest::header::HOST) {
            bail!("Tinfoil does not support overriding the Host header");
        }
        // The low-level SDK transport does not inject the prompt-cache secret.
        // Scope cache reuse to this provider instance before sending any prompts.
        if request.method() == reqwest::Method::POST
            && request.url().path() == "/v1/chat/completions"
        {
            let bytes = request
                .body()
                .and_then(reqwest::Body::as_bytes)
                .ok_or_else(|| anyhow::anyhow!("Tinfoil requires a JSON chat request"))?;
            let mut payload: Value = serde_json::from_slice(bytes)?;
            payload
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("Tinfoil requires a JSON chat object"))?
                .insert(
                    "user_cache_secret".to_string(),
                    self.cache_secret.clone().into(),
                );
            *request.body_mut() = Some(serde_json::to_vec(&payload)?.into());
            request
                .headers_mut()
                .remove(reqwest::header::CONTENT_LENGTH);
        }
        *request.timeout_mut() = Some(Duration::from_secs(DEFAULT_PROVIDER_TIMEOUT_SECS));
        let retry = request.try_clone();
        let mut enclave = self.enclave.lock().await;
        if !enclave.is_verified() {
            enclave
                .verify()
                .await
                .context("Tinfoil attestation verification failed")?;
        }
        match enclave.execute(request).await {
            Err(error)
                if error.chain().any(|cause| {
                    cause
                        .downcast_ref::<reqwest::Error>()
                        .is_some_and(reqwest::Error::is_connect)
                }) =>
            {
                // A rotated attested key fails the TLS handshake. Refresh trust
                // before retrying; never replace the pin with ordinary TLS.
                enclave
                    .verify()
                    .await
                    .context("Tinfoil attestation re-verification failed")?;
                match retry {
                    Some(request) => enclave.execute(request).await,
                    None => Err(error),
                }
            }
            result => result,
        }
    }
}

#[cfg(test)]
#[path = "../../tests/providers/tinfoil.rs"]
mod tests;
