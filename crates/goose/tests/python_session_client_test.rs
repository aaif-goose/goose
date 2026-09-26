use goose::agents::extension::PlatformExtensionContext;
use goose::agents::extension_manager::ExtensionManager;
use goose::agents::mcp_client::McpClientTrait;
use goose::agents::platform_extensions::developer::python_session::PythonSessionClient;
use goose::agents::ToolCallContext;
use goose::config::GooseMode;
use goose::conversation::message::Message;
use goose::session::SessionType;
use serde_json::json;
use serial_test::serial;
use tokio_util::sync::CancellationToken;

fn python_available() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Keep session snapshots out of the developer's real data directory: the client
/// persists them under `Paths::data_dir()`, which honors `GOOSE_PATH_ROOT`.
fn isolate_data_dir() {
    static ROOT: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("GOOSE_PATH_ROOT", dir.path());
        dir
    });
}

async fn setup() -> (
    PythonSessionClient,
    String,
    tempfile::TempDir,
    PlatformExtensionContext,
) {
    isolate_data_dir();
    let temp_dir = tempfile::tempdir().unwrap();
    let em = ExtensionManager::new_without_provider(temp_dir.path().to_path_buf());
    let context = em.get_context().clone();
    let session = context
        .session_manager
        .create_session(
            temp_dir.path().to_path_buf(),
            "python-session-test".to_string(),
            SessionType::User,
            GooseMode::Auto,
        )
        .await
        .unwrap();
    let client = PythonSessionClient::new(context.clone()).unwrap();
    (client, session.id, temp_dir, context)
}

/// Records a `python` call the agent no longer sees, as compaction or a tool-pair
/// summary leaves it.
async fn hide_python_call(context: &PlatformExtensionContext, session_id: &str) {
    let call = rmcp::model::CallToolRequestParams::new("python");
    let hidden = Message::assistant()
        .with_tool_request("call_hidden", Ok(call))
        .user_only();
    context
        .session_manager
        .add_message(session_id, &hidden)
        .await
        .unwrap();
}

fn result_text(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| block.as_text().map(|text| text.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

async fn run_cell(
    client: &PythonSessionClient,
    session_id: &str,
    code: &str,
) -> rmcp::model::CallToolResult {
    let ctx = ToolCallContext::new(session_id.to_string(), None, None);
    client
        .call_tool(
            &ctx,
            "python",
            Some(json!({"code": code}).as_object().unwrap().clone()),
            CancellationToken::new(),
        )
        .await
        .unwrap()
}

#[tokio::test]
#[serial]
async fn cells_share_state_and_moim_waits_for_compaction() {
    if !python_available() {
        eprintln!("skipping: python3 not available");
        return;
    }
    let (client, session_id, _dir, _) = setup().await;

    let result = run_cell(&client, &session_id, "report = 'x' * 50_000\nlen(report)").await;
    assert_ne!(result.is_error, Some(true));
    assert!(result_text(&result).contains("=> 50000"));

    let result = run_cell(&client, &session_id, "len(report) // 1000").await;
    assert!(result_text(&result).contains("=> 50"));

    assert!(
        client.get_moim(&session_id).await.is_none(),
        "the namespace listing is only emitted once the conversation has been compacted"
    );
}

#[tokio::test]
#[serial]
async fn kernel_death_is_reported_and_snapshot_restores_the_next_cell() {
    if !python_available() {
        eprintln!("skipping: python3 not available");
        return;
    }
    let (client, session_id, _dir, _) = setup().await;

    run_cell(&client, &session_id, "precious = 41").await;
    let result = run_cell(&client, &session_id, "import os; os._exit(3)").await;
    assert_eq!(result.is_error, Some(true));
    assert!(result_text(&result).contains("restarted"));

    let result = run_cell(&client, &session_id, "precious + 1").await;
    assert_ne!(result.is_error, Some(true));
    let text = result_text(&result);
    assert!(
        text.contains("restored from a previous process") && text.contains("=> 42"),
        "snapshot should bring the variable back after a crash, got: {text}"
    );
}

#[tokio::test]
#[serial]
async fn copied_conversation_is_told_the_namespace_is_fresh() {
    let (client, session_id, _dir, context) = setup().await;
    assert!(
        client.get_moim(&session_id).await.is_none(),
        "a session without python history gets no notice"
    );

    let call = rmcp::model::CallToolRequestParams::new("python");
    let message = Message::assistant().with_tool_request("call_1", Ok(call));
    context
        .session_manager
        .add_message(&session_id, &message)
        .await
        .unwrap();
    let fork = context
        .session_manager
        .copy_session(&session_id, "fork".to_string())
        .await
        .unwrap();

    let notice = client
        .get_moim(&fork.id)
        .await
        .expect("a copied conversation with python calls gets a fresh-namespace notice");
    assert!(
        notice.contains("none of their variables exist here"),
        "got: {notice}"
    );
}

#[tokio::test]
#[serial]
async fn reused_session_id_does_not_inherit_the_deleted_namespace() {
    if !python_available() {
        eprintln!("skipping: python3 not available");
        return;
    }
    let (client, session_id, dir, context) = setup().await;

    run_cell(&client, &session_id, "secret_of_deleted = 1").await;
    hide_python_call(&context, &session_id).await;
    let listing = client.get_moim(&session_id).await.unwrap_or_default();
    assert!(listing.contains("secret_of_deleted"), "got: {listing}");

    context
        .session_manager
        .delete_session(&session_id)
        .await
        .unwrap();
    let reused = context
        .session_manager
        .create_session(
            dir.path().to_path_buf(),
            "reused".to_string(),
            SessionType::User,
            GooseMode::Auto,
        )
        .await
        .unwrap();
    assert_eq!(reused.id, session_id, "the newest id is handed out again");
    hide_python_call(&context, &reused.id).await;

    let moim = client.get_moim(&reused.id).await.unwrap_or_default();
    assert!(
        !moim.contains("secret_of_deleted"),
        "the deleted session's variables leaked into its successor: {moim}"
    );
}

#[tokio::test]
#[serial]
async fn listing_follows_once_a_python_call_is_hidden_from_the_agent() {
    if !python_available() {
        eprintln!("skipping: python3 not available");
        return;
    }
    let (client, session_id, _dir, context) = setup().await;

    run_cell(&client, &session_id, "summarized_away = [1, 2, 3]").await;
    let call = rmcp::model::CallToolRequestParams::new("python");
    let visible = Message::assistant().with_tool_request("call_visible", Ok(call));
    context
        .session_manager
        .add_message(&session_id, &visible)
        .await
        .unwrap();
    assert!(
        client.get_moim(&session_id).await.is_none(),
        "no listing while every python call is still visible"
    );

    hide_python_call(&context, &session_id).await;
    let listing = client
        .get_moim(&session_id)
        .await
        .expect("a hidden python call brings the namespace listing");
    assert!(listing.contains("summarized_away"), "got: {listing}");
}
