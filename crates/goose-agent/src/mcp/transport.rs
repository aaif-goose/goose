use super::{CallContext, Client};
use crate::operation::OperationFuture;
use anyhow::Result;
use async_trait::async_trait;
use rmcp::{
    model::*,
    service::{PeerRequestOptions, RunningService},
    transport::IntoTransport,
    ClientHandler, ClientServiceExt, RoleClient,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

pub type ElicitationHandler = Arc<
    dyn Fn(
            ElicitRequestParams,
            CancellationToken,
        ) -> OperationFuture<'static, Result<ElicitResult, ErrorData>>
        + Send
        + Sync,
>;

/// All client behavior is explicit; there are no config, credential, or database lookups.
#[derive(Clone)]
#[allow(deprecated)]
pub struct ClientOptions {
    pub client_info: Implementation,
    pub roots: Vec<Root>,
    pub elicitation: Option<ElicitationHandler>,
    pub timeout: Duration,
}
impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            client_info: Implementation::new("gdk", env!("CARGO_PKG_VERSION")),
            roots: Vec::new(),
            elicitation: None,
            timeout: Duration::from_secs(300),
        }
    }
}

pub struct Handler {
    options: ClientOptions,
    notifications: broadcast::Sender<ServerNotification>,
}
impl ClientHandler for Handler {
    #[expect(deprecated)]
    fn get_info(&self) -> ClientConfig {
        let mut capabilities = ClientCapabilities::builder().enable_roots().build();
        if self.options.elicitation.is_some() {
            capabilities.elicitation = Some(Default::default());
        }
        ClientConfig::new(capabilities, self.options.client_info.clone())
    }
    #[expect(deprecated)]
    async fn list_roots(
        &self,
        _context: rmcp::service::RequestContext<RoleClient>,
    ) -> Result<ListRootsResult, ErrorData> {
        Ok(ListRootsResult::new(self.options.roots.clone()))
    }
    async fn create_elicitation(
        &self,
        request: ElicitRequestParams,
        context: rmcp::service::RequestContext<RoleClient>,
    ) -> Result<ElicitResult, ErrorData> {
        match &self.options.elicitation {
            Some(handler) => handler(request, context.ct).await,
            None => Err(ErrorData::invalid_request(
                "elicitation is not supported",
                None,
            )),
        }
    }
    async fn on_tool_list_changed(&self, _context: rmcp::service::NotificationContext<RoleClient>) {
        let _ = self
            .notifications
            .send(ServerNotification::ToolListChangedNotification(
                Default::default(),
            ));
    }
    async fn on_progress(
        &self,
        params: ProgressNotificationParam,
        _context: rmcp::service::NotificationContext<RoleClient>,
    ) {
        let _ = self
            .notifications
            .send(ServerNotification::ProgressNotification(Notification::new(
                params,
            )));
    }
    #[expect(deprecated)]
    async fn on_logging_message(
        &self,
        params: LoggingMessageNotificationParam,
        _context: rmcp::service::NotificationContext<RoleClient>,
    ) {
        let _ = self
            .notifications
            .send(ServerNotification::LoggingMessageNotification(
                Notification::new(params),
            ));
    }
}

/// An initialized rmcp service; dropping its last owner closes the transport.
pub struct TransportClient<H: ClientHandler = Handler> {
    server_info: Option<InitializeResult>,
    service: Arc<RunningService<RoleClient, H>>,
    notifications: broadcast::Sender<ServerNotification>,
    timeout: Duration,
}
impl TransportClient {
    pub async fn connect<T, E, A>(transport: T, options: ClientOptions) -> Result<Self>
    where
        T: IntoTransport<RoleClient, E, A>,
        E: std::error::Error + From<std::io::Error> + Send + Sync + 'static,
    {
        let (notifications, _) = broadcast::channel(64);
        let handler = Handler {
            options: options.clone(),
            notifications: notifications.clone(),
        };
        let service =
            Self::connect_handler(transport, handler, options.timeout, None, notifications).await?;
        Ok(service)
    }
}

impl<H: ClientHandler> TransportClient<H> {
    pub async fn connect_handler<T, E, A>(
        transport: T,
        handler: H,
        timeout: Duration,
        lifecycle: Option<rmcp::service::ClientLifecycleMode>,
        notifications: broadcast::Sender<ServerNotification>,
    ) -> Result<Self, Box<rmcp::service::ClientInitializeError>>
    where
        T: IntoTransport<RoleClient, E, A>,
        E: std::error::Error + From<std::io::Error> + Send + Sync + 'static,
    {
        let startup = handler.serve_with_lifecycle(
            transport,
            lifecycle.unwrap_or(rmcp::service::ClientLifecycleMode::Initialize),
        );
        tokio::pin!(startup);
        let service = tokio::select! {
            result = &mut startup => result?,
            _ = super::request::sleep(timeout) => return Err(Box::new(rmcp::service::ClientInitializeError::Cancelled)),
        };
        let server_info = service.peer_info().map(|info| {
            let mut result = InitializeResult::new(info.capabilities.clone())
                .with_protocol_version(info.protocol_version.clone());
            if let Some(server_info) = &info.server_info {
                result = result.with_server_info(server_info.clone());
            }
            result.instructions = info.instructions.clone();
            result.meta = info.meta.clone();
            result
        });
        Ok(Self {
            server_info,
            service: Arc::new(service),
            notifications,
            timeout,
        })
    }

    pub fn service(&self) -> &Arc<RunningService<RoleClient, H>> {
        &self.service
    }
    pub fn server_info(&self) -> Option<&InitializeResult> {
        self.server_info.as_ref()
    }
    pub async fn request(
        &self,
        request: ClientRequest,
        cancel: CancellationToken,
    ) -> Result<ServerResult, rmcp::ServiceError> {
        let handle = self
            .service
            .peer()
            .send_cancellable_request(request, PeerRequestOptions::no_options())
            .await?;
        super::await_response(handle, self.timeout, &cancel).await
    }
}

/// Transport boundary: hosts decorate requests and handle authentication without
/// reimplementing tools, resources, or prompts protocol dispatch.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait RequestSender<C: goose_provider_types::maybe_send::MaybeSync>:
    goose_provider_types::maybe_send::MaybeSend + goose_provider_types::maybe_send::MaybeSync
{
    fn scope<'a>(&self, context: &'a C) -> &'a str;
    fn info(&self) -> Option<&InitializeResult>;
    async fn send(
        &self,
        scope: &str,
        context: Option<&C>,
        request: ClientRequest,
        cancel: CancellationToken,
    ) -> Result<ServerResult, rmcp::ServiceError>;
    async fn notifications(&self) -> tokio::sync::mpsc::Receiver<ServerNotification>;
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl RequestSender<CallContext> for TransportClient {
    fn scope<'a>(&self, context: &'a CallContext) -> &'a str {
        &context.scope
    }
    fn info(&self) -> Option<&InitializeResult> {
        self.server_info.as_ref()
    }
    async fn send(
        &self,
        _scope: &str,
        _context: Option<&CallContext>,
        request: ClientRequest,
        cancel: CancellationToken,
    ) -> Result<ServerResult, rmcp::ServiceError> {
        self.request(request, cancel).await
    }
    async fn notifications(&self) -> tokio::sync::mpsc::Receiver<ServerNotification> {
        let mut source = self.notifications.subscribe();
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let forward = async move {
            loop {
                match source.recv().await {
                    Ok(notification) => {
                        if tx.send(notification).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(forward);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(forward);
        rx
    }
}
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<C, T> Client<C> for T
where
    C: goose_provider_types::maybe_send::MaybeSync,
    T: RequestSender<C>,
{
    async fn list_tools(
        &self,
        scope: &str,
        cursor: Option<String>,
        cancel: CancellationToken,
    ) -> Result<ListToolsResult, rmcp::ServiceError> {
        match self
            .send(
                scope,
                None,
                ClientRequest::ListToolsRequest(RequestOptionalParam::with_param(
                    PaginatedRequestParams::default().with_cursor(cursor),
                )),
                cancel,
            )
            .await?
        {
            ServerResult::ListToolsResult(result) => Ok(result),
            _ => Err(rmcp::ServiceError::UnexpectedResponse),
        }
    }
    async fn call_tool(
        &self,
        context: &C,
        name: &str,
        arguments: Option<JsonObject>,
        cancel: CancellationToken,
    ) -> Result<CallToolResult, rmcp::ServiceError> {
        match self
            .send(
                self.scope(context),
                Some(context),
                ClientRequest::CallToolRequest(Request::new(
                    CallToolRequestParams::new(name.to_string())
                        .with_arguments(arguments.unwrap_or_default()),
                )),
                cancel,
            )
            .await?
        {
            ServerResult::CallToolResult(result) => Ok(result),
            _ => Err(rmcp::ServiceError::UnexpectedResponse),
        }
    }
    async fn list_resources(
        &self,
        scope: &str,
        cursor: Option<String>,
        cancel: CancellationToken,
    ) -> Result<ListResourcesResult, rmcp::ServiceError> {
        match self
            .send(
                scope,
                None,
                ClientRequest::ListResourcesRequest(RequestOptionalParam::with_param(
                    PaginatedRequestParams::default().with_cursor(cursor),
                )),
                cancel,
            )
            .await?
        {
            ServerResult::ListResourcesResult(result) => Ok(result),
            _ => Err(rmcp::ServiceError::UnexpectedResponse),
        }
    }
    async fn read_resource(
        &self,
        scope: &str,
        uri: &str,
        cancel: CancellationToken,
    ) -> Result<ReadResourceResult, rmcp::ServiceError> {
        match self
            .send(
                scope,
                None,
                ClientRequest::ReadResourceRequest(Request::new(ReadResourceRequestParams::new(
                    uri,
                ))),
                cancel,
            )
            .await?
        {
            ServerResult::ReadResourceResult(result) => Ok(result),
            _ => Err(rmcp::ServiceError::UnexpectedResponse),
        }
    }
    async fn list_prompts(
        &self,
        scope: &str,
        cursor: Option<String>,
        cancel: CancellationToken,
    ) -> Result<ListPromptsResult, rmcp::ServiceError> {
        match self
            .send(
                scope,
                None,
                ClientRequest::ListPromptsRequest(RequestOptionalParam::with_param(
                    PaginatedRequestParams::default().with_cursor(cursor),
                )),
                cancel,
            )
            .await?
        {
            ServerResult::ListPromptsResult(result) => Ok(result),
            _ => Err(rmcp::ServiceError::UnexpectedResponse),
        }
    }
    async fn get_prompt(
        &self,
        scope: &str,
        name: &str,
        arguments: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<GetPromptResult, rmcp::ServiceError> {
        let mut params = GetPromptRequestParams::new(name);
        params.arguments = arguments.as_object().cloned();
        match self
            .send(
                scope,
                None,
                ClientRequest::GetPromptRequest(Request::new(params)),
                cancel,
            )
            .await?
        {
            ServerResult::GetPromptResult(result) => Ok(result),
            _ => Err(rmcp::ServiceError::UnexpectedResponse),
        }
    }
    fn get_info(&self) -> Option<&InitializeResult> {
        self.info()
    }
    async fn subscribe(&self) -> tokio::sync::mpsc::Receiver<ServerNotification> {
        self.notifications().await
    }
}

#[cfg(all(not(target_arch = "wasm32"), feature = "mcp-stdio"))]
impl TransportClient {
    /// The host constructs the command, including environment, cwd and launch policy.
    pub async fn stdio(command: tokio::process::Command, options: ClientOptions) -> Result<Self> {
        Self::connect(rmcp::transport::TokioChildProcess::new(command)?, options).await
    }
}

#[cfg(all(not(target_arch = "wasm32"), feature = "mcp-http"))]
impl TransportClient {
    /// Supply a configured HTTP client for headers, TLS and authentication policy.
    pub async fn http(
        client: reqwest::Client,
        config: rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig,
        options: ClientOptions,
    ) -> Result<Self> {
        Self::connect(
            rmcp::transport::StreamableHttpClientTransport::with_client(client, config),
            options,
        )
        .await
    }
}
