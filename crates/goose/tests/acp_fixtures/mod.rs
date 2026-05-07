#![recursion_limit = "256"]
#![allow(unused_attributes)]

use agent_client_protocol::schema::{
    CreateTerminalResponse, KillTerminalResponse, ListSessionsResponse, McpServer,
    ReadTextFileRequest, ReadTextFileResponse, ReleaseTerminalResponse, SessionModeState,
    SessionModelState, SessionUpdate, TerminalExitStatus, TerminalId, TerminalOutputResponse,
    ToolCallContent, ToolCallStatus, ToolKind, WaitForTerminalExitResponse, WriteTextFileRequest,
    WriteTextFileResponse,
};
use async_trait::async_trait;
use fs_err as fs;
use goose::acp::server::{serve, AcpProviderFactory, GooseAcpAgent};
pub use goose::acp::{map_permission_response, PermissionDecision};
use goose::agents::GoosePlatform;
use goose::builtin_extension::register_builtin_extensions;
use goose::config::paths::Paths;
use goose::config::{GooseMode, PermissionManager};
use goose::providers::api_client::{ApiClient, AuthMethod as ApiAuthMethod};
use goose::providers::base::Provider;
use goose::providers::openai::OpenAiProvider;
use goose::session_context::SESSION_ID_HEADER;
use goose_test_support::{ExpectedSessionId, TEST_MODEL};
use std::collections::VecDeque;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub struct OpenAiFixture {
    _server: MockServer,
    base_url: String,
    exchanges: Vec<(String, &'static str)>,
    queue: Arc<Mutex<VecDeque<(String, &'static str)>>>,
}

impl OpenAiFixture {
    /// Mock OpenAI streaming endpoint. Exchanges are (pattern, response) pairs.
    /// On mismatch, returns 417 of the diff in OpenAI error format.
    pub async fn new(
        exchanges: Vec<(String, &'static str)>,
        expected_session_id: Arc<dyn ExpectedSessionId>,
    ) -> Self {
        let mock_server = MockServer::start().await;
        let queue = Arc::new(Mutex::new(VecDeque::from(exchanges.clone())));

        // Always return the models when asked, as there is no POST data to validate
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_string(include_str!("../acp_test_data/openai_models.json")),
            )
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with({
                let queue = queue.clone();
                let expected_session_id = expected_session_id.clone();
                move |req: &wiremock::Request| {
                    let body = std::str::from_utf8(&req.body).unwrap_or("");

                    // Validate session ID header
                    let actual = req
                        .headers
                        .get(SESSION_ID_HEADER)
                        .and_then(|v| v.to_str().ok());
                    if let Err(e) = expected_session_id.validate(actual) {
                        return ResponseTemplate::new(417)
                            .insert_header("content-type", "application/json")
                            .set_body_json(serde_json::json!({"error": {"message": e}}));
                    }

                    // See if the actual request matches the expected pattern
                    let mut q = queue.lock().unwrap();
                    let (expected_body, response) = q.front().cloned().unwrap_or_default();
                    if !expected_body.is_empty() && body.contains(&expected_body) {
                        q.pop_front();
                        return ResponseTemplate::new(200)
                            .insert_header("content-type", "text/event-stream")
                            .set_body_string(response);
                    }
                    drop(q);

                    // If there was no body, the request was unexpected. Otherwise, it is a mismatch.
                    let message = if expected_body.is_empty() {
                        format!("Unexpected request:\n  {}", body)
                    } else {
                        format!(
                            "Expected body to contain:\n  {}\n\nActual body:\n  {}",
                            expected_body, body
                        )
                    };
                    // Use OpenAI's error response schema so the provider will pass the error through.
                    ResponseTemplate::new(417)
                        .insert_header("content-type", "application/json")
                        .set_body_json(serde_json::json!({"error": {"message": message}}))
                }
            })
            .mount(&mock_server)
            .await;

        let base_url = mock_server.uri();
        Self {
            _server: mock_server,
            base_url,
            exchanges,
            queue,
        }
    }

    pub fn uri(&self) -> &str {
        &self.base_url
    }

    pub fn reset(&self) {
        let mut queue = self.queue.lock().unwrap();
        *queue = VecDeque::from(self.exchanges.clone());
    }
}

pub type DuplexTransport = agent_client_protocol::ByteStreams<
    tokio_util::compat::Compat<tokio::io::DuplexStream>,
    tokio_util::compat::Compat<tokio::io::DuplexStream>,
>;

/// Wires up duplex streams, spawns `serve` for the given agent, and returns
/// a ready-to-use agent_client_protocol transport plus the server handle.
#[allow(dead_code)]
pub async fn serve_agent_in_process(
    agent: Arc<GooseAcpAgent>,
) -> (DuplexTransport, JoinHandle<()>) {
    let (client_read, server_write) = tokio::io::duplex(64 * 1024);
    let (server_read, client_write) = tokio::io::duplex(64 * 1024);

    let handle = tokio::spawn(async move {
        if let Err(e) = serve(agent, server_read.compat(), server_write.compat_write()).await {
            tracing::error!("ACP server error: {e}");
        }
    });

    let transport =
        agent_client_protocol::ByteStreams::new(client_write.compat_write(), client_read.compat());
    (transport, handle)
}

#[allow(dead_code)]
pub async fn spawn_acp_server_in_process(
    openai_base_url: &str,
    builtins: &[String],
    data_root: &std::path::Path,
    goose_mode: GooseMode,
    provider_factory: Option<AcpProviderFactory>,
    current_model: &str,
    disable_session_naming: bool,
) -> (DuplexTransport, JoinHandle<()>, Arc<PermissionManager>) {
    fs::create_dir_all(data_root).unwrap();
    // TODO: Paths::in_state_dir is global, ignoring per-test data_root
    fs::create_dir_all(Paths::in_state_dir("logs")).unwrap();
    let config_path = data_root.join(goose::config::base::CONFIG_YAML_NAME);
    if !config_path.exists() {
        fs::write(
            &config_path,
            format!("GOOSE_MODEL: {current_model}\nGOOSE_PROVIDER: openai\n"),
        )
        .unwrap();
    }
    let provider_factory = provider_factory.unwrap_or_else(|| {
        let base_url = openai_base_url.to_string();
        Arc::new(move |_provider_name, model_config, _extensions| {
            let base_url = base_url.clone();
            Box::pin(async move {
                let api_client =
                    ApiClient::new(base_url, ApiAuthMethod::BearerToken("test-key".to_string()))
                        .unwrap();
                let provider: Arc<dyn Provider> =
                    Arc::new(OpenAiProvider::new(api_client, model_config));
                Ok(provider)
            })
        })
    });

    let agent = GooseAcpAgent::new(
        provider_factory,
        builtins.to_vec(),
        data_root.to_path_buf(),
        data_root.to_path_buf(),
        goose_mode,
        disable_session_naming,
        GoosePlatform::GooseCli,
    )
    .await
    .unwrap();
    let agent = Arc::new(agent);
    let permission_manager = agent.permission_manager();
    let (transport, handle) = serve_agent_in_process(agent).await;

    (transport, handle, permission_manager)
}

#[derive(Debug)]
pub struct TestOutput {
    pub text: String,
    pub tool_status: Option<ToolCallStatus>,
}

#[derive(Debug, PartialEq)]
pub enum Notification {
    UserMessage,
    AgentMessage,
    AgentThought,
    ToolCall,
    ToolCallKind(ToolKind),
    ToolCallContent(String),
    ToolCallStatus(ToolCallStatus),
    Plan,
    AvailableCommands,
    CurrentMode,
    ConfigOption,
    SessionInfoUpdate {
        title: Option<String>,
        updated_at: Option<String>,
        message_count: Option<u64>,
        user_set_name: Option<bool>,
    },
}

pub fn to_notifications(updates: &[SessionUpdate]) -> Vec<Notification> {
    let mut out = Vec::new();
    for u in updates {
        match u {
            SessionUpdate::UserMessageChunk(_) => {
                if out.last() != Some(&Notification::UserMessage) {
                    out.push(Notification::UserMessage);
                }
            }
            SessionUpdate::AgentMessageChunk(_) => {
                if out.last() != Some(&Notification::AgentMessage) {
                    out.push(Notification::AgentMessage);
                }
            }
            SessionUpdate::AgentThoughtChunk(_) => {
                if out.last() != Some(&Notification::AgentThought) {
                    out.push(Notification::AgentThought);
                }
            }
            SessionUpdate::ToolCall(_) => out.push(Notification::ToolCall),
            SessionUpdate::ToolCallUpdate(upd) => {
                if let Some(kind) = upd.fields.kind {
                    out.push(Notification::ToolCallKind(kind));
                }
                if let Some(ref content) = upd.fields.content {
                    for c in content {
                        let tag = match c {
                            ToolCallContent::Content(_) => "content",
                            ToolCallContent::Diff(_) => "diff",
                            ToolCallContent::Terminal(_) => "terminal",
                            _ => "unknown",
                        };
                        out.push(Notification::ToolCallContent(tag.into()));
                    }
                }
                if let Some(status) = upd.fields.status {
                    out.push(Notification::ToolCallStatus(status));
                }
            }
            SessionUpdate::Plan(_) => out.push(Notification::Plan),
            SessionUpdate::AvailableCommandsUpdate(_) => out.push(Notification::AvailableCommands),
            SessionUpdate::CurrentModeUpdate(_) => out.push(Notification::CurrentMode),
            SessionUpdate::ConfigOptionUpdate(_) => out.push(Notification::ConfigOption),
            SessionUpdate::SessionInfoUpdate(update) => {
                let meta = update.meta.as_ref();
                out.push(Notification::SessionInfoUpdate {
                    title: update.title.value().cloned(),
                    updated_at: update.updated_at.value().cloned(),
                    message_count: meta
                        .and_then(|m| m.get("messageCount"))
                        .and_then(|v| v.as_u64()),
                    user_set_name: meta
                        .and_then(|m| m.get("userSetName"))
                        .and_then(|v| v.as_bool()),
                });
            }
            _ => {}
        }
    }
    out
}

pub fn assert_notifications(actual: &[Notification], expected: &[Notification]) {
    assert_eq!(actual, expected);
}

type ReadTextFileHandler =
    Arc<dyn Fn(&ReadTextFileRequest) -> Result<ReadTextFileResponse, String> + Send + Sync>;
type WriteTextFileHandler =
    Arc<dyn Fn(&WriteTextFileRequest) -> Result<WriteTextFileResponse, String> + Send + Sync>;

#[derive(Clone)]
pub struct FsFixture {
    calls: Arc<Mutex<Vec<Result<(), String>>>>,
}

impl FsFixture {
    pub fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn read_handler(&self, expected_path: &str, content: &str) -> ReadTextFileHandler {
        let calls = self.calls.clone();
        let expected_path = expected_path.to_string();
        let content = content.to_string();
        Arc::new(move |req: &ReadTextFileRequest| {
            let path = req.path.to_str().unwrap_or("");
            if path != expected_path {
                let err = format!("expected path {expected_path}, got {path}");
                calls.lock().unwrap().push(Err(err.clone()));
                return Err(err);
            }
            calls.lock().unwrap().push(Ok(()));
            Ok(ReadTextFileResponse::new(&content))
        })
    }

    pub fn write_handler(
        &self,
        expected_path: &str,
        expected_content: &str,
    ) -> WriteTextFileHandler {
        let calls = self.calls.clone();
        let expected_path = expected_path.to_string();
        let expected_content = expected_content.to_string();
        Arc::new(move |req: &WriteTextFileRequest| {
            let path = req.path.to_str().unwrap_or("");
            if path != expected_path {
                let err = format!("expected path {expected_path}, got {path}");
                calls.lock().unwrap().push(Err(err.clone()));
                return Err(err);
            }
            if req.content != expected_content {
                let err = format!("expected content {expected_content}, got {}", req.content);
                calls.lock().unwrap().push(Err(err.clone()));
                return Err(err);
            }
            calls.lock().unwrap().push(Ok(()));
            Ok(WriteTextFileResponse::new())
        })
    }

    pub fn assert_called(&self) {
        let calls = self.calls.lock().unwrap();
        assert!(!calls.is_empty(), "fs handler was never called");
        let errors: Vec<_> = calls.iter().filter_map(|c| c.as_ref().err()).collect();
        assert!(errors.is_empty(), "fs handler errors: {errors:?}");
    }
}

/// Expected terminal calls. Each variant carries (expected_input, return_value) data,
/// like OpenAiFixture's (pattern, response) pairs.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum TerminalCall {
    Create(String, String),      // (command, terminal_id)
    WaitForExit(String, u32),    // (terminal_id, exit_code)
    Output(String, String, u32), // (terminal_id, text, exit_code)
    Release(String),             // terminal_id
    Kill(String),                // terminal_id
}

impl TerminalCall {
    fn name(&self) -> &'static str {
        match self {
            Self::Create(..) => "create",
            Self::WaitForExit(..) => "wait_for_exit",
            Self::Output(..) => "output",
            Self::Release(_) => "release",
            Self::Kill(_) => "kill",
        }
    }
}

pub struct TerminalFixture {
    queue: Arc<Mutex<VecDeque<TerminalCall>>>,
    errors: Arc<Mutex<Vec<String>>>,
}

impl TerminalFixture {
    pub fn new(calls: Vec<TerminalCall>) -> Arc<Self> {
        Arc::new(Self {
            queue: Arc::new(Mutex::new(VecDeque::from(calls))),
            errors: Arc::new(Mutex::new(Vec::new())),
        })
    }

    fn pop(&self, expected: &str) -> Option<TerminalCall> {
        let Some(call) = self.queue.lock().unwrap().pop_front() else {
            self.record_error(format!("unexpected {expected}: queue empty"));
            return None;
        };
        if call.name() != expected {
            self.record_error(format!("expected {expected}, got {}", call.name()));
            return None;
        }
        Some(call)
    }

    fn record_error(&self, msg: String) {
        self.errors.lock().unwrap().push(msg);
    }

    fn validate_terminal_id(&self, method: &str, expected: &str, actual: &TerminalId) {
        if expected != actual.0.as_ref() {
            self.record_error(format!(
                "{method}: expected terminal_id {expected}, got {actual}"
            ));
        }
    }

    pub fn on_create(&self, command: &str) -> CreateTerminalResponse {
        if let Some(TerminalCall::Create(expect_command, terminal_id)) = self.pop("create") {
            if command != expect_command {
                self.record_error(format!(
                    "create: expected command {expect_command}, got {command}"
                ));
            }
            CreateTerminalResponse::new(TerminalId::new(terminal_id))
        } else {
            CreateTerminalResponse::new(TerminalId::new("error"))
        }
    }

    pub fn on_wait_for_exit(&self, terminal_id: &TerminalId) -> WaitForTerminalExitResponse {
        if let Some(TerminalCall::WaitForExit(expected_id, exit_code)) = self.pop("wait_for_exit") {
            self.validate_terminal_id("wait_for_exit", &expected_id, terminal_id);
            WaitForTerminalExitResponse::new(TerminalExitStatus::new().exit_code(exit_code))
        } else {
            WaitForTerminalExitResponse::new(TerminalExitStatus::new().exit_code(1))
        }
    }

    pub fn on_output(&self, terminal_id: &TerminalId) -> TerminalOutputResponse {
        if let Some(TerminalCall::Output(expected_id, text, exit_code)) = self.pop("output") {
            self.validate_terminal_id("output", &expected_id, terminal_id);
            TerminalOutputResponse::new(text, false)
                .exit_status(TerminalExitStatus::new().exit_code(exit_code))
        } else {
            TerminalOutputResponse::new("", false)
        }
    }

    pub fn on_release(&self, terminal_id: &TerminalId) -> ReleaseTerminalResponse {
        if let Some(TerminalCall::Release(expected_id)) = self.pop("release") {
            self.validate_terminal_id("release", &expected_id, terminal_id);
        }
        ReleaseTerminalResponse::new()
    }

    pub fn on_kill(&self, terminal_id: &TerminalId) -> KillTerminalResponse {
        if let Some(TerminalCall::Kill(expected_id)) = self.pop("kill") {
            self.validate_terminal_id("kill", &expected_id, terminal_id);
        }
        KillTerminalResponse::new()
    }

    pub fn assert_called(&self) {
        let errors = self.errors.lock().unwrap();
        assert!(errors.is_empty(), "terminal fixture errors: {errors:?}");
        let queue = self.queue.lock().unwrap();
        assert!(
            queue.is_empty(),
            "terminal fixture has unconsumed calls: {queue:?}"
        );
    }
}

#[derive(Debug)]
pub struct SessionData<S> {
    pub session: S,
    pub models: Option<SessionModelState>,
    pub modes: Option<SessionModeState>,
}

pub struct TestConnectionConfig {
    pub mcp_servers: Vec<McpServer>,
    pub builtins: Vec<String>,
    pub goose_mode: GooseMode,
    pub cwd: Option<tempfile::TempDir>,
    pub data_root: PathBuf,
    pub provider_factory: Option<AcpProviderFactory>,
    pub read_text_file: Option<ReadTextFileHandler>,
    pub write_text_file: Option<WriteTextFileHandler>,
    pub terminal: Option<Arc<TerminalFixture>>,
    // When true, strips config_options from responses to test the legacy set_mode/set_model path.
    #[allow(dead_code)]
    pub strip_config_options: bool,
    // The model the server-side provider starts with. Defaults to TEST_MODEL.
    pub current_model: String,
    pub disable_session_naming: bool,
}

impl Default for TestConnectionConfig {
    fn default() -> Self {
        Self {
            mcp_servers: Vec::new(),
            builtins: Vec::new(),
            goose_mode: GooseMode::default(),
            cwd: None,
            data_root: PathBuf::new(),
            provider_factory: None,
            read_text_file: None,
            write_text_file: None,
            terminal: None,
            strip_config_options: false,
            current_model: TEST_MODEL.to_string(),
            disable_session_naming: true,
        }
    }
}

#[async_trait]
pub trait Connection: Sized {
    type Session: Session;

    fn expected_session_id() -> Arc<dyn ExpectedSessionId>;
    async fn new(config: TestConnectionConfig, openai: OpenAiFixture) -> Self;
    async fn new_session(&mut self) -> anyhow::Result<SessionData<Self::Session>>;
    async fn load_session(
        &mut self,
        session_id: &str,
        mcp_servers: Vec<McpServer>,
    ) -> anyhow::Result<SessionData<Self::Session>>;
    async fn list_sessions(&self) -> anyhow::Result<ListSessionsResponse>;
    async fn close_session(&self, session_id: &str) -> anyhow::Result<()>;
    async fn delete_session(&self, session_id: &str) -> anyhow::Result<()>;
    async fn set_mode(&self, session_id: &str, mode_id: &str) -> anyhow::Result<()>;
    async fn set_model(&self, session_id: &str, model_id: &str) -> anyhow::Result<()>;
    async fn set_config_option(
        &self,
        session_id: &str,
        config_id: &str,
        value: &str,
    ) -> anyhow::Result<()>;
    fn data_root(&self) -> std::path::PathBuf;
    fn reset_openai(&self);
    fn reset_permissions(&self);
}

#[async_trait]
pub trait Session: std::fmt::Debug {
    fn session_id(&self) -> &agent_client_protocol::schema::SessionId;
    fn work_dir(&self) -> std::path::PathBuf;
    fn notifications(&self) -> Vec<Notification>;
    async fn prompt(
        &mut self,
        text: &str,
        decision: PermissionDecision,
    ) -> anyhow::Result<TestOutput>;
    async fn prompt_with_image(
        &mut self,
        text: &str,
        image_b64: &str,
        mime_type: &str,
        decision: PermissionDecision,
    ) -> anyhow::Result<TestOutput>;
}

#[allow(dead_code)]
pub fn run_test<F>(fut: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    register_builtin_extensions(goose_mcp::BUILTIN_EXTENSIONS.clone());

    let handle = std::thread::Builder::new()
        .name("acp-test".to_string())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_stack_size(8 * 1024 * 1024)
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(fut);
        })
        .unwrap();
    if let Err(err) = handle.join() {
        // Re-raise the original panic so the test shows the real failure message.
        std::panic::resume_unwind(err);
    }
}

pub async fn send_custom(
    cx: &agent_client_protocol::ConnectionTo<agent_client_protocol::Agent>,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, agent_client_protocol::Error> {
    let msg = agent_client_protocol::UntypedMessage::new(method, params).unwrap();
    cx.send_request(msg).block_task().await
}

pub mod provider;
pub mod server;
