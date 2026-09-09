use super::{
    map_permission_response, spawn_acp_server_in_process, Connection, ModelStateFixture,
    PermissionDecision, Session, SessionData, TestConnectionConfig, TestOutput,
};
use agent_client_protocol::schema::v1::{
    ClientCapabilities, CloseSessionRequest, ContentBlock, CreateTerminalRequest,
    DeleteSessionRequest, FileSystemCapabilities, ImageContent, InitializeRequest,
    KillTerminalRequest, ListSessionsRequest, ListSessionsResponse, LoadSessionRequest, McpServer,
    NewSessionRequest, PromptRequest, ReadTextFileRequest, ReleaseTerminalRequest,
    RequestPermissionRequest, SessionConfigKind, SessionConfigOptionCategory,
    SessionConfigOptionValue, SessionId, SessionModeId, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, SetSessionModeRequest, StopReason, TerminalOutputRequest,
    TextContent, ToolCallStatus, WaitForTerminalExitRequest, WriteTextFileRequest,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{Agent, Client, ConnectionTo};
use async_trait::async_trait;
use futures::io::BufReader;
use futures::{AsyncBufReadExt, AsyncWriteExt, StreamExt};
use goose::config::PermissionManager;
use goose_test_support::{ExpectedSessionId, IgnoreSessionId};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

pub struct AcpServerConnection {
    cx: ConnectionTo<Agent>,
    // MCP servers from config, consumed by the first new_session call.
    pending_mcp_servers: Vec<McpServer>,
    cwd: Option<tempfile::TempDir>,
    data_root: std::path::PathBuf,
    updates: Arc<Mutex<Vec<SessionNotification>>>,
    permission: Arc<Mutex<PermissionDecision>>,
    notify: Arc<Notify>,
    permission_manager: Arc<PermissionManager>,
    _openai: super::OpenAiFixture,
    _temp_dir: Option<tempfile::TempDir>,
}

pub struct AcpServerSession {
    cx: ConnectionTo<Agent>,
    session_id: agent_client_protocol::schema::v1::SessionId,
    updates: Arc<Mutex<Vec<SessionNotification>>>,
    permission: Arc<Mutex<PermissionDecision>>,
    notify: Arc<Notify>,
    _work_dir: tempfile::TempDir,
}

impl std::fmt::Debug for AcpServerSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcpServerSession")
            .field("session_id", &self.session_id)
            .finish()
    }
}

impl AcpServerSession {
    pub fn session_updates(&self) -> Vec<SessionUpdate> {
        self.updates
            .lock()
            .unwrap()
            .drain(..)
            .map(|n| n.update)
            .collect()
    }

    async fn send_prompt(
        &mut self,
        content: Vec<ContentBlock>,
        decision: PermissionDecision,
    ) -> anyhow::Result<TestOutput> {
        *self.permission.lock().unwrap() = decision;
        self.updates.lock().unwrap().clear();

        let response = self
            .cx
            .send_request(PromptRequest::new(self.session_id.clone(), content))
            .block_task()
            .await?;

        assert_eq!(response.stop_reason, StopReason::EndTurn);

        let mut updates_len = self.updates.lock().unwrap().len();
        while updates_len == 0 {
            self.notify.notified().await;
            updates_len = self.updates.lock().unwrap().len();
        }

        let text = collect_agent_text(&self.updates);
        let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
        let mut tool_status = extract_tool_status(&self.updates);
        while tool_status.is_none() && tokio::time::Instant::now() < deadline {
            tokio::task::yield_now().await;
            tool_status = extract_tool_status(&self.updates);
        }

        Ok(TestOutput { text, tool_status })
    }
}

impl AcpServerConnection {
    #[allow(dead_code)]
    pub fn cx(&self) -> &ConnectionTo<Agent> {
        &self.cx
    }
}

pub async fn assert_session_response_precedes_available_commands(
    transport: super::DuplexTransport,
    method: &str,
    params: serde_json::Value,
) {
    let (mut outgoing, incoming) = transport.into_parts();
    let mut incoming = BufReader::new(incoming).lines();

    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {}
        }
    });
    outgoing
        .write_all(format!("{initialize}\n").as_bytes())
        .await
        .unwrap();
    outgoing.flush().await.unwrap();
    let initialize_response = incoming.next().await.unwrap().unwrap();
    let initialize_response: serde_json::Value =
        serde_json::from_str(&initialize_response).unwrap();
    assert_eq!(initialize_response["id"], 1);

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": method,
        "params": params,
    });
    outgoing
        .write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();
    outgoing.flush().await.unwrap();

    let response = incoming.next().await.unwrap().unwrap();
    let response: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(response["id"], 2);
    let session_id = response["result"]["sessionId"]
        .as_str()
        .unwrap()
        .to_string();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let message = tokio::time::timeout_at(deadline, incoming.next())
            .await
            .expect("timed out waiting for available commands")
            .expect("ACP connection closed")
            .unwrap();
        let message: serde_json::Value = serde_json::from_str(&message).unwrap();
        if message["params"]["update"]["sessionUpdate"] != "available_commands_update" {
            continue;
        }

        assert_eq!(message["params"]["sessionId"], session_id);
        assert!(message["params"]["update"]["availableCommands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|command| command["name"] == "goal"));
        break;
    }
}

#[async_trait]
impl Connection for AcpServerConnection {
    type Session = AcpServerSession;

    fn expected_session_id() -> Arc<dyn ExpectedSessionId> {
        // The ACP session ID returned to clients is now a thread ID, which is
        // intentionally different from the internal session ID the agent sends
        // to the LLM provider. Skip strict matching.
        Arc::new(IgnoreSessionId)
    }

    async fn new(config: TestConnectionConfig, openai: super::OpenAiFixture) -> Self {
        let (data_root, temp_dir) = match config.data_root.as_os_str().is_empty() {
            true => {
                let temp_dir = tempfile::tempdir().unwrap();
                (temp_dir.path().to_path_buf(), Some(temp_dir))
            }
            false => (config.data_root.clone(), None),
        };

        let (transport, _handle, permission_manager) = spawn_acp_server_in_process(
            openai.uri(),
            &config.builtins,
            data_root.as_path(),
            config.goose_mode,
            config.provider_factory,
            &config.current_model,
            config.disable_session_naming,
        )
        .await;

        let updates = Arc::new(Mutex::new(Vec::new()));
        let notify = Arc::new(Notify::new());
        let permission = Arc::new(Mutex::new(PermissionDecision::Cancel));

        let mut fs_cap = FileSystemCapabilities::default();
        if config.read_text_file.is_some() {
            fs_cap = fs_cap.read_text_file(true);
        }
        if config.write_text_file.is_some() {
            fs_cap = fs_cap.write_text_file(true);
        }

        let cx = {
            let updates_clone = updates.clone();
            let notify_clone = notify.clone();
            let permission_clone = permission.clone();
            let read_handler = config.read_text_file;
            let write_handler = config.write_text_file;
            let terminal = config.terminal;

            let cx_holder: Arc<Mutex<Option<ConnectionTo<Agent>>>> = Arc::new(Mutex::new(None));
            let cx_holder_clone = cx_holder.clone();

            let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();

            tokio::spawn(async move {
                let result = Client
                    .builder()
                    .on_receive_notification(
                        {
                            let updates = updates_clone.clone();
                            let notify = notify_clone.clone();
                            async move |notification: SessionNotification, _cx| {
                                updates.lock().unwrap().push(notification);
                                notify.notify_waiters();
                                Ok(())
                            }
                        },
                        agent_client_protocol::on_receive_notification!(),
                    )
                    .on_receive_request(
                        {
                            let permission = permission_clone.clone();
                            async move |req: RequestPermissionRequest, responder, _connection_cx| {
                                let decision = *permission.lock().unwrap();
                                responder.respond(map_permission_response(&req, decision))
                            }
                        },
                        agent_client_protocol::on_receive_request!(),
                    )
                    .on_receive_request(
                        async move |req: ReadTextFileRequest, responder, _cx| match read_handler {
                            Some(ref rh) => match rh(&req) {
                                Ok(resp) => responder.respond(resp),
                                Err(msg) => responder.respond_with_internal_error(msg),
                            },
                            None => responder.respond_with_error(
                                agent_client_protocol::Error::method_not_found(),
                            ),
                        },
                        agent_client_protocol::on_receive_request!(),
                    )
                    .on_receive_request(
                        async move |req: WriteTextFileRequest, responder, _cx| match write_handler {
                            Some(ref wh) => match wh(&req) {
                                Ok(resp) => responder.respond(resp),
                                Err(msg) => responder.respond_with_internal_error(msg),
                            },
                            None => responder.respond_with_error(
                                agent_client_protocol::Error::method_not_found(),
                            ),
                        },
                        agent_client_protocol::on_receive_request!(),
                    )
                    .on_receive_request(
                        {
                            let t = terminal.clone();
                            async move |req: CreateTerminalRequest, responder, _cx| match t {
                                Some(ref f) => responder.respond(f.on_create(&req.command)),
                                None => responder.respond_with_error(
                                    agent_client_protocol::Error::method_not_found(),
                                ),
                            }
                        },
                        agent_client_protocol::on_receive_request!(),
                    )
                    .on_receive_request(
                        {
                            let t = terminal.clone();
                            async move |req: WaitForTerminalExitRequest, responder, _cx| match t {
                                Some(ref f) => {
                                    responder.respond(f.on_wait_for_exit(&req.terminal_id))
                                }
                                None => responder.respond_with_error(
                                    agent_client_protocol::Error::method_not_found(),
                                ),
                            }
                        },
                        agent_client_protocol::on_receive_request!(),
                    )
                    .on_receive_request(
                        {
                            let t = terminal.clone();
                            async move |req: TerminalOutputRequest, responder, _cx| match t {
                                Some(ref f) => responder.respond(f.on_output(&req.terminal_id)),
                                None => responder.respond_with_error(
                                    agent_client_protocol::Error::method_not_found(),
                                ),
                            }
                        },
                        agent_client_protocol::on_receive_request!(),
                    )
                    .on_receive_request(
                        {
                            let t = terminal.clone();
                            async move |req: ReleaseTerminalRequest, responder, _cx| match t {
                                Some(ref f) => responder.respond(f.on_release(&req.terminal_id)),
                                None => responder.respond_with_error(
                                    agent_client_protocol::Error::method_not_found(),
                                ),
                            }
                        },
                        agent_client_protocol::on_receive_request!(),
                    )
                    .on_receive_request(
                        {
                            let t = terminal.clone();
                            async move |req: KillTerminalRequest, responder, _cx| match t {
                                Some(ref f) => responder.respond(f.on_kill(&req.terminal_id)),
                                None => responder.respond_with_error(
                                    agent_client_protocol::Error::method_not_found(),
                                ),
                            }
                        },
                        agent_client_protocol::on_receive_request!(),
                    )
                    .connect_with(transport.into_byte_streams(), {
                        let cx_holder = cx_holder_clone;
                        async move |cx: ConnectionTo<Agent>| {
                            let resp = cx
                                .send_request(
                                    InitializeRequest::new(ProtocolVersion::V1)
                                        .client_capabilities(
                                            ClientCapabilities::new()
                                                .fs(fs_cap)
                                                .terminal(terminal.is_some()),
                                        ),
                                )
                                .block_task()
                                .await
                                .unwrap();
                            assert_eq!(
                                resp.protocol_version,
                                ProtocolVersion::V1,
                                "initialize response must negotiate ACP V1"
                            );
                            assert_eq!(
                                resp.agent_info.as_ref().map(|info| info.name.as_str()),
                                Some("goose"),
                                "initialize response must identify the agent"
                            );
                            assert!(
                                resp.agent_capabilities
                                    .session_capabilities
                                    .delete
                                    .as_ref()
                                    .is_some(),
                                "initialize response must advertise session/delete"
                            );

                            *cx_holder.lock().unwrap() = Some(cx.clone());
                            let _ = ready_tx.send(());

                            std::future::pending::<Result<(), agent_client_protocol::Error>>().await
                        }
                    })
                    .await;

                if let Err(e) = result {
                    tracing::error!("SACP client error: {e}");
                }
            });

            ready_rx.await.unwrap();
            let cx = cx_holder.lock().unwrap().take().unwrap();
            cx
        };

        Self {
            cx,
            pending_mcp_servers: config.mcp_servers,
            cwd: config.cwd,
            data_root,
            updates,
            permission,
            notify,
            permission_manager,
            _openai: openai,
            _temp_dir: temp_dir,
        }
    }

    async fn new_session(&mut self) -> anyhow::Result<SessionData<AcpServerSession>> {
        let work_dir = self
            .cwd
            .take()
            .unwrap_or_else(|| tempfile::tempdir().unwrap());
        let mcp_servers = std::mem::take(&mut self.pending_mcp_servers);
        let response = self
            .cx
            .send_request(NewSessionRequest::new(work_dir.path()).mcp_servers(mcp_servers))
            .block_task()
            .await?;
        let session = AcpServerSession {
            cx: self.cx.clone(),
            session_id: response.session_id.clone(),
            updates: self.updates.clone(),
            permission: self.permission.clone(),
            notify: self.notify.clone(),
            _work_dir: work_dir,
        };
        let models = extract_model_state_from_config_options(response.config_options.as_deref());
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !self.updates.lock().unwrap().iter().any(|notification| {
            notification.session_id == response.session_id
                && matches!(
                    &notification.update,
                    SessionUpdate::AvailableCommandsUpdate(_)
                )
        }) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for initial available commands"
            );
            tokio::task::yield_now().await;
        }
        self.updates.lock().unwrap().clear();
        Ok(SessionData {
            session,
            models,
            modes: response.modes,
        })
    }

    async fn load_session(
        &mut self,
        session_id: &str,
        mcp_servers: Vec<McpServer>,
    ) -> anyhow::Result<SessionData<AcpServerSession>> {
        self.updates.lock().unwrap().clear();
        let work_dir = tempfile::tempdir().unwrap();
        let session_id = agent_client_protocol::schema::v1::SessionId::new(session_id.to_string());
        let response = self
            .cx
            .send_request(
                LoadSessionRequest::new(session_id.clone(), work_dir.path())
                    .mcp_servers(mcp_servers),
            )
            .block_task()
            .await?;
        let session = AcpServerSession {
            cx: self.cx.clone(),
            session_id,
            updates: self.updates.clone(),
            permission: self.permission.clone(),
            notify: self.notify.clone(),
            _work_dir: work_dir,
        };
        let models = extract_model_state_from_config_options(response.config_options.as_deref());
        Ok(SessionData {
            session,
            models,
            modes: response.modes,
        })
    }

    async fn list_sessions(&self) -> anyhow::Result<ListSessionsResponse> {
        self.cx
            .send_request(ListSessionsRequest::new())
            .block_task()
            .await
            .map_err(|e| e.into())
    }

    async fn close_session(&self, session_id: &str) -> anyhow::Result<()> {
        self.cx
            .send_request(CloseSessionRequest::new(SessionId::new(session_id)))
            .block_task()
            .await
            .map(|_| ())
            .map_err(|e| e.into())
    }

    async fn delete_session(&self, session_id: &str) -> anyhow::Result<()> {
        self.cx
            .send_request(DeleteSessionRequest::new(SessionId::new(session_id)))
            .block_task()
            .await
            .map(|_| ())
            .map_err(|e| e.into())
    }

    async fn set_mode(&self, session_id: &str, mode_id: &str) -> anyhow::Result<()> {
        self.cx
            .send_request(SetSessionModeRequest::new(
                SessionId::new(session_id),
                SessionModeId::new(mode_id),
            ))
            .block_task()
            .await
            .map(|_| ())
            .map_err(|e| e.into())
    }

    async fn set_model(&self, session_id: &str, model_id: &str) -> anyhow::Result<()> {
        self.set_config_option(session_id, "model", model_id).await
    }

    async fn set_config_option(
        &self,
        session_id: &str,
        config_id: &str,
        value: &str,
    ) -> anyhow::Result<()> {
        self.cx
            .send_request(SetSessionConfigOptionRequest::new(
                SessionId::new(session_id),
                config_id.to_string(),
                SessionConfigOptionValue::value_id(value.to_string()),
            ))
            .block_task()
            .await
            .map(|_| ())
            .map_err(|e| e.into())
    }

    fn data_root(&self) -> std::path::PathBuf {
        self.data_root.clone()
    }

    fn reset_openai(&self) {
        self._openai.reset();
    }

    fn reset_permissions(&self) {
        self.permission_manager.clear_permissions();
    }
}

#[async_trait]
impl Session for AcpServerSession {
    fn session_id(&self) -> &agent_client_protocol::schema::v1::SessionId {
        &self.session_id
    }

    fn work_dir(&self) -> std::path::PathBuf {
        self._work_dir.path().to_path_buf()
    }

    fn session_updates(&self) -> Vec<SessionUpdate> {
        AcpServerSession::session_updates(self)
    }

    fn notifications(&self) -> Vec<super::Notification> {
        super::to_notifications(&self.session_updates())
    }

    async fn prompt(
        &mut self,
        text: &str,
        decision: PermissionDecision,
    ) -> anyhow::Result<TestOutput> {
        self.send_prompt(vec![ContentBlock::Text(TextContent::new(text))], decision)
            .await
    }

    async fn prompt_with_image(
        &mut self,
        text: &str,
        image_b64: &str,
        mime_type: &str,
        decision: PermissionDecision,
    ) -> anyhow::Result<TestOutput> {
        self.send_prompt(
            vec![
                ContentBlock::Image(ImageContent::new(image_b64, mime_type)),
                ContentBlock::Text(TextContent::new(text)),
            ],
            decision,
        )
        .await
    }
}

fn extract_model_state_from_config_options(
    config_options: Option<&[agent_client_protocol::schema::v1::SessionConfigOption]>,
) -> Option<ModelStateFixture> {
    let option = config_options?
        .iter()
        .find(|option| option.category.as_ref() == Some(&SessionConfigOptionCategory::Model))?;
    let SessionConfigKind::Select(select) = &option.kind else {
        return None;
    };

    let available_models = match &select.options {
        agent_client_protocol::schema::v1::SessionConfigSelectOptions::Ungrouped(options) => {
            options
                .iter()
                .map(|option| option.value.0.to_string())
                .collect()
        }
        agent_client_protocol::schema::v1::SessionConfigSelectOptions::Grouped(groups) => groups
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

    Some(ModelStateFixture {
        current_model_id: select.current_value.0.to_string(),
        available_models,
    })
}

fn collect_agent_text(updates: &Arc<Mutex<Vec<SessionNotification>>>) -> String {
    let guard = updates.lock().unwrap();
    let mut text = String::new();

    for notification in guard.iter() {
        if let SessionUpdate::AgentMessageChunk(chunk) = &notification.update {
            if let ContentBlock::Text(t) = &chunk.content {
                text.push_str(&t.text);
            }
        }
    }

    text
}

fn extract_tool_status(updates: &Arc<Mutex<Vec<SessionNotification>>>) -> Option<ToolCallStatus> {
    let guard = updates.lock().unwrap();
    guard.iter().find_map(|notification| {
        if let SessionUpdate::ToolCallUpdate(update) = &notification.update {
            return update.fields.status;
        }
        None
    })
}
