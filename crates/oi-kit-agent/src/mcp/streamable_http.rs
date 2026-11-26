use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::sync::{Arc, PoisonError};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use rmcp::model::{
    CallToolResult, ClientJsonRpcMessage, ErrorCode, ErrorData, GetPromptResult, ProtocolVersion,
    ServerConfig, ServerNotification,
};
use rmcp::service::{ClientInitializeError, ServiceError};
use rmcp::transport::auth::AuthClient;
use rmcp::transport::common::client_side_sse::BoxedSseResponse;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClient, StreamableHttpClientTransportConfig, StreamableHttpError,
    StreamableHttpPostResponse,
};
use rmcp::transport::{DynamicTransportError, IntoTransport, StreamableHttpClientTransport};
use rmcp::RoleClient;
use serde_json::Value;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::{ExtensionAuth, RejectedRequest, StaticOAuthClientConfig, StoredCredentialsError};
use crate::extension::{ExtensionError, ExtensionResult};
use crate::mcp::client::McpCallContext as ToolCallContext;
use crate::mcp::client::{ConnectContext, McpClient, McpClientTrait};

/// Whether the server refused the request's credentials, with a typed
/// challenge or a bare 401/403.
fn refuses_credentials<E>(error: &StreamableHttpError<E>) -> bool
where
    E: std::error::Error + Send + Sync + 'static,
{
    match error {
        StreamableHttpError::AuthRequired(_) | StreamableHttpError::InsufficientScope(_) => true,
        StreamableHttpError::UnexpectedServerResponse(body) => {
            body.starts_with("HTTP 401") || body.starts_with("HTTP 403")
        }
        _ => false,
    }
}

/// Retry with OAuth for typed auth challenges and wrapped bare HTTP 401 responses.
fn is_oauth_auth_failure(err: &ClientInitializeError) -> bool {
    let ClientInitializeError::TransportError {
        error: DynamicTransportError { error, .. },
        ..
    } = err
    else {
        return false;
    };

    if let Some(http_err) = error.downcast_ref::<StreamableHttpError<reqwest::Error>>() {
        return refuses_credentials(http_err);
    }

    #[cfg(unix)]
    if let Some(http_err) = error
        .downcast_ref::<StreamableHttpError<rmcp::transport::common::unix_socket::UnixSocketError>>(
        )
    {
        return refuses_credentials(http_err);
    }

    let message = error.to_string();
    message.contains("unexpected server response: HTTP 401")
        || message.contains("unexpected server response: HTTP 403")
        || message.contains("Auth required")
        || message.contains("Authorization required")
}

/// Extract the `WWW-Authenticate` challenge from a failed initialization, so
/// OAuth discovery can be seeded from the server's 401/403 response instead of
/// probing well-known locations.
fn auth_challenge_from_error(err: &ClientInitializeError) -> Option<String> {
    let ClientInitializeError::TransportError {
        error: DynamicTransportError { error, .. },
        ..
    } = err
    else {
        return None;
    };

    if let Some(http_err) = error.downcast_ref::<StreamableHttpError<reqwest::Error>>() {
        return http_err.auth_challenge().map(str::to_string);
    }

    #[cfg(unix)]
    if let Some(http_err) = error
        .downcast_ref::<StreamableHttpError<rmcp::transport::common::unix_socket::UnixSocketError>>(
        )
    {
        return http_err.auth_challenge().map(str::to_string);
    }

    None
}

/// Extract the `WWW-Authenticate` challenge from a post-initialization request
/// failure (401 auth required or 403 insufficient scope), so a step-up
/// authorization can be started reactively.
fn auth_challenge_from_service_error(err: &ServiceError) -> Option<String> {
    let ServiceError::TransportSend(DynamicTransportError { error, .. }) = err else {
        return None;
    };

    if let Some(http_err) = error.downcast_ref::<StreamableHttpError<reqwest::Error>>() {
        return http_err.auth_challenge().map(str::to_string);
    }

    #[cfg(unix)]
    if let Some(http_err) = error
        .downcast_ref::<StreamableHttpError<rmcp::transport::common::unix_socket::UnixSocketError>>(
        )
    {
        return http_err.auth_challenge().map(str::to_string);
    }

    None
}

pub(super) fn resolve_static_oauth_client(
    client_id: Option<&str>,
    client_secret_key: Option<&str>,
    scopes: &[String],
    envs: &HashMap<String, String>,
) -> ExtensionResult<Option<StaticOAuthClientConfig>> {
    let Some(client_id) = client_id else {
        if client_secret_key.is_some() {
            return Err(ExtensionError::ConfigError(
                "client_secret_key requires client_id".to_string(),
            ));
        }
        if !scopes.is_empty() {
            return Err(ExtensionError::ConfigError(
                "scopes requires client_id".to_string(),
            ));
        }
        return Ok(None);
    };

    let client_secret =
        match client_secret_key {
            Some(key) => Some(envs.get(key).cloned().ok_or_else(|| {
                ExtensionError::ConfigError(format!("Secret '{}' not found", key))
            })?),
            None => None,
        };

    Ok(Some(StaticOAuthClientConfig {
        client_id: client_id.to_string(),
        client_secret,
        scopes: scopes.to_vec(),
    }))
}

fn header_map(headers: &HashMap<String, String>, version: &str) -> ExtensionResult<HeaderMap> {
    let mut map = HeaderMap::new();
    if let Ok(user_agent) = HeaderValue::from_str(&format!("goose/{version}")) {
        map.insert(reqwest::header::USER_AGENT, user_agent);
    }
    for (key, value) in headers {
        map.insert(
            HeaderName::try_from(key)
                .map_err(|_| ExtensionError::ConfigError(format!("invalid header: {}", key)))?,
            value.parse().map_err(|_| {
                ExtensionError::ConfigError(format!("invalid header value: {}", key))
            })?,
        );
    }
    Ok(map)
}

fn http_client(
    headers: &HashMap<String, String>,
    ctx: &ConnectContext,
) -> ExtensionResult<reqwest::Client> {
    #[allow(unused_mut)]
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .default_headers(header_map(headers, &ctx.client_version)?);
    #[cfg(target_os = "linux")]
    {
        builder = builder.tcp_user_timeout(Some(ctx.timeout));
    }
    builder
        .build()
        .map_err(|_| ExtensionError::ConfigError("could not construct http client".to_string()))
}

fn should_retry_legacy_after_empty_discover(
    result: &Result<McpClient, Box<ClientInitializeError>>,
    capabilities: &super::client::GooseMcpClientCapabilities,
) -> bool {
    capabilities.protocol_version.is_none()
        && result.as_ref().is_err_and(|error| {
            error.to_string().contains("empty sse stream")
                || matches!(
                    error.as_ref(),
                    ClientInitializeError::ConnectionClosed(context)
                        if context == "discover response"
                )
        })
}

// TODO: Remove this compatibility retry once rmcp handles empty discovery SSE
// responses upstream.
async fn connect_with_legacy_retry<T, E, A>(
    make_transport: impl Fn() -> T,
    ctx: ConnectContext,
) -> Result<McpClient, Box<ClientInitializeError>>
where
    T: IntoTransport<RoleClient, E, A>,
    E: std::error::Error + From<std::io::Error> + Send + Sync + 'static,
{
    let result = McpClient::connect(make_transport(), ctx.clone()).await;
    if !should_retry_legacy_after_empty_discover(&result, &ctx.capabilities) {
        return result;
    }
    let mut ctx = ctx;
    ctx.capabilities.protocol_version = Some(ProtocolVersion::V_2025_11_25);
    McpClient::connect(make_transport(), ctx).await
}

/// Access tokens the server refused on one connection. Each refusal records
/// the token its own request sent, so a token another request on the same
/// connection refreshes in the meantime is never mistaken for the refused one.
#[derive(Clone, Default)]
struct RefusedTokens(Arc<std::sync::Mutex<VecDeque<String>>>);

impl RefusedTokens {
    /// A refusal is followed by a refresh or a new authorization, so only the
    /// most recent refused tokens can still be the stored one.
    const KEPT: usize = 8;

    fn record(&self, token: String) {
        let mut tokens = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if tokens.contains(&token) {
            return;
        }
        if tokens.len() == Self::KEPT {
            tokens.pop_front();
        }
        tokens.push_back(token);
    }

    fn snapshot(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }
}

/// Sits under rmcp's `AuthClient`, which picks the token for every attempt,
/// and records each token the server refuses.
#[derive(Clone)]
struct RefusalRecordingClient<C> {
    inner: C,
    refused: RefusedTokens,
}

impl<C: StreamableHttpClient> RefusalRecordingClient<C> {
    fn record_refusal<T>(
        &self,
        auth_token: Option<String>,
        result: Result<T, StreamableHttpError<C::Error>>,
    ) -> Result<T, StreamableHttpError<C::Error>> {
        if let (Some(token), Err(error)) = (auth_token, &result) {
            if refuses_credentials(error) {
                self.refused.record(token);
            }
        }
        result
    }
}

impl<C: StreamableHttpClient + Sync> StreamableHttpClient for RefusalRecordingClient<C> {
    type Error = C::Error;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_token: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        let result = self
            .inner
            .post_message(uri, message, session_id, auth_token.clone(), custom_headers)
            .await;
        self.record_refusal(auth_token, result)
    }

    async fn post_message_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_token: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
        max_sse_event_size: usize,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        let result = self
            .inner
            .post_message_with_max_sse_event_size(
                uri,
                message,
                session_id,
                auth_token.clone(),
                custom_headers,
                max_sse_event_size,
            )
            .await;
        self.record_refusal(auth_token, result)
    }

    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        auth_token: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), StreamableHttpError<Self::Error>> {
        let result = self
            .inner
            .delete_session(uri, session_id, auth_token.clone(), custom_headers)
            .await;
        self.record_refusal(auth_token, result)
    }

    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_token: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxedSseResponse, StreamableHttpError<Self::Error>> {
        let result = self
            .inner
            .get_stream(
                uri,
                session_id,
                last_event_id,
                auth_token.clone(),
                custom_headers,
            )
            .await;
        self.record_refusal(auth_token, result)
    }

    async fn get_stream_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_token: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
        max_sse_event_size: usize,
    ) -> Result<BoxedSseResponse, StreamableHttpError<Self::Error>> {
        let result = self
            .inner
            .get_stream_with_max_sse_event_size(
                uri,
                session_id,
                last_event_id,
                auth_token.clone(),
                custom_headers,
                max_sse_event_size,
            )
            .await;
        self.record_refusal(auth_token, result)
    }
}

async fn connect_with_auth(
    auth_manager: rmcp::transport::AuthorizationManager,
    uri: &str,
    headers: &HashMap<String, String>,
    ctx: ConnectContext,
    refused: RefusedTokens,
) -> ExtensionResult<McpClient> {
    let recording_client = RefusalRecordingClient {
        inner: http_client(headers, &ctx)?,
        refused,
    };
    let auth_client = AuthClient::new(recording_client, auth_manager);
    Ok(connect_with_legacy_retry(
        || {
            StreamableHttpClientTransport::with_client(
                auth_client.clone(),
                StreamableHttpClientTransportConfig::with_uri(uri),
            )
        },
        ctx,
    )
    .await?)
}

/// Connection parameters needed to re-establish an authorized streamable HTTP
/// client after a post-initialization auth challenge (401/403).
#[derive(Clone)]
pub(super) struct ConnectParams {
    pub(super) uri: String,
    pub(super) name: String,
    pub(super) headers: HashMap<String, String>,
    pub(super) static_oauth_client: Option<StaticOAuthClientConfig>,
    pub(super) auth: Option<Arc<dyn ExtensionAuth>>,
    pub(super) ctx: ConnectContext,
}

/// A connected client and the access tokens the server refused on it.
struct Connection {
    client: McpClient,
    refused_tokens: RefusedTokens,
}

/// Wraps a streamable HTTP `McpClient` and handles step-up authorization:
/// when a request fails with a 401/403 carrying a `WWW-Authenticate`
/// challenge after initialization succeeded, re-authorize using the challenge
/// (requesting the union of scopes), reconnect, and retry the request once.
struct OAuthStepUpClient {
    inner: tokio::sync::RwLock<Connection>,
    server_info: Option<ServerConfig>,
    params: tokio::sync::RwLock<ConnectParams>,
    step_up_lock: tokio::sync::Mutex<()>,
    notification_subscribers: Arc<Mutex<Vec<mpsc::Sender<ServerNotification>>>>,
}

impl OAuthStepUpClient {
    async fn new(client: McpClient, refused_tokens: RefusedTokens, params: ConnectParams) -> Self {
        let server_info = client.get_info().cloned();
        let notification_subscribers = Arc::new(Mutex::new(Vec::new()));
        Self::forward_notifications(&client, notification_subscribers.clone()).await;
        Self {
            inner: tokio::sync::RwLock::new(Connection {
                client,
                refused_tokens,
            }),
            server_info,
            params: tokio::sync::RwLock::new(params),
            step_up_lock: tokio::sync::Mutex::new(()),
            notification_subscribers,
        }
    }

    async fn forward_notifications(
        client: &McpClient,
        subscribers: Arc<Mutex<Vec<mpsc::Sender<ServerNotification>>>>,
    ) {
        let mut receiver = client.subscribe().await;
        tokio::spawn(async move {
            while let Some(notification) = receiver.recv().await {
                let mut subscribers = subscribers.lock().await;
                subscribers.retain(|subscriber| subscriber.try_send(notification.clone()).is_ok());
            }
        });
    }

    async fn step_up_reconnect(
        &self,
        rejected: RejectedRequest,
    ) -> Result<(), crate::mcp::client::Error> {
        let params = self.params.read().await;
        let Some(auth) = params.auth.as_ref() else {
            return Err(crate::mcp::client::Error::McpError(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                "step-up authorization is not available",
                None,
            )));
        };
        let manager = auth
            .authorize(
                &params.uri,
                &params.name,
                params.static_oauth_client.as_ref(),
                Some(rejected),
            )
            .await
            .map_err(|e| {
                crate::mcp::client::Error::McpError(ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("step-up authorization failed: {e}"),
                    None,
                ))
            })?;
        let refused_tokens = RefusedTokens::default();
        let client = connect_with_auth(
            manager,
            &params.uri,
            &params.headers,
            params.ctx.clone(),
            refused_tokens.clone(),
        )
        .await
        .map_err(|e| {
            crate::mcp::client::Error::McpError(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                format!("reconnect after step-up authorization failed: {e}"),
                None,
            ))
        })?;
        Self::forward_notifications(&client, self.notification_subscribers.clone()).await;
        *self.inner.write().await = Connection {
            client,
            refused_tokens,
        };
        Ok(())
    }

    /// Run `op` against the current client; on an auth challenge, re-authorize
    /// and retry once.
    async fn with_step_up_retry<T, F>(&self, op: F) -> Result<T, crate::mcp::client::Error>
    where
        F: for<'a> Fn(
            &'a McpClient,
        ) -> Pin<
            Box<dyn std::future::Future<Output = Result<T, crate::mcp::client::Error>> + Send + 'a>,
        >,
    {
        let first = {
            let connection = self.inner.read().await;
            op(&connection.client).await
        };
        match first {
            Err(err) if auth_challenge_from_service_error(&err).is_some() => {
                let _step_up_guard = self.step_up_lock.lock().await;
                let (retry, refused_tokens) = {
                    let connection = self.inner.read().await;
                    let retry = op(&connection.client).await;
                    (retry, connection.refused_tokens.snapshot())
                };
                let retry_err = match retry {
                    Ok(value) => return Ok(value),
                    Err(retry_err) => retry_err,
                };
                let Some(challenge) = auth_challenge_from_service_error(&retry_err) else {
                    return Err(retry_err);
                };
                self.step_up_reconnect(RejectedRequest {
                    challenge: Some(challenge),
                    refused_tokens,
                })
                .await?;
                let connection = self.inner.read().await;
                op(&connection.client).await
            }
            result => result,
        }
    }
}

#[async_trait::async_trait]
impl McpClientTrait for OAuthStepUpClient {
    async fn list_tools(
        &self,
        session_id: &str,
        next_cursor: Option<String>,
        cancel_token: CancellationToken,
    ) -> Result<rmcp::model::ListToolsResult, crate::mcp::client::Error> {
        let session_id = session_id.to_string();
        self.with_step_up_retry(move |client| {
            let session_id = session_id.clone();
            let next_cursor = next_cursor.clone();
            let cancel_token = cancel_token.clone();
            Box::pin(async move {
                client
                    .list_tools(&session_id, next_cursor, cancel_token)
                    .await
            })
        })
        .await
    }

    async fn call_tool(
        &self,
        ctx: &ToolCallContext,
        name: &str,
        arguments: Option<rmcp::model::JsonObject>,
        cancel_token: CancellationToken,
    ) -> Result<CallToolResult, crate::mcp::client::Error> {
        let ctx = ctx.clone();
        let name = name.to_string();
        self.with_step_up_retry(move |client| {
            let ctx = ctx.clone();
            let name = name.clone();
            let arguments = arguments.clone();
            let cancel_token = cancel_token.clone();
            Box::pin(async move { client.call_tool(&ctx, &name, arguments, cancel_token).await })
        })
        .await
    }

    fn get_info(&self) -> Option<&ServerConfig> {
        self.server_info.as_ref()
    }

    async fn list_resources(
        &self,
        session_id: &str,
        next_cursor: Option<String>,
        cancel_token: CancellationToken,
    ) -> Result<rmcp::model::ListResourcesResult, crate::mcp::client::Error> {
        let session_id = session_id.to_string();
        self.with_step_up_retry(move |client| {
            let session_id = session_id.clone();
            let next_cursor = next_cursor.clone();
            let cancel_token = cancel_token.clone();
            Box::pin(async move {
                client
                    .list_resources(&session_id, next_cursor, cancel_token)
                    .await
            })
        })
        .await
    }

    async fn read_resource(
        &self,
        session_id: &str,
        uri: &str,
        cancel_token: CancellationToken,
    ) -> Result<rmcp::model::ReadResourceResult, crate::mcp::client::Error> {
        let session_id = session_id.to_string();
        let uri = uri.to_string();
        self.with_step_up_retry(move |client| {
            let session_id = session_id.clone();
            let uri = uri.clone();
            let cancel_token = cancel_token.clone();
            Box::pin(async move { client.read_resource(&session_id, &uri, cancel_token).await })
        })
        .await
    }

    async fn list_prompts(
        &self,
        session_id: &str,
        next_cursor: Option<String>,
        cancel_token: CancellationToken,
    ) -> Result<rmcp::model::ListPromptsResult, crate::mcp::client::Error> {
        let session_id = session_id.to_string();
        self.with_step_up_retry(move |client| {
            let session_id = session_id.clone();
            let next_cursor = next_cursor.clone();
            let cancel_token = cancel_token.clone();
            Box::pin(async move {
                client
                    .list_prompts(&session_id, next_cursor, cancel_token)
                    .await
            })
        })
        .await
    }

    async fn get_prompt(
        &self,
        session_id: &str,
        name: &str,
        arguments: Value,
        cancel_token: CancellationToken,
    ) -> Result<GetPromptResult, crate::mcp::client::Error> {
        let session_id = session_id.to_string();
        let name = name.to_string();
        self.with_step_up_retry(move |client| {
            let session_id = session_id.clone();
            let name = name.clone();
            let arguments = arguments.clone();
            let cancel_token = cancel_token.clone();
            Box::pin(async move {
                client
                    .get_prompt(&session_id, &name, arguments, cancel_token)
                    .await
            })
        })
        .await
    }

    async fn subscribe(&self) -> tokio::sync::mpsc::Receiver<rmcp::model::ServerNotification> {
        let (sender, receiver) = mpsc::channel(32);
        self.notification_subscribers.lock().await.push(sender);
        receiver
    }
}

pub(super) async fn connect(
    params: ConnectParams,
    socket: Option<&str>,
) -> ExtensionResult<Box<dyn McpClientTrait>> {
    #[cfg(unix)]
    if let Some(socket_path) = socket {
        return connect_over_unix_socket(&params, socket_path).await;
    }
    #[cfg(not(unix))]
    if socket.is_some() {
        return Err(ExtensionError::ConfigError(
            "Unix domain socket transport is not supported on this platform".to_string(),
        ));
    }

    let ConnectParams {
        uri,
        name,
        headers,
        static_oauth_client,
        auth,
        ctx,
    } = &params;
    let http_client = http_client(headers, ctx)?;
    let connect_unauthenticated = || {
        connect_with_legacy_retry(
            || {
                StreamableHttpClientTransport::with_client(
                    http_client.clone(),
                    StreamableHttpClientTransportConfig::with_uri(uri.as_str()),
                )
            },
            ctx.clone(),
        )
    };

    let Some(auth) = auth else {
        let client = connect_unauthenticated().await?;
        return Ok(Box::new(
            OAuthStepUpClient::new(client, RefusedTokens::default(), params).await,
        ));
    };

    // With stored OAuth credentials, connect with them directly. This avoids
    // an unnecessary 401 → browser re-auth cycle on every new session.
    let mut refused_stored_grant = None;
    let mut stored_grant_unusable = false;
    if auth
        .credential_store(name)
        .load()
        .await
        .is_ok_and(|c| c.is_some())
    {
        match auth
            .authorize(uri, name, static_oauth_client.as_ref(), None)
            .await
        {
            Ok(manager) => {
                let refused_tokens = RefusedTokens::default();
                match connect_with_auth(manager, uri, headers, ctx.clone(), refused_tokens.clone())
                    .await
                {
                    Ok(client) => {
                        return Ok(Box::new(
                            OAuthStepUpClient::new(client, refused_tokens, params).await,
                        ));
                    }
                    Err(ExtensionError::InitializeError(err)) if is_oauth_auth_failure(&err) => {
                        warn!("[OAuth:{}] Stored token was refused, re-authorizing", name);
                        let rejected = RejectedRequest {
                            challenge: auth_challenge_from_error(&err),
                            refused_tokens: refused_tokens.snapshot(),
                        };
                        refused_stored_grant =
                            Some((rejected, ExtensionError::InitializeError(err)));
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(e) => match e.downcast_ref::<StoredCredentialsError>() {
                Some(StoredCredentialsError::NeedAuthorization) => stored_grant_unusable = true,
                Some(StoredCredentialsError::AuthorizationServerFailing(_)) => {
                    return Err(ExtensionError::SetupError(format!("{e:#}")));
                }
                None => warn!(
                    "[OAuth:{}] Stored credentials could not be used: {}, falling back to unauthenticated attempt",
                    name, e
                ),
            },
        }
    }

    let (rejected, fallback) = match refused_stored_grant {
        Some((rejected, error)) => (rejected, Err(error)),
        None => match connect_unauthenticated().await {
            Err(err) if is_oauth_auth_failure(&err) => (
                RejectedRequest {
                    challenge: auth_challenge_from_error(&err),
                    refused_tokens: Vec::new(),
                },
                Err(ExtensionError::InitializeError(err)),
            ),
            // A server that also accepts anonymous clients never refuses this
            // connection, so the unusable grant is replaced now; otherwise the
            // session would quietly lose what the grant gave it access to.
            Ok(client) if stored_grant_unusable => (
                RejectedRequest {
                    challenge: None,
                    refused_tokens: Vec::new(),
                },
                Ok(client),
            ),
            client_res => {
                return Ok(Box::new(
                    OAuthStepUpClient::new(client_res?, RefusedTokens::default(), params).await,
                ));
            }
        },
    };

    match auth
        .authorize(uri, name, static_oauth_client.as_ref(), Some(rejected))
        .await
    {
        Ok(manager) => {
            let refused_tokens = RefusedTokens::default();
            let client =
                connect_with_auth(manager, uri, headers, ctx.clone(), refused_tokens.clone())
                    .await?;
            Ok(Box::new(
                OAuthStepUpClient::new(client, refused_tokens, params).await,
            ))
        }
        Err(e) => {
            warn!("[OAuth:{}] Authorization failed: {:#}", name, e);
            Ok(Box::new(
                OAuthStepUpClient::new(fallback?, RefusedTokens::default(), params).await,
            ))
        }
    }
}

#[cfg(unix)]
async fn connect_over_unix_socket(
    params: &ConnectParams,
    socket_path: &str,
) -> ExtensionResult<Box<dyn McpClientTrait>> {
    use rmcp::transport::UnixSocketHttpClient;

    let unix_client = UnixSocketHttpClient::new(socket_path, &params.uri);
    let custom_headers: HashMap<HeaderName, HeaderValue> =
        header_map(&params.headers, &params.ctx.client_version)?
            .into_iter()
            .filter_map(|(name, value)| name.map(|name| (name, value)))
            .collect();

    let client_res = connect_with_legacy_retry(
        || {
            StreamableHttpClientTransport::with_client(
                unix_client.clone(),
                StreamableHttpClientTransportConfig::with_uri(params.uri.as_str())
                    .custom_headers(custom_headers.clone()),
            )
        },
        params.ctx.clone(),
    )
    .await;

    if client_res
        .as_ref()
        .is_err_and(|error| is_oauth_auth_failure(error))
    {
        warn!(
            "Extension '{}' returned 401 over Unix domain socket transport; \
             OAuth is not supported for UDS connections",
            params.name,
        );
    }
    Ok(Box::new(client_res?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_required_manager::ActionRequiredManager;
    use crate::mcp::client::GooseMcpClientCapabilities;
    use goose_test_support::mcp::McpFixture;
    use rmcp::transport::auth::{CredentialStore, InMemoryCredentialStore};
    use std::time::Duration;
    use tempfile::tempdir;

    fn test_ctx(working_dir: &std::path::Path) -> ConnectContext {
        ConnectContext {
            timeout: Duration::from_secs(5),
            client_name: "goose-test".to_string(),
            client_version: "1.0.0".to_string(),
            capabilities: GooseMcpClientCapabilities {
                mcpui: false,
                host_info: None,
                elicitation_handler: None,
                protocol_version: None,
            },
            working_dir: working_dir.to_path_buf(),
            docker_container: None,
            action_required: Arc::new(ActionRequiredManager::new()),
            tools_version: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            trace_context: None,
        }
    }

    fn test_params(
        uri: &str,
        headers: HashMap<String, String>,
        working_dir: &std::path::Path,
    ) -> ConnectParams {
        ConnectParams {
            uri: uri.to_string(),
            name: "test-ext".to_string(),
            headers,
            static_oauth_client: None,
            auth: None,
            ctx: test_ctx(working_dir),
        }
    }

    fn transport_err(error: Box<dyn std::error::Error + Send + Sync>) -> ClientInitializeError {
        ClientInitializeError::TransportError {
            error: rmcp::transport::DynamicTransportError::from_parts(
                "test",
                std::any::TypeId::of::<()>(),
                error,
            ),
            context: "test context".into(),
        }
    }

    fn streamable_err(
        e: rmcp::transport::streamable_http_client::StreamableHttpError<reqwest::Error>,
    ) -> ClientInitializeError {
        transport_err(Box::new(e))
    }

    #[test]
    fn test_oauth_fallback_on_typed_auth_required() {
        let err = streamable_err(
            rmcp::transport::streamable_http_client::StreamableHttpError::AuthRequired(
                rmcp::transport::streamable_http_client::AuthRequiredError::new(
                    "Bearer realm=\"test\"".to_string(),
                ),
            ),
        );
        assert!(is_oauth_auth_failure(&err));
    }

    #[test]
    fn test_oauth_fallback_on_unexpected_response_http_401_prefix() {
        let err = streamable_err(
            rmcp::transport::streamable_http_client::StreamableHttpError::UnexpectedServerResponse(
                std::borrow::Cow::Borrowed("HTTP 401 Unauthorized"),
            ),
        );
        assert!(is_oauth_auth_failure(&err));
    }

    #[tokio::test]
    async fn test_invalid_header_name_returns_config_error() {
        let mut headers = HashMap::new();
        headers.insert("bad header name".to_string(), "value".to_string());

        let temp_dir = tempdir().unwrap();

        let result = connect(
            test_params("http://localhost:1", headers, temp_dir.path()),
            None,
        )
        .await;

        let Err(ExtensionError::ConfigError(msg)) = result else {
            panic!("expected ConfigError, got a different result");
        };
        assert!(
            msg.contains("invalid header"),
            "unexpected error message: {msg}"
        );
    }

    #[tokio::test]
    async fn test_invalid_header_value_returns_config_error() {
        let mut headers = HashMap::new();
        headers.insert("x-valid-name".to_string(), "bad\r\nvalue".to_string());

        let temp_dir = tempdir().unwrap();

        let result = connect(
            test_params("http://localhost:1", headers, temp_dir.path()),
            None,
        )
        .await;

        let Err(ExtensionError::ConfigError(msg)) = result else {
            panic!("expected ConfigError, got a different result");
        };
        assert!(
            msg.contains("invalid header value"),
            "unexpected error message: {msg}"
        );
    }

    #[tokio::test]
    async fn test_custom_headers_forwarded_to_http_extension() {
        use wiremock::matchers::any;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let mut headers = HashMap::new();
        headers.insert("x-api-key".to_string(), "test-secret-123".to_string());

        let temp_dir = tempdir().unwrap();

        // The MCP handshake will fail against the stub server. We only care that
        // the outgoing HTTP request carried the custom header.
        let _ = connect(
            test_params(&mock_server.uri(), headers, temp_dir.path()),
            None,
        )
        .await;

        let received = mock_server.received_requests().await.unwrap();
        assert!(
            !received.is_empty(),
            "expected at least one HTTP request to reach the mock server"
        );
        let header_found = received.iter().any(|req| {
            req.headers
                .get("x-api-key")
                .map(|v| v == "test-secret-123")
                .unwrap_or(false)
        });
        assert!(
            header_found,
            "custom header x-api-key was not forwarded to the extension server"
        );
    }

    /// Directly exercises `connect_with_auth`, which is the code path fixed by
    /// the PR (custom headers were dropped when the OAuth connection path was
    /// taken).  Uses a pre-seeded `InMemoryCredentialStore` with a fake,
    /// non-expiring token so `get_access_token()` returns immediately without
    /// touching any OAuth endpoints or the system keychain.
    #[tokio::test]
    async fn test_custom_headers_forwarded_oauth_path() {
        use rmcp::transport::auth::{OAuthTokenResponse, StoredCredentials};
        use wiremock::matchers::any;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let mut headers = HashMap::new();
        headers.insert("x-api-key".to_string(), "test-secret-oauth".to_string());

        // Build a fake, non-expiring token. token_received_at=None skips the
        // expiry check, so get_access_token() returns without any network call.
        let token_response: OAuthTokenResponse = serde_json::from_value(serde_json::json!({
            "access_token": "fake-test-token",
            "token_type": "bearer",
        }))
        .expect("valid fake token JSON");
        let creds = StoredCredentials::new(
            "test-client".to_string(),
            Some(token_response),
            vec![],
            None,
        );
        let store = InMemoryCredentialStore::new();
        store.save(creds).await.unwrap();

        let mut auth_manager = rmcp::transport::AuthorizationManager::new(mock_server.uri())
            .await
            .expect("AuthorizationManager::new should not make network calls");
        auth_manager.set_credential_store(store);

        let temp_dir = tempdir().unwrap();

        // connect_with_auth will fail (mock server isn't an MCP server) but we
        // only care that the outgoing request carried the custom header.
        let _ = connect_with_auth(
            auth_manager,
            &mock_server.uri(),
            &headers,
            test_ctx(temp_dir.path()),
            RefusedTokens::default(),
        )
        .await;

        let received = mock_server.received_requests().await.unwrap();
        assert!(
            !received.is_empty(),
            "expected at least one HTTP request to reach the mock server"
        );
        let header_found = received.iter().any(|req| {
            req.headers
                .get("x-api-key")
                .map(|v| v == "test-secret-oauth")
                .unwrap_or(false)
        });
        assert!(
            header_found,
            "custom header x-api-key was not forwarded through the OAuth connection path"
        );
    }

    /// Option-B verification (unauthenticated client): a server 3xx must be
    /// surfaced to the transport and the redirect target must never be contacted.
    #[tokio::test]
    async fn test_redirect_not_followed_unauthenticated() {
        use wiremock::matchers::any;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let target = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200))
            .mount(&target)
            .await;

        let redirector = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(307).insert_header("location", target.uri()))
            .mount(&redirector)
            .await;

        let temp_dir = tempdir().unwrap();
        let result = connect(
            test_params(&redirector.uri(), HashMap::new(), temp_dir.path()),
            None,
        )
        .await;

        // The 3xx is surfaced (connection fails); it is not silently followed.
        assert!(result.is_err(), "expected 3xx to be surfaced as an error");
        // The redirect target must never have been contacted.
        let target_requests = target.received_requests().await.unwrap();
        assert!(
            target_requests.is_empty(),
            "redirect target was contacted: {target_requests:?}"
        );
        // The redirector itself did receive the request.
        assert!(!redirector.received_requests().await.unwrap().is_empty());
    }

    /// Option-B verification (authenticated client): same guarantee on the
    /// `connect_with_auth` path, and the custom auth header is sent to the
    /// redirector — never forwarded to the redirect target.
    #[tokio::test]
    async fn test_redirect_not_followed_with_auth_headers() {
        use rmcp::transport::auth::{
            InMemoryCredentialStore, OAuthTokenResponse, StoredCredentials,
        };
        use wiremock::matchers::any;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let target = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200))
            .mount(&target)
            .await;

        let redirector = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(307).insert_header("location", target.uri()))
            .mount(&redirector)
            .await;

        let mut headers = HashMap::new();
        headers.insert("x-api-key".to_string(), "test-secret-redirect".to_string());

        let token_response: OAuthTokenResponse = serde_json::from_value(serde_json::json!({
            "access_token": "fake-test-token",
            "token_type": "bearer",
        }))
        .expect("valid fake token JSON");
        let creds = StoredCredentials::new(
            "test-client".to_string(),
            Some(token_response),
            vec![],
            None,
        );
        let store = InMemoryCredentialStore::new();
        store.save(creds).await.unwrap();

        let mut auth_manager = rmcp::transport::AuthorizationManager::new(redirector.uri())
            .await
            .expect("AuthorizationManager::new should not make network calls");
        auth_manager.set_credential_store(store);

        let temp_dir = tempdir().unwrap();
        let result = connect_with_auth(
            auth_manager,
            &redirector.uri(),
            &headers,
            test_ctx(temp_dir.path()),
            RefusedTokens::default(),
        )
        .await;

        assert!(result.is_err(), "expected 3xx to be surfaced as an error");
        let target_requests = target.received_requests().await.unwrap();
        assert!(
            target_requests.is_empty(),
            "redirect target was contacted: {target_requests:?}"
        );
        // The auth header reached the redirector only.
        let redirect_requests = redirector.received_requests().await.unwrap();
        assert!(!redirect_requests.is_empty());
        assert!(redirect_requests.iter().any(|req| {
            req.headers
                .get("x-api-key")
                .map(|v| v == "test-secret-redirect")
                .unwrap_or(false)
        }));
    }

    mod static_oauth_client {
        use super::*;

        #[test]
        fn absent_client_id_yields_no_static_client() {
            let resolved = resolve_static_oauth_client(None, None, &[], &HashMap::new()).unwrap();

            assert_eq!(resolved, None);
        }

        #[test]
        fn client_id_without_secret_resolves_public_client() {
            let resolved = resolve_static_oauth_client(
                Some("registered-client"),
                None,
                &["scope.read".to_string()],
                &HashMap::new(),
            )
            .unwrap()
            .unwrap();

            assert_eq!(resolved.client_id, "registered-client");
            assert_eq!(resolved.client_secret, None);
            assert_eq!(resolved.scopes, vec!["scope.read"]);
        }

        #[test]
        fn client_secret_resolves_from_envs() {
            let envs =
                HashMap::from([("OAUTH_CLIENT_SECRET".to_string(), "env-secret".to_string())]);

            let resolved = resolve_static_oauth_client(
                Some("registered-client"),
                Some("OAUTH_CLIENT_SECRET"),
                &[],
                &envs,
            )
            .unwrap()
            .unwrap();

            assert_eq!(resolved.client_secret.as_deref(), Some("env-secret"));
        }

        #[test]
        fn client_secret_key_without_client_id_is_rejected() {
            let error = resolve_static_oauth_client(
                None,
                Some("OAUTH_CLIENT_SECRET"),
                &[],
                &HashMap::new(),
            )
            .unwrap_err();

            assert!(error
                .to_string()
                .contains("client_secret_key requires client_id"));
        }

        #[test]
        fn scopes_without_client_id_are_rejected() {
            let error = resolve_static_oauth_client(
                None,
                None,
                &["scope.read".to_string()],
                &HashMap::new(),
            )
            .unwrap_err();

            assert!(error.to_string().contains("scopes requires client_id"));
        }

        #[test]
        fn missing_client_secret_key_is_an_error() {
            let error = resolve_static_oauth_client(
                Some("registered-client"),
                Some("MISSING_KEY"),
                &[],
                &HashMap::new(),
            )
            .unwrap_err();

            assert!(error.to_string().contains("MISSING_KEY"));
        }
    }

    /// Refuses one access token and requests without one; accepts any other.
    #[derive(Clone)]
    struct RefusesToken(&'static str);

    impl RefusesToken {
        fn answer(
            &self,
            auth_token: Option<&str>,
        ) -> Result<(), StreamableHttpError<std::io::Error>> {
            if auth_token.is_none_or(|token| token == self.0) {
                return Err(StreamableHttpError::AuthRequired(
                    rmcp::transport::streamable_http_client::AuthRequiredError::new(
                        r#"Bearer error="invalid_token""#.to_string(),
                    ),
                ));
            }
            Ok(())
        }
    }

    impl StreamableHttpClient for RefusesToken {
        type Error = std::io::Error;

        async fn post_message(
            &self,
            _uri: Arc<str>,
            _message: ClientJsonRpcMessage,
            _session_id: Option<Arc<str>>,
            auth_token: Option<String>,
            _custom_headers: HashMap<HeaderName, HeaderValue>,
        ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
            tokio::task::yield_now().await;
            self.answer(auth_token.as_deref())
                .map(|()| StreamableHttpPostResponse::Accepted)
        }

        async fn delete_session(
            &self,
            _uri: Arc<str>,
            _session_id: Arc<str>,
            auth_token: Option<String>,
            _custom_headers: HashMap<HeaderName, HeaderValue>,
        ) -> Result<(), StreamableHttpError<Self::Error>> {
            self.answer(auth_token.as_deref())
        }

        async fn get_stream(
            &self,
            _uri: Arc<str>,
            _session_id: Option<Arc<str>>,
            _last_event_id: Option<String>,
            auth_token: Option<String>,
            _custom_headers: HashMap<HeaderName, HeaderValue>,
        ) -> Result<BoxedSseResponse, StreamableHttpError<Self::Error>> {
            self.answer(auth_token.as_deref())?;
            Err(StreamableHttpError::ServerDoesNotSupportSse)
        }
    }

    fn initialized_notification() -> ClientJsonRpcMessage {
        serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn a_refusal_is_recorded_with_the_token_its_own_request_sent() {
        let refused = RefusedTokens::default();
        let client = RefusalRecordingClient {
            inner: RefusesToken("t1"),
            refused: refused.clone(),
        };
        let post = |token: Option<&str>| {
            client.post_message(
                Arc::from("https://mcp.example"),
                initialized_notification(),
                None,
                token.map(str::to_string),
                HashMap::new(),
            )
        };

        let (with_refused_token, with_successor, unauthenticated) =
            tokio::join!(post(Some("t1")), post(Some("t2")), post(None));

        assert!(with_refused_token.is_err());
        assert!(with_successor.is_ok());
        assert!(unauthenticated.is_err());
        assert_eq!(refused.snapshot(), ["t1"]);
    }

    #[test]
    fn refused_tokens_keep_the_most_recent_ones_once() {
        let refused = RefusedTokens::default();
        for index in 0..=RefusedTokens::KEPT {
            refused.record(format!("t{index}"));
        }
        refused.record(format!("t{}", RefusedTokens::KEPT));

        let tokens = refused.snapshot();
        assert_eq!(tokens.len(), RefusedTokens::KEPT);
        assert_eq!(tokens.first().map(String::as_str), Some("t1"));
        assert_eq!(tokens.last(), Some(&format!("t{}", RefusedTokens::KEPT)));
    }

    /// Without a refusal, reports why the stored credentials cannot be used.
    /// After one, authorizes with the stored token or fails like a closed
    /// browser would.
    struct ScriptedAuth {
        store: InMemoryCredentialStore,
        stored_credentials_error: fn() -> StoredCredentialsError,
        authorization_succeeds: bool,
        calls: std::sync::Mutex<Vec<&'static str>>,
    }

    impl ScriptedAuth {
        fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl ExtensionAuth for ScriptedAuth {
        fn credential_store(&self, _name: &str) -> Box<dyn CredentialStore> {
            Box::new(self.store.clone())
        }

        async fn authorize(
            &self,
            uri: &str,
            _name: &str,
            _static_client: Option<&StaticOAuthClientConfig>,
            rejected: Option<RejectedRequest>,
        ) -> anyhow::Result<rmcp::transport::AuthorizationManager> {
            if rejected.is_none() {
                self.calls.lock().unwrap().push("stored credentials");
                return Err((self.stored_credentials_error)().into());
            }
            self.calls.lock().unwrap().push("authorization");
            if !self.authorization_succeeds {
                anyhow::bail!("the browser was closed");
            }
            let mut manager = rmcp::transport::AuthorizationManager::new(uri).await?;
            manager.set_credential_store(self.store.clone());
            Ok(manager)
        }
    }

    async fn store_with_grant() -> InMemoryCredentialStore {
        use rmcp::transport::auth::{OAuthTokenResponse, StoredCredentials};

        let token_response: OAuthTokenResponse = serde_json::from_value(serde_json::json!({
            "access_token": "stored-token",
            "token_type": "bearer",
        }))
        .unwrap();
        let store = InMemoryCredentialStore::new();
        store
            .save(StoredCredentials::new(
                "test-client".to_string(),
                Some(token_response),
                vec![],
                None,
            ))
            .await
            .unwrap();
        store
    }

    async fn connect_to(fixture: &McpFixture, auth: &Arc<ScriptedAuth>) -> ExtensionResult<()> {
        let temp_dir = tempdir().unwrap();
        let mut params = test_params(&fixture.url, HashMap::new(), temp_dir.path());
        params.auth = Some(auth.clone());
        connect(params, None).await.map(|_client| ())
    }

    #[tokio::test]
    async fn an_unusable_stored_grant_is_reauthorized_when_the_server_accepts_anonymous_clients() {
        let fixture = McpFixture::new().await;
        let auth = Arc::new(ScriptedAuth {
            store: store_with_grant().await,
            stored_credentials_error: || StoredCredentialsError::NeedAuthorization,
            authorization_succeeds: true,
            calls: Default::default(),
        });

        connect_to(&fixture, &auth).await.unwrap();

        assert_eq!(auth.calls(), ["stored credentials", "authorization"]);
    }

    #[tokio::test]
    async fn the_anonymous_connection_is_kept_when_reauthorization_fails() {
        let fixture = McpFixture::new().await;
        let auth = Arc::new(ScriptedAuth {
            store: store_with_grant().await,
            stored_credentials_error: || StoredCredentialsError::NeedAuthorization,
            authorization_succeeds: false,
            calls: Default::default(),
        });

        connect_to(&fixture, &auth).await.unwrap();

        assert_eq!(auth.calls(), ["stored credentials", "authorization"]);
    }

    #[tokio::test]
    async fn a_failing_authorization_server_fails_the_connection_without_an_anonymous_attempt() {
        let fixture = McpFixture::new().await;
        let auth = Arc::new(ScriptedAuth {
            store: store_with_grant().await,
            stored_credentials_error: || {
                StoredCredentialsError::AuthorizationServerFailing(
                    "token endpoint returned 503".into(),
                )
            },
            authorization_succeeds: true,
            calls: Default::default(),
        });

        let error = connect_to(&fixture, &auth).await.unwrap_err();

        assert!(error.to_string().contains("token endpoint returned 503"));
        assert_eq!(auth.calls(), ["stored credentials"]);
        assert_eq!(fixture.request_count(), 0);
    }

    #[tokio::test]
    async fn without_stored_credentials_an_anonymous_server_is_used_without_authorizing() {
        let fixture = McpFixture::new().await;
        let auth = Arc::new(ScriptedAuth {
            store: InMemoryCredentialStore::new(),
            stored_credentials_error: || StoredCredentialsError::NeedAuthorization,
            authorization_succeeds: true,
            calls: Default::default(),
        });

        connect_to(&fixture, &auth).await.unwrap();

        assert!(auth.calls().is_empty());
    }
}
