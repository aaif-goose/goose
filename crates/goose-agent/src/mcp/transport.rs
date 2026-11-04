use super::Client;
use crate::operation::OperationFuture;
use anyhow::Result;
use async_trait::async_trait;
use rmcp::{
    model::*,
    service::{PeerRequestOptions, RunningService},
    transport::IntoTransport,
    ClientHandler, RoleClient, ServiceExt,
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
pub struct TransportClient {
    service: Arc<RunningService<RoleClient, Handler>>,
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
        let service = handler.serve(transport).await?;
        Ok(Self {
            service: Arc::new(service),
            notifications,
            timeout: options.timeout,
        })
    }

    async fn request(
        &self,
        request: ClientRequest,
        cancel: CancellationToken,
    ) -> Result<ServerResult> {
        let handle = self
            .service
            .peer()
            .send_cancellable_request(request, PeerRequestOptions::no_options())
            .await?;
        let peer = self.service.peer().clone();
        let request_id = handle.id.clone();
        let done = CancellationToken::new();
        let finished = done.clone();
        let guard = cancel.clone().drop_guard();
        // The watcher outlives a dropped caller future and sends protocol cancellation.
        let watcher = async move {
            tokio::select! {
                biased;
                _ = finished.cancelled() => {},
                _ = cancel.cancelled() => {
                    let _ = peer.notify_cancelled(CancelledNotificationParam::new(Some(request_id), None)).await;
                }
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(watcher);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(watcher);
        let result = tokio::time::timeout(self.timeout, handle.await_response()).await;
        match result {
            Ok(result) => {
                done.cancel();
                guard.disarm();
                result.map_err(anyhow::Error::from)
            }
            Err(error) => Err(error.into()),
        }
    }
}
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl Client for TransportClient {
    async fn list_tools(
        &self,
        cursor: Option<String>,
        cancel: CancellationToken,
    ) -> Result<ListToolsResult> {
        match self
            .request(
                ClientRequest::ListToolsRequest(RequestOptionalParam::with_param(
                    PaginatedRequestParams::default().with_cursor(cursor),
                )),
                cancel,
            )
            .await?
        {
            ServerResult::ListToolsResult(result) => Ok(result),
            _ => anyhow::bail!("unexpected tools response"),
        }
    }
    async fn call_tool(
        &self,
        _request_id: &str,
        call: CallToolRequestParams,
        cancel: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        match self
            .request(ClientRequest::CallToolRequest(Request::new(call)), cancel)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
        {
            ServerResult::CallToolResult(result) => Ok(result),
            _ => Err(ErrorData::internal_error("unexpected tool response", None)),
        }
    }
    async fn list_resources(
        &self,
        cursor: Option<String>,
        cancel: CancellationToken,
    ) -> Result<ListResourcesResult> {
        match self
            .request(
                ClientRequest::ListResourcesRequest(RequestOptionalParam::with_param(
                    PaginatedRequestParams::default().with_cursor(cursor),
                )),
                cancel,
            )
            .await?
        {
            ServerResult::ListResourcesResult(result) => Ok(result),
            _ => anyhow::bail!("unexpected resources response"),
        }
    }
    async fn read_resource(
        &self,
        uri: &str,
        cancel: CancellationToken,
    ) -> Result<ReadResourceResult> {
        match self
            .request(
                ClientRequest::ReadResourceRequest(Request::new(ReadResourceRequestParams::new(
                    uri,
                ))),
                cancel,
            )
            .await?
        {
            ServerResult::ReadResourceResult(result) => Ok(result),
            _ => anyhow::bail!("unexpected resource response"),
        }
    }
    async fn list_prompts(
        &self,
        cursor: Option<String>,
        cancel: CancellationToken,
    ) -> Result<ListPromptsResult> {
        match self
            .request(
                ClientRequest::ListPromptsRequest(RequestOptionalParam::with_param(
                    PaginatedRequestParams::default().with_cursor(cursor),
                )),
                cancel,
            )
            .await?
        {
            ServerResult::ListPromptsResult(result) => Ok(result),
            _ => anyhow::bail!("unexpected prompts response"),
        }
    }
    async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<JsonObject>,
        cancel: CancellationToken,
    ) -> Result<GetPromptResult> {
        let mut params = GetPromptRequestParams::new(name);
        params.arguments = arguments;
        match self
            .request(
                ClientRequest::GetPromptRequest(Request::new(params)),
                cancel,
            )
            .await?
        {
            ServerResult::GetPromptResult(result) => Ok(result),
            _ => anyhow::bail!("unexpected prompt response"),
        }
    }
    async fn instructions(&self) -> Option<String> {
        self.service
            .peer_info()
            .and_then(|info| info.instructions.clone())
    }
    fn subscribe(&self) -> Option<broadcast::Receiver<ServerNotification>> {
        Some(self.notifications.subscribe())
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
