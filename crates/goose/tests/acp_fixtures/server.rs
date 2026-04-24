use super::{
    map_permission_response, spawn_acp_server_in_process, Connection, PermissionDecision, Session,
    SessionData, TestConnectionConfig, TestOutput,
};
use async_trait::async_trait;
use goose::config::PermissionManager;
use goose_test_support::{ExpectedSessionId, IgnoreSessionId};
use sacp::schema::{
    ClientCapabilities, CloseSessionRequest, ContentBlock, CreateTerminalRequest,
    FileSystemCapabilities, ImageContent, InitializeRequest, KillTerminalRequest,
    ListSessionsRequest, ListSessionsResponse, LoadSessionRequest, McpServer, ModelId, ModelInfo,
    NewSessionRequest, PromptRequest, ProtocolVersion, ReadTextFileRequest, ReleaseTerminalRequest,
    RequestPermissionRequest, SessionConfigKind, SessionConfigOptionCategory,
    SessionConfigOptionValue, SessionId, SessionModeId, SessionModelState, SessionNotification,
    SessionUpdate, SetSessionConfigOptionRequest, SetSessionModeRequest, SetSessionModelRequest,
    StopReason, TerminalOutputRequest, TextContent, ToolCallStatus, WaitForTerminalExitRequest,
    WriteTextFileRequest,
};
use sacp::{Agent, Client, ConnectionTo};
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
    session_id: sacp::schema::SessionId,
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
                        sacp::on_receive_notification!(),
                    )
                    .on_receive_request(
                        {
                            let permission = permission_clone.clone();
                            async move |req: RequestPermissionRequest, responder, _connection_cx| {
                                let decision = *permission.lock().unwrap();
                                responder.respond(map_permission_response(&req, decision))
                            }
                        },
                        sacp::on_receive_request!(),
                    )
                    .on_receive_request(
                        async move |req: ReadTextFileRequest, responder, _cx| match read_handler {
                            Some(ref rh) => match rh(&req) {
                                Ok(resp) => responder.respond(resp),
                                Err(msg) => responder.respond_with_internal_error(msg),
                            },
                            None => responder.respond_with_error(sacp::Error::method_not_found()),
                        },
                        sacp::on_receive_request!(),
                    )
                    .on_receive_request(
                        async move |req: WriteTextFileRequest, responder, _cx| match write_handler {
                            Some(ref wh) => match wh(&req) {
                                Ok(resp) => responder.respond(resp),
                                Err(msg) => responder.respond_with_internal_error(msg),
                            },
                            None => responder.respond_with_error(sacp::Error::method_not_found()),
                        },
                        sacp::on_receive_request!(),
                    )
                    .on_receive_request(
                        {
                            let t = terminal.clone();
                            async move |req: CreateTerminalRequest, responder, _cx| match t {
                                Some(ref f) => responder.respond(f.on_create(&req.command)),
                                None => {
                                    responder.respond_with_error(sacp::Error::method_not_found())
                                }
                            }
                        },
                        sacp::on_receive_request!(),
                    )
                    .on_receive_request(
                        {
                            let t = terminal.clone();
                            async move |req: WaitForTerminalExitRequest, responder, _cx| match t {
                                Some(ref f) => {
                                    responder.respond(f.on_wait_for_exit(&req.terminal_id))
                                }
                                None => {
                                    responder.respond_with_error(sacp::Error::method_not_found())
                                }
                            }
                        },
                        sacp::on_receive_request!(),
                    )
                    .on_receive_request(
                        {
                            let t = terminal.clone();
                            async move |req: TerminalOutputRequest, responder, _cx| match t {
                                Some(ref f) => responder.respond(f.on_output(&req.terminal_id)),
                                None => {
                                    responder.respond_with_error(sacp::Error::method_not_found())
                                }
                            }
                        },
                        sacp::on_receive_request!(),
                    )
                    .on_receive_request(
                        {
                            let t = terminal.clone();
                            async move |req: ReleaseTerminalRequest, responder, _cx| match t {
                                Some(ref f) => responder.respond(f.on_release(&req.terminal_id)),
                                None => {
                                    responder.respond_with_error(sacp::Error::method_not_found())
                                }
                            }
                        },
                        sacp::on_receive_request!(),
                    )
                    .on_receive_request(
                        {
                            let t = terminal.clone();
                            async move |req: KillTerminalRequest, responder, _cx| match t {
                                Some(ref f) => responder.respond(f.on_kill(&req.terminal_id)),
                                None => {
                                    responder.respond_with_error(sacp::Error::method_not_found())
                                }
                            }
                        },
                        sacp::on_receive_request!(),
                    )
                    .connect_with(transport, {
                        let cx_holder = cx_holder_clone;
                        async move |cx: ConnectionTo<Agent>| {
                            let _resp = cx
                                .send_request(
                                    InitializeRequest::new(ProtocolVersion::LATEST)
                                        .client_capabilities(
                                            ClientCapabilities::new()
                                                .fs(fs_cap)
                                                .terminal(terminal.is_some()),
                                        ),
                                )
                                .block_task()
                                .await
                                .unwrap();

                            *cx_holder.lock().unwrap() = Some(cx.clone());
                            let _ = ready_tx.send(());

                            std::future::pending::<Result<(), sacp::Error>>().await
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
        let models = response.models.or_else(|| {
            extract_model_state_from_config_options(response.config_options.as_deref())
        });
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
        let session_id = sacp::schema::SessionId::new(session_id.to_string());
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
        Ok(SessionData {
            session,
            models: response.models,
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
        super::send_custom(
            &self.cx,
            "session/delete",
            serde_json::json!({ "sessionId": session_id }),
        )
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
        self.cx
            .send_request(SetSessionModelRequest::new(
                SessionId::new(session_id),
                model_id.to_string(),
            ))
            .block_task()
            .await
            .map(|_| ())
            .map_err(|e| e.into())
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
        // "" matches all extensions, clearing all stored permission decisions
        self.permission_manager.remove_extension("");
    }
}

#[async_trait]
impl Session for AcpServerSession {
    fn session_id(&self) -> &sacp::schema::SessionId {
        &self.session_id
    }

    fn work_dir(&self) -> std::path::PathBuf {
        self._work_dir.path().to_path_buf()
    }

    fn notifications(&self) -> Vec<super::Notification> {
        let updates: Vec<_> = self
            .updates
            .lock()
            .unwrap()
            .drain(..)
            .map(|n| n.update)
            .collect();
        super::to_notifications(&updates)
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
    config_options: Option<&[sacp::schema::SessionConfigOption]>,
) -> Option<SessionModelState> {
    let option = config_options?
        .iter()
        .find(|option| option.category.as_ref() == Some(&SessionConfigOptionCategory::Model))?;
    let SessionConfigKind::Select(select) = &option.kind else {
        return None;
    };

    let available_models = match &select.options {
        sacp::schema::SessionConfigSelectOptions::Ungrouped(options) => options
            .iter()
            .map(|option| {
                ModelInfo::new(
                    ModelId::new(option.value.0.to_string()),
                    option.name.clone(),
                )
            })
            .collect(),
        sacp::schema::SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|group| {
                group.options.iter().map(|option| {
                    ModelInfo::new(
                        ModelId::new(option.value.0.to_string()),
                        option.name.clone(),
                    )
                })
            })
            .collect(),
        _ => Vec::new(),
    };

    Some(SessionModelState::new(
        ModelId::new(select.current_value.0.to_string()),
        available_models,
    ))
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
