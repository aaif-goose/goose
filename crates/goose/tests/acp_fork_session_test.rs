#![recursion_limit = "256"]
#[allow(dead_code)]
#[path = "acp_common_tests/mod.rs"]
mod common_tests;

use agent_client_protocol::schema::v1::{ForkSessionRequest, ForkSessionResponse, SessionId};
use common_tests::fixtures::server::{
    assert_session_response_precedes_available_commands, AcpServerConnection,
};
use common_tests::fixtures::{
    run_test, spawn_acp_server_in_process, Connection, OpenAiFixture, TestConnectionConfig,
};
use goose::config::GooseMode;
use goose::conversation::message::{Message, MessageContent};
use goose::session::{SessionManager, SessionType};
use std::path::Path;

async fn new_connection(data_root: &Path) -> AcpServerConnection {
    let openai = OpenAiFixture::new(
        vec![],
        <AcpServerConnection as Connection>::expected_session_id(),
    )
    .await;
    <AcpServerConnection as Connection>::new(
        TestConnectionConfig {
            data_root: data_root.to_path_buf(),
            ..Default::default()
        },
        openai,
    )
    .await
}

async fn fork_session_request(
    conn: &AcpServerConnection,
    request: ForkSessionRequest,
) -> anyhow::Result<ForkSessionResponse> {
    conn.cx()
        .send_request(request)
        .block_task()
        .await
        .map_err(Into::into)
}

async fn seed_session_with_messages(
    session_manager: &SessionManager,
    cwd: &Path,
    messages: &[(&str, i64)],
) -> goose::session::Session {
    let session = session_manager
        .create_session(
            cwd.to_path_buf(),
            "Fork before".to_string(),
            SessionType::Acp,
            GooseMode::default(),
        )
        .await
        .unwrap();

    for (text, created) in messages {
        let mut message = Message::user().with_text(*text);
        message.created = *created;
        session_manager
            .add_message(&session.id, &message)
            .await
            .unwrap();
    }

    session
}

async fn session_texts(session_manager: &SessionManager, session_id: &str) -> Vec<String> {
    session_manager
        .get_session(session_id, true)
        .await
        .unwrap()
        .conversation
        .unwrap()
        .messages()
        .iter()
        .flat_map(|message| {
            message.content.iter().filter_map(|content| match content {
                MessageContent::Text(text) => Some(text.text.clone()),
                _ => None,
            })
        })
        .collect()
}

fn conversation_before_meta(timestamp: i64) -> serde_json::Map<String, serde_json::Value> {
    let mut meta = serde_json::Map::new();
    meta.insert(
        "conversationBefore".to_string(),
        serde_json::Value::Number(timestamp.into()),
    );
    meta
}

#[test]
fn fork_session_response_precedes_available_commands() {
    run_test(async {
        let data_root = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let session_manager = SessionManager::new(data_root.path().to_path_buf());
        let source = seed_session_with_messages(&session_manager, cwd.path(), &[]).await;
        let openai = OpenAiFixture::new(
            vec![],
            <AcpServerConnection as Connection>::expected_session_id(),
        )
        .await;
        let (transport, _handle, _permission_manager) = spawn_acp_server_in_process(
            openai.uri(),
            &[],
            data_root.path(),
            GooseMode::default(),
            None,
            goose_test_support::TEST_MODEL,
            true,
        )
        .await;

        assert_session_response_precedes_available_commands(
            transport,
            "session/fork",
            serde_json::json!({
                "sessionId": source.id,
                "cwd": cwd.path(),
                "mcpServers": []
            }),
        )
        .await;
    });
}

#[test]
fn fork_session_conversation_before_matches_rest_cutoff() {
    run_test(async {
        let data_root = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let session_manager = SessionManager::new(data_root.path().to_path_buf());
        let session = seed_session_with_messages(
            &session_manager,
            cwd.path(),
            &[
                ("first", 1_718_000_000),
                ("second", 1_718_000_060),
                ("third", 1_718_000_120),
            ],
        )
        .await;
        let conn = new_connection(data_root.path()).await;

        let response = fork_session_request(
            &conn,
            ForkSessionRequest::new(SessionId::new(session.id.clone()), cwd.path())
                .meta(conversation_before_meta(1_718_000_120)),
        )
        .await
        .unwrap();

        assert_eq!(
            session_texts(&session_manager, response.session_id.0.as_ref()).await,
            vec!["first", "second"]
        );
        assert_eq!(
            session_texts(&session_manager, &session.id).await,
            vec!["first", "second", "third"]
        );
    });
}
