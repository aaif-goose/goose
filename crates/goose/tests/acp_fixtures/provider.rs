use super::{
    spawn_acp_server_in_process, Connection, DuplexTransport, OpenAiFixture, PermissionDecision,
    Session, SessionData, TestConnectionConfig, TestOutput,
};
use agent_client_protocol::schema::{
    ListSessionsResponse, McpServer, ModelId, ModelInfo, SessionModelState, SessionUpdate,
    ToolCallStatus,
};
use agent_client_protocol::{Channel, Client, ConnectTo, DynConnectTo};
use async_trait::async_trait;
use futures::StreamExt;
use goose::acp::{AcpProvider, AcpProviderConfig};
use goose::config::{GooseMode, PermissionManager};
use goose::conversation::message::{ActionRequiredData, Message, MessageContent};
use goose::model::ModelConfig;
use goose::permission::permission_confirmation::PrincipalType;
use goose::permission::{Permission, PermissionConfirmation};
use goose::providers::base::Provider;
use goose_test_support::{ExpectedSessionId, IgnoreSessionId, TEST_MODEL};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use strum::VariantNames;
use tokio::sync::Mutex;

pub type NotificationSink = Arc<std::sync::Mutex<Vec<SessionUpdate>>>;
type SessionModels = Arc<std::sync::Mutex<HashMap<String, ModelConfig>>>;

#[allow(dead_code)]
pub struct AcpProviderConnection {
    /// Option so close_session can trigger session/close via Drop.
    provider: Arc<Mutex<Option<AcpProvider>>>,
    permission_manager: Arc<PermissionManager>,
    session_counter: usize,
    notification_sink: NotificationSink,
    session_models: SessionModels,
    strip_config_options: bool,
    work_dir: std::path::PathBuf,
    data_root: std::path::PathBuf,
    _openai: OpenAiFixture,
    _temp_dir: Option<tempfile::TempDir>,
    _cwd: Option<tempfile::TempDir>,
}

#[allow(dead_code)]
pub struct AcpProviderSession {
    provider: Arc<Mutex<Option<AcpProvider>>>,
    session_id: agent_client_protocol::schema::SessionId,
    notification_sink: NotificationSink,
    session_models: SessionModels,
    work_dir: std::path::PathBuf,
}

impl std::fmt::Debug for AcpProviderSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpProviderSession")
            .field("session_id", &self.session_id)
            .finish()
    }
}

impl AcpProviderSession {
    #[allow(dead_code)]
    async fn send_message(
        &mut self,
        message: Message,
        decision: PermissionDecision,
    ) -> anyhow::Result<TestOutput> {
        let session_id = self.session_id.0.clone();
        let guard = self.provider.lock().await;
        let provider = guard.as_ref().unwrap();
        self.notification_sink.lock().unwrap().clear();
        let model_config = self
            .session_models
            .lock()
            .unwrap()
            .get(session_id.as_ref())
            .cloned()
            .unwrap_or_else(|| provider.get_model_config());
        let mut stream = provider
            .stream(&model_config, &session_id, "", &[message], &[])
            .await?;
        let mut text = String::new();
        let mut tool_error = false;
        let mut saw_tool = false;

        while let Some(item) = stream.next().await {
            let (msg, _) = item.unwrap();
            if let Some(msg) = msg {
                for content in msg.content {
                    match content {
                        MessageContent::Text(t) => {
                            text.push_str(&t.text);
                        }
                        MessageContent::ToolResponse(resp) => {
                            saw_tool = true;
                            if let Ok(result) = resp.tool_result {
                                tool_error |= result.is_error.unwrap_or(false);
                            }
                        }
                        MessageContent::ActionRequired(action) => {
                            if let ActionRequiredData::ToolConfirmation { id, .. } = action.data {
                                saw_tool = true;
                                tool_error |= decision.should_record_rejection();

                                let confirmation = PermissionConfirmation {
                                    principal_type: PrincipalType::Tool,
                                    permission: Permission::from(decision),
                                };

                                let handled = provider
                                    .handle_permission_confirmation(&id, &confirmation)
                                    .await;
                                assert!(handled);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let tool_status = if saw_tool {
            Some(if tool_error {
                ToolCallStatus::Failed
            } else {
                ToolCallStatus::Completed
            })
        } else {
            None
        };

        Ok(TestOutput { text, tool_status })
    }
}

#[async_trait]
impl Connection for AcpProviderConnection {
    type Session = AcpProviderSession;

    fn expected_session_id() -> Arc<dyn ExpectedSessionId> {
        Arc::new(IgnoreSessionId)
    }

    async fn new(config: TestConnectionConfig, openai: OpenAiFixture) -> Self {
        let (data_root, temp_dir) = match config.data_root.as_os_str().is_empty() {
            true => {
                let temp_dir = tempfile::tempdir().unwrap();
                (temp_dir.path().to_path_buf(), Some(temp_dir))
            }
            false => (config.data_root.clone(), None),
        };

        let goose_mode = config.goose_mode;
        let mcp_servers = config.mcp_servers;

        let current_model = config.current_model.clone();
        let (transport, _handle, permission_manager) = spawn_acp_server_in_process(
            openai.uri(),
            &config.builtins,
            data_root.as_path(),
            goose_mode,
            config.provider_factory,
            &current_model,
            config.disable_session_naming,
        )
        .await;

        let cwd_path = config
            .cwd
            .as_ref()
            .map(|td| td.path().to_path_buf())
            .unwrap_or_else(|| data_root.clone());

        let notification_sink: NotificationSink = Arc::new(std::sync::Mutex::new(Vec::new()));
        let session_models: SessionModels = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let sink_clone = notification_sink.clone();
        let provider_config = AcpProviderConfig {
            command: "unused".into(),
            args: vec![],
            env: vec![],
            env_remove: vec![],
            work_dir: cwd_path.clone(),
            mcp_servers,
            session_mode_id: None,
            mode_mapping: GooseMode::VARIANTS
                .iter()
                .map(|v| {
                    let mode = GooseMode::from_str(v).unwrap();
                    (mode, mode.to_string())
                })
                .collect(),
            notification_callback: Some(Arc::new(move |n| {
                sink_clone.lock().unwrap().push(n.update.clone());
            })),
        };

        // Server always advertises both configOptions and legacy; only the client fallback needs testing.
        let transport: DynConnectTo<Client> = if config.strip_config_options {
            DynConnectTo::new(strip_config_options(transport))
        } else {
            DynConnectTo::new(transport)
        };
        let provider = AcpProvider::connect_with_transport(
            "acp-test".to_string(),
            ModelConfig::new(TEST_MODEL).unwrap(),
            goose_mode,
            provider_config,
            transport,
        )
        .await
        .unwrap();

        Self {
            provider: Arc::new(Mutex::new(Some(provider))),
            permission_manager,
            session_counter: 0,
            notification_sink,
            session_models,
            strip_config_options: config.strip_config_options,
            work_dir: cwd_path,
            data_root,
            _openai: openai,
            _temp_dir: temp_dir,
            _cwd: config.cwd,
        }
    }

    async fn new_session(&mut self) -> anyhow::Result<SessionData<AcpProviderSession>> {
        self.session_counter += 1;
        let goose_id = format!("test-session-{}", self.session_counter);

        let models = if self.strip_config_options {
            None
        } else {
            let provider = self.provider.lock().await;
            let provider = provider.as_ref().unwrap();
            let available_models = provider.fetch_supported_models().await?;
            Some(SessionModelState::new(
                ModelId::new(provider.get_model_config().model_name.clone()),
                available_models
                    .into_iter()
                    .map(|model_id| ModelInfo::new(ModelId::new(model_id.clone()), model_id))
                    .collect(),
            ))
        };

        let session = AcpProviderSession {
            provider: Arc::clone(&self.provider),
            session_id: agent_client_protocol::schema::SessionId::new(goose_id),
            notification_sink: self.notification_sink.clone(),
            session_models: self.session_models.clone(),
            work_dir: self.work_dir.clone(),
        };
        self.notification_sink.lock().unwrap().clear();
        Ok(SessionData {
            session,
            models,
            modes: None,
        })
    }

    async fn load_session(
        &mut self,
        _session_id: &str,
        _mcp_servers: Vec<McpServer>,
    ) -> anyhow::Result<SessionData<AcpProviderSession>> {
        Err(agent_client_protocol::Error::internal_error()
            .data("load_session not implemented for ACP provider")
            .into())
    }

    async fn list_sessions(&self) -> anyhow::Result<ListSessionsResponse> {
        Err(anyhow::anyhow!("not implemented for AcpProviderConnection"))
    }

    async fn close_session(&self, _session_id: &str) -> anyhow::Result<()> {
        // ACP close exists but SessionManager isn't integrated with it; drop the provider instead.
        self.provider.lock().await.take();
        Ok(())
    }

    async fn delete_session(&self, _session_id: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("not implemented for AcpProviderConnection"))
    }

    fn data_root(&self) -> std::path::PathBuf {
        self.data_root.clone()
    }

    async fn set_mode(&self, _session_id: &str, _mode_id: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("not implemented for AcpProviderConnection"))
    }

    async fn set_model(&self, _session_id: &str, _model_id: &str) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("not implemented for AcpProviderConnection"))
    }

    async fn set_config_option(
        &self,
        _session_id: &str,
        _config_id: &str,
        _value: &str,
    ) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("not implemented for AcpProviderConnection"))
    }

    fn reset_openai(&self) {
        self._openai.reset();
    }

    fn reset_permissions(&self) {
        // "" matches all extensions, clearing all stored permission decisions
        self.permission_manager.remove_extension("");
    }
}

#[async_trait]
impl Session for AcpProviderSession {
    fn session_id(&self) -> &agent_client_protocol::schema::SessionId {
        &self.session_id
    }

    fn work_dir(&self) -> std::path::PathBuf {
        self.work_dir.clone()
    }

    fn notifications(&self) -> Vec<super::Notification> {
        let updates: Vec<_> = self.notification_sink.lock().unwrap().drain(..).collect();
        super::to_notifications(&updates)
    }

    async fn prompt(
        &mut self,
        prompt: &str,
        decision: PermissionDecision,
    ) -> anyhow::Result<TestOutput> {
        self.send_message(Message::user().with_text(prompt), decision)
            .await
    }

    async fn prompt_with_image(
        &mut self,
        prompt: &str,
        image_b64: &str,
        mime_type: &str,
        decision: PermissionDecision,
    ) -> anyhow::Result<TestOutput> {
        let message = Message::user()
            .with_image(image_b64, mime_type)
            .with_text(prompt);
        self.send_message(message, decision).await
    }
}

// Strips config_options from responses so goose falls back to legacy set_mode/set_model.
#[allow(dead_code)]
fn strip_config_options(transport: DuplexTransport) -> Channel {
    let (server, server_future) = ConnectTo::<Client>::into_channel_and_future(transport);
    let (client_channel, filter) = Channel::duplex();

    tokio::spawn(async move {
        if let Err(e) = server_future.await {
            tracing::error!("config_options filter transport error: {e}");
        }
    });

    tokio::spawn(async move {
        let mut stripped_initial_config = HashSet::new();

        let goose_to_server = async {
            let mut from_goose = filter.rx;
            while let Some(msg) = from_goose.next().await {
                if server.tx.unbounded_send(msg).is_err() {
                    break;
                }
            }
        };

        let server_to_goose = async {
            let mut from_server = server.rx;
            while let Some(msg) = from_server.next().await {
                let msg = match msg {
                    Ok(m) => match m {
                        agent_client_protocol::jsonrpcmsg::Message::Response(mut resp) => {
                            if let Some(ref mut result) = resp.result {
                                if let Some(obj) = result.as_object_mut() {
                                    obj.remove("configOptions");
                                }
                            }
                            Ok(Some(agent_client_protocol::jsonrpcmsg::Message::Response(
                                resp,
                            )))
                        }
                        agent_client_protocol::jsonrpcmsg::Message::Request(req)
                            if req.id.is_none()
                                && req.method == "session/update"
                                && req
                                    .params
                                    .as_ref()
                                    .and_then(|params| match params {
                                        agent_client_protocol::jsonrpcmsg::Params::Object(obj) => {
                                            Some(obj)
                                        }
                                        _ => None,
                                    })
                                    .and_then(|obj| obj.get("update"))
                                    .and_then(|update| update.get("sessionUpdate"))
                                    .and_then(|session_update| session_update.as_str())
                                    == Some("config_option_update") =>
                        {
                            let session_id = req
                                .params
                                .as_ref()
                                .and_then(|params| match params {
                                    agent_client_protocol::jsonrpcmsg::Params::Object(obj) => {
                                        Some(obj)
                                    }
                                    _ => None,
                                })
                                .and_then(|obj| obj.get("sessionId"))
                                .and_then(|session_id| session_id.as_str())
                                .map(str::to_owned);
                            if let Some(session_id) = session_id {
                                if stripped_initial_config.insert(session_id) {
                                    Ok(None)
                                } else {
                                    Ok(Some(agent_client_protocol::jsonrpcmsg::Message::Request(
                                        req,
                                    )))
                                }
                            } else {
                                Ok(Some(agent_client_protocol::jsonrpcmsg::Message::Request(
                                    req,
                                )))
                            }
                        }
                        other => Ok(Some(other)),
                    },
                    Err(err) => Err(err),
                };
                match msg {
                    Ok(Some(msg)) => {
                        if filter.tx.unbounded_send(Ok(msg)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => continue,
                    Err(err) => {
                        if filter.tx.unbounded_send(Err(err)).is_err() {
                            break;
                        }
                    }
                }
            }
        };

        futures::join!(goose_to_server, server_to_goose);
    });

    client_channel
}
