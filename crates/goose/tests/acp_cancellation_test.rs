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
use goose::conversation::message::Message;
use goose_providers::model::ModelConfig;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;
use tokio::time::{timeout, Duration};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

const SESSION_ID: &str = "remote-cancellation-session";
const WAIT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn dropping_prompt_stream_cancels_and_drains_before_the_next_prompt() {
    let (client_read, agent_write) = tokio::io::duplex(64 * 1024);
    let (agent_read, client_write) = tokio::io::duplex(64 * 1024);
    let cancellations = Arc::new(Mutex::new(Vec::new()));
    let cancel_received = Arc::new(Notify::new());
    let prompts = Arc::new(Mutex::new(0usize));
    let second_received = Arc::new(Notify::new());
    let emit_late_chunk = Arc::new(Notify::new());
    let late_chunk_received = Arc::new(Notify::new());
    let finish_first_prompt = Arc::new(Notify::new());
    let first_returned = Arc::new(Mutex::new(false));

    let agent_cancellations = cancellations.clone();
    let agent_cancel_received = cancel_received.clone();
    let agent_prompts = prompts.clone();
    let agent_second_received = second_received.clone();
    let agent_emit_late_chunk = emit_late_chunk.clone();
    let agent_finish_first_prompt = finish_first_prompt.clone();
    let agent_first_returned = first_returned.clone();
    let agent = tokio::spawn(async move {
        SacpAgent
            .builder()
            .name("cancellation-agent")
            .on_receive_request(
                async |_req: InitializeRequest, responder, _cx| {
                    responder.respond(InitializeResponse::new(ProtocolVersion::LATEST))
                },
                on_receive_request!(),
            )
            .on_receive_request(
                async |_req: NewSessionRequest, responder, _cx| {
                    responder.respond(NewSessionResponse::new(SessionId::new(SESSION_ID)))
                },
                on_receive_request!(),
            )
            .on_receive_notification(
                async move |notification: CancelNotification, _cx| {
                    agent_cancellations
                        .lock()
                        .unwrap()
                        .push(notification.session_id);
                    agent_cancel_received.notify_one();
                    Ok(())
                },
                on_receive_notification!(),
            )
            .on_receive_request(
                async move |req: PromptRequest, responder, cx| {
                    assert_eq!(req.session_id, SessionId::new(SESSION_ID));
                    let number = {
                        let mut count = agent_prompts.lock().unwrap();
                        *count += 1;
                        *count
                    };
                    if number == 1 {
                        let emit_late_chunk = agent_emit_late_chunk.clone();
                        let finish_first_prompt = agent_finish_first_prompt.clone();
                        let first_returned = agent_first_returned.clone();
                        let task_cx = cx.clone();
                        // The handler must return so it can receive session/cancel while
                        // the original prompt's responder remains outstanding.
                        cx.spawn(async move {
                            task_cx.send_notification(SessionNotification::new(
                                SessionId::new(SESSION_ID),
                                SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                    ContentBlock::Text(TextContent::new("first chunk")),
                                )),
                            ))?;
                            timeout(WAIT, emit_late_chunk.notified()).await.unwrap();
                            task_cx.send_notification(SessionNotification::new(
                                SessionId::new(SESSION_ID),
                                SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                    ContentBlock::Text(TextContent::new("late old chunk")),
                                )),
                            ))?;
                            timeout(WAIT, finish_first_prompt.notified()).await.unwrap();
                            responder.respond(PromptResponse::new(StopReason::Cancelled))?;
                            *first_returned.lock().unwrap() = true;
                            Ok(())
                        })
                    } else {
                        assert_eq!(number, 2);
                        assert!(*agent_first_returned.lock().unwrap());
                        agent_second_received.notify_one();
                        cx.send_notification(SessionNotification::new(
                            SessionId::new(SESSION_ID),
                            SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                ContentBlock::Text(TextContent::new("new chunk")),
                            )),
                        ))?;
                        responder.respond(PromptResponse::new(StopReason::EndTurn))
                    }
                },
                on_receive_request!(),
            )
            .connect_to(ByteStreams::new(
                agent_write.compat_write(),
                agent_read.compat(),
            ))
            .await
    });

    let callback_late_chunk_received = late_chunk_received.clone();
    let provider = timeout(
        WAIT,
        AcpProvider::connect_with_transport(
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
                notification_callback: Some(Arc::new(move |notification| {
                    if let SessionUpdate::AgentMessageChunk(chunk) = &notification.update {
                        if let agent_client_protocol::schema::v1::ContentBlock::Text(text) =
                            &chunk.content
                        {
                            if text.text == "late old chunk" {
                                callback_late_chunk_received.notify_one();
                            }
                        }
                    }
                })),
            },
            ByteStreams::new(client_write.compat_write(), client_read.compat()),
        ),
    )
    .await
    .expect("connection timed out")
    .expect("provider should connect");

    let model = ModelConfig::new("test-model");
    let mut first = timeout(
        WAIT,
        provider.prompt_messages(&model, &[Message::user().with_text("first prompt")]),
    )
    .await
    .unwrap()
    .unwrap();
    let (message, _) = timeout(WAIT, first.next())
        .await
        .expect("first chunk timed out")
        .expect("first stream ended early")
        .unwrap();
    assert_eq!(message.unwrap().as_concat_text(), "first chunk");
    drop(first);
    timeout(WAIT, cancel_received.notified())
        .await
        .expect("dropping the stream must send session/cancel");
    assert_eq!(
        *cancellations.lock().unwrap(),
        vec![SessionId::new(SESSION_ID)]
    );

    let mut second = timeout(
        WAIT,
        provider.prompt_messages(&model, &[Message::user().with_text("second prompt")]),
    )
    .await
    .expect("queueing the second prompt timed out")
    .unwrap();
    assert!(timeout(Duration::from_millis(100), second.next())
        .await
        .is_err());
    assert_eq!(*prompts.lock().unwrap(), 1);
    assert!(!*first_returned.lock().unwrap());

    emit_late_chunk.notify_one();
    timeout(WAIT, late_chunk_received.notified())
        .await
        .expect("late chunk must be processed before releasing the first responder");
    assert_eq!(*prompts.lock().unwrap(), 1);
    finish_first_prompt.notify_one();
    timeout(WAIT, second_received.notified())
        .await
        .expect("second prompt should run after the cancelled prompt returns");

    let text = timeout(WAIT, async {
        let mut text = String::new();
        while let Some(result) = second.next().await {
            let (message, _) = result.unwrap();
            if let Some(message) = message {
                text.push_str(&message.as_concat_text());
            }
        }
        text
    })
    .await
    .expect("second stream should finish");
    assert_eq!(text, "new chunk");
    assert!(!text.contains("late old chunk"));
    assert_eq!(*prompts.lock().unwrap(), 2);
    assert_eq!(
        *cancellations.lock().unwrap(),
        vec![SessionId::new(SESSION_ID)]
    );

    drop(second);
    drop(provider);
    agent.abort();
}
