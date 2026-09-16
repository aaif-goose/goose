use super::*;
use agent_client_protocol::JsonRpcMessage;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Debug, Default)]
struct CountingProvider {
    calls: AtomicUsize,
    pause_activation: AtomicBool,
    activation_entered: tokio::sync::Notify,
    activation_release: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl Provider for CountingProvider {
    fn get_name(&self) -> &str {
        "openai"
    }

    async fn apply_model_selection(
        &self,
        _: &goose_providers::model::ModelConfig,
    ) -> Result<(), ProviderError> {
        if self.pause_activation.swap(false, Ordering::SeqCst) {
            self.activation_entered.notify_one();
            self.activation_release.notified().await;
        }
        Ok(())
    }

    async fn stream(
        &self,
        _: &goose_providers::model::ModelConfig,
        _: &str,
        _: &[Message],
        _: &[rmcp::model::Tool],
    ) -> Result<crate::providers::base::MessageStream, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Box::pin(stream::once(async {
            Ok((Some(Message::assistant().with_text("done")), None))
        })))
    }
}

async fn connection_with_services(
    root: &Path,
    services: Arc<StateMachineServices>,
) -> Arc<AcpConnection> {
    let live_voice = Arc::new(LiveVoiceService::from_config(services.runtime.runs.clone()));
    Arc::new(
        AcpConnection::new(AcpConnectionOptions {
            provider_factory: Arc::new(|_| {
                Box::pin(async { anyhow::bail!("unexpected provider construction") })
            }),
            builtin_selection: AcpBuiltinSelection::default(),
            config_dir: root.to_path_buf(),
            services,
            additional_source_roots: Vec::new(),
            session_cwd: None,
            live_voice,
        })
        .await
        .unwrap(),
    )
}

async fn server_with_session(
    lazy: bool,
) -> (
    tempfile::TempDir,
    Arc<AcpConnection>,
    Session,
    Arc<CountingProvider>,
) {
    let root = tempfile::tempdir().unwrap();
    let services = StateMachineServices::with_config(acp_services_config(
        Arc::new(SessionManager::new(root.path().to_path_buf())),
        root.path().to_path_buf(),
        true,
        GoosePlatform::GooseCli,
    ));
    let server = connection_with_services(root.path(), services).await;
    let session = server
        .services
        .session_manager
        .create_session(
            root.path().to_path_buf(),
            "Prompt cancellation test".to_string(),
            SessionType::Acp,
            GooseMode::Auto,
        )
        .await
        .unwrap();
    server
        .services
        .session_manager
        .update(&session.id)
        .extension_data(enabled_extensions_data(&session, Vec::new()).unwrap())
        .apply()
        .await
        .unwrap();
    let provider = Arc::new(CountingProvider::default());
    server
        .services
        .update_provider(
            provider.clone(),
            goose_providers::model::ModelConfig::new("gpt-4o"),
            &session.id,
        )
        .await
        .unwrap();
    if lazy {
        provider.pause_activation.store(true, Ordering::SeqCst);
        assert!(!server.has_session(&session.id));
    } else {
        server
            .services
            .attach_session(
                &session.id,
                server.connection_id,
                Attachment {
                    client: ClientContext::new(&server.services.goose_platform, None),
                    session_names: None,
                },
            )
            .await;
    }
    (root, server, session, provider)
}

fn prompt(session_id: &str) -> PromptRequest {
    PromptRequest::new(
        SessionId::new(session_id.to_string()),
        vec![ContentBlock::Text(TextContent::new("hello"))],
    )
}

struct CancelBeforePromptTask {
    handler: AcpConnectionHandler,
}

impl HandleDispatchFrom<Client> for CancelBeforePromptTask {
    fn describe_chain(&self) -> impl std::fmt::Debug {
        "cancel-before-prompt-task"
    }

    async fn handle_dispatch_from(
        &mut self,
        message: Dispatch,
        cx: ConnectionTo<Client>,
    ) -> Result<Handled<Dispatch>, agent_client_protocol::Error> {
        let cancellation = match &message {
            Dispatch::Request(request, _) if request.method == "session/prompt" => {
                let request = PromptRequest::parse_message(&request.method, &request.params)?;
                Some(CancelNotification::new(request.session_id).to_untyped_message()?)
            }
            _ => None,
        };
        if let Some(cancellation) = cancellation {
            let result = self
                .handler
                .handle_dispatch_from(message, cx.clone())
                .now_or_never()
                .expect("prompt dispatch unexpectedly yielded")?;
            let _ = self
                .handler
                .handle_dispatch_from(Dispatch::Notification(cancellation), cx)
                .now_or_never()
                .expect("cancel dispatch unexpectedly yielded")?;
            Ok(result)
        } else {
            self.handler.handle_dispatch_from(message, cx).await
        }
    }
}

#[tokio::test]
async fn dispatch_cancel_before_prompt_task_prevents_work() {
    let (_root, server, session, provider) = server_with_session(false).await;
    let handler = CancelBeforePromptTask {
        handler: AcpConnectionHandler {
            connection: server.clone(),
        },
    };
    tokio::time::timeout(
        Duration::from_secs(10),
        Client
            .builder()
            .connect_with(SacpAgent.builder().with_handler(handler), async |cx| {
                let response = cx.send_request(prompt(&session.id)).block_task().await?;
                assert_eq!(response.stop_reason, StopReason::Cancelled);
                assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
                assert!(!server.runs().is_active(&session.id));
                let stored = server
                    .services
                    .session_manager
                    .get_session(&session.id, true)
                    .await
                    .unwrap();
                assert!(stored.conversation.unwrap_or_default().is_empty());
                Ok(())
            }),
    )
    .await
    .expect("prompt timed out")
    .unwrap();
}

#[tokio::test]
async fn dispatch_cancel_during_lazy_activation_prevents_work() {
    let (_root, server, session, provider) = server_with_session(true).await;
    tokio::time::timeout(
        Duration::from_secs(10),
        Client.builder().connect_with(
            SacpAgent.builder().with_handler(AcpConnectionHandler {
                connection: server.clone(),
            }),
            async |cx| {
                let prompt_cx = cx.clone();
                let request = prompt(&session.id);
                let pending =
                    tokio::spawn(async move { prompt_cx.send_request(request).block_task().await });
                provider.activation_entered.notified().await;
                assert!(server.runs().is_active(&session.id));
                cx.send_notification(CancelNotification::new(SessionId::new(session.id.clone())))?;
                cx.send_request(AuthenticateRequest::new("test"))
                    .block_task()
                    .await?;
                assert!(server
                    .runs()
                    .agent_cancel_token(&session.id)
                    .is_some_and(|token| token.is_cancelled()));
                provider.activation_release.notify_one();
                assert_eq!(pending.await.unwrap()?.stop_reason, StopReason::Cancelled);
                assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
                assert!(!server.runs().is_active(&session.id));
                let stored = server
                    .services
                    .session_manager
                    .get_session(&session.id, true)
                    .await
                    .unwrap();
                assert!(stored.conversation.unwrap_or_default().is_empty());

                let response = cx.send_request(prompt(&session.id)).block_task().await?;
                assert_eq!(response.stop_reason, StopReason::EndTurn);
                assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
                assert!(!server.runs().is_active(&session.id));
                Ok(())
            },
        ),
    )
    .await
    .expect("prompt timed out")
    .unwrap();
}

#[tokio::test]
async fn dispatch_prompt_error_and_idle_cancel_allow_later_prompt() {
    let (_root, server, session, provider) = server_with_session(false).await;
    tokio::time::timeout(
        Duration::from_secs(10),
        Client.builder().connect_with(
            SacpAgent.builder().with_handler(AcpConnectionHandler {
                connection: server.clone(),
            }),
            async |cx| {
                for _ in 0..2 {
                    assert!(cx
                        .send_request(prompt("missing-session"))
                        .block_task()
                        .await
                        .is_err());
                    assert!(!server.runs().is_active("missing-session"));
                }
                cx.send_notification(CancelNotification::new(SessionId::new(session.id.clone())))?;
                let response = cx.send_request(prompt(&session.id)).block_task().await?;
                assert_eq!(response.stop_reason, StopReason::EndTurn);
                assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
                assert!(!server.runs().is_active(&session.id));
                Ok(())
            },
        ),
    )
    .await
    .expect("prompt timed out")
    .unwrap();
}

#[tokio::test]
async fn pending_run_preserves_owner_token_and_generation() {
    let (_root, server, session, _) = server_with_session(false).await;
    let run = server.reserve_prompt_run(&session.id).await.unwrap();
    assert!(server.reserve_prompt_run(&session.id).await.is_err());
    let other = connection_with_services(_root.path(), server.services.clone()).await;
    assert_eq!(
        other
            .require_active_run(&session.id, &run.run_id)
            .await
            .unwrap(),
        run.run_id
    );
    other
        .on_cancel(CancelNotification::new(SessionId::new(session.id.clone())))
        .await
        .unwrap();
    assert!(run.cancel_token.is_cancelled());
    server.clear_active_run(&session.id, &run.run_id).await;

    let next = server.reserve_prompt_run(&session.id).await.unwrap();
    drop(run);
    tokio::task::yield_now().await;
    assert_eq!(
        server.runs().agent_run_id(&session.id),
        Some(next.run_id.clone())
    );
    assert!(!next.cancel_token.is_cancelled());
    server.clear_active_run(&session.id, &next.run_id).await;
}

#[tokio::test]
async fn dropping_unpolled_prompt_releases_pending_run() {
    let (_root, server, session, provider) = server_with_session(false).await;
    Client
        .builder()
        .connect_with(
            SacpAgent.builder().with_handler(AcpConnectionHandler {
                connection: server.clone(),
            }),
            async |cx| {
                cx.send_request(AuthenticateRequest::new("test"))
                    .block_task()
                    .await?;
                let run = server.reserve_prompt_run(&session.id).await.unwrap();
                let token = run.cancel_token.clone();
                let future =
                    server.on_prompt(server.client_cx.get().unwrap(), prompt(&session.id), run);
                drop(future);
                assert!(token.is_cancelled());
                tokio::time::timeout(Duration::from_secs(2), async {
                    while server.runs().is_active(&session.id) {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("dropped prompt retained its reservation");
                assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
                Ok(())
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn disconnect_during_activation_releases_pending_run() {
    let (_root, server, session, provider) = server_with_session(true).await;
    let pending = tokio::time::timeout(
        Duration::from_secs(10),
        Client.builder().connect_with(
            SacpAgent.builder().with_handler(AcpConnectionHandler {
                connection: server.clone(),
            }),
            async |cx| {
                let request = prompt(&session.id);
                let pending =
                    tokio::spawn(async move { cx.send_request(request).block_task().await });
                provider.activation_entered.notified().await;
                assert!(server.runs().is_active(&session.id));
                assert!(server.runs().is_active(&session.id));
                Ok(pending)
            },
        ),
    )
    .await
    .expect("activation timed out")
    .unwrap();
    assert!(pending.await.unwrap().is_err());
    tokio::time::timeout(Duration::from_secs(2), async {
        while server.runs().is_active(&session.id) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("disconnected prompt retained its reservation");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}
