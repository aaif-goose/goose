use super::*;
use agent_client_protocol::JsonRpcMessage;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use test_case::test_case;

#[derive(Debug, Default)]
struct CountingProvider {
    calls: AtomicUsize,
    pause_activation: bool,
    activation_entered: tokio::sync::Notify,
    activation_release: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl Provider for CountingProvider {
    fn get_name(&self) -> &str {
        "prompt-cancellation-test"
    }

    async fn update_mode(&self, _: &str, _: GooseMode) -> Result<(), ProviderError> {
        if self.pause_activation {
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

async fn server_with_session(
    lazy: bool,
) -> (
    tempfile::TempDir,
    Arc<GooseAcpAgent>,
    Session,
    Arc<CountingProvider>,
) {
    let root = tempfile::tempdir().unwrap();
    let active_runs = Arc::new(ActiveRunRegistry::default());
    let live_voice = Arc::new(LiveVoiceService::from_config(active_runs.clone()));
    let server = Arc::new(
        GooseAcpAgent::new(GooseAcpAgentOptions {
            provider_factory: Arc::new(|_, _, _, _| {
                Box::pin(async { anyhow::bail!("unexpected provider construction") })
            }),
            builtin_selection: AcpBuiltinSelection::default(),
            data_dir: root.path().to_path_buf(),
            config_dir: root.path().to_path_buf(),
            disable_session_naming: true,
            goose_platform: GoosePlatform::GooseCli,
            additional_source_roots: Vec::new(),
            scheduler: None,
            session_cwd: None,
            active_runs,
            live_voice,
        })
        .await
        .unwrap(),
    );
    let session = server
        .session_manager
        .create_session(
            root.path().to_path_buf(),
            "Prompt cancellation test".to_string(),
            SessionType::Acp,
            GooseMode::Auto,
        )
        .await
        .unwrap();
    let provider = Arc::new(CountingProvider {
        pause_activation: lazy,
        ..Default::default()
    });
    if lazy {
        server
            .agent_manager
            .set_default_provider(provider.clone())
            .await;
        return (root, server, session, provider);
    }
    let agent = Arc::new(Agent::with_config(AgentConfig::new(
        server.session_manager.clone(),
        server.permission_manager.clone(),
        None,
        GooseMode::Auto,
        true,
        GoosePlatform::GooseCli,
    )));
    agent
        .update_provider(
            provider.clone(),
            goose_providers::model::ModelConfig::new("test-model"),
            &session.id,
        )
        .await
        .unwrap();
    server.register_acp_session(session.id.clone(), agent).await;
    (root, server, session, provider)
}

fn prompt(session_id: &str, state_machine: bool) -> PromptRequest {
    let mut request = PromptRequest::new(
        SessionId::new(session_id.to_string()),
        vec![ContentBlock::Text(TextContent::new("hello"))],
    );
    request.meta = Some(
        serde_json::from_value(serde_json::json!({
            "goose": { "unrolledAgentLoop": state_machine }
        }))
        .unwrap(),
    );
    request
}

struct CancelBeforePromptTask {
    handler: GooseAcpHandler,
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
            // Both real dispatches must finish in this poll, before the SDK can
            // poll the prompt task it queues. No scheduler timing assumption.
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

#[test_case(false; "legacy")]
#[test_case(true; "state_machine")]
#[tokio::test]
async fn dispatch_cancel_before_prompt_task_prevents_work(state_machine: bool) {
    let (_root, server, session, provider) = server_with_session(false).await;
    let handler = CancelBeforePromptTask {
        handler: GooseAcpHandler {
            agent: server.clone(),
        },
    };
    tokio::time::timeout(
        Duration::from_secs(10),
        Client
            .builder()
            .connect_with(SacpAgent.builder().with_handler(handler), async |cx| {
                let response = cx
                    .send_request(prompt(&session.id, state_machine))
                    .block_task()
                    .await?;
                assert_eq!(response.stop_reason, StopReason::Cancelled);
                assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
                assert!(!server.active_runs.is_active(&session.id));
                let stored = server
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

#[test_case(false; "legacy")]
#[test_case(true; "state_machine")]
#[tokio::test]
async fn dispatch_cancel_during_lazy_activation_prevents_work(state_machine: bool) {
    let (_root, server, session, provider) = server_with_session(true).await;
    tokio::time::timeout(
        Duration::from_secs(10),
        Client.builder().connect_with(
            SacpAgent.builder().with_handler(GooseAcpHandler {
                agent: server.clone(),
            }),
            async |cx| {
                let prompt_cx = cx.clone();
                let request = prompt(&session.id, state_machine);
                let pending =
                    tokio::spawn(async move { prompt_cx.send_request(request).block_task().await });
                provider.activation_entered.notified().await;
                cx.send_notification(CancelNotification::new(SessionId::new(session.id.clone())))?;
                // Authenticate runs inline, so its response acknowledges that the
                // preceding cancellation notification has completed dispatch.
                cx.send_request(AuthenticateRequest::new("test"))
                    .block_task()
                    .await?;
                assert!(server
                    .active_runs
                    .agent_cancel_token(&session.id)
                    .is_some_and(|token| token.is_cancelled()));
                provider.activation_release.notify_one();
                assert_eq!(pending.await.unwrap()?.stop_reason, StopReason::Cancelled);
                assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
                assert!(!server.active_runs.is_active(&session.id));
                let stored = server
                    .session_manager
                    .get_session(&session.id, true)
                    .await
                    .unwrap();
                assert!(stored.conversation.unwrap_or_default().is_empty());

                let response = cx
                    .send_request(prompt(&session.id, state_machine))
                    .block_task()
                    .await?;
                assert_eq!(response.stop_reason, StopReason::EndTurn);
                assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
                assert!(!server.active_runs.is_active(&session.id));
                Ok(())
            },
        ),
    )
    .await
    .expect("prompt timed out")
    .unwrap();
}

#[test_case(false; "legacy")]
#[test_case(true; "state_machine")]
#[tokio::test]
async fn dispatch_prompt_error_and_idle_cancel_allow_later_prompt(state_machine: bool) {
    let (_root, server, session, provider) = server_with_session(false).await;
    tokio::time::timeout(
        Duration::from_secs(10),
        Client.builder().connect_with(
            SacpAgent.builder().with_handler(GooseAcpHandler {
                agent: server.clone(),
            }),
            async |cx| {
                for _ in 0..2 {
                    assert!(cx
                        .send_request(prompt("missing-session", state_machine))
                        .block_task()
                        .await
                        .is_err());
                    assert!(!server.active_runs.is_active("missing-session"));
                }
                cx.send_notification(CancelNotification::new(SessionId::new(session.id.clone())))?;
                let response = cx
                    .send_request(prompt(&session.id, state_machine))
                    .block_task()
                    .await?;
                assert_eq!(response.stop_reason, StopReason::EndTurn);
                assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
                assert!(!server.active_runs.is_active(&session.id));
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
    assert!(server
        .require_active_run(&session.id, &run.run_id)
        .await
        .is_err());
    server
        .on_cancel(CancelNotification::new(SessionId::new(session.id.clone())))
        .await
        .unwrap();
    let owner = server.sessions.lock().await[&session.id].agent.clone();
    server
        .attach_active_run_agent(&run, owner.clone())
        .await
        .unwrap();
    let (_, resolved) = server
        .require_active_run(&session.id, &run.run_id)
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&owner, &resolved));
    assert!(run.cancel_token.is_cancelled());
    server.clear_active_run(&session.id, &run.run_id).await;

    let next = server.reserve_prompt_run(&session.id).await.unwrap();
    drop(run);
    tokio::task::yield_now().await;
    assert_eq!(
        server
            .active_runs
            .prompt_run(&session.id)
            .map(|(run_id, _)| run_id),
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
            SacpAgent.builder().with_handler(GooseAcpHandler {
                agent: server.clone(),
            }),
            async |cx| {
                cx.send_request(AuthenticateRequest::new("test"))
                    .block_task()
                    .await?;
                let run = server.reserve_prompt_run(&session.id).await.unwrap();
                let token = run.cancel_token.clone();
                let future = server.on_prompt(
                    server.client_cx.get().unwrap(),
                    prompt(&session.id, false),
                    run,
                );
                drop(future);
                assert!(token.is_cancelled());
                tokio::time::timeout(Duration::from_secs(2), async {
                    while server.active_runs.is_active(&session.id) {
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
    let pending = Client
        .builder()
        .connect_with(
            SacpAgent.builder().with_handler(GooseAcpHandler {
                agent: server.clone(),
            }),
            async |cx| {
                let request = prompt(&session.id, false);
                let pending =
                    tokio::spawn(async move { cx.send_request(request).block_task().await });
                provider.activation_entered.notified().await;
                assert!(server.active_runs.is_active(&session.id));
                Ok(pending)
            },
        )
        .await
        .unwrap();
    assert!(pending.await.unwrap().is_err());
    tokio::time::timeout(Duration::from_secs(2), async {
        while server.active_runs.is_active(&session.id) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("disconnected prompt retained its reservation");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
}
