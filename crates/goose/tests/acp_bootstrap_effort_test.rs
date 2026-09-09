use agent_client_protocol::schema::v1::{
    AgentCapabilities, ConfigOptionUpdate, InitializeRequest, InitializeResponse,
    LoadSessionRequest, LoadSessionResponse, NewSessionRequest, NewSessionResponse,
    SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOption, SessionId,
    SessionNotification, SessionUpdate, SetSessionConfigOptionRequest,
    SetSessionConfigOptionResponse, UsageUpdate,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{on_receive_request, Agent as SacpAgent, ByteStreams};
use goose::acp::{AcpProvider, AcpProviderConfig};
use goose::config::GooseMode;
use goose::providers::base::Provider;
use goose_providers::thinking::ThinkingEffortSupport;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;
use tokio::time::{timeout, Duration};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

fn effort_option(current: &str, values: &[&str]) -> SessionConfigOption {
    SessionConfigOption::select(
        "effort",
        "Thinking",
        current.to_string(),
        values
            .iter()
            .map(|value| SessionConfigSelectOption::new(value.to_string(), *value))
            .collect::<Vec<_>>(),
    )
    .category(SessionConfigOptionCategory::ThoughtLevel)
}

/// Agents that pin a model during session bootstrap rebuild their per-model
/// effort levels in that response, so it supersedes the `session/new` snapshot
/// the mirrored capability was first built from.
#[tokio::test]
async fn bootstrap_config_option_response_refreshes_the_effort_mirror() {
    let (client_read, agent_write) = tokio::io::duplex(64 * 1024);
    let (agent_read, client_write) = tokio::io::duplex(64 * 1024);

    let bootstrap_picks: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let recorded_picks = bootstrap_picks.clone();

    let agent = tokio::spawn(async move {
        SacpAgent
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
                    responder.respond(
                        NewSessionResponse::new(SessionId::new("scripted-session")).config_options(
                            vec![effort_option("medium", &["low", "medium", "high"])],
                        ),
                    )
                },
                on_receive_request!(),
            )
            .on_receive_request(
                async |req: SetSessionConfigOptionRequest, responder, _cx| {
                    let value = req
                        .value
                        .as_value_id()
                        .expect("select option value")
                        .0
                        .to_string();
                    recorded_picks
                        .lock()
                        .unwrap()
                        .push((req.config_id.0.to_string(), value));
                    responder.respond(SetSessionConfigOptionResponse::new(vec![effort_option(
                        "high",
                        &["minimal", "high"],
                    )]))
                },
                on_receive_request!(),
            )
            .connect_to(ByteStreams::new(
                agent_write.compat_write(),
                agent_read.compat(),
            ))
            .await
    });

    let config = AcpProviderConfig {
        command: "unused".into(),
        args: vec![],
        env: vec![],
        env_remove: vec![],
        work_dir: std::env::temp_dir(),
        mcp_servers: vec![],
        session_mode_id: None,
        session_config_options: vec![("model".to_string(), "gpt-5".to_string())],
        model_config_option_id: Some("model".to_string()),
        mode_mapping: HashMap::new(),
        notification_callback: None,
    };

    let provider = AcpProvider::connect_with_transport(
        "scripted-acp".to_string(),
        GooseMode::default(),
        config,
        ByteStreams::new(client_write.compat_write(), client_read.compat()),
    )
    .await
    .expect("provider should connect to the scripted agent");

    assert_eq!(
        *bootstrap_picks.lock().unwrap(),
        vec![("model".to_string(), "gpt-5".to_string())]
    );

    match provider.thinking_effort_support() {
        ThinkingEffortSupport::Options(capability) => {
            assert_eq!(capability.current.as_deref(), Some("high"));
            assert_eq!(
                capability
                    .values
                    .iter()
                    .map(|option| option.value.as_str())
                    .collect::<Vec<_>>(),
                vec!["minimal", "high"]
            );
        }
        other => panic!("expected the agent's rebuilt effort options, got {other:?}"),
    }

    drop(provider);
    agent.abort();
}

#[tokio::test]
async fn new_session_preserves_pre_response_effort_update() {
    let (client_read, agent_write) = tokio::io::duplex(64 * 1024);
    let (agent_read, client_write) = tokio::io::duplex(64 * 1024);

    let agent = tokio::spawn(async move {
        SacpAgent
            .builder()
            .name("scripted-agent")
            .on_receive_request(
                async |_req: InitializeRequest, responder, _cx| {
                    responder.respond(InitializeResponse::new(ProtocolVersion::LATEST))
                },
                on_receive_request!(),
            )
            .on_receive_request(
                async |_req: NewSessionRequest, responder, cx| {
                    cx.send_notification(SessionNotification::new(
                        SessionId::new("scripted-session"),
                        SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(vec![
                            effort_option("high", &["default", "high", "xhigh"]),
                        ])),
                    ))?;
                    responder.respond(NewSessionResponse::new(SessionId::new("scripted-session")))
                },
                on_receive_request!(),
            )
            .connect_to(ByteStreams::new(
                agent_write.compat_write(),
                agent_read.compat(),
            ))
            .await
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
    .expect("provider should preserve the pre-response update");

    match provider.thinking_effort_support() {
        ThinkingEffortSupport::Options(capability) => {
            assert_eq!(capability.current.as_deref(), Some("high"));
            assert_eq!(
                capability
                    .values
                    .iter()
                    .map(|option| option.value.as_str())
                    .collect::<Vec<_>>(),
                vec!["default", "high", "xhigh"]
            );
        }
        other => panic!("expected the pre-response effort options, got {other:?}"),
    }

    drop(provider);
    agent.abort();
}

#[tokio::test]
async fn loaded_session_refreshes_the_effort_mirror() {
    let (client_read, agent_write) = tokio::io::duplex(64 * 1024);
    let (agent_read, client_write) = tokio::io::duplex(64 * 1024);

    let emit_late_update = Arc::new(Notify::new());
    let agent_emit_late_update = emit_late_update.clone();
    let active_update_received = Arc::new(Notify::new());
    let callback_active_update_received = active_update_received.clone();

    let agent = tokio::spawn(async move {
        SacpAgent
            .builder()
            .name("scripted-agent")
            .on_receive_request(
                async |_req: InitializeRequest, responder, _cx| {
                    responder.respond(
                        InitializeResponse::new(ProtocolVersion::LATEST)
                            .agent_capabilities(AgentCapabilities::new().load_session(true)),
                    )
                },
                on_receive_request!(),
            )
            .on_receive_request(
                async |_req: NewSessionRequest, responder, _cx| {
                    responder.respond(
                        NewSessionResponse::new(SessionId::new("temporary-session"))
                            .config_options(vec![effort_option(
                                "medium",
                                &["low", "medium", "high"],
                            )]),
                    )
                },
                on_receive_request!(),
            )
            .on_receive_request(
                async move |req: LoadSessionRequest, responder, cx| {
                    assert_eq!(req.session_id.0.as_ref(), "saved-session");
                    responder.respond(LoadSessionResponse::new().config_options(vec![
                        effort_option("xhigh", &["default", "high", "xhigh"]),
                    ]))?;
                    agent_emit_late_update.notified().await;
                    cx.send_notification(SessionNotification::new(
                        SessionId::new("temporary-session"),
                        SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(vec![
                            effort_option("medium", &["low", "medium", "high"]),
                        ])),
                    ))?;
                    cx.send_notification(SessionNotification::new(
                        SessionId::new("saved-session"),
                        SessionUpdate::UsageUpdate(UsageUpdate::new(1, 100)),
                    ))
                },
                on_receive_request!(),
            )
            .connect_to(ByteStreams::new(
                agent_write.compat_write(),
                agent_read.compat(),
            ))
            .await
    });

    let config = AcpProviderConfig {
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
            if notification.session_id.0.as_ref() == "saved-session"
                && matches!(notification.update, SessionUpdate::UsageUpdate(_))
            {
                callback_active_update_received.notify_one();
            }
        })),
    };

    let provider = AcpProvider::connect_with_transport(
        "scripted-acp".to_string(),
        GooseMode::default(),
        config,
        ByteStreams::new(client_write.compat_write(), client_read.compat()),
    )
    .await
    .expect("provider should connect to the scripted agent");

    provider
        .resume("saved-session")
        .await
        .expect("provider should load the saved session");

    emit_late_update.notify_one();
    timeout(Duration::from_secs(1), active_update_received.notified())
        .await
        .expect("active-session barrier notification should be received");

    match provider.thinking_effort_support() {
        ThinkingEffortSupport::Options(capability) => {
            assert_eq!(capability.current.as_deref(), Some("xhigh"));
            assert_eq!(
                capability
                    .values
                    .iter()
                    .map(|option| option.value.as_str())
                    .collect::<Vec<_>>(),
                vec!["default", "high", "xhigh"]
            );
        }
        other => panic!("expected the loaded session's effort options, got {other:?}"),
    }

    drop(provider);
    agent.abort();
}
