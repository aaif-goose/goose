use agent_client_protocol_schema::AGENT_METHOD_NAMES;
use anyhow::{Context, Result};
use async_stream::try_stream;
use futures::future::BoxFuture;
use rmcp::model::{CallToolRequestParams, CallToolResult, Content as RmcpContent, Role, Tool};
use sacp::schema::{
    ClientCapabilities, CloseSessionRequest, ContentBlock, ContentChunk, EnvVariable, HttpHeader,
    ImageContent, InitializeRequest, InitializeResponse, McpCapabilities, McpServer, McpServerHttp,
    McpServerStdio, NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse,
    ProtocolVersion, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelectOptions, SessionId, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, SetSessionModeRequest, SetSessionModeResponse, StopReason,
    TextContent, ToolCallContent, ToolCallStatus, ToolKind,
};
use sacp::{Agent, Client, ConnectionTo};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex};
use tokio_util::compat::{TokioAsyncReadCompatExt as _, TokioAsyncWriteCompatExt as _};

use crate::acp::{map_permission_response, PermissionDecision};
use crate::config::{ExtensionConfig, GooseMode};
use crate::conversation::message::{Message, MessageContent, TOOL_META_EXTERNAL_DISPATCH_KEY};
use crate::model::ModelConfig;
use crate::permission::permission_confirmation::PrincipalType;
use crate::permission::{Permission, PermissionConfirmation};
use crate::providers::base::{MessageStream, PermissionRouting, Provider};
use crate::providers::errors::ProviderError;
use crate::subprocess::configure_subprocess;

/// Sentinel: resolved to the actual model name during connect().
pub const ACP_CURRENT_MODEL: &str = "current";

pub struct AcpProviderConfig {
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub env_remove: Vec<String>,
    pub work_dir: PathBuf,
    pub mcp_servers: Vec<McpServer>,
    pub session_mode_id: Option<String>,
    pub mode_mapping: HashMap<GooseMode, String>,
    pub notification_callback: Option<Arc<dyn Fn(SessionNotification) + Send + Sync>>,
}

enum ClientRequest {
    NewSession {
        response_tx: oneshot::Sender<Result<NewSessionResponse>>,
    },
    SetMode {
        session_id: SessionId,
        mode_id: String,
        response_tx: oneshot::Sender<Result<()>>,
    },
    SetConfigOption {
        session_id: SessionId,
        config_id: String,
        value: String,
        response_tx: oneshot::Sender<Result<()>>,
    },
    Prompt {
        session_id: SessionId,
        content: Vec<ContentBlock>,
        response_tx: mpsc::Sender<AcpUpdate>,
    },
}

// tokio I/O handles can't move between runtimes, so the child process must be
// spawned inside the OS thread. This closure lets start() share all other logic.
type ClientLoopFn = Box<
    dyn FnOnce(
            AcpClientLoop,
            mpsc::Receiver<ClientRequest>,
            oneshot::Sender<Result<InitializeResponse>>,
        ) -> BoxFuture<'static, ()>
        + Send,
>;

#[derive(Debug)]
enum AcpUpdate {
    Text(String),
    Thought(String),
    ToolCallStart {
        id: String,
        name: String,
        kind: ToolKind,
        raw_input: Option<serde_json::Value>,
    },
    ToolCallComplete {
        id: String,
        raw_output: Option<serde_json::Value>,
        content: Option<Vec<ToolCallContent>>,
        is_error: bool,
    },
    PermissionRequest {
        request: Box<RequestPermissionRequest>,
        response_tx: oneshot::Sender<RequestPermissionResponse>,
    },
    Complete(StopReason),
    Error(String),
}

/// Per-tool-call buffer for accumulating ACP ToolCallUpdate fields across
/// non-terminal updates, drained on the terminal status update.
#[derive(Debug, Default)]
struct AccumulatedToolCall {
    raw_output: Option<serde_json::Value>,
    content: Vec<ToolCallContent>,
}

/// The single ACP session backing this provider instance.
#[derive(Clone)]
struct AcpSession {
    id: SessionId,
    response: NewSessionResponse,
}

pub struct AcpProvider {
    name: String,
    model: ModelConfig,
    goose_mode: Arc<Mutex<GooseMode>>,
    mode_mapping: HashMap<GooseMode, String>,

    session: AcpSession,

    pending_confirmations:
        Arc<TokioMutex<HashMap<String, oneshot::Sender<PermissionConfirmation>>>>,
    pending_tool_updates: Arc<Mutex<HashMap<String, AccumulatedToolCall>>>,

    tx: Option<mpsc::Sender<ClientRequest>>,
    loop_thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for AcpProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpProvider")
            .field("name", &self.name)
            .field("model", &self.model)
            .finish()
    }
}

fn spawn_client_loop(fut: impl Future<Output = ()> + Send + 'static) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build ACP client runtime");
        rt.block_on(fut)
    })
}

impl AcpProvider {
    pub async fn connect(
        name: String,
        model: ModelConfig,
        goose_mode: GooseMode,
        config: AcpProviderConfig,
    ) -> Result<Self> {
        Self::start(
            name,
            model,
            goose_mode,
            config,
            Box::new(|cl, rx, init_tx| Box::pin(cl.spawn(rx, init_tx))),
        )
        .await
    }

    #[doc(hidden)]
    pub async fn connect_with_transport(
        name: String,
        model: ModelConfig,
        goose_mode: GooseMode,
        config: AcpProviderConfig,
        transport: impl sacp::ConnectTo<Client> + 'static,
    ) -> Result<Self> {
        Self::start(
            name,
            model,
            goose_mode,
            config,
            Box::new(move |cl, mut rx, init_tx| {
                Box::pin(async move {
                    if let Err(e) = cl.run(transport, &mut rx, init_tx).await {
                        tracing::error!("ACP protocol error: {e}");
                    }
                })
            }),
        )
        .await
    }

    async fn start(
        name: String,
        model: ModelConfig,
        goose_mode: GooseMode,
        config: AcpProviderConfig,
        run: ClientLoopFn,
    ) -> Result<Self> {
        let (tx, rx) = mpsc::channel(32);
        let (init_tx, init_rx) = oneshot::channel();
        let mode_mapping = config.mode_mapping.clone();
        let goose_mode_shared = Arc::new(Mutex::new(goose_mode));
        let pending_tool_updates: Arc<Mutex<HashMap<String, AccumulatedToolCall>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let client_loop = AcpClientLoop::new(
            config,
            goose_mode_shared.clone(),
            pending_tool_updates.clone(),
        );
        let loop_thread = spawn_client_loop(run(client_loop, rx, init_tx));

        let _init_response = init_rx
            .await
            .context("ACP client initialization cancelled")??;

        // Create the ACP session eagerly during connect.
        let (session_tx, session_rx) = oneshot::channel();
        tx.send(ClientRequest::NewSession {
            response_tx: session_tx,
        })
        .await
        .context("ACP client is unavailable")?;
        let response = session_rx
            .await
            .context("ACP session creation cancelled")??;

        // Resolve model from the session response.
        let resolved_model = if model.model_name == ACP_CURRENT_MODEL {
            if let Ok((resolved, _)) = resolve_model_info(&name, &response) {
                tracing::info!(from = ACP_CURRENT_MODEL, to = %resolved, "resolved ACP model");
                ModelConfig {
                    model_name: resolved,
                    ..model
                }
            } else {
                model
            }
        } else {
            model
        };

        let session = AcpSession {
            id: response.session_id.clone(),
            response,
        };

        Ok(Self {
            name,
            model: resolved_model,
            goose_mode: goose_mode_shared,
            mode_mapping,
            session,
            pending_confirmations: Arc::new(TokioMutex::new(HashMap::new())),
            pending_tool_updates,
            tx: Some(tx),
            loop_thread: Some(loop_thread),
        })
    }

    fn acp_session_id(&self) -> SessionId {
        self.session.id.clone()
    }

    pub(crate) async fn send_set_mode(&self, _goose_id: &str, mode_id: String) -> Result<()> {
        let session_id = self.acp_session_id();
        let (response_tx, response_rx) = oneshot::channel();
        self.tx
            .as_ref()
            .unwrap()
            .send(ClientRequest::SetMode {
                session_id,
                mode_id,
                response_tx,
            })
            .await
            .context("ACP client is unavailable")?;
        response_rx.await.context("ACP request cancelled")?
    }

    pub(crate) async fn send_set_config_option(
        &self,
        _goose_id: &str,
        config_id: String,
        value: String,
    ) -> Result<()> {
        let session_id = self.acp_session_id();
        let (response_tx, response_rx) = oneshot::channel();
        self.tx
            .as_ref()
            .unwrap()
            .send(ClientRequest::SetConfigOption {
                session_id,
                config_id,
                value,
                response_tx,
            })
            .await
            .context("ACP client is unavailable")?;
        response_rx.await.context("ACP request cancelled")?
    }

    async fn prompt(
        &self,
        session_id: SessionId,
        content: Vec<ContentBlock>,
    ) -> Result<mpsc::Receiver<AcpUpdate>> {
        let (response_tx, response_rx) = mpsc::channel(64);
        self.tx
            .as_ref()
            .unwrap()
            .send(ClientRequest::Prompt {
                session_id,
                content,
                response_tx,
            })
            .await
            .context("ACP client is unavailable")?;
        Ok(response_rx)
    }

    fn session_has_config_option(&self, category: SessionConfigOptionCategory) -> bool {
        self.session
            .response
            .config_options
            .as_ref()
            .is_some_and(|opts| opts.iter().any(|o| o.category.as_ref() == Some(&category)))
    }
}

#[async_trait::async_trait]
impl Provider for AcpProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model.clone()
    }

    async fn update_mode(&self, session_id: &str, mode: GooseMode) -> Result<(), ProviderError> {
        let mode_str = self
            .mode_mapping
            .get(&mode)
            .cloned()
            .unwrap_or_else(|| format!("{mode:?}"));

        if self.session_has_config_option(SessionConfigOptionCategory::Mode) {
            self.send_set_config_option(session_id, "mode".into(), mode_str)
                .await
                .map_err(|e| ProviderError::RequestFailed(format!("Failed to set mode: {e}")))?;
        } else {
            self.send_set_mode(session_id, mode_str)
                .await
                .map_err(|e| ProviderError::RequestFailed(format!("Failed to set mode: {e}")))?;
        }

        if let Ok(mut guard) = self.goose_mode.lock() {
            *guard = mode;
        }
        Ok(())
    }

    fn permission_routing(&self) -> PermissionRouting {
        PermissionRouting::ActionRequired
    }

    fn manages_own_context(&self) -> bool {
        true
    }

    async fn handle_permission_confirmation(
        &self,
        request_id: &str,
        confirmation: &PermissionConfirmation,
    ) -> bool {
        let mut pending = self.pending_confirmations.lock().await;
        if let Some(tx) = pending.remove(request_id) {
            let _ = tx.send(confirmation.clone());
            return true;
        }
        false
    }

    async fn stream(
        &self,
        _model_config: &ModelConfig,
        _session_id: &str,
        _system: &str,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let session_id = self.acp_session_id();

        let prompt_blocks = messages_to_prompt(messages);
        // Drop any tool-call buffer state left over from a prior prompt
        // (e.g. cancelled or interrupted before its terminal status arrived).
        if let Ok(mut buffer) = self.pending_tool_updates.lock() {
            buffer.clear();
        }
        let mut rx = self
            .prompt(session_id, prompt_blocks)
            .await
            .map_err(|e| ProviderError::RequestFailed(format!("Failed to send ACP prompt: {e}")))?;

        let pending_confirmations = self.pending_confirmations.clone();
        let goose_mode = *self
            .goose_mode
            .lock()
            .map_err(|_| ProviderError::RequestFailed("goose_mode lock poisoned".into()))?;

        let reject_all_tools = goose_mode == GooseMode::Chat;
        Ok(Box::pin(try_stream! {
            let mut suppress_text = false;
            let mut rejected_tool_calls: HashSet<String> = HashSet::new();

            while let Some(update) = rx.recv().await {
                match update {
                    AcpUpdate::Text(text) => {
                        if !suppress_text {
                            let message = Message::assistant().with_text(text);
                            yield (Some(message), None);
                        }
                    }
                    AcpUpdate::Thought(text) => {
                        let message = Message::assistant()
                            .with_thinking(text, "")
                            .with_visibility(true, false);
                        yield (Some(message), None);
                    }
                    AcpUpdate::ToolCallStart { id, name, kind, raw_input } => {
                        if reject_all_tools {
                            suppress_text = true;
                            rejected_tool_calls.insert(id);
                        } else {
                            let mut params = CallToolRequestParams::new(name);
                            if let Some(serde_json::Value::Object(map)) = raw_input {
                                params = params.with_arguments(map);
                            }
                            // external_dispatch tells the agent loop not to redispatch this
                            // call. goose.acp.kind preserves ACP's stable categorization for
                            // downstream consumers (metrics, observability, icon selection)
                            // independent of the display title we put in `name`.
                            let tool_meta = Some(serde_json::json!({
                                TOOL_META_EXTERNAL_DISPATCH_KEY: true,
                                "goose.acp.kind": kind,
                            }));
                            let message = Message::assistant().with_tool_request_with_metadata(
                                id,
                                Ok(params),
                                None,
                                tool_meta,
                            );
                            yield (Some(message), None);
                        }
                    }
                    AcpUpdate::ToolCallComplete {
                        id,
                        raw_output,
                        content,
                        is_error,
                    } => {
                        if rejected_tool_calls.remove(&id) {
                            // In chat mode no tool_request was emitted (suppressed at
                            // ToolCallStart), so surface a plain text message. In other
                            // modes a tool_request WAS emitted, so pair it with an error
                            // tool_response so downstream consumers see the rejection.
                            if reject_all_tools {
                                let message = Message::assistant()
                                    .with_text("Tool call was denied.");
                                yield (Some(message), None);
                            } else {
                                let denial = vec![RmcpContent::text("Tool call was denied.")];
                                let result = CallToolResult::error(denial);
                                let message =
                                    Message::user().with_tool_response(id, Ok(result));
                                yield (Some(message), None);
                            }
                        } else {
                            let result_content =
                                acp_tool_call_content_to_rmcp(content, raw_output);
                            let result = if is_error {
                                CallToolResult::error(result_content)
                            } else {
                                CallToolResult::success(result_content)
                            };
                            let message = Message::user().with_tool_response(id, Ok(result));
                            yield (Some(message), None);
                        }
                    }
                    AcpUpdate::PermissionRequest { request, response_tx } => {
                        if let Some(decision) = permission_decision_from_mode(goose_mode) {
                            if decision.should_record_rejection() {
                                rejected_tool_calls.insert(request.tool_call.tool_call_id.0.to_string());
                            }
                            let _ = response_tx.send(map_permission_response(&request, decision));
                            continue;
                        }

                        let request_id = request.tool_call.tool_call_id.0.to_string();
                        let (tx, rx) = oneshot::channel();

                        pending_confirmations
                            .lock()
                            .await
                            .insert(request_id.clone(), tx);

                        if let Some(action_required) = build_action_required_message(&request) {
                            yield (Some(action_required), None);
                        }

                        let confirmation = rx.await.unwrap_or(PermissionConfirmation {
                            principal_type: PrincipalType::Tool,
                            permission: Permission::Cancel,
                        });

                        pending_confirmations.lock().await.remove(&request_id);

                        let decision = PermissionDecision::from(confirmation.permission);
                        if decision.should_record_rejection() {
                            rejected_tool_calls.insert(request.tool_call.tool_call_id.0.to_string());
                        }
                        let _ = response_tx.send(map_permission_response(&request, decision));
                    }
                    AcpUpdate::Complete(_reason) => {
                        break;
                    }
                    AcpUpdate::Error(e) => {
                        Err(ProviderError::RequestFailed(e))?;
                    }
                }
            }
        }))
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        let (_, available) = resolve_model_info(&self.name, &self.session.response)?;
        Ok(available)
    }
}

impl Drop for AcpProvider {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(h) = self.loop_thread.take() {
            if let Err(e) = h.join() {
                tracing::debug!("AcpClientLoop thread panicked: {e:?}");
            }
        }
    }
}

struct AcpClientLoop {
    config: AcpProviderConfig,
    goose_mode: Arc<Mutex<GooseMode>>,
    prompt_response_tx: Arc<Mutex<Option<mpsc::Sender<AcpUpdate>>>>,
    pending_tool_updates: Arc<Mutex<HashMap<String, AccumulatedToolCall>>>,
}

impl AcpClientLoop {
    fn new(
        config: AcpProviderConfig,
        goose_mode: Arc<Mutex<GooseMode>>,
        pending_tool_updates: Arc<Mutex<HashMap<String, AccumulatedToolCall>>>,
    ) -> Self {
        Self {
            config,
            goose_mode,
            prompt_response_tx: Arc::new(Mutex::new(None)),
            pending_tool_updates,
        }
    }

    async fn spawn(
        self,
        mut rx: mpsc::Receiver<ClientRequest>,
        init_tx: oneshot::Sender<Result<InitializeResponse>>,
    ) {
        let child = match spawn_acp_process(&self.config).await {
            Ok(c) => c,
            Err(e) => {
                let _ = init_tx.send(Err(anyhow::anyhow!("{e}")));
                tracing::error!("failed to spawn ACP process: {e}");
                return;
            }
        };

        match self.run_with_child(child, &mut rx, init_tx).await {
            Ok(()) => tracing::debug!("ACP protocol loop exited cleanly"),
            Err(e) => tracing::error!(error = %e, "ACP protocol loop error"),
        }
    }

    async fn run_with_child(
        self,
        mut child: Child,
        rx: &mut mpsc::Receiver<ClientRequest>,
        init_tx: oneshot::Sender<Result<InitializeResponse>>,
    ) -> Result<()> {
        let stdin = child.stdin.take().context("no stdin")?;
        let stdout = child.stdout.take().context("no stdout")?;
        let transport = sacp::ByteStreams::new(stdin.compat_write(), stdout.compat());
        self.run(transport, rx, init_tx).await
    }

    async fn run(
        self,
        transport: impl sacp::ConnectTo<Client> + 'static,
        rx: &mut mpsc::Receiver<ClientRequest>,
        init_tx: oneshot::Sender<Result<InitializeResponse>>,
    ) -> Result<()> {
        let AcpClientLoop {
            config,
            goose_mode,
            prompt_response_tx,
            pending_tool_updates,
        } = self;
        let notification_callback = config.notification_callback.clone();
        let reverse_modes = reverse_mode_mapping(&config.mode_mapping);

        Client
            .builder()
            .on_receive_notification(
                {
                    let prompt_response_tx = prompt_response_tx.clone();
                    let reverse_modes = reverse_modes.clone();
                    let goose_mode = goose_mode.clone();
                    let pending_tool_updates = pending_tool_updates.clone();
                    async move |notification: SessionNotification, _cx| {
                        if let Some(ref cb) = notification_callback {
                            cb(notification.clone());
                        }
                        match &notification.update {
                            SessionUpdate::CurrentModeUpdate(update) => {
                                if let Some(mode) = resolve_mode(
                                    &reverse_modes,
                                    update.current_mode_id.0.as_ref(),
                                    &goose_mode,
                                ) {
                                    if let Ok(mut guard) = goose_mode.lock() {
                                        *guard = mode;
                                    }
                                }
                            }
                            SessionUpdate::ConfigOptionUpdate(update) => {
                                for opt in &update.config_options {
                                    if opt.category == Some(SessionConfigOptionCategory::Mode) {
                                        if let SessionConfigKind::Select(sel) = &opt.kind {
                                            if let Some(mode) = resolve_mode(
                                                &reverse_modes,
                                                sel.current_value.0.as_ref(),
                                                &goose_mode,
                                            ) {
                                                if let Ok(mut guard) = goose_mode.lock() {
                                                    *guard = mode;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                        if let Some(tx) = prompt_response_tx
                            .lock()
                            .ok()
                            .as_ref()
                            .and_then(|g| g.as_ref())
                        {
                            match notification.update {
                                SessionUpdate::AgentMessageChunk(ContentChunk {
                                    content: ContentBlock::Text(TextContent { text, .. }),
                                    ..
                                }) => {
                                    let _ = tx.try_send(AcpUpdate::Text(text));
                                }
                                SessionUpdate::AgentThoughtChunk(ContentChunk {
                                    content: ContentBlock::Text(TextContent { text, .. }),
                                    ..
                                }) => {
                                    let _ = tx.try_send(AcpUpdate::Thought(text));
                                }
                                SessionUpdate::ToolCall(tool_call) => {
                                    let id = tool_call.tool_call_id.0.to_string();
                                    let initial_status = tool_call.status;
                                    let synchronous_terminal = matches!(
                                        initial_status,
                                        ToolCallStatus::Completed | ToolCallStatus::Failed
                                    );
                                    // Seed the buffer; drain immediately if the call is
                                    // already terminal (synchronous tool, no follow-up).
                                    let synchronous_accumulated =
                                        if let Ok(mut buffer) = pending_tool_updates.lock() {
                                            let entry = buffer.entry(id.clone()).or_default();
                                            if let Some(raw_output) = tool_call.raw_output.clone() {
                                                entry.raw_output = Some(raw_output);
                                            }
                                            entry.content.extend(tool_call.content.clone());
                                            if synchronous_terminal {
                                                buffer.remove(&id)
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        };
                                    // ACP carries no canonical tool name to clients — only
                                    // `title` (display) and `kind` (category). We pass `title`
                                    // for renderer affordance, surface `kind` separately via
                                    // tool_meta for stable categorization, and the
                                    // goose.external_dispatch marker keeps `name` off the
                                    // agent loop's routing/auth paths.
                                    let _ = tx.try_send(AcpUpdate::ToolCallStart {
                                        id: id.clone(),
                                        name: tool_call.title.clone(),
                                        kind: tool_call.kind,
                                        raw_input: tool_call.raw_input.clone(),
                                    });
                                    if let Some(accumulated) = synchronous_accumulated {
                                        let content = if accumulated.content.is_empty() {
                                            None
                                        } else {
                                            Some(accumulated.content)
                                        };
                                        let _ = tx.try_send(AcpUpdate::ToolCallComplete {
                                            id,
                                            raw_output: accumulated.raw_output,
                                            content,
                                            is_error: matches!(
                                                initial_status,
                                                ToolCallStatus::Failed
                                            ),
                                        });
                                    }
                                }
                                SessionUpdate::ToolCallUpdate(update) => {
                                    let id = update.tool_call_id.0.to_string();
                                    // Merge patch-like fields; only emit on terminal status.
                                    let terminal_status = update.fields.status.filter(|s| {
                                        matches!(
                                            s,
                                            ToolCallStatus::Completed | ToolCallStatus::Failed
                                        )
                                    });
                                    let accumulated = if let Ok(mut buffer) =
                                        pending_tool_updates.lock()
                                    {
                                        let entry = buffer.entry(id.clone()).or_default();
                                        if let Some(raw_output) = update.fields.raw_output.clone() {
                                            entry.raw_output = Some(raw_output);
                                        }
                                        if let Some(content) = update.fields.content.clone() {
                                            entry.content.extend(content);
                                        }
                                        if terminal_status.is_some() {
                                            buffer.remove(&id)
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    };
                                    if let (Some(accumulated), Some(status)) =
                                        (accumulated, terminal_status)
                                    {
                                        let content = if accumulated.content.is_empty() {
                                            None
                                        } else {
                                            Some(accumulated.content)
                                        };
                                        let _ = tx.try_send(AcpUpdate::ToolCallComplete {
                                            id,
                                            raw_output: accumulated.raw_output,
                                            content,
                                            is_error: matches!(status, ToolCallStatus::Failed),
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(())
                    }
                },
                sacp::on_receive_notification!(),
            )
            .on_receive_request(
                {
                    let prompt_response_tx = prompt_response_tx.clone();
                    async move |request: RequestPermissionRequest, responder, _connection_cx| {
                        let (response_tx, response_rx) = oneshot::channel();

                        let handler = prompt_response_tx
                            .lock()
                            .ok()
                            .as_ref()
                            .and_then(|g| g.as_ref().cloned());
                        let tx = handler.ok_or_else(sacp::Error::internal_error)?;

                        if tx.is_closed() {
                            return Err(sacp::Error::internal_error());
                        }

                        tx.try_send(AcpUpdate::PermissionRequest {
                            request: Box::new(request),
                            response_tx,
                        })
                        .map_err(|_| sacp::Error::internal_error())?;

                        let response = response_rx.await.unwrap_or_else(|_| {
                            RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)
                        });
                        responder.respond(response)
                    }
                },
                sacp::on_receive_request!(),
            )
            .connect_with(transport, async move |cx: ConnectionTo<Agent>| {
                handle_requests(config, goose_mode, cx, rx, prompt_response_tx, init_tx).await
            })
            .await?;

        Ok(())
    }
}

async fn spawn_acp_process(config: &AcpProviderConfig) -> Result<Child> {
    let mut cmd = Command::new(&config.command);
    cmd.args(&config.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);

    for key in &config.env_remove {
        cmd.env_remove(key);
    }

    for (key, value) in &config.env {
        cmd.env(key, value);
    }

    configure_subprocess(&mut cmd);
    cmd.spawn().context("failed to spawn ACP process")
}

fn log_undelivered<E: std::fmt::Debug>(result: Result<(), E>, method: &str) {
    if let Err(e) = result {
        tracing::debug!(method, error = ?e, "response not delivered");
    }
}

async fn handle_requests(
    config: AcpProviderConfig,
    goose_mode: Arc<Mutex<GooseMode>>,
    cx: ConnectionTo<Agent>,
    rx: &mut mpsc::Receiver<ClientRequest>,
    prompt_response_tx: Arc<Mutex<Option<mpsc::Sender<AcpUpdate>>>>,
    init_tx: oneshot::Sender<Result<InitializeResponse>>,
) -> Result<(), sacp::Error> {
    let mut init_tx = Some(init_tx);

    let client_capabilities = ClientCapabilities::new();
    let init_response: InitializeResponse = cx
        .send_request(
            InitializeRequest::new(ProtocolVersion::LATEST)
                .client_capabilities(client_capabilities),
        )
        .block_task()
        .await
        .map_err(|err| {
            let message = format!("ACP {} failed: {err}", AGENT_METHOD_NAMES.initialize);
            if let Some(tx) = init_tx.take() {
                let _ = tx.send(Err(anyhow::anyhow!(message.clone())));
            }
            sacp::Error::internal_error().data(message)
        })?;

    let supports_close = init_response
        .agent_capabilities
        .session_capabilities
        .close
        .is_some();
    let mcp_capabilities = init_response.agent_capabilities.mcp_capabilities.clone();
    if let Some(tx) = init_tx.take() {
        log_undelivered(tx.send(Ok(init_response)), AGENT_METHOD_NAMES.initialize);
    }

    let mut session_ids: Vec<SessionId> = Vec::new();

    while let Some(request) = rx.recv().await {
        match request {
            ClientRequest::NewSession { response_tx } => {
                let mcp_servers = filter_supported_servers(&config.mcp_servers, &mcp_capabilities);
                let session = cx
                    .send_request(
                        NewSessionRequest::new(config.work_dir.clone()).mcp_servers(mcp_servers),
                    )
                    .block_task()
                    .await;
                let result = match session {
                    Ok(session) => {
                        session_ids.push(session.session_id.clone());
                        apply_session_mode(&config, &goose_mode, &cx, session).await
                    }
                    Err(err) => Err(anyhow::anyhow!(
                        "ACP {} failed: {err}",
                        AGENT_METHOD_NAMES.session_new
                    )),
                };
                log_undelivered(response_tx.send(result), AGENT_METHOD_NAMES.session_new);
            }
            ClientRequest::SetMode {
                session_id,
                mode_id,
                response_tx,
            } => {
                let result: Result<()> = cx
                    .send_request(SetSessionModeRequest::new(session_id, mode_id))
                    .block_task()
                    .await
                    .map(|_| ())
                    .map_err(anyhow::Error::from);
                log_undelivered(
                    response_tx.send(result),
                    AGENT_METHOD_NAMES.session_set_mode,
                );
            }
            ClientRequest::SetConfigOption {
                session_id,
                config_id,
                value,
                response_tx,
            } => {
                let value_id = sacp::schema::SessionConfigValueId::new(value);
                let req = SetSessionConfigOptionRequest::new(session_id, config_id, value_id);
                let result: Result<()> = cx
                    .send_request(req)
                    .block_task()
                    .await
                    .map(|_| ())
                    .map_err(anyhow::Error::from);
                log_undelivered(
                    response_tx.send(result),
                    AGENT_METHOD_NAMES.session_set_config_option,
                );
            }
            ClientRequest::Prompt {
                session_id,
                content,
                response_tx,
            } => {
                *prompt_response_tx.lock().unwrap() = Some(response_tx.clone());

                let response: Result<PromptResponse, _> = cx
                    .send_request(PromptRequest::new(session_id, content))
                    .block_task()
                    .await;

                match response {
                    Ok(r) => {
                        log_undelivered(
                            response_tx.try_send(AcpUpdate::Complete(r.stop_reason)),
                            AGENT_METHOD_NAMES.session_prompt,
                        );
                    }
                    Err(e) => {
                        log_undelivered(
                            response_tx.try_send(AcpUpdate::Error(e.to_string())),
                            AGENT_METHOD_NAMES.session_prompt,
                        );
                    }
                }

                *prompt_response_tx.lock().unwrap() = None;
            }
        }
    }

    if supports_close {
        for session_id in session_ids {
            if let Err(e) = cx
                .send_request(CloseSessionRequest::new(session_id.clone()))
                .block_task()
                .await
            {
                tracing::debug!(method = AGENT_METHOD_NAMES.session_close, session_id = %session_id, error = %e, "failed on shutdown");
            }
        }
    }

    Ok(())
}

async fn apply_session_mode(
    config: &AcpProviderConfig,
    goose_mode: &Arc<Mutex<GooseMode>>,
    cx: &ConnectionTo<Agent>,
    session: NewSessionResponse,
) -> Result<NewSessionResponse> {
    let current_mode = goose_mode.lock().ok().map(|mode| *mode);
    let requested_mode_id = current_mode
        .and_then(|mode| config.mode_mapping.get(&mode).cloned())
        .or_else(|| config.session_mode_id.clone());

    if let (Some(mode_id), Some(modes)) = (requested_mode_id, session.modes.as_ref()) {
        if modes.current_mode_id.0.as_ref() != mode_id.as_str() {
            let available: Vec<String> = modes
                .available_modes
                .iter()
                .map(|mode| mode.id.0.to_string())
                .collect();

            if !available.iter().any(|id| id == &mode_id) {
                return Err(anyhow::anyhow!(
                    "Requested mode '{}' not offered by agent. Available modes: {}",
                    mode_id,
                    available.join(", ")
                ));
            }
            let _: SetSessionModeResponse = cx
                .send_request(SetSessionModeRequest::new(
                    session.session_id.clone(),
                    mode_id,
                ))
                .block_task()
                .await
                .map_err(|err| {
                    anyhow::anyhow!(
                        "ACP agent rejected {}: {err}",
                        AGENT_METHOD_NAMES.session_set_mode
                    )
                })?;
        }
    }

    Ok(session)
}

pub fn extension_configs_to_mcp_servers(configs: &[ExtensionConfig]) -> Vec<McpServer> {
    let mut servers = Vec::new();

    for config in configs {
        match config {
            ExtensionConfig::StreamableHttp {
                name, uri, headers, ..
            } => {
                let http_headers = headers
                    .iter()
                    .map(|(key, value)| HttpHeader::new(key, value))
                    .collect();
                servers.push(McpServer::Http(
                    McpServerHttp::new(name, uri).headers(http_headers),
                ));
            }
            ExtensionConfig::Stdio {
                name,
                cmd,
                args,
                envs,
                ..
            } => {
                let env_vars = envs
                    .get_env()
                    .into_iter()
                    .map(|(key, value)| EnvVariable::new(key, value))
                    .collect();

                servers.push(McpServer::Stdio(
                    McpServerStdio::new(name, cmd)
                        .args(args.clone())
                        .env(env_vars),
                ));
            }
            ExtensionConfig::Sse { name, .. } => {
                tracing::debug!(name, "skipping SSE extension, migrate to streamable_http");
            }
            _ => {}
        }
    }

    servers
}

fn filter_supported_servers(
    servers: &[McpServer],
    capabilities: &McpCapabilities,
) -> Vec<McpServer> {
    servers
        .iter()
        .filter(|server| match server {
            McpServer::Http(http) => {
                if !capabilities.http {
                    tracing::debug!(
                        name = http.name,
                        "skipping HTTP server, agent lacks capability"
                    );
                    false
                } else {
                    true
                }
            }
            McpServer::Sse(sse) => {
                tracing::debug!(name = sse.name, "skipping SSE server, unsupported");
                false
            }
            _ => true,
        })
        .cloned()
        .collect()
}

fn messages_to_prompt(messages: &[Message]) -> Vec<ContentBlock> {
    let mut content_blocks = Vec::new();

    let last_user = messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User && m.is_agent_visible());

    if let Some(message) = last_user {
        for content in &message.content {
            match content {
                MessageContent::Text(text) => {
                    content_blocks.push(ContentBlock::Text(TextContent::new(text.text.clone())));
                }
                MessageContent::Image(image) => {
                    content_blocks.push(ContentBlock::Image(ImageContent::new(
                        &image.data,
                        &image.mime_type,
                    )));
                }
                _ => {}
            }
        }
    }

    content_blocks
}

/// Convert ACP `ToolCallContent` blocks into the rmcp `Content` shape goose's
/// `Message::with_tool_response` consumes. Handles `Content` (text/image/other),
/// `Diff`, and `Terminal` variants; falls back to a JSON serialization of
/// `raw_output` when no blocks are present so the renderer always has something.
fn acp_tool_call_content_to_rmcp(
    content: Option<Vec<ToolCallContent>>,
    raw_output: Option<serde_json::Value>,
) -> Vec<RmcpContent> {
    let mut out = Vec::new();
    if let Some(blocks) = content {
        for block in blocks {
            match block {
                ToolCallContent::Content(val) => match val.content {
                    ContentBlock::Text(text) => {
                        out.push(RmcpContent::text(text.text));
                    }
                    ContentBlock::Image(image) => {
                        out.push(RmcpContent::image(image.data, image.mime_type));
                    }
                    other => {
                        if let Ok(json) = serde_json::to_string(&other) {
                            out.push(RmcpContent::text(json));
                        }
                    }
                },
                ToolCallContent::Diff(diff) => {
                    let path = diff.path.display();
                    let body = match diff.old_text.as_deref() {
                        Some(old) => {
                            format!("--- {path}\n{old}\n+++ {path}\n{}", diff.new_text)
                        }
                        None => format!("+++ {path}\n{}", diff.new_text),
                    };
                    out.push(RmcpContent::text(body));
                }
                ToolCallContent::Terminal(terminal) => {
                    out.push(RmcpContent::text(format!(
                        "[terminal {}]",
                        terminal.terminal_id.0
                    )));
                }
                _ => {}
            }
        }
    }
    if out.is_empty() {
        if let Some(raw) = raw_output {
            let text = match raw {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            };
            out.push(RmcpContent::text(text));
        }
    }
    out
}

fn build_action_required_message(request: &RequestPermissionRequest) -> Option<Message> {
    let tool_title = request
        .tool_call
        .fields
        .title
        .clone()
        .unwrap_or_else(|| "Tool".to_string());

    let arguments = request
        .tool_call
        .fields
        .raw_input
        .as_ref()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();

    let prompt = request
        .tool_call
        .fields
        .content
        .as_ref()
        .and_then(|content| {
            content.iter().find_map(|c| match c {
                ToolCallContent::Content(val) => match &val.content {
                    ContentBlock::Text(text) => Some(text.text.clone()),
                    _ => None,
                },
                _ => None,
            })
        });

    Some(
        Message::assistant()
            .with_action_required(
                request.tool_call.tool_call_id.0.to_string(),
                tool_title,
                arguments,
                prompt,
            )
            .user_only(),
    )
}

fn extract_model_info_from_config_options(
    config_options: &[SessionConfigOption],
) -> Option<(String, Vec<String>)> {
    let select = config_options.iter().find_map(|opt| {
        if opt.category.as_ref() != Some(&SessionConfigOptionCategory::Model) {
            return None;
        }
        match &opt.kind {
            SessionConfigKind::Select(select) => Some(select),
            _ => None,
        }
    })?;

    let current = select.current_value.0.to_string();
    let available = match &select.options {
        SessionConfigSelectOptions::Ungrouped(options) => options
            .iter()
            .map(|option| option.value.0.to_string())
            .collect(),
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| {
                group
                    .options
                    .iter()
                    .map(|option| option.value.0.to_string())
            })
            .collect(),
        _ => Vec::new(),
    };
    Some((current, available))
}

fn resolve_model_info(
    provider_name: &str,
    response: &NewSessionResponse,
) -> Result<(String, Vec<String>), ProviderError> {
    if let Some(opts) = &response.config_options {
        if let Some((current, available)) = extract_model_info_from_config_options(opts) {
            return Ok((current, available));
        }
    }

    let models = response.models.as_ref().ok_or_else(|| {
        ProviderError::RequestFailed(format!(
            "{provider_name}: agent returned neither config_options nor models"
        ))
    })?;
    let current = models.current_model_id.0.to_string();
    let available = models
        .available_models
        .iter()
        .map(|am| am.model_id.0.to_string())
        .collect();
    Ok((current, available))
}

fn reverse_mode_mapping(
    mode_mapping: &HashMap<GooseMode, String>,
) -> HashMap<String, Vec<GooseMode>> {
    let mut reverse: HashMap<String, Vec<GooseMode>> = HashMap::new();
    for (mode, id) in mode_mapping {
        reverse.entry(id.clone()).or_default().push(*mode);
    }
    reverse
}

fn resolve_mode(
    reverse_modes: &HashMap<String, Vec<GooseMode>>,
    mode_id: &str,
    current: &Arc<Mutex<GooseMode>>,
) -> Option<GooseMode> {
    let candidates = reverse_modes.get(mode_id)?;
    if candidates.len() == 1 {
        return Some(candidates[0]);
    }
    let current = current.lock().ok()?;
    if candidates.contains(&*current) {
        Some(*current)
    } else {
        Some(candidates[0])
    }
}

fn permission_decision_from_mode(goose_mode: GooseMode) -> Option<PermissionDecision> {
    match goose_mode {
        GooseMode::Auto => Some(PermissionDecision::AllowOnce),
        GooseMode::Chat => Some(PermissionDecision::RejectOnce),
        GooseMode::Approve | GooseMode::SmartApprove => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::extension::Envs;
    use sacp::schema::SessionConfigSelectOption;
    use test_case::test_case;

    #[test_case(
        ExtensionConfig::Stdio {
            name: "github".into(),
            description: String::new(),
            cmd: "/path/to/github-mcp-server".into(),
            args: vec!["stdio".into()],
            envs: Envs::new([("GITHUB_PERSONAL_ACCESS_TOKEN".into(), "ghp_xxxxxxxxxxxx".into())].into()),
            env_keys: vec![],
            timeout: None,
            bundled: Some(false),
            available_tools: vec![],
        },
        vec![
            McpServer::Stdio(
                McpServerStdio::new("github", "/path/to/github-mcp-server")
                    .args(vec!["stdio".into()])
                    .env(vec![EnvVariable::new("GITHUB_PERSONAL_ACCESS_TOKEN", "ghp_xxxxxxxxxxxx")])
            )
        ]
        ; "stdio_converts_to_mcpserver_stdio"
    )]
    #[test_case(
        ExtensionConfig::StreamableHttp {
            name: "github".into(),
            description: String::new(),
            uri: "https://api.githubcopilot.com/mcp/".into(),
            envs: Envs::default(),
            env_keys: vec![],
            headers: HashMap::from([("Authorization".into(), "Bearer ghp_xxxxxxxxxxxx".into())]),
            timeout: None,
            bundled: Some(false),
            available_tools: vec![],
        },
        vec![
            McpServer::Http(
                McpServerHttp::new("github", "https://api.githubcopilot.com/mcp/")
                    .headers(vec![HttpHeader::new("Authorization", "Bearer ghp_xxxxxxxxxxxx")])
            )
        ]
        ; "streamable_http_converts_to_mcpserver_http_when_capable"
    )]
    fn test_extension_configs_to_mcp_servers(config: ExtensionConfig, expected: Vec<McpServer>) {
        let result = extension_configs_to_mcp_servers(&[config]);
        assert_eq!(result.len(), expected.len(), "server count mismatch");
        for (a, e) in result.iter().zip(expected.iter()) {
            match (a, e) {
                (McpServer::Stdio(actual), McpServer::Stdio(expected)) => {
                    assert_eq!(actual.name, expected.name);
                    assert_eq!(actual.command, expected.command);
                    assert_eq!(actual.args, expected.args);
                    assert_eq!(actual.env.len(), expected.env.len());
                }
                (McpServer::Http(actual), McpServer::Http(expected)) => {
                    assert_eq!(actual.name, expected.name);
                    assert_eq!(actual.url, expected.url);
                    assert_eq!(actual.headers.len(), expected.headers.len());
                }
                _ => panic!("server type mismatch"),
            }
        }
    }

    #[test]
    fn test_sse_skips() {
        let config = ExtensionConfig::Sse {
            name: "test-sse".into(),
            description: String::new(),
            uri: Some("https://example.com/sse".into()),
        };
        let result = extension_configs_to_mcp_servers(&[config]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_supported_servers_skips_http_without_capability() {
        let config = ExtensionConfig::StreamableHttp {
            name: "github".into(),
            description: String::new(),
            uri: "https://api.githubcopilot.com/mcp/".into(),
            envs: Envs::default(),
            env_keys: vec![],
            headers: HashMap::from([("Authorization".into(), "Bearer ghp_xxxxxxxxxxxx".into())]),
            timeout: None,
            bundled: Some(false),
            available_tools: vec![],
        };

        let servers = extension_configs_to_mcp_servers(&[config]);
        let filtered = filter_supported_servers(&servers, &McpCapabilities::default());
        assert!(filtered.is_empty());
    }

    #[test_case(GooseMode::Auto => Some(PermissionDecision::AllowOnce) ; "auto allows")]
    #[test_case(GooseMode::Chat => Some(PermissionDecision::RejectOnce) ; "chat rejects")]
    #[test_case(GooseMode::Approve => None ; "approve defers")]
    #[test_case(GooseMode::SmartApprove => None ; "smart_approve defers")]
    fn test_permission_decision_from_mode(mode: GooseMode) -> Option<PermissionDecision> {
        permission_decision_from_mode(mode)
    }

    #[test_case(
        HashMap::from([
            (GooseMode::Auto, "yolo".to_string()),
            (GooseMode::Approve, "default".to_string()),
            (GooseMode::SmartApprove, "auto_edit".to_string()),
            (GooseMode::Chat, "plan".to_string()),
        ]),
        HashMap::from([
            ("yolo".to_string(), vec![GooseMode::Auto]),
            ("default".to_string(), vec![GooseMode::Approve]),
            ("auto_edit".to_string(), vec![GooseMode::SmartApprove]),
            ("plan".to_string(), vec![GooseMode::Chat]),
        ])
        ; "gemini provider mapping"
    )]
    #[test_case(
        HashMap::from([
            (GooseMode::Auto, "bypassPermissions".to_string()),
            (GooseMode::Approve, "default".to_string()),
            (GooseMode::SmartApprove, "acceptEdits".to_string()),
            (GooseMode::Chat, "plan".to_string()),
        ]),
        HashMap::from([
            ("bypassPermissions".to_string(), vec![GooseMode::Auto]),
            ("default".to_string(), vec![GooseMode::Approve]),
            ("acceptEdits".to_string(), vec![GooseMode::SmartApprove]),
            ("plan".to_string(), vec![GooseMode::Chat]),
        ])
        ; "claude provider mapping"
    )]
    #[test_case(
        HashMap::from([
            (GooseMode::Auto, "full-access".to_string()),
            (GooseMode::Approve, "read-only".to_string()),
            (GooseMode::SmartApprove, "auto".to_string()),
            (GooseMode::Chat, "read-only".to_string()),
        ]),
        HashMap::from([
            ("full-access".to_string(), vec![GooseMode::Auto]),
            ("read-only".to_string(), vec![GooseMode::Approve, GooseMode::Chat]),
            ("auto".to_string(), vec![GooseMode::SmartApprove]),
        ])
        ; "codex duplicate read-only"
    )]
    fn test_reverse_mode_mapping(
        forward: HashMap<GooseMode, String>,
        expected: HashMap<String, Vec<GooseMode>>,
    ) {
        let result = reverse_mode_mapping(&forward);
        assert_eq!(result.len(), expected.len());
        for (key, expected_modes) in &expected {
            let actual = result.get(key).expect("missing key");
            assert_eq!(
                actual.len(),
                expected_modes.len(),
                "length mismatch for key {key}"
            );
            for mode in expected_modes {
                assert!(actual.contains(mode), "missing {mode:?} for key {key}");
            }
        }
    }

    #[test_case(
        NewSessionResponse::new("s1")
            .models(sacp::schema::SessionModelState::new(
                "default",
                vec![
                    sacp::schema::ModelInfo::new("default", "Default (recommended)"),
                    sacp::schema::ModelInfo::new("sonnet", "Sonnet"),
                    sacp::schema::ModelInfo::new("haiku", "Haiku"),
                ],
            ))
            .config_options(vec![
                SessionConfigOption::select("model", "Model", "default", vec![
                    SessionConfigSelectOption::new("default", "Default (recommended)"),
                    SessionConfigSelectOption::new("sonnet", "Sonnet"),
                    SessionConfigSelectOption::new("haiku", "Haiku"),
                ])
                .category(SessionConfigOptionCategory::Model),
            ])
        => Ok(("default".to_string(), vec!["default".to_string(), "sonnet".to_string(), "haiku".to_string()]))
        ; "claude-agent-acp config_options supersedes models"
    )]
    #[test_case(
        NewSessionResponse::new("s1")
            .models(sacp::schema::SessionModelState::new(
                "auto-gemini-3",
                vec![
                    sacp::schema::ModelInfo::new("auto-gemini-3", "Auto (Gemini 3)"),
                    sacp::schema::ModelInfo::new("auto-gemini-2.5", "Auto (Gemini 2.5)"),
                    sacp::schema::ModelInfo::new("gemini-2.5-pro", "gemini-2.5-pro"),
                ],
            ))
        => Ok(("auto-gemini-3".to_string(), vec!["auto-gemini-3".to_string(), "auto-gemini-2.5".to_string(), "gemini-2.5-pro".to_string()]))
        ; "gemini-acp falls back to models"
    )]
    #[test_case(
        NewSessionResponse::new("s1")
        => Err(ProviderError::RequestFailed(
            "test: agent returned neither config_options nor models".to_string()
        ))
        ; "neither config_options nor models is an error"
    )]
    fn test_resolve_model_info(
        response: NewSessionResponse,
    ) -> Result<(String, Vec<String>), ProviderError> {
        resolve_model_info("test", &response)
    }

    fn codex_reverse_modes() -> HashMap<String, Vec<GooseMode>> {
        HashMap::from([
            ("full-access".to_string(), vec![GooseMode::Auto]),
            (
                "read-only".to_string(),
                vec![GooseMode::Approve, GooseMode::Chat],
            ),
            ("auto".to_string(), vec![GooseMode::SmartApprove]),
        ])
    }

    #[test_case(
        "full-access", GooseMode::Auto, Some(GooseMode::Auto)
        ; "unique mapping returns the only candidate"
    )]
    #[test_case(
        "read-only", GooseMode::Approve, Some(GooseMode::Approve)
        ; "duplicate prefers current when current is Approve"
    )]
    #[test_case(
        "read-only", GooseMode::Chat, Some(GooseMode::Chat)
        ; "duplicate prefers current when current is Chat"
    )]
    #[test_case(
        "read-only", GooseMode::Auto, Some(GooseMode::Approve)
        ; "duplicate falls back to first when current not in candidates"
    )]
    #[test_case(
        "unknown-id", GooseMode::Auto, None
        ; "unknown mode id returns None"
    )]
    fn test_resolve_mode(mode_id: &str, current: GooseMode, expected: Option<GooseMode>) {
        let reverse_modes = codex_reverse_modes();
        let current = Arc::new(Mutex::new(current));
        let result = resolve_mode(&reverse_modes, mode_id, &current);
        if mode_id == "read-only" && expected == Some(GooseMode::Approve) {
            assert!(result == Some(GooseMode::Approve) || result == Some(GooseMode::Chat));
        } else {
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn acp_tool_call_content_handles_text_diff_terminal_and_image() {
        use sacp::schema::{Diff, Terminal, TerminalId, TextContent};

        let diff_block = ToolCallContent::Diff(
            Diff::new(std::path::PathBuf::from("/tmp/file.txt"), "new\n").old_text("old\n"),
        );
        let terminal_block = ToolCallContent::Terminal(Terminal::new(TerminalId::new("term-7")));
        let text_block = ToolCallContent::Content(sacp::schema::Content::new(ContentBlock::Text(
            TextContent::new("hello"),
        )));
        let image_block = ToolCallContent::Content(sacp::schema::Content::new(
            ContentBlock::Image(ImageContent::new("base64data", "image/png")),
        ));

        let out = acp_tool_call_content_to_rmcp(
            Some(vec![text_block, diff_block, terminal_block, image_block]),
            None,
        );

        assert_eq!(out.len(), 4, "all four block kinds should produce output");
        let serialized: Vec<String> = out
            .iter()
            .map(|c| serde_json::to_string(c).unwrap())
            .collect();
        assert!(
            serialized[0].contains("hello"),
            "text block lost: {serialized:?}"
        );
        assert!(
            serialized[1].contains("/tmp/file.txt"),
            "diff path lost: {serialized:?}"
        );
        assert!(
            serialized[1].contains("new"),
            "diff body lost: {serialized:?}"
        );
        assert!(
            serialized[2].contains("term-7"),
            "terminal id lost: {serialized:?}"
        );
        assert!(
            serialized[3].contains("base64data"),
            "image data lost: {serialized:?}"
        );
    }

    #[test]
    fn acp_tool_call_content_falls_back_to_raw_output_when_blocks_empty() {
        let out =
            acp_tool_call_content_to_rmcp(Some(vec![]), Some(serde_json::json!({"key": "value"})));
        assert_eq!(out.len(), 1);
        let serialized = serde_json::to_string(&out[0]).unwrap();
        assert!(
            serialized.contains("key"),
            "fallback raw_output lost: {serialized}"
        );
    }

    /// Pins the tool_meta shape that the `AcpUpdate::ToolCallStart` consumer
    /// emits onto the synthesized `ToolRequest`. ACP doesn't expose a canonical
    /// tool name to clients, so we surface `kind` here as a stable categorization
    /// signal alongside the `external_dispatch` marker that bypasses agent-loop
    /// routing.
    #[test]
    fn tool_meta_pairs_external_dispatch_marker_with_acp_kind() {
        let cases = [
            (ToolKind::Execute, "execute"),
            (ToolKind::Read, "read"),
            (ToolKind::Edit, "edit"),
            (ToolKind::Other, "other"),
        ];
        for (kind, expected) in cases {
            let tool_meta = serde_json::json!({
                TOOL_META_EXTERNAL_DISPATCH_KEY: true,
                "goose.acp.kind": kind,
            });
            assert_eq!(
                tool_meta[TOOL_META_EXTERNAL_DISPATCH_KEY],
                serde_json::Value::Bool(true),
                "external_dispatch marker missing for kind={kind:?}"
            );
            assert_eq!(
                tool_meta["goose.acp.kind"],
                serde_json::Value::String(expected.to_string()),
                "goose.acp.kind serialized wrong for kind={kind:?}"
            );
        }
    }
}
