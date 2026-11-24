use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock, ContentChunk, InitializeRequest, InitializeResponse,
    NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse, SessionId,
    SessionNotification, SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{
    on_receive_notification, on_receive_request, Agent as SacpAgent, ByteStreams,
};
use futures::StreamExt;
use goose::acp::{AcpProvider, AcpProviderConfig};
use goose::config::GooseMode;
use goose::conversation::message::{Message, MessageContent};
use goose::providers::base::{MessageStream, Provider};
use oi_kit_providers::model::ModelConfig;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Notify};
use tokio::time::{timeout, Duration};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

const SESSION: &str = "scripted-session";

fn agent_text(text: &str) -> SessionNotification {
    SessionNotification::new(
        SessionId::new(SESSION),
        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            text,
        )))),
    )
}

async fn connect(
    honors_cancel: bool,
) -> (
    AcpProvider,
    mpsc::UnboundedReceiver<String>,
    tokio::task::JoinHandle<()>,
) {
    let (client_read, agent_write) = tokio::io::duplex(64 * 1024);
    let (agent_read, client_write) = tokio::io::duplex(64 * 1024);
    let (cancels_tx, cancels_rx) = mpsc::unbounded_channel();
    let cancelled = Arc::new(Notify::new());
    let prompts = Arc::new(AtomicUsize::new(0));

    let agent = tokio::spawn({
        let cancelled = cancelled.clone();
        async move {
            let _ = SacpAgent
                .builder()
                .name("scripted-agent")
                .on_receive_request(
                    async |_req: InitializeRequest, responder, _cx| {
                        responder.respond(InitializeResponse::new(ProtocolVersion::LATEST))
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async |_req: NewSessionRequest, responder, _cx| {
                        responder.respond(NewSessionResponse::new(SessionId::new(SESSION)))
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    {
                        let cancelled = cancelled.clone();
                        async move |_req: PromptRequest, responder, cx| {
                            let prompt = prompts.fetch_add(1, Ordering::SeqCst);
                            let cancelled = cancelled.clone();
                            let cx_spawn = cx.clone();
                            cx.spawn(async move {
                                if prompt == 0 {
                                    cx_spawn.send_notification(agent_text("working"))?;
                                    cancelled.notified().await;
                                    if !honors_cancel {
                                        std::future::pending::<()>().await;
                                    }
                                    responder.respond(PromptResponse::new(StopReason::Cancelled))
                                } else {
                                    cx_spawn.send_notification(agent_text("second reply"))?;
                                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                                }
                            })
                        }
                    },
                    on_receive_request!(),
                )
                .on_receive_notification(
                    async move |notification: CancelNotification, _cx| {
                        let _ = cancels_tx.send(notification.session_id.0.to_string());
                        cancelled.notify_one();
                        Ok(())
                    },
                    on_receive_notification!(),
                )
                .connect_to(ByteStreams::new(
                    agent_write.compat_write(),
                    agent_read.compat(),
                ))
                .await;
        }
    });

    let provider = AcpProvider::connect_with_transport(
        "scripted-acp".to_string(),
        GooseMode::default(),
        AcpProviderConfig {
            command: "unused".into(),
            args: vec![],
            env: vec![],
            env_remove: vec![],
            work_dir: std::env::temp_dir(),
            mcp_servers: vec![],
            session_mode_id: None,
            session_config_options: vec![],
            model_config_option_id: None,
            mode_mapping: HashMap::new(),
            notification_callback: None,
        },
        ByteStreams::new(client_write.compat_write(), client_read.compat()),
    )
    .await
    .expect("provider should connect to the scripted agent");

    (provider, cancels_rx, agent)
}

async fn prompt(provider: &AcpProvider, text: &str) -> MessageStream {
    provider
        .stream(
            &ModelConfig::new("scripted-model"),
            "",
            &[Message::user().with_text(text)],
            &[],
        )
        .await
        .expect("prompt should be sent")
}

async fn collect_text(mut stream: MessageStream) -> String {
    let mut text = String::new();
    while let Some(item) = stream.next().await {
        let (message, _) = item.expect("stream item");
        for content in message.into_iter().flat_map(|message| message.content) {
            if let MessageContent::Text(chunk) = content {
                text.push_str(&chunk.text);
            }
        }
    }
    text
}

async fn abandon_first_prompt(
    provider: &AcpProvider,
    cancels: &mut mpsc::UnboundedReceiver<String>,
) {
    let mut stream = prompt(provider, "first").await;
    timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("agent should start working")
        .expect("stream should yield the agent's first update")
        .expect("first update should not be an error");
    drop(stream);

    let cancelled_session = timeout(Duration::from_secs(5), cancels.recv())
        .await
        .expect("dropping the stream should send session/cancel");
    assert_eq!(cancelled_session.as_deref(), Some(SESSION));
}

#[tokio::test]
async fn dropping_a_prompt_stream_cancels_it_on_the_agent() {
    let (provider, mut cancels, agent) = connect(true).await;

    abandon_first_prompt(&provider, &mut cancels).await;

    let second = timeout(
        Duration::from_secs(2),
        collect_text(prompt(&provider, "second").await),
    )
    .await
    .expect("next prompt should run once the agent ends the cancelled one");
    assert_eq!(second, "second reply");

    drop(provider);
    agent.abort();
}

#[tokio::test]
async fn agent_that_ignores_cancel_only_delays_the_next_prompt() {
    let (provider, mut cancels, agent) = connect(false).await;

    abandon_first_prompt(&provider, &mut cancels).await;

    let second = timeout(
        Duration::from_secs(15),
        collect_text(prompt(&provider, "second").await),
    )
    .await
    .expect("next prompt should run after the cancel grace period");
    assert_eq!(second, "second reply");

    drop(provider);
    agent.abort();
}
