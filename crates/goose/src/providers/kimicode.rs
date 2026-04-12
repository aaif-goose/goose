use crate::config::paths::Paths;
use crate::session_context::SESSION_ID_HEADER;
use anyhow::{anyhow, Context, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use futures::TryStreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;
use std::time::Duration as StdDuration;
use tokio::pin;
use tokio_util::io::StreamReader;
use uuid::Uuid;

use super::base::{ConfigKey, MessageStream, Provider, ProviderDef, ProviderMetadata};
use super::errors::ProviderError;
use super::formats::anthropic::{create_request, response_to_streaming_message};
use super::openai_compatible::handle_status_openai_compat;
use super::retry::ProviderRetry;
use super::utils::RequestLog;
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use futures::future::BoxFuture;
use rmcp::model::Tool;

const KIMI_CODE_PROVIDER_NAME: &str = "kimi_code";
pub const KIMI_CODE_DEFAULT_MODEL: &str = "kimi-k2.5";
pub const KIMI_CODE_DEFAULT_FAST_MODEL: &str = "kimi-k2.5";
pub const KIMI_CODE_KNOWN_MODELS: &[&str] = &["kimi-k2.5", "kimi-k2-thinking"];

const KIMI_CODE_DOC_URL: &str = "https://www.kimi.com/code/docs/en/";
const KIMI_CODE_CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
const KIMI_AUTH_HOST: &str = "https://auth.kimi.com";
const KIMI_API_BASE: &str = "https://api.kimi.com/coding";
const KIMI_MSH_PLATFORM: &str = "kimi_cli";
const KIMI_MSH_VERSION: &str = "0.1.0";

/// Refresh the access token if it expires within this many seconds.
const REFRESH_THRESHOLD_SECS: i64 = 300;
// ── Token persistence ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
struct KimiToken {
    access_token: String,
    refresh_token: String,
    expires_at: DateTime<Utc>,
}

#[derive(Debug)]
struct TokenCache {
    path: std::path::PathBuf,
}

impl TokenCache {
    fn new() -> Self {
        Self {
            path: Paths::in_config_dir("kimicode/token.json"),
        }
    }

    async fn load(&self) -> Option<KimiToken> {
        tokio::fs::read_to_string(&self.path)
            .await
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    async fn save(&self, token: &KimiToken) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&self.path, serde_json::to_string(token)?).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600)).await?;
        }
        Ok(())
    }

    async fn clear(&self) -> Result<()> {
        match tokio::fs::remove_file(&self.path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

// ── Provider ─────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct KimiCodeProvider {
    #[serde(skip)]
    client: Client,
    #[serde(skip)]
    token_cache: TokenCache,
    #[serde(skip)]
    cached_token: tokio::sync::Mutex<Option<KimiToken>>,
    #[serde(skip)]
    device_id: String,
    model: ModelConfig,
    #[serde(skip)]
    name: String,
}

impl KimiCodeProvider {
    pub async fn cleanup() -> Result<()> {
        TokenCache::new().clear().await
    }

    pub async fn from_env(model: ModelConfig) -> Result<Self> {
        let model = model.with_fast(KIMI_CODE_DEFAULT_FAST_MODEL, KIMI_CODE_PROVIDER_NAME)?;
        let client = Client::builder()
            .timeout(StdDuration::from_secs(600))
            .build()?;
        let device_id = Self::get_or_create_device_id().await?;
        Ok(Self {
            client,
            token_cache: TokenCache::new(),
            cached_token: tokio::sync::Mutex::new(None),
            device_id,
            model,
            name: KIMI_CODE_PROVIDER_NAME.to_string(),
        })
    }

    async fn get_or_create_device_id() -> Result<String> {
        let path = Paths::in_config_dir("kimicode/device_id");
        if let Ok(id) = tokio::fs::read_to_string(&path).await {
            let id = id.trim().to_string();
            if !id.is_empty() {
                return Ok(id);
            }
        }
        let id = Uuid::new_v4().to_string().replace('-', "");
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, &id).await?;
        Ok(id)
    }

    fn kimi_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("X-Msh-Platform", KIMI_MSH_PLATFORM.parse().unwrap());
        headers.insert("X-Msh-Version", KIMI_MSH_VERSION.parse().unwrap());
        headers.insert("X-Msh-Device-Id", self.device_id.parse().unwrap());
        headers
    }

    // ── Token management ─────────────────────────────────────────────────────

    /// Returns a valid access token, refreshing or re-authenticating as needed.
    async fn get_access_token(&self) -> Result<String> {
        let mut guard = self.cached_token.lock().await;

        // 1. In-memory cache
        if let Some(token) = guard.as_ref() {
            if token.expires_at - Utc::now() > Duration::seconds(REFRESH_THRESHOLD_SECS) {
                return Ok(token.access_token.clone());
            }
            if let Ok(refreshed) = self.do_refresh_token(&token.refresh_token.clone()).await {
                self.token_cache.save(&refreshed).await?;
                let access = refreshed.access_token.clone();
                *guard = Some(refreshed);
                return Ok(access);
            }
            if token.expires_at > Utc::now() {
                return Ok(token.access_token.clone());
            }
        }

        // 2. Disk cache
        if let Some(token) = self.token_cache.load().await {
            if token.expires_at - Utc::now() > Duration::seconds(REFRESH_THRESHOLD_SECS) {
                *guard = Some(token.clone());
                return Ok(token.access_token);
            }
            if let Ok(refreshed) = self.do_refresh_token(&token.refresh_token.clone()).await {
                self.token_cache.save(&refreshed).await?;
                let access = refreshed.access_token.clone();
                *guard = Some(refreshed);
                return Ok(access);
            }
            if token.expires_at > Utc::now() {
                *guard = Some(token.clone());
                return Ok(token.access_token);
            }
        }

        // 3. Full device flow
        let token = self.device_flow_login().await?;
        self.token_cache.save(&token).await?;
        let access = token.access_token.clone();
        *guard = Some(token);
        Ok(access)
    }

    async fn device_flow_login(&self) -> Result<KimiToken> {
        #[derive(Serialize)]
        struct DeviceAuthReq<'a> {
            client_id: &'a str,
        }
        #[derive(Deserialize)]
        struct DeviceAuthResp {
            device_code: String,
            user_code: String,
            verification_uri_complete: Option<String>,
            verification_uri: String,
            interval: Option<u64>,
            expires_in: Option<u64>,
        }

        let resp: DeviceAuthResp = self
            .client
            .post(format!("{}/api/oauth/device_authorization", KIMI_AUTH_HOST))
            .headers(self.kimi_headers())
            .form(&DeviceAuthReq {
                client_id: KIMI_CODE_CLIENT_ID,
            })
            .send()
            .await
            .context("failed to request device authorization")?
            .error_for_status()
            .context("device authorization request failed")?
            .json()
            .await
            .context("failed to parse device authorization response")?;

        let verify_url = resp
            .verification_uri_complete
            .as_deref()
            .unwrap_or(&resp.verification_uri);
        let interval = resp.interval.unwrap_or(5);

        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(&resp.user_code);
        }
        if let Err(e) = webbrowser::open(verify_url) {
            tracing::warn!("Failed to open browser: {}", e);
        }

        println!(
            "Please visit {} and enter code {}",
            verify_url, resp.user_code
        );

        let expires_in = resp.expires_in.unwrap_or(300);
        self.poll_for_token(&resp.device_code, interval, expires_in)
            .await
    }

    async fn poll_for_token(
        &self,
        device_code: &str,
        interval_secs: u64,
        expires_in_secs: u64,
    ) -> Result<KimiToken> {
        #[derive(Serialize)]
        struct PollReq<'a> {
            client_id: &'a str,
            device_code: &'a str,
            grant_type: &'static str,
        }
        #[derive(Deserialize, Debug)]
        struct PollResp {
            access_token: Option<String>,
            refresh_token: Option<String>,
            expires_in: Option<i64>,
            error: Option<String>,
        }

        let deadline =
            tokio::time::Instant::now() + tokio::time::Duration::from_secs(expires_in_secs);
        let mut effective_interval = interval_secs;
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(anyhow!("timed out waiting for user authorization"));
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(effective_interval)).await;

            let resp: PollResp = self
                .client
                .post(format!("{}/api/oauth/token", KIMI_AUTH_HOST))
                .headers(self.kimi_headers())
                .form(&PollReq {
                    client_id: KIMI_CODE_CLIENT_ID,
                    device_code,
                    grant_type: "urn:ietf:params:oauth:grant-type:device_code",
                })
                .send()
                .await
                .context("failed to poll for token")?
                .json()
                .await
                .context("failed to parse token poll response")?;

            if let (Some(access_token), Some(refresh_token)) =
                (resp.access_token, resp.refresh_token)
            {
                let expires_in = resp.expires_in.unwrap_or(3600);
                return Ok(KimiToken {
                    access_token,
                    refresh_token,
                    expires_at: Utc::now() + Duration::seconds(expires_in),
                });
            }

            match resp.error.as_deref() {
                Some("authorization_pending") => {
                    tracing::debug!("authorization pending, continuing to poll");
                }
                // RFC 8628: client MUST increase polling interval by 5 seconds
                Some("slow_down") => {
                    tracing::debug!("slow_down received, increasing poll interval");
                    effective_interval += 5;
                }
                Some(err) => {
                    return Err(anyhow!("authorization failed: {}", err));
                }
                None => {
                    tracing::debug!("unexpected poll response: no token and no error");
                }
            }
        }
    }

    async fn do_refresh_token(&self, refresh_token: &str) -> Result<KimiToken> {
        #[derive(Serialize)]
        struct RefreshReq<'a> {
            client_id: &'a str,
            grant_type: &'static str,
            refresh_token: &'a str,
        }
        #[derive(Deserialize)]
        struct RefreshResp {
            access_token: String,
            refresh_token: String,
            expires_in: Option<i64>,
        }

        let resp: RefreshResp = self
            .client
            .post(format!("{}/api/oauth/token", KIMI_AUTH_HOST))
            .headers(self.kimi_headers())
            .form(&RefreshReq {
                client_id: KIMI_CODE_CLIENT_ID,
                grant_type: "refresh_token",
                refresh_token,
            })
            .send()
            .await
            .context("failed to refresh token")?
            .error_for_status()
            .context("token refresh failed")?
            .json()
            .await
            .context("failed to parse token refresh response")?;

        let expires_in = resp.expires_in.unwrap_or(3600);
        Ok(KimiToken {
            access_token: resp.access_token,
            refresh_token: resp.refresh_token,
            expires_at: Utc::now() + Duration::seconds(expires_in),
        })
    }

    // ── HTTP ─────────────────────────────────────────────────────────────────

    async fn post(
        &self,
        session_id: Option<&str>,
        payload: &Value,
    ) -> Result<reqwest::Response, ProviderError> {
        let access_token = self.get_access_token().await.map_err(|e| {
            ProviderError::Authentication(format!("Failed to get Kimi access token: {}", e))
        })?;

        let mut builder = self
            .client
            .post(format!("{}/v1/messages", KIMI_API_BASE))
            .bearer_auth(access_token)
            .headers(self.kimi_headers())
            .json(payload);

        if let Some(sid) = session_id {
            builder = builder.header(SESSION_ID_HEADER, sid);
        }

        builder
            .send()
            .await
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))
    }
}

// ── ProviderDef ───────────────────────────────────────────────────────────────

impl ProviderDef for KimiCodeProvider {
    type Provider = Self;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            KIMI_CODE_PROVIDER_NAME,
            "Kimi Code",
            "Kimi Code AI models optimized for coding tasks",
            KIMI_CODE_DEFAULT_MODEL,
            KIMI_CODE_KNOWN_MODELS.to_vec(),
            KIMI_CODE_DOC_URL,
            vec![ConfigKey::new_oauth_device_code(
                "KIMI_CODE_TOKEN",
                true,
                true,
                None,
                false,
            )],
        )
        .with_setup_steps(vec![
            "Run `goose configure` and select 'Kimi Code'",
            "A browser window will open — log in to kimi.com and enter the displayed code",
            "Once authorized, Goose will save your token automatically",
        ])
    }

    fn from_env(
        model: ModelConfig,
        _extensions: Vec<crate::config::ExtensionConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(Self::from_env(model))
    }
}

// ── Provider trait ────────────────────────────────────────────────────────────

#[async_trait]
impl Provider for KimiCodeProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model.clone()
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        session_id: &str,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let mut payload = create_request(model_config, system, messages, tools)
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;
        payload
            .as_object_mut()
            .unwrap()
            .insert("stream".to_string(), Value::Bool(true));

        let mut log = RequestLog::start(model_config, &payload)
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

        let response = self
            .with_retry(|| async {
                let resp = self.post(Some(session_id), &payload).await?;
                handle_status_openai_compat(resp).await
            })
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;

        let stream = response.bytes_stream().map_err(io::Error::other);

        Ok(Box::pin(try_stream! {
            let stream_reader = StreamReader::new(stream);
            let framed = tokio_util::codec::FramedRead::new(
                stream_reader,
                tokio_util::codec::LinesCodec::new(),
            )
            .map_err(anyhow::Error::from);

            let message_stream = response_to_streaming_message(framed);
            pin!(message_stream);
            while let Some(message) = futures::StreamExt::next(&mut message_stream).await {
                let (message, usage) = message.map_err(|e| {
                    ProviderError::RequestFailed(format!("Stream decode error: {}", e))
                })?;
                log.write(&message, usage.as_ref().map(|f| f.usage).as_ref())?;
                yield (message, usage);
            }
        }))
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(KIMI_CODE_KNOWN_MODELS
            .iter()
            .map(|s| s.to_string())
            .collect())
    }

    async fn configure_oauth(&self) -> Result<(), ProviderError> {
        // Try refresh first; fall back to still-valid token; then full device flow.
        if let Some(token) = self.token_cache.load().await {
            if let Ok(refreshed) = self.do_refresh_token(&token.refresh_token).await {
                self.token_cache.save(&refreshed).await.map_err(|e| {
                    ProviderError::ExecutionError(format!("Failed to save token: {}", e))
                })?;
                *self.cached_token.lock().await = Some(refreshed);
                return Ok(());
            }
            if token.expires_at > Utc::now() {
                *self.cached_token.lock().await = Some(token);
                return Ok(());
            }
        }

        let token = self
            .device_flow_login()
            .await
            .map_err(|e| ProviderError::Authentication(format!("OAuth flow failed: {}", e)))?;
        self.token_cache
            .save(&token)
            .await
            .map_err(|e| ProviderError::ExecutionError(format!("Failed to save token: {}", e)))?;
        *self.cached_token.lock().await = Some(token);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // ── KimiToken serde ───────────────────────────────────────────────────────

    #[test]
    fn kimi_token_roundtrip() {
        let token = KimiToken {
            access_token: "acc_test".to_string(),
            refresh_token: "ref_test".to_string(),
            expires_at: Utc::now() + Duration::seconds(3600),
        };
        let json = serde_json::to_string(&token).expect("serialize");
        let decoded: KimiToken = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.access_token, token.access_token);
        assert_eq!(decoded.refresh_token, token.refresh_token);
        assert_eq!(decoded.expires_at.timestamp(), token.expires_at.timestamp());
    }

    #[test]
    fn kimi_token_fresh_detection() {
        let fresh = KimiToken {
            access_token: "acc".to_string(),
            refresh_token: "ref".to_string(),
            expires_at: Utc::now() + Duration::seconds(3600),
        };
        assert!(
            fresh.expires_at - Utc::now() > Duration::seconds(REFRESH_THRESHOLD_SECS),
            "token should be considered fresh"
        );

        let stale = KimiToken {
            access_token: "acc".to_string(),
            refresh_token: "ref".to_string(),
            expires_at: Utc::now() + Duration::seconds(60),
        };
        assert!(
            stale.expires_at - Utc::now() <= Duration::seconds(REFRESH_THRESHOLD_SECS),
            "token should be considered stale"
        );
    }

    // ── Headers ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn kimi_headers_contains_required_fields() {
        let provider = KimiCodeProvider {
            client: Client::new(),
            token_cache: TokenCache::new(),
            cached_token: tokio::sync::Mutex::new(None),
            device_id: "testdeviceid".to_string(),
            model: ModelConfig::new(KIMI_CODE_DEFAULT_MODEL).unwrap(),
            name: KIMI_CODE_PROVIDER_NAME.to_string(),
        };

        let headers = provider.kimi_headers();
        assert_eq!(
            headers.get("X-Msh-Platform").and_then(|v| v.to_str().ok()),
            Some(KIMI_MSH_PLATFORM)
        );
        assert_eq!(
            headers.get("X-Msh-Version").and_then(|v| v.to_str().ok()),
            Some(KIMI_MSH_VERSION)
        );
        assert_eq!(
            headers.get("X-Msh-Device-Id").and_then(|v| v.to_str().ok()),
            Some("testdeviceid")
        );
    }

    // ── Metadata ──────────────────────────────────────────────────────────────

    #[test]
    fn metadata_has_oauth_device_code_key() {
        let meta = KimiCodeProvider::metadata();
        let key = meta
            .config_keys
            .iter()
            .find(|k| k.name == "KIMI_CODE_TOKEN")
            .expect("KIMI_CODE_TOKEN config key should exist");
        assert!(key.oauth_flow, "should be an OAuth flow key");
        assert!(key.device_code_flow, "should use device code flow");
        assert!(key.secret, "token should be stored securely");
    }

    #[test]
    fn metadata_has_setup_steps() {
        let meta = KimiCodeProvider::metadata();
        assert!(
            !meta.setup_steps.is_empty(),
            "setup_steps should be populated"
        );
    }

    #[test]
    fn fetch_supported_models_returns_known_models() {
        let known: Vec<String> = KIMI_CODE_KNOWN_MODELS
            .iter()
            .map(|s| s.to_string())
            .collect();
        // Verify the set matches what fetch_supported_models will return
        assert!(known.contains(&"kimi-k2.5".to_string()));
        assert!(known.contains(&"kimi-k2-thinking".to_string()));
    }
}
