use crate::action_required_manager::ActionRequiredManager;
use crate::agents::tool_execution::ToolCallContext;
use crate::agents::types::SharedProvider;
use crate::session_context::{SESSION_ID_HEADER, WORKING_DIR_HEADER};
use rmcp::model::{
    CreateElicitationRequestParams, CreateElicitationResult, ElicitationAction, ErrorCode,
    ExtensionCapabilities, Extensions, JsonObject, ListRootsResult, LoggingMessageNotification,
    Meta, Root, SamplingMessageContent,
};
/// MCP client implementation for Goose
use rmcp::{
    model::{
        CallToolRequestParams, CallToolResult, CancelledNotificationParam, ClientCapabilities,
        ClientInfo, ClientRequest, CreateMessageRequestParams, CreateMessageResult,
        GetPromptRequestParams, GetPromptResult, Implementation, InitializeRequestParams,
        InitializeResult, ListPromptsResult, ListResourcesResult, ListToolsResult, Notification,
        PaginatedRequestParams, ProtocolVersion, ReadResourceRequestParams, ReadResourceResult,
        Request, RequestId, RequestOptionalParam, Role, SamplingMessage, ServerNotification,
        ServerResult,
    },
    service::{
        ClientInitializeError, PeerRequestOptions, RequestContext, RequestHandle, RunningService,
        ServiceRole,
    },
    transport::IntoTransport,
    ClientHandler, ErrorData, Peer, RoleClient, ServiceError, ServiceExt,
};
use serde_json::Value;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{
    mpsc::{self, Sender},
    Mutex,
};
use tokio_util::sync::CancellationToken;

pub type BoxError = Box<dyn std::error::Error + Sync + Send>;

pub type Error = rmcp::ServiceError;

#[async_trait::async_trait]
pub trait McpClientTrait: Send + Sync {
    async fn list_tools(
        &self,
        session_id: &str,
        next_cursor: Option<String>,
        cancel_token: CancellationToken,
    ) -> Result<ListToolsResult, Error>;

    async fn call_tool(
        &self,
        ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        cancel_token: CancellationToken,
    ) -> Result<CallToolResult, Error>;

    fn get_info(&self) -> Option<&InitializeResult>;

    async fn list_resources(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Result<ListResourcesResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn read_resource(
        &self,
        _session_id: &str,
        _uri: &str,
        _cancel_token: CancellationToken,
    ) -> Result<ReadResourceResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn list_prompts(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancel_token: CancellationToken,
    ) -> Result<ListPromptsResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn get_prompt(
        &self,
        _session_id: &str,
        _name: &str,
        _arguments: Value,
        _cancel_token: CancellationToken,
    ) -> Result<GetPromptResult, Error> {
        Err(Error::TransportClosed)
    }

    async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
        mpsc::channel(1).1
    }

    async fn get_moim(&self, _session_id: &str) -> Option<String> {
        None
    }

    async fn update_working_dir(&self, _new_dir: PathBuf) -> Result<(), Error> {
        Ok(())
    }
}

pub struct GooseClient {
    notification_handlers: Arc<Mutex<Vec<Sender<ServerNotification>>>>,
    provider: SharedProvider,
    session_id: Mutex<Option<String>>,
    client_name: String,
    capabilities: GooseMcpClientCapabilities,
    working_dir: Arc<tokio::sync::RwLock<PathBuf>>,
}

impl GooseClient {
    pub fn new(
        handlers: Arc<Mutex<Vec<Sender<ServerNotification>>>>,
        provider: SharedProvider,
        client_name: String,
        capabilities: GooseMcpClientCapabilities,
        working_dir: PathBuf,
    ) -> Self {
        GooseClient {
            notification_handlers: handlers,
            provider,
            session_id: Mutex::new(None),
            client_name,
            capabilities,
            working_dir: Arc::new(tokio::sync::RwLock::new(working_dir)),
        }
    }

    pub fn shared_working_dir(&self) -> Arc<tokio::sync::RwLock<PathBuf>> {
        self.working_dir.clone()
    }

    async fn set_session_id(&self, session_id: &str) {
        let mut slot = self.session_id.lock().await;
        assert!(
            slot.as_deref().is_none_or(|s| s == session_id),
            "McpClient received requests from different sessions"
        );
        *slot = Some(session_id.to_string());
    }

    async fn current_session_id(&self) -> Option<String> {
        self.session_id.lock().await.clone()
    }

    async fn resolve_session_id(&self, extensions: &Extensions) -> Option<String> {
        // Prefer explicit MCP metadata, then the active request scope.
        let current_session_id = self.current_session_id().await;
        Self::session_id_from_extensions(extensions).or(current_session_id)
    }

    fn session_id_from_extensions(extensions: &Extensions) -> Option<String> {
        let meta = extensions.get::<Meta>()?;
        meta.0
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(SESSION_ID_HEADER))
            .and_then(|(_, value)| value.as_str())
            .map(|value| value.to_string())
    }
}

fn working_dir_roots(dir: &std::path::Path) -> ListRootsResult {
    let uri = url::Url::from_file_path(dir)
        .map(|u| u.to_string())
        .unwrap_or_else(|()| format!("file://{}", dir.display()));
    ListRootsResult::new(vec![Root::new(uri).with_name("working_directory")])
}

impl ClientHandler for GooseClient {
    async fn list_roots(
        &self,
        _context: RequestContext<RoleClient>,
    ) -> Result<ListRootsResult, ErrorData> {
        Ok(working_dir_roots(&self.working_dir.read().await))
    }

    async fn on_progress(
        &self,
        params: rmcp::model::ProgressNotificationParam,
        context: rmcp::service::NotificationContext<rmcp::RoleClient>,
    ) {
        self.notification_handlers
            .lock()
            .await
            .iter()
            .for_each(|handler| {
                let mut not = Notification::new(params.clone());
                not.extensions = context.extensions.clone();
                let _ = handler.try_send(ServerNotification::ProgressNotification(not));
            });
    }

    async fn on_logging_message(
        &self,
        params: rmcp::model::LoggingMessageNotificationParam,
        context: rmcp::service::NotificationContext<rmcp::RoleClient>,
    ) {
        self.notification_handlers
            .lock()
            .await
            .iter()
            .for_each(|handler| {
                let mut notification = LoggingMessageNotification::new(params.clone());
                notification.extensions = context.extensions.clone();
                let _ =
                    handler.try_send(ServerNotification::LoggingMessageNotification(notification));
            });
    }

    async fn create_message(
        &self,
        params: CreateMessageRequestParams,
        context: RequestContext<RoleClient>,
    ) -> Result<CreateMessageResult, ErrorData> {
        let provider = self
            .provider
            .lock()
            .await
            .as_ref()
            .ok_or(ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                "Could not use provider",
                None,
            ))?
            .clone();

        // Prefer explicit MCP metadata, then the active request scope.
        let session_id = self.resolve_session_id(&context.extensions).await;

        let provider_ready_messages: Vec<crate::conversation::message::Message> = params
            .messages
            .iter()
            .map(|msg| {
                let base = match msg.role {
                    Role::User => crate::conversation::message::Message::user(),
                    Role::Assistant => crate::conversation::message::Message::assistant(),
                };

                match msg.content.first().and_then(|c| c.as_text()) {
                    Some(text) => base.with_text(&text.text),
                    None => base,
                }
            })
            .collect();

        let system_prompt = params
            .system_prompt
            .as_deref()
            .unwrap_or("You are a general-purpose AI agent called goose");

        let model_config = provider.get_model_config();
        let (response, usage) = provider
            .complete(
                &model_config,
                session_id.as_deref().unwrap_or(""),
                system_prompt,
                &provider_ready_messages,
                &[],
            )
            .await
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    "Unexpected error while completing the prompt",
                    Some(Value::from(e.to_string())),
                )
            })?;

        Ok(CreateMessageResult::new(
            SamplingMessage::new(
                Role::Assistant,
                if let Some(content) = response.content.first() {
                    match content {
                        crate::conversation::message::MessageContent::Text(text) => {
                            SamplingMessageContent::text(&text.text)
                        }
                        crate::conversation::message::MessageContent::Image(img) => {
                            SamplingMessageContent::Image(rmcp::model::RawImageContent {
                                data: img.data.clone(),
                                mime_type: img.mime_type.clone(),
                                meta: None,
                            })
                        }
                        _ => SamplingMessageContent::text(""),
                    }
                } else {
                    SamplingMessageContent::text("")
                },
            ),
            usage.model,
        )
        .with_stop_reason(CreateMessageResult::STOP_REASON_END_TURN))
    }

    async fn create_elicitation(
        &self,
        request: CreateElicitationRequestParams,
        _context: RequestContext<RoleClient>,
    ) -> Result<CreateElicitationResult, ErrorData> {
        let (message, schema_value) = match &request {
            CreateElicitationRequestParams::FormElicitationParams {
                message,
                requested_schema,
                ..
            } => {
                let schema_value = serde_json::to_value(requested_schema).map_err(|e| {
                    ErrorData::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to serialize elicitation schema: {}", e),
                        None,
                    )
                })?;
                (message.clone(), schema_value)
            }
            CreateElicitationRequestParams::UrlElicitationParams { message, url, .. } => {
                (message.clone(), serde_json::json!({ "url": url }))
            }
        };

        ActionRequiredManager::global()
            .request_and_wait(message, schema_value, Duration::from_secs(300))
            .await
            .map(|user_data| {
                CreateElicitationResult::new(ElicitationAction::Accept).with_content(user_data)
            })
            .map_err(|e| {
                ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    format!("Elicitation request timed out or failed: {}", e),
                    None,
                )
            })
    }

    fn get_info(&self) -> ClientInfo {
        let mut extensions = ExtensionCapabilities::new();

        if self.capabilities.mcpui {
            // Build MCP Apps UI extension capability
            // See: https://github.com/modelcontextprotocol/ext-apps/blob/main/specification/2026-01-26/apps.mdx
            let mut ui_extension_settings = JsonObject::new();
            ui_extension_settings.insert(
                "mimeTypes".to_string(),
                serde_json::json!(["text/html;profile=mcp-app"]),
            );
            extensions.insert(
                "io.modelcontextprotocol/ui".to_string(),
                ui_extension_settings,
            );
        }

        InitializeRequestParams::new(
            ClientCapabilities::builder()
                .enable_roots()
                .enable_extensions_with(extensions)
                .enable_sampling()
                .enable_elicitation()
                .build(),
            Implementation::new(
                self.client_name.clone(),
                std::env::var("GOOSE_MCP_CLIENT_VERSION")
                    .unwrap_or(env!("CARGO_PKG_VERSION").to_owned()),
            ),
        )
        .with_protocol_version(ProtocolVersion::V_2025_03_26)
    }
}

#[derive(Debug, Clone)]
pub struct GooseMcpClientCapabilities {
    pub mcpui: bool,
}

/// The MCP client is the interface for MCP operations.
pub struct McpClient {
    client: Mutex<RunningService<RoleClient, GooseClient>>,
    notification_subscribers: Arc<Mutex<Vec<mpsc::Sender<ServerNotification>>>>,
    server_info: Option<InitializeResult>,
    timeout: std::time::Duration,
    docker_container: Option<String>,
}

impl McpClient {
    pub async fn connect<T, E, A>(
        transport: T,
        timeout: std::time::Duration,
        provider: SharedProvider,
        client_name: String,
        capabilities: GooseMcpClientCapabilities,
        working_dir: PathBuf,
    ) -> Result<Self, ClientInitializeError>
    where
        T: IntoTransport<RoleClient, E, A>,
        E: std::error::Error + From<std::io::Error> + Send + Sync + 'static,
    {
        Self::connect_with_container(
            transport,
            timeout,
            provider,
            None,
            client_name,
            capabilities,
            working_dir,
        )
        .await
    }

    pub async fn connect_with_container<T, E, A>(
        transport: T,
        timeout: std::time::Duration,
        provider: SharedProvider,
        docker_container: Option<String>,
        client_name: String,
        capabilities: GooseMcpClientCapabilities,
        working_dir: PathBuf,
    ) -> Result<Self, ClientInitializeError>
    where
        T: IntoTransport<RoleClient, E, A>,
        E: std::error::Error + From<std::io::Error> + Send + Sync + 'static,
    {
        let notification_subscribers =
            Arc::new(Mutex::new(Vec::<mpsc::Sender<ServerNotification>>::new()));

        let client = GooseClient::new(
            notification_subscribers.clone(),
            provider,
            client_name.clone(),
            capabilities.clone(),
            working_dir,
        );
        let client: rmcp::service::RunningService<rmcp::RoleClient, GooseClient> =
            client.serve(transport).await?;
        let server_info = client.peer_info().cloned();

        Ok(Self {
            client: Mutex::new(client),
            notification_subscribers,
            server_info,
            timeout,
            docker_container,
        })
    }

    pub fn docker_container(&self) -> Option<&str> {
        self.docker_container.as_deref()
    }

    async fn do_update_working_dir(&self, new_dir: PathBuf) -> Result<(), Error> {
        let client = self.client.lock().await;
        let shared = client.service().shared_working_dir();
        *shared.write().await = new_dir;
        client.peer().notify_roots_list_changed().await?;
        Ok(())
    }

    async fn send_request_with_context(
        &self,
        session_id: &str,
        working_dir: Option<&str>,
        request: ClientRequest,
        cancel_token: CancellationToken,
    ) -> Result<ServerResult, Error> {
        let request = inject_session_context_into_request(request, Some(session_id), working_dir);
        // The inner mutex is held only for the send; the actual response wait
        // happens outside the lock so concurrent calls can overlap.
        let handle = {
            let client = self.client.lock().await;
            client.service().set_session_id(session_id).await;
            client
                .send_cancellable_request(request, PeerRequestOptions::no_options())
                .await
        }?;

        await_response(handle, self.timeout, &cancel_token).await
    }
}

async fn await_response(
    handle: RequestHandle<RoleClient>,
    timeout: Duration,
    cancel_token: &CancellationToken,
) -> Result<<RoleClient as ServiceRole>::PeerResp, ServiceError> {
    let receiver = handle.rx;
    let peer = handle.peer;
    let request_id = handle.id;
    tokio::select! {
        result = receiver => {
            result.map_err(|_e| ServiceError::TransportClosed)?
        }
        _ = tokio::time::sleep(timeout) => {
            send_cancel_message(&peer, request_id, Some("timed out".to_owned())).await?;
            Err(ServiceError::Timeout{timeout})
        }
        _ = cancel_token.cancelled() => {
            send_cancel_message(&peer, request_id, Some("operation cancelled".to_owned())).await?;
            Err(ServiceError::Cancelled { reason: None })
        }
    }
}

async fn send_cancel_message(
    peer: &Peer<RoleClient>,
    request_id: RequestId,
    reason: Option<String>,
) -> Result<(), ServiceError> {
    peer.send_notification(
        Notification::new(CancelledNotificationParam { request_id, reason }).into(),
    )
    .await
}

#[async_trait::async_trait]
impl McpClientTrait for McpClient {
    fn get_info(&self) -> Option<&InitializeResult> {
        self.server_info.as_ref()
    }

    async fn list_resources(
        &self,
        session_id: &str,
        cursor: Option<String>,
        cancel_token: CancellationToken,
    ) -> Result<ListResourcesResult, Error> {
        let res = self
            .send_request_with_context(
                session_id,
                None,
                ClientRequest::ListResourcesRequest(RequestOptionalParam::with_param(
                    PaginatedRequestParams::default().with_cursor(cursor),
                )),
                cancel_token,
            )
            .await?;

        match res {
            ServerResult::ListResourcesResult(result) => Ok(result),
            _ => Err(ServiceError::UnexpectedResponse),
        }
    }

    async fn read_resource(
        &self,
        session_id: &str,
        uri: &str,
        cancel_token: CancellationToken,
    ) -> Result<ReadResourceResult, Error> {
        let res = self
            .send_request_with_context(
                session_id,
                None,
                ClientRequest::ReadResourceRequest(Request::new(ReadResourceRequestParams::new(
                    uri.to_string(),
                ))),
                cancel_token,
            )
            .await?;

        match res {
            ServerResult::ReadResourceResult(result) => Ok(result),
            _ => Err(ServiceError::UnexpectedResponse),
        }
    }

    async fn list_tools(
        &self,
        session_id: &str,
        cursor: Option<String>,
        cancel_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        let res = self
            .send_request_with_context(
                session_id,
                None,
                ClientRequest::ListToolsRequest(RequestOptionalParam::with_param(
                    PaginatedRequestParams::default().with_cursor(cursor),
                )),
                cancel_token,
            )
            .await?;

        match res {
            ServerResult::ListToolsResult(result) => Ok(result),
            _ => Err(ServiceError::UnexpectedResponse),
        }
    }

    async fn call_tool(
        &self,
        ctx: &ToolCallContext,
        name: &str,
        arguments: Option<JsonObject>,
        cancel_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let mut params = CallToolRequestParams::new(name.to_string());
        if let Some(args) = arguments {
            params = params.with_arguments(args);
        }
        let request = ClientRequest::CallToolRequest(Request::new(params));

        let result = self
            .send_request_with_context(
                &ctx.session_id,
                ctx.working_dir_str(),
                request,
                cancel_token,
            )
            .await;

        match result? {
            ServerResult::CallToolResult(result) => Ok(result),
            _ => Err(ServiceError::UnexpectedResponse),
        }
    }

    async fn list_prompts(
        &self,
        session_id: &str,
        cursor: Option<String>,
        cancel_token: CancellationToken,
    ) -> Result<ListPromptsResult, Error> {
        let res = self
            .send_request_with_context(
                session_id,
                None,
                ClientRequest::ListPromptsRequest(RequestOptionalParam::with_param(
                    PaginatedRequestParams::default().with_cursor(cursor),
                )),
                cancel_token,
            )
            .await?;

        match res {
            ServerResult::ListPromptsResult(result) => Ok(result),
            _ => Err(ServiceError::UnexpectedResponse),
        }
    }

    async fn get_prompt(
        &self,
        session_id: &str,
        name: &str,
        arguments: Value,
        cancel_token: CancellationToken,
    ) -> Result<GetPromptResult, Error> {
        let arguments = match arguments {
            Value::Object(map) => Some(map),
            _ => None,
        };
        let mut params = GetPromptRequestParams::new(name.to_string());
        if let Some(args) = arguments {
            params = params.with_arguments(args);
        }
        let res = self
            .send_request_with_context(
                session_id,
                None,
                ClientRequest::GetPromptRequest(Request::new(params)),
                cancel_token,
            )
            .await?;

        match res {
            ServerResult::GetPromptResult(result) => Ok(result),
            _ => Err(ServiceError::UnexpectedResponse),
        }
    }

    async fn subscribe(&self) -> mpsc::Receiver<ServerNotification> {
        let (tx, rx) = mpsc::channel(16);
        self.notification_subscribers.lock().await.push(tx);
        rx
    }

    async fn update_working_dir(&self, new_dir: PathBuf) -> Result<(), Error> {
        self.do_update_working_dir(new_dir).await
    }
}

/// Injects the given session_id and working_dir into Extensions._meta.
/// None (or empty) removes any existing values.
fn inject_session_context_into_extensions(
    mut extensions: Extensions,
    session_id: Option<&str>,
    working_dir: Option<&str>,
) -> Extensions {
    let session_id = session_id.filter(|id| !id.is_empty());
    let working_dir = working_dir.filter(|dir| !dir.is_empty());
    let mut meta_map = extensions
        .get::<Meta>()
        .map(|meta| meta.0.clone())
        .unwrap_or_default();

    // JsonObject is case-sensitive, so we use retain for case-insensitive removal
    meta_map.retain(|k, _| {
        !k.eq_ignore_ascii_case(SESSION_ID_HEADER) && !k.eq_ignore_ascii_case(WORKING_DIR_HEADER)
    });

    if let Some(session_id) = session_id {
        meta_map.insert(
            SESSION_ID_HEADER.to_string(),
            Value::String(session_id.to_string()),
        );
    }

    if let Some(working_dir) = working_dir {
        meta_map.insert(
            WORKING_DIR_HEADER.to_string(),
            Value::String(working_dir.to_string()),
        );
    }

    extensions.insert(Meta(meta_map));
    extensions
}

fn inject_session_context_into_request(
    request: ClientRequest,
    session_id: Option<&str>,
    working_dir: Option<&str>,
) -> ClientRequest {
    match request {
        ClientRequest::ListResourcesRequest(mut req) => {
            req.extensions =
                inject_session_context_into_extensions(req.extensions, session_id, working_dir);
            ClientRequest::ListResourcesRequest(req)
        }
        ClientRequest::ReadResourceRequest(mut req) => {
            req.extensions =
                inject_session_context_into_extensions(req.extensions, session_id, working_dir);
            ClientRequest::ReadResourceRequest(req)
        }
        ClientRequest::ListToolsRequest(mut req) => {
            req.extensions =
                inject_session_context_into_extensions(req.extensions, session_id, working_dir);
            ClientRequest::ListToolsRequest(req)
        }
        ClientRequest::CallToolRequest(mut req) => {
            req.extensions =
                inject_session_context_into_extensions(req.extensions, session_id, working_dir);
            ClientRequest::CallToolRequest(req)
        }
        ClientRequest::ListPromptsRequest(mut req) => {
            req.extensions =
                inject_session_context_into_extensions(req.extensions, session_id, working_dir);
            ClientRequest::ListPromptsRequest(req)
        }
        ClientRequest::GetPromptRequest(mut req) => {
            req.extensions =
                inject_session_context_into_extensions(req.extensions, session_id, working_dir);
            ClientRequest::GetPromptRequest(req)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::GoosePlatform;
    use serde_json::json;
    use test_case::test_case;

    fn new_client(platform: GoosePlatform) -> GooseClient {
        let capabilities = match platform {
            GoosePlatform::GooseDesktop => GooseMcpClientCapabilities { mcpui: true },
            GoosePlatform::GooseCli => GooseMcpClientCapabilities { mcpui: false },
        };

        GooseClient::new(
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(None)),
            platform.to_string(),
            capabilities,
            std::env::current_dir().unwrap_or_default(),
        )
    }

    fn request_extensions(request: &ClientRequest) -> Option<&Extensions> {
        match request {
            ClientRequest::ListResourcesRequest(req) => Some(&req.extensions),
            ClientRequest::ReadResourceRequest(req) => Some(&req.extensions),
            ClientRequest::ListToolsRequest(req) => Some(&req.extensions),
            ClientRequest::CallToolRequest(req) => Some(&req.extensions),
            ClientRequest::ListPromptsRequest(req) => Some(&req.extensions),
            ClientRequest::GetPromptRequest(req) => Some(&req.extensions),
            _ => None,
        }
    }

    fn list_resources_request(extensions: Extensions) -> ClientRequest {
        let mut req = RequestOptionalParam::with_param(PaginatedRequestParams::default());
        req.extensions = extensions;
        ClientRequest::ListResourcesRequest(req)
    }

    fn read_resource_request(extensions: Extensions) -> ClientRequest {
        let mut req = Request::new(ReadResourceRequestParams::new(
            "test://resource".to_string(),
        ));
        req.extensions = extensions;
        ClientRequest::ReadResourceRequest(req)
    }

    fn list_tools_request(extensions: Extensions) -> ClientRequest {
        let mut req = RequestOptionalParam::with_param(PaginatedRequestParams::default());
        req.extensions = extensions;
        ClientRequest::ListToolsRequest(req)
    }

    fn call_tool_request(extensions: Extensions) -> ClientRequest {
        let mut req = Request::new(CallToolRequestParams::new("tool".to_string()));
        req.extensions = extensions;
        ClientRequest::CallToolRequest(req)
    }

    fn list_prompts_request(extensions: Extensions) -> ClientRequest {
        let mut req = RequestOptionalParam::with_param(PaginatedRequestParams::default());
        req.extensions = extensions;
        ClientRequest::ListPromptsRequest(req)
    }

    fn get_prompt_request(extensions: Extensions) -> ClientRequest {
        let mut req = Request::new(GetPromptRequestParams::new("prompt".to_string()));
        req.extensions = extensions;
        ClientRequest::GetPromptRequest(req)
    }

    #[test_case(
        Some("ext-session"),
        Some("current-session"),
        Some("ext-session");
        "extensions win"
    )]
    #[test_case(
        None,
        Some("current-session"),
        Some("current-session");
        "current when no extensions"
    )]
    #[test_case(
        None,
        None,
        None;
        "no session when no extensions or current"
    )]
    fn test_resolve_session_id(
        ext_session: Option<&str>,
        current_session: Option<&str>,
        expected: Option<&str>,
    ) {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let client = new_client(GoosePlatform::GooseCli);
            if let Some(session_id) = current_session {
                client.set_session_id(session_id).await;
            }

            let extensions =
                inject_session_context_into_extensions(Extensions::new(), ext_session, None);

            let resolved = client.resolve_session_id(&extensions).await;

            let expected = expected.map(str::to_string);
            assert_eq!(resolved, expected);
        });
    }

    #[test_case(list_resources_request; "list_resources")]
    #[test_case(read_resource_request; "read_resource")]
    #[test_case(list_tools_request; "list_tools")]
    #[test_case(call_tool_request; "call_tool")]
    #[test_case(list_prompts_request; "list_prompts")]
    #[test_case(get_prompt_request; "get_prompt")]
    fn test_request_injects_session(request_builder: fn(Extensions) -> ClientRequest) {
        let session_id = "test-session-id";
        let mut extensions = Extensions::new();
        extensions.insert(
            serde_json::from_value::<Meta>(json!({
                "Goose-Session-Id": "old-session-id",
                "other-key": "preserve-me"
            }))
            .unwrap(),
        );

        let request = request_builder(extensions);
        let request = inject_session_context_into_request(request, Some(session_id), None);
        let extensions = request_extensions(&request).expect("request should have extensions");
        let meta = extensions
            .get::<Meta>()
            .expect("extensions should contain meta");

        assert_eq!(
            meta.0.get(SESSION_ID_HEADER),
            Some(&Value::String(session_id.to_string()))
        );
        assert_eq!(
            meta.0.get("other-key"),
            Some(&Value::String("preserve-me".to_string()))
        );
    }

    #[test]
    fn test_session_id_in_mcp_meta() {
        let session_id = "test-session-789";
        let extensions =
            inject_session_context_into_extensions(Default::default(), Some(session_id), None);
        let mcp_meta = extensions.get::<Meta>().unwrap();

        assert_eq!(
            &mcp_meta.0,
            json!({
                SESSION_ID_HEADER: session_id
            })
            .as_object()
            .unwrap()
        );
    }

    #[test_case(
        Some("new-session-id"),
        json!({
            SESSION_ID_HEADER: "new-session-id",
            "other-key": "preserve-me"
        });
        "replace"
    )]
    #[test_case(
        None,
        json!({
            "other-key": "preserve-me"
        });
        "remove"
    )]
    #[test_case(
        Some(""),
        json!({
            "other-key": "preserve-me"
        });
        "empty removes"
    )]
    fn test_session_id_case_insensitive_replacement(
        session_id: Option<&str>,
        expected_meta: serde_json::Value,
    ) {
        use rmcp::model::Extensions;
        use serde_json::from_value;

        let mut extensions = Extensions::new();
        extensions.insert(
            from_value::<Meta>(json!({
                SESSION_ID_HEADER: "old-session-1",
                "Agent-Session-Id": "old-session-2",
                "other-key": "preserve-me"
            }))
            .unwrap(),
        );

        let extensions = inject_session_context_into_extensions(extensions, session_id, None);
        let mcp_meta = extensions.get::<Meta>().unwrap();

        assert_eq!(&mcp_meta.0, expected_meta.as_object().unwrap());
    }

    #[test]
    fn test_client_info_advertises_mcp_apps_ui_extension() {
        let client = new_client(GoosePlatform::GooseDesktop);
        let info = ClientHandler::get_info(&client);

        // Verify the client advertises the MCP Apps UI extension capability
        let extensions = info
            .capabilities
            .extensions
            .expect("capabilities should have extensions");

        let ui_ext = extensions
            .get("io.modelcontextprotocol/ui")
            .expect("should have io.modelcontextprotocol/ui extension");

        let mime_types = ui_ext
            .get("mimeTypes")
            .expect("ui extension should have mimeTypes");

        assert_eq!(mime_types, &json!(["text/html;profile=mcp-app"]));
    }

    #[test]
    fn test_client_capabilities_advertise_roots() {
        let client = new_client(GoosePlatform::GooseCli);
        let info = ClientHandler::get_info(&client);
        assert!(
            info.capabilities.roots.is_some(),
            "client should advertise roots capability"
        );
    }

    #[test]
    fn test_working_dir_roots_returns_current_dir_as_root() {
        let dir = PathBuf::from("/tmp/test-project");
        let result = working_dir_roots(&dir);
        assert_eq!(result.roots.len(), 1);
        assert_eq!(result.roots[0].uri, "file:///tmp/test-project");
        assert_eq!(result.roots[0].name.as_deref(), Some("working_directory"));
    }
}
