#[allow(dead_code)]
#[path = "acp_common_tests/mod.rs"]
mod common_tests;
use agent_client_protocol::schema::{ListSessionsRequest, ListSessionsResponse};
use agent_client_protocol::ErrorCode;
use common_tests::fixtures::server::AcpServerConnection;
use common_tests::fixtures::{run_test, Connection, OpenAiFixture, TestConnectionConfig};
#[cfg(feature = "code-mode")]
use common_tests::run_prompt_codemode;
use common_tests::{
    run_close_session, run_config_mcp, run_config_option_mode_set, run_config_option_model_set,
    run_delete_session, run_fs_read_text_file_true, run_fs_write_text_file_false,
    run_fs_write_text_file_true, run_initialize_doesnt_hit_provider, run_list_sessions,
    run_load_mode, run_load_model, run_load_session_error, run_load_session_mcp, run_mode_set,
    run_model_list, run_model_set, run_model_set_error_session_not_found,
    run_new_session_returns_initial_config, run_permission_persistence, run_prompt_basic,
    run_prompt_error, run_prompt_image, run_prompt_image_attachment, run_prompt_mcp,
    run_prompt_model_mismatch, run_prompt_skill, run_session_name_update_notification,
    run_shell_terminal_false, run_shell_terminal_true,
};
use goose::config::GooseMode;
use goose::conversation::message::Message;
use goose::session::{SessionManager, SessionType};
use std::path::Path;

tests_config_option_set_error!(AcpServerConnection);
tests_mode_set_error!(AcpServerConnection);

async fn seed_list_sessions(data_root: &Path, working_dir: &Path, count: usize) {
    let session_manager = SessionManager::new(data_root.to_path_buf());
    for index in 0..count {
        let session = session_manager
            .create_session(
                working_dir.to_path_buf(),
                format!("Seed session {index}"),
                SessionType::Acp,
                GooseMode::default(),
            )
            .await
            .unwrap();
        session_manager
            .add_message(&session.id, &Message::user().with_text("hello"))
            .await
            .unwrap();
    }
}

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

async fn list_sessions_request(
    conn: &AcpServerConnection,
    request: ListSessionsRequest,
) -> anyhow::Result<ListSessionsResponse> {
    conn.cx()
        .send_request(request)
        .block_task()
        .await
        .map_err(Into::into)
}

fn assert_invalid_params(error: anyhow::Error) {
    let acp_error = error.downcast::<agent_client_protocol::Error>().unwrap();
    assert_eq!(acp_error.code, ErrorCode::InvalidParams);
}

#[test]
fn test_config_mcp() {
    run_test(async { run_config_mcp::<AcpServerConnection>().await });
}

#[test]
fn test_config_option_mode_set() {
    run_test(async { run_config_option_mode_set::<AcpServerConnection>().await });
}

#[test]
fn test_list_sessions() {
    run_test(async { run_list_sessions::<AcpServerConnection>().await });
}

#[test]
fn test_list_sessions_pagination() {
    run_test(async {
        let data_root = tempfile::tempdir().unwrap();
        seed_list_sessions(data_root.path(), Path::new("/tmp/acp-session-list"), 51).await;
        let conn = new_connection(data_root.path()).await;

        let first = list_sessions_request(&conn, ListSessionsRequest::new())
            .await
            .unwrap();
        assert_eq!(first.sessions.len(), 50);

        let second = list_sessions_request(
            &conn,
            ListSessionsRequest::new().cursor(first.next_cursor.clone().unwrap()),
        )
        .await
        .unwrap();
        assert_eq!(second.sessions.len(), 1);
        assert!(second.next_cursor.is_none());

        let second_id = &second.sessions[0].session_id;
        assert!(first
            .sessions
            .iter()
            .all(|session| session.session_id != *second_id));
    });
}

#[test]
fn test_list_sessions_invalid_params() {
    run_test(async {
        let data_root = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let other_cwd = tempfile::tempdir().unwrap();
        seed_list_sessions(data_root.path(), cwd.path(), 51).await;
        let conn = new_connection(data_root.path()).await;

        let error =
            list_sessions_request(&conn, ListSessionsRequest::new().cursor("*".to_string()))
                .await
                .unwrap_err();
        assert_invalid_params(error);

        let error = list_sessions_request(
            &conn,
            ListSessionsRequest::new().cwd(std::path::PathBuf::from("relative/path")),
        )
        .await
        .unwrap_err();
        assert_invalid_params(error);

        let first = list_sessions_request(&conn, ListSessionsRequest::new().cwd(cwd.path()))
            .await
            .unwrap();

        let error = list_sessions_request(
            &conn,
            ListSessionsRequest::new()
                .cwd(other_cwd.path())
                .cursor(first.next_cursor.unwrap()),
        )
        .await
        .unwrap_err();
        assert_invalid_params(error);
    });
}

#[test]
fn test_session_name_update_notification() {
    run_test(async { run_session_name_update_notification::<AcpServerConnection>().await });
}

#[test]
fn test_close_session() {
    run_test(async { run_close_session::<AcpServerConnection>().await });
}

#[test]
fn test_config_option_model_set() {
    run_test(async { run_config_option_model_set::<AcpServerConnection>().await });
}

#[test]
fn test_delete_session() {
    run_test(async { run_delete_session::<AcpServerConnection>().await });
}

#[test]
fn test_fs_read_text_file_true() {
    run_test(async { run_fs_read_text_file_true::<AcpServerConnection>().await });
}

#[test]
fn test_fs_write_text_file_false() {
    run_test(async { run_fs_write_text_file_false::<AcpServerConnection>().await });
}

#[test]
fn test_fs_write_text_file_true() {
    run_test(async { run_fs_write_text_file_true::<AcpServerConnection>().await });
}

#[test]
fn test_initialize_doesnt_hit_provider() {
    run_test(async { run_initialize_doesnt_hit_provider::<AcpServerConnection>().await });
}

#[test]
fn test_load_mode() {
    run_test(async { run_load_mode::<AcpServerConnection>().await });
}

#[test]
fn test_load_model() {
    run_test(async { run_load_model::<AcpServerConnection>().await });
}

#[test]
fn test_load_session_error_session_not_found() {
    run_test(async { run_load_session_error::<AcpServerConnection>().await });
}

#[test]
fn test_load_session_mcp() {
    run_test(async { run_load_session_mcp::<AcpServerConnection>().await });
}

#[test]
fn test_mode_set() {
    run_test(async { run_mode_set::<AcpServerConnection>().await });
}

#[test]
fn test_model_list() {
    run_test(async { run_model_list::<AcpServerConnection>().await });
}

#[test]
fn test_new_session_returns_initial_config() {
    run_test(async { run_new_session_returns_initial_config::<AcpServerConnection>().await });
}

#[test]
fn test_model_set() {
    run_test(async { run_model_set::<AcpServerConnection>().await });
}

#[test]
fn test_model_set_error_session_not_found() {
    run_test(async { run_model_set_error_session_not_found::<AcpServerConnection>().await });
}

#[test]
fn test_permission_persistence() {
    run_test(async { run_permission_persistence::<AcpServerConnection>().await });
}

#[test]
fn test_prompt_basic() {
    run_test(async { run_prompt_basic::<AcpServerConnection>().await });
}

#[test]
#[cfg(feature = "code-mode")]
fn test_prompt_codemode() {
    run_test(async { run_prompt_codemode::<AcpServerConnection>().await });
}

#[test]
fn test_prompt_error_session_not_found() {
    run_test(async { run_prompt_error::<AcpServerConnection>().await });
}

#[test]
fn test_prompt_image() {
    run_test(async { run_prompt_image::<AcpServerConnection>().await });
}

#[test]
fn test_prompt_image_attachment() {
    run_test(async { run_prompt_image_attachment::<AcpServerConnection>().await });
}

#[test]
fn test_prompt_mcp() {
    run_test(async { run_prompt_mcp::<AcpServerConnection>().await });
}

#[test]
fn test_prompt_model_mismatch() {
    run_test(async { run_prompt_model_mismatch::<AcpServerConnection>().await });
}

#[test]
fn test_prompt_skill() {
    run_test(async { run_prompt_skill::<AcpServerConnection>().await });
}

#[test]
fn test_shell_terminal_false() {
    run_test(async { run_shell_terminal_false::<AcpServerConnection>().await });
}

#[test]
fn test_shell_terminal_true() {
    run_test(async { run_shell_terminal_true::<AcpServerConnection>().await });
}
