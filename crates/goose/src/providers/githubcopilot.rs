use crate::config::paths::Paths;
use crate::providers::api_client::{ApiClient, AuthMethod};
use crate::providers::oauth_device_flow::{run_device_flow, DeviceFlowConfig, RequestEncoding};
use crate::providers::openai_compatible::{
    handle_status, stream_openai_compat, stream_responses_compat,
};
use crate::providers::private_file::write_private_file;
use anyhow::Result;
use async_trait::async_trait;
use axum::http;
use chrono::{DateTime, Utc};
use goose_providers::errors::ProviderError;
use goose_providers::formats::openai::is_openai_responses_model;
use goose_providers::images::ImageFormat;
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use url::{Host, Url};

// Task-local so complete() and stream() can't race on the same provider instance.
tokio::task_local! {
    static IS_AGENT_CALL: bool;
}

use super::base::{
    collect_stream, Provider, ProviderDef, ProviderMetadata, DEFAULT_PROVIDER_TIMEOUT_SECS,
};
use super::openai_compatible::handle_response_openai_compat;
use super::retry::ProviderRetry;
use super::utils::get_model;
use goose_providers::formats::openai::{create_request, get_usage, response_to_message};
use goose_providers::formats::openai_responses::create_responses_request;

use crate::config::{Config, ConfigError};
use crate::conversation::message::{Message, MessageContent};

use crate::providers::base::{ConfigKey, MessageStream};
use futures::future::BoxFuture;
use goose_providers::conversation::token_usage::{ProviderUsage, Usage};
use goose_providers::model::ModelConfig;
use goose_providers::request_log::{start_log, LoggerHandleExt};
use rmcp::model::{ContentBlock, Tool};

const GITHUB_COPILOT_PROVIDER_NAME: &str = "github_copilot";
pub const GITHUB_COPILOT_DEFAULT_MODEL: &str = "gpt-4.1";
pub const GITHUB_COPILOT_KNOWN_MODELS: &[&str] = &[
    "claude-haiku-4.5",
    "claude-opus-4.5",
    "claude-opus-4.6",
    "claude-opus-4.7",
    "claude-sonnet-4",
    "claude-sonnet-4.5",
    "claude-sonnet-4.6",
    "gemini-2.5-pro",
    "gemini-3-flash-preview",
    "gemini-3.1-pro-preview",
    "gpt-4.1",
    "gpt-4o",
    "grok-code-fast-1",
    "gpt-5-mini",
    "gpt-5.2",
    "gpt-5.2-codex",
    "gpt-5.3-codex",
    "gpt-5.4",
    "gpt-5.4-mini",
    "gpt-5.5",
];

// Models that support streaming on the /chat/completions path.
// Models routed to /responses always stream and don't need to be listed here.
pub const GITHUB_COPILOT_STREAM_MODELS: &[&str] = &[
    "gpt-4.1",
    "gpt-4o",
    "grok-code-fast-1",
    "gemini-2.5-pro",
    "gemini-3-flash-preview",
    "gemini-3.1-pro-preview",
];

const GITHUB_COPILOT_DOC_URL: &str =
    "https://docs.github.com/en/copilot/using-github-copilot/ai-models";
const DEFAULT_GITHUB_HOST: &str = "github.com";
const DEFAULT_GITHUB_COPILOT_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

fn normalize_host(host: &str) -> String {
    let host = host.trim_end_matches('/');
    let host = host.strip_prefix("https://").unwrap_or(host);
    host.to_string()
}

fn validate_copilot_api_endpoint(endpoint: &str) -> Result<bool, ProviderError> {
    let url = Url::parse(endpoint).map_err(|_| {
        ProviderError::RequestFailed("Invalid GitHub Copilot API endpoint".to_string())
    })?;
    let host = url.host().ok_or_else(|| {
        ProviderError::RequestFailed("Invalid GitHub Copilot API endpoint".to_string())
    })?;
    let is_loopback = match host {
        Host::Domain(host) => host.eq_ignore_ascii_case("localhost"),
        Host::Ipv4(address) => address.is_loopback(),
        Host::Ipv6(address) => address.is_loopback(),
    };

    if url.scheme() == "https" {
        Ok(true)
    } else if url.scheme() == "http" && is_loopback {
        Ok(false)
    } else {
        Err(ProviderError::RequestFailed(
            "GitHub Copilot API endpoint must use HTTPS unless it targets loopback".to_string(),
        ))
    }
}

#[derive(Debug, Clone)]
struct GithubCopilotUrls {
    device_code_url: String,
    access_token_url: String,
    copilot_token_url: String,
}

impl GithubCopilotUrls {
    fn new(host: &str, copilot_token_url: Option<&str>) -> Self {
        if host == "github.com" {
            Self {
                device_code_url: "https://github.com/login/device/code".to_string(),
                access_token_url: "https://github.com/login/oauth/access_token".to_string(),
                copilot_token_url: "https://api.github.com/copilot_internal/v2/token".to_string(),
            }
        } else {
            let base = format!("https://{}", host);
            let copilot_token_url = copilot_token_url
                .map(|u| u.trim_end_matches('/').to_string())
                .unwrap_or_else(|| format!("https://api.{}/copilot_internal/v2/token", host));
            Self {
                device_code_url: format!("{}/login/device/code", base),
                access_token_url: format!("{}/login/oauth/access_token", base),
                copilot_token_url,
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct CopilotTokenEndpoints {
    api: String,
    #[serde(flatten)]
    _extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(dead_code)] // useful for debugging
struct CopilotTokenInfo {
    token: String,
    expires_at: i64,
    refresh_in: i64,
    endpoints: CopilotTokenEndpoints,
    #[serde(flatten)]
    _extra: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct CopilotState {
    expires_at: DateTime<Utc>,
    info: CopilotTokenInfo,
}

#[derive(Debug)]
struct DiskCache {
    cache_path: PathBuf,
}

impl DiskCache {
    fn new(host: &str) -> Self {
        let cache_path = if host == DEFAULT_GITHUB_HOST {
            Paths::in_config_dir("githubcopilot/info.json")
        } else {
            let safe_host = host.replace(['/', ':', '.'], "_");
            Paths::in_config_dir(&format!("githubcopilot/{}/info.json", safe_host))
        };
        Self { cache_path }
    }

    async fn load(&self) -> Option<CopilotState> {
        if let Ok(contents) = tokio::fs::read_to_string(&self.cache_path).await {
            if let Ok(info) = serde_json::from_str::<CopilotState>(&contents) {
                return Some(info);
            }
        }
        None
    }

    async fn save(&self, info: &CopilotState) -> Result<()> {
        let contents = serde_json::to_string(info)?;
        let cache_path = self.cache_path.clone();
        tokio::task::spawn_blocking(move || write_private_file(&cache_path, &contents)).await??;
        Ok(())
    }

    async fn clear(&self) -> Result<()> {
        match tokio::fs::remove_file(&self.cache_path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct GithubCopilotProvider {
    #[serde(skip)]
    client: Client,
    #[serde(skip)]
    cache: DiskCache,
    #[serde(skip)]
    mu: tokio::sync::Mutex<RefCell<Option<CopilotState>>>,
    #[serde(skip)]
    urls: GithubCopilotUrls,
    #[serde(skip)]
    client_id: String,
    #[serde(skip)]
    name: String,
    #[serde(skip)]
    tls_config: Option<crate::providers::api_client::TlsConfig>,
}

impl GithubCopilotProvider {
    pub async fn cleanup() -> Result<()> {
        let config = Config::global();
        let host = normalize_host(
            &config
                .get_param::<String>("GITHUB_COPILOT_HOST")
                .unwrap_or_else(|_| DEFAULT_GITHUB_HOST.to_string()),
        );
        DiskCache::new(&host).clear().await
    }

    fn messages_contain_image(messages: &[Message]) -> bool {
        messages.iter().any(|m| {
            m.content.iter().any(|c| match c {
                MessageContent::Image(_) => true,
                MessageContent::ToolResponse(resp) => resp.tool_result.as_ref().is_ok_and(|r| {
                    r.content
                        .iter()
                        .any(|item| matches!(item, ContentBlock::Image(_)))
                }),
                _ => false,
            })
        })
    }

    fn authenticated_api_client(
        &self,
        endpoint: String,
        token: String,
        headers: http::HeaderMap,
    ) -> Result<ApiClient, ProviderError> {
        let https_only = validate_copilot_api_endpoint(&endpoint)?;
        let mut client = ApiClient::new_with_tls(
            endpoint,
            AuthMethod::BearerToken(token),
            self.tls_config.clone(),
        )?
        .with_request_builder(crate::session_context::session_id_request_builder())
        .with_headers(headers)?;
        if https_only {
            client = client.with_https_only()?;
        } else {
            client = client.with_loopback_http_only()?;
        }
        Ok(client)
    }

    pub async fn from_env(
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> Result<Self> {
        let config = Config::global();
        let host = normalize_host(
            &config
                .get_param::<String>("GITHUB_COPILOT_HOST")
                .unwrap_or_else(|_| DEFAULT_GITHUB_HOST.to_string()),
        );
        let client_id: String = config
            .get_param("GITHUB_COPILOT_CLIENT_ID")
            .unwrap_or_else(|_| DEFAULT_GITHUB_COPILOT_CLIENT_ID.to_string());
        let copilot_token_url: Option<String> = config.get_param("GITHUB_COPILOT_TOKEN_URL").ok();
        let urls = GithubCopilotUrls::new(&host, copilot_token_url.as_deref());
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_PROVIDER_TIMEOUT_SECS))
            .build()?;
        let cache = DiskCache::new(&host);
        let mu = tokio::sync::Mutex::new(RefCell::new(None));
        Ok(Self {
            client,
            cache,
            mu,
            urls,
            client_id,
            name: GITHUB_COPILOT_PROVIDER_NAME.to_string(),
            tls_config,
        })
    }

    async fn post(
        &self,
        model_config: &ModelConfig,
        path: &str,
        is_user_initiated: bool,
        payload: &mut Value,
        has_images: bool,
        streaming: bool,
    ) -> Result<Response, ProviderError> {
        let (endpoint, token) = self.get_api_info().await?;
        let mut headers = self.get_github_headers();
        if has_images {
            headers.insert("Copilot-Vision-Request", "true".parse().unwrap());
        }
        let initiator = if is_user_initiated { "user" } else { "agent" };
        headers.insert("X-Initiator", initiator.parse().unwrap());
        let api_client = self.authenticated_api_client(endpoint, token, headers)?;

        api_client
            .request(path)
            .model_headers(model_config)?
            .streaming(streaming)
            .response_post(payload)
            .await
            .map_err(|e| e.into())
    }

    async fn get_api_info(&self) -> Result<(String, String), ProviderError> {
        let guard = self.mu.lock().await;

        if let Some(state) = guard.borrow().as_ref() {
            if state.expires_at > Utc::now() {
                validate_copilot_api_endpoint(&state.info.endpoints.api)?;
                return Ok((state.info.endpoints.api.clone(), state.info.token.clone()));
            }
        }

        if let Some(state) = self.cache.load().await {
            if state.expires_at > Utc::now() {
                validate_copilot_api_endpoint(&state.info.endpoints.api)?;
                if guard.borrow().is_none() {
                    guard.replace(Some(state.clone()));
                }
                return Ok((state.info.endpoints.api, state.info.token));
            }
        }

        let config = Config::global();
        let github_token = match config.get_secret::<String>("GITHUB_COPILOT_TOKEN") {
            Ok(token) => token,
            Err(ConfigError::NotFound(_)) => return Err(ProviderError::NotConfigured),
            Err(error) => return Err(ProviderError::ExecutionError(error.to_string())),
        };

        const MAX_ATTEMPTS: i32 = 3;
        let mut last_error = None;
        for attempt in 0..MAX_ATTEMPTS {
            tracing::trace!("attempt {} to refresh api info", attempt + 1);
            let info = match self.refresh_api_info(&github_token).await {
                Ok(data) => data,
                Err(err) => {
                    tracing::warn!("failed to refresh api info: {}", err);
                    last_error = Some(err);
                    continue;
                }
            };
            let expires_at = Utc::now() + chrono::Duration::seconds(info.refresh_in);
            let new_state = CopilotState { info, expires_at };
            self.cache
                .save(&new_state)
                .await
                .map_err(ProviderError::from)?;
            guard.replace(Some(new_state.clone()));
            return Ok((new_state.info.endpoints.api, new_state.info.token));
        }
        Err(last_error.unwrap())
    }

    async fn refresh_api_info(
        &self,
        github_token: &str,
    ) -> Result<CopilotTokenInfo, ProviderError> {
        let response = self
            .client
            .get(&self.urls.copilot_token_url)
            .headers(self.get_github_headers())
            .header(
                http::header::AUTHORIZATION,
                format!("bearer {github_token}"),
            )
            .send()
            .await?;
        if matches!(
            response.status(),
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
        ) {
            return Err(ProviderError::Authentication(format!(
                "GitHub Copilot token request failed ({})",
                response.status()
            )));
        }
        let resp = response.error_for_status()?.text().await?;
        tracing::trace!("copilot token response: {}", resp);
        let info: CopilotTokenInfo = serde_json::from_str(&resp)
            .map_err(|error| ProviderError::RequestFailed(error.to_string()))?;
        validate_copilot_api_endpoint(&info.endpoints.api)?;
        Ok(info)
    }

    async fn login(&self) -> Result<String> {
        let cfg = DeviceFlowConfig {
            device_auth_url: Some(&self.urls.device_code_url),
            token_url: &self.urls.access_token_url,
            client_id: &self.client_id,
            scopes: Some("read:user"),
            extra_headers: self.get_github_headers(),
            encoding: RequestEncoding::Json,
        };
        let tokens = run_device_flow(&self.client, &cfg).await?;
        Ok(tokens.access_token)
    }

    fn get_github_headers(&self) -> http::HeaderMap {
        let mut headers = http::HeaderMap::new();
        headers.insert(http::header::ACCEPT, "application/json".parse().unwrap());
        headers.insert(
            http::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        headers.insert(
            http::header::USER_AGENT,
            "GithubCopilot/1.155.0".parse().unwrap(),
        );
        headers.insert("editor-version", "vscode/1.85.1".parse().unwrap());
        headers.insert("editor-plugin-version", "copilot/1.155.0".parse().unwrap());
        headers
    }

    #[allow(clippy::too_many_arguments)]
    async fn stream_responses(
        &self,
        model_config: &ModelConfig,
        is_user_initiated: bool,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
        has_images: bool,
    ) -> Result<MessageStream, ProviderError> {
        let mut payload = create_responses_request(model_config, system, messages, tools)
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;
        payload["stream"] = serde_json::Value::Bool(true);

        let mut log = start_log(model_config, &payload)?;

        let response = self
            .with_retry(|| async {
                let mut payload_clone = payload.clone();
                let resp = self
                    .post(
                        model_config,
                        "responses",
                        is_user_initiated,
                        &mut payload_clone,
                        has_images,
                        true,
                    )
                    .await?;
                handle_status(resp).await
            })
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;

        stream_responses_compat(response, log)
    }

    #[allow(clippy::too_many_arguments)]
    async fn stream_chat_completions(
        &self,
        model_config: &ModelConfig,
        is_user_initiated: bool,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
        has_images: bool,
    ) -> Result<MessageStream, ProviderError> {
        let supports_streaming = GITHUB_COPILOT_STREAM_MODELS
            .iter()
            .any(|prefix| model_config.model_name.starts_with(prefix));

        if supports_streaming {
            let payload = create_request(
                model_config,
                system,
                messages,
                tools,
                &ImageFormat::OpenAi,
                true,
            )?;
            let mut log = start_log(model_config, &payload)?;

            let response = self
                .with_retry(|| async {
                    let mut payload_clone = payload.clone();
                    let resp = self
                        .post(
                            model_config,
                            "chat/completions",
                            is_user_initiated,
                            &mut payload_clone,
                            has_images,
                            true,
                        )
                        .await?;
                    handle_status(resp).await
                })
                .await
                .inspect_err(|e| {
                    let _ = log.error(e);
                })?;

            stream_openai_compat(response, log)
        } else {
            let payload = create_request(
                model_config,
                system,
                messages,
                tools,
                &ImageFormat::OpenAi,
                false,
            )?;
            let mut log = start_log(model_config, &payload)?;

            let response = self
                .with_retry(|| async {
                    let mut payload_clone = payload.clone();
                    self.post(
                        model_config,
                        "chat/completions",
                        is_user_initiated,
                        &mut payload_clone,
                        has_images,
                        false,
                    )
                    .await
                })
                .await?;
            let response = handle_response_openai_compat(response).await?;

            let response = promote_tool_choice(response);

            let message = response_to_message(&response)?;
            let usage = response.get("usage").map(get_usage).unwrap_or_else(|| {
                tracing::debug!("Failed to get usage data");
                Usage::default()
            });
            let response_model = get_model(&response);
            log.write(&response, Some(&usage))?;

            Ok(super::base::stream_from_single_message(
                message,
                ProviderUsage::new(response_model, usage),
            ))
        }
    }
}

impl goose_providers::base::ProviderDescriptor for GithubCopilotProvider {
    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            GITHUB_COPILOT_PROVIDER_NAME,
            "GitHub Copilot",
            "GitHub Copilot. Run `goose configure` and select copilot to set up.",
            GITHUB_COPILOT_DEFAULT_MODEL,
            GITHUB_COPILOT_KNOWN_MODELS.to_vec(),
            GITHUB_COPILOT_DOC_URL,
            vec![
                ConfigKey::new_oauth_device_code("GITHUB_COPILOT_TOKEN", true, true, None, false),
                ConfigKey::new("GITHUB_COPILOT_HOST", false, false, None, false),
                ConfigKey::new("GITHUB_COPILOT_CLIENT_ID", false, false, None, false),
                ConfigKey::new("GITHUB_COPILOT_TOKEN_URL", false, false, None, false),
            ],
        )
        .with_setup(
            crate::providers::catalog::ProviderSetupMetadata::new(
                crate::providers::catalog::ProviderSetupCategory::Model,
                crate::providers::catalog::ProviderSetupMethod::OauthDeviceCode,
                crate::providers::catalog::ProviderSetupGroup::Default,
            )
            .with_native_connect_query("GitHub Copilot")
            .with_capabilities(false, true, false),
        )
    }
}

impl ProviderDef for GithubCopilotProvider {
    type Provider = Self;

    fn from_env(
        _extensions: Vec<crate::config::ExtensionConfig>,
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(Self::from_env(tls_config))
    }
}

#[async_trait]
impl Provider for GithubCopilotProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    async fn complete(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<(Message, ProviderUsage), ProviderError> {
        IS_AGENT_CALL
            .scope(true, async {
                collect_stream(self.stream(model_config, system, messages, tools).await?).await
            })
            .await
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let is_agent_call = IS_AGENT_CALL.try_with(|&v| v).unwrap_or(false);
        let last_is_tool_response = messages.last().is_some_and(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, MessageContent::ToolResponse(_)))
        });
        let is_user_initiated = !is_agent_call && !last_is_tool_response;
        let has_images = Self::messages_contain_image(messages);

        if is_openai_responses_model(&model_config.model_name) {
            self.stream_responses(
                model_config,
                is_user_initiated,
                system,
                messages,
                tools,
                has_images,
            )
            .await
        } else {
            self.stream_chat_completions(
                model_config,
                is_user_initiated,
                system,
                messages,
                tools,
                has_images,
            )
            .await
        }
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        let (endpoint, token) = self.get_api_info().await?;

        let mut headers = http::HeaderMap::new();
        headers.insert(http::header::ACCEPT, "application/json".parse().unwrap());
        headers.insert(
            http::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        headers.insert("Copilot-Integration-Id", "vscode-chat".parse().unwrap());
        let api_client = self.authenticated_api_client(endpoint, token, headers)?;
        let response = api_client.response_get("models").await?;

        let json: serde_json::Value = response.json().await?;

        let arr = json.get("data").and_then(|v| v.as_array()).ok_or_else(|| {
            ProviderError::RequestFailed(
                "Missing 'data' array in GitHub Copilot models response".to_string(),
            )
        })?;
        let mut models: Vec<String> = arr
            .iter()
            .filter_map(|m| {
                if let Some(s) = m.as_str() {
                    Some(s.to_string())
                } else if let Some(obj) = m.as_object() {
                    obj.get("id").and_then(|v| v.as_str()).map(str::to_string)
                } else {
                    None
                }
            })
            .collect();
        models.sort();
        Ok(models)
    }

    async fn configure_oauth(&self) -> Result<(), ProviderError> {
        let config = Config::global();

        if let Ok(github_token) = config.get_secret::<String>("GITHUB_COPILOT_TOKEN") {
            match self.refresh_api_info(&github_token).await {
                Ok(_) => return Ok(()),
                Err(_) => {
                    tracing::debug!("Existing token is invalid, starting OAuth flow");
                }
            }
        }

        let token = self
            .login()
            .await
            .map_err(|e| ProviderError::Authentication(format!("OAuth flow failed: {}", e)))?;

        config
            .set_secret("GITHUB_COPILOT_TOKEN", &token)
            .map_err(|e| ProviderError::ExecutionError(format!("Failed to save token: {}", e)))?;

        Ok(())
    }
}

// Copilot sometimes returns multiple choices in a completion response for
// Claude models and places the `tool_calls` payload in a non-zero index choice.
// This function ensures the first choice contains tool metadata so the shared formatter emits a
// `ToolRequest` instead of returning only the plain-text choice.
fn promote_tool_choice(response: Value) -> Value {
    let Some(choices) = response.get("choices").and_then(|c| c.as_array()) else {
        return response;
    };

    let tool_choice_idx = choices.iter().position(|choice| {
        choice
            .get("message")
            .and_then(|m| m.get("tool_calls"))
            .and_then(|tc| tc.as_array())
            .map(|arr| !arr.is_empty())
            .unwrap_or(false)
    });

    if let Some(idx) = tool_choice_idx {
        if idx != 0 {
            let mut new_response = response;
            if let Some(new_choices) = new_response
                .get_mut("choices")
                .and_then(|c| c.as_array_mut())
            {
                let choice = new_choices.remove(idx);
                new_choices.insert(0, choice);
            }
            return new_response;
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn copilot_api_endpoint_policy_requires_https_or_loopback() {
        assert!(validate_copilot_api_endpoint("https://api.example.com").unwrap());
        for endpoint in [
            "http://localhost:8080",
            "http://127.0.0.1:8080",
            "http://[::1]:8080",
        ] {
            assert!(!validate_copilot_api_endpoint(endpoint).unwrap());
        }
        for endpoint in [
            "http://api.example.com",
            "http://localhost.example",
            "ftp://127.0.0.1/resource",
            "not a URL",
            "https://",
        ] {
            assert!(
                validate_copilot_api_endpoint(endpoint).is_err(),
                "accepted invalid endpoint {endpoint}"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn disk_cache_saves_owner_only_file() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let directory = tempfile::tempdir().unwrap();
        let cache_path = directory.path().join("info.json");
        std::fs::write(&cache_path, "old-secret").unwrap();
        std::fs::set_permissions(&cache_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        let cache = DiskCache {
            cache_path: cache_path.clone(),
        };
        let state = CopilotState {
            expires_at: Utc::now(),
            info: CopilotTokenInfo {
                token: "copilot-secret".to_string(),
                expires_at: 1,
                refresh_in: 1,
                endpoints: CopilotTokenEndpoints {
                    api: "https://api.githubcopilot.com".to_string(),
                    _extra: HashMap::new(),
                },
                _extra: HashMap::new(),
            },
        };

        cache.save(&state).await.unwrap();

        let metadata = std::fs::metadata(&cache_path).unwrap();
        assert_eq!(metadata.mode() & 0o777, 0o600);
        let saved: CopilotState =
            serde_json::from_str(&std::fs::read_to_string(cache_path).unwrap()).unwrap();
        assert_eq!(saved.info.token, "copilot-secret");
    }

    #[tokio::test]
    async fn get_api_info_uses_valid_cache_without_github_token() {
        let directory = tempfile::tempdir().unwrap();
        let cache = DiskCache {
            cache_path: directory.path().join("info.json"),
        };
        let state = CopilotState {
            expires_at: Utc::now() + chrono::Duration::minutes(10),
            info: CopilotTokenInfo {
                token: "copilot-secret".to_string(),
                expires_at: 1,
                refresh_in: 600,
                endpoints: CopilotTokenEndpoints {
                    api: "https://api.githubcopilot.com".to_string(),
                    _extra: HashMap::new(),
                },
                _extra: HashMap::new(),
            },
        };
        cache.save(&state).await.unwrap();
        let provider = GithubCopilotProvider {
            client: Client::new(),
            cache,
            mu: tokio::sync::Mutex::new(RefCell::new(None)),
            urls: GithubCopilotUrls::new("github.com", None),
            client_id: DEFAULT_GITHUB_COPILOT_CLIENT_ID.to_string(),
            name: GITHUB_COPILOT_PROVIDER_NAME.to_string(),
            tls_config: None,
        };

        let (endpoint, token) = provider.get_api_info().await.unwrap();

        assert_eq!(endpoint, "https://api.githubcopilot.com");
        assert_eq!(token, "copilot-secret");
    }

    #[tokio::test]
    async fn get_api_info_rejects_plaintext_legacy_cache() {
        let directory = tempfile::tempdir().unwrap();
        let cache = DiskCache {
            cache_path: directory.path().join("info.json"),
        };
        let state = CopilotState {
            expires_at: Utc::now() + chrono::Duration::minutes(10),
            info: CopilotTokenInfo {
                token: "copilot-secret".to_string(),
                expires_at: 1,
                refresh_in: 600,
                endpoints: CopilotTokenEndpoints {
                    api: "http://api.example.com".to_string(),
                    _extra: HashMap::new(),
                },
                _extra: HashMap::new(),
            },
        };
        cache.save(&state).await.unwrap();
        let provider = GithubCopilotProvider {
            client: Client::new(),
            cache,
            mu: tokio::sync::Mutex::new(RefCell::new(None)),
            urls: GithubCopilotUrls::new("github.com", None),
            client_id: DEFAULT_GITHUB_COPILOT_CLIENT_ID.to_string(),
            name: GITHUB_COPILOT_PROVIDER_NAME.to_string(),
            tls_config: None,
        };

        let error = provider.get_api_info().await.unwrap_err();

        assert!(matches!(error, ProviderError::RequestFailed(_)));
        assert!(provider.mu.lock().await.borrow().is_none());
    }

    #[tokio::test]
    async fn fetch_supported_models_accepts_loopback_api_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [{ "id": "gpt-test" }]
            })))
            .mount(&server)
            .await;
        let directory = tempfile::tempdir().unwrap();
        let cache = DiskCache {
            cache_path: directory.path().join("info.json"),
        };
        cache
            .save(&CopilotState {
                expires_at: Utc::now() + chrono::Duration::minutes(10),
                info: CopilotTokenInfo {
                    token: "copilot-secret".to_string(),
                    expires_at: 1,
                    refresh_in: 600,
                    endpoints: CopilotTokenEndpoints {
                        api: server.uri(),
                        _extra: HashMap::new(),
                    },
                    _extra: HashMap::new(),
                },
            })
            .await
            .unwrap();
        let provider = GithubCopilotProvider {
            client: Client::new(),
            cache,
            mu: tokio::sync::Mutex::new(RefCell::new(None)),
            urls: GithubCopilotUrls::new("github.com", None),
            client_id: DEFAULT_GITHUB_COPILOT_CLIENT_ID.to_string(),
            name: GITHUB_COPILOT_PROVIDER_NAME.to_string(),
            tls_config: None,
        };

        assert_eq!(
            provider.fetch_supported_models().await.unwrap(),
            vec!["gpt-test".to_string()]
        );
    }

    #[tokio::test]
    async fn refresh_api_info_returns_authentication_for_rejected_token() {
        for status in [401, 403] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/copilot-token"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
            let directory = tempfile::tempdir().unwrap();
            let provider = GithubCopilotProvider {
                client: Client::new(),
                cache: DiskCache {
                    cache_path: directory.path().join("info.json"),
                },
                mu: tokio::sync::Mutex::new(RefCell::new(None)),
                urls: GithubCopilotUrls {
                    device_code_url: String::new(),
                    access_token_url: String::new(),
                    copilot_token_url: format!("{}/copilot-token", server.uri()),
                },
                client_id: DEFAULT_GITHUB_COPILOT_CLIENT_ID.to_string(),
                name: GITHUB_COPILOT_PROVIDER_NAME.to_string(),
                tls_config: None,
            };

            let error = provider.refresh_api_info("rejected").await.unwrap_err();

            assert!(matches!(error, ProviderError::Authentication(_)));
        }
    }

    #[tokio::test]
    async fn refresh_api_info_rejects_plaintext_remote_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/copilot-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "token": "copilot-secret",
                "expires_at": 0,
                "refresh_in": 600,
                "endpoints": { "api": "http://api.example.com" }
            })))
            .mount(&server)
            .await;
        let directory = tempfile::tempdir().unwrap();
        let provider = GithubCopilotProvider {
            client: Client::new(),
            cache: DiskCache {
                cache_path: directory.path().join("info.json"),
            },
            mu: tokio::sync::Mutex::new(RefCell::new(None)),
            urls: GithubCopilotUrls {
                device_code_url: String::new(),
                access_token_url: String::new(),
                copilot_token_url: format!("{}/copilot-token", server.uri()),
            },
            client_id: DEFAULT_GITHUB_COPILOT_CLIENT_ID.to_string(),
            name: GITHUB_COPILOT_PROVIDER_NAME.to_string(),
            tls_config: None,
        };

        let error = provider.refresh_api_info("github-token").await.unwrap_err();

        assert!(matches!(error, ProviderError::RequestFailed(_)));
    }

    #[tokio::test]
    async fn refresh_api_info_accepts_loopback_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/copilot-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "token": "copilot-secret",
                "expires_at": 0,
                "refresh_in": 600,
                "endpoints": { "api": server.uri() }
            })))
            .mount(&server)
            .await;
        let directory = tempfile::tempdir().unwrap();
        let provider = GithubCopilotProvider {
            client: Client::new(),
            cache: DiskCache {
                cache_path: directory.path().join("info.json"),
            },
            mu: tokio::sync::Mutex::new(RefCell::new(None)),
            urls: GithubCopilotUrls {
                device_code_url: String::new(),
                access_token_url: String::new(),
                copilot_token_url: format!("{}/copilot-token", server.uri()),
            },
            client_id: DEFAULT_GITHUB_COPILOT_CLIENT_ID.to_string(),
            name: GITHUB_COPILOT_PROVIDER_NAME.to_string(),
            tls_config: None,
        };

        let info = provider.refresh_api_info("github-token").await.unwrap();

        assert_eq!(info.endpoints.api, server.uri());
    }

    #[test]
    fn responses_models_routed_correctly() {
        assert!(is_openai_responses_model("gpt-5.5"));
        assert!(is_openai_responses_model("gpt-5.4"));
        assert!(is_openai_responses_model("gpt-5"));
        assert!(is_openai_responses_model("gpt-5-mini"));
        assert!(is_openai_responses_model("gpt-5-codex"));
        assert!(is_openai_responses_model("o3"));
        assert!(is_openai_responses_model("o3-mini"));

        assert!(!is_openai_responses_model("gpt-4.1"));
        assert!(!is_openai_responses_model("gpt-4o"));
        assert!(!is_openai_responses_model("claude-sonnet-4"));
        assert!(!is_openai_responses_model("claude-haiku-4.5"));
        assert!(!is_openai_responses_model("gemini-2.5-pro"));
    }

    #[test]
    fn detects_images_in_messages() {
        use crate::conversation::message::Message;

        let messages_with_image = vec![Message::user()
            .with_text("describe this")
            .with_image("base64data", "image/png")];
        assert!(GithubCopilotProvider::messages_contain_image(
            &messages_with_image
        ));

        let messages_without_image = vec![Message::user().with_text("plain text")];
        assert!(!GithubCopilotProvider::messages_contain_image(
            &messages_without_image
        ));
    }

    #[test]
    fn detects_images_in_tool_responses() {
        use crate::conversation::message::{Message, MessageContent};
        use rmcp::model::{CallToolResult, ContentBlock};

        let image_content =
            ContentBlock::image("aW1hZ2VkYXRh".to_string(), "image/png".to_string());
        let tool_result = Ok(CallToolResult::success(vec![image_content]));

        let messages =
            vec![Message::user()
                .with_content(MessageContent::tool_response("call_123", tool_result))];
        assert!(GithubCopilotProvider::messages_contain_image(&messages));

        let text_result = Ok(CallToolResult::success(vec![ContentBlock::text(
            "no images",
        )]));
        let messages_text_only =
            vec![Message::user()
                .with_content(MessageContent::tool_response("call_456", text_result))];
        assert!(!GithubCopilotProvider::messages_contain_image(
            &messages_text_only
        ));
    }

    #[test]
    fn promotes_choice_with_tool_call() {
        let response = json!({
            "choices": [
                {"message": {"content": "plain text"}},
                {"message": {"tool_calls": [{"function": {"name": "foo", "arguments": "{}"}}]}}
            ]
        });

        let promoted = promote_tool_choice(response);
        assert_eq!(
            promoted
                .get("choices")
                .and_then(|c| c.as_array())
                .map(|c| c.len()),
            Some(2)
        );
        let first_choice = promoted
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
            .unwrap();

        assert!(first_choice
            .get("message")
            .and_then(|m| m.get("tool_calls"))
            .is_some());
    }

    #[test]
    fn leaves_response_when_tool_choice_first() {
        let response = json!({
            "choices": [
                {"message": {"tool_calls": [{"function": {"name": "foo", "arguments": "{}"}}]}},
                {"message": {"content": "plain text"}}
            ]
        });

        let promoted = promote_tool_choice(response.clone());
        assert_eq!(promoted, response);
    }

    #[test]
    fn normalize_host_strips_prefix_and_slash() {
        assert_eq!(normalize_host("github.com"), "github.com");
        assert_eq!(normalize_host("https://github.com"), "github.com");
        assert_eq!(normalize_host("github.com/"), "github.com");
        assert_eq!(normalize_host("https://github.com/"), "github.com");
        assert_eq!(
            normalize_host("https://my-enterprise.ghe.com/"),
            "my-enterprise.ghe.com"
        );
    }

    #[test]
    fn urls_default_github_com() {
        let urls = GithubCopilotUrls::new("github.com", None);
        assert_eq!(urls.device_code_url, "https://github.com/login/device/code");
        assert_eq!(
            urls.access_token_url,
            "https://github.com/login/oauth/access_token"
        );
        assert_eq!(
            urls.copilot_token_url,
            "https://api.github.com/copilot_internal/v2/token"
        );
    }

    #[test]
    fn urls_enterprise_host() {
        let urls = GithubCopilotUrls::new("my-enterprise.ghe.com", None);
        assert_eq!(
            urls.device_code_url,
            "https://my-enterprise.ghe.com/login/device/code"
        );
        assert_eq!(
            urls.access_token_url,
            "https://my-enterprise.ghe.com/login/oauth/access_token"
        );
        assert_eq!(
            urls.copilot_token_url,
            "https://api.my-enterprise.ghe.com/copilot_internal/v2/token"
        );
    }

    #[test]
    fn urls_enterprise_with_token_url_override() {
        let urls = GithubCopilotUrls::new(
            "my-enterprise.ghe.com",
            Some("https://my-enterprise.ghe.com/api/v3/copilot_internal/v2/token"),
        );
        assert_eq!(
            urls.copilot_token_url,
            "https://my-enterprise.ghe.com/api/v3/copilot_internal/v2/token"
        );
    }
}
