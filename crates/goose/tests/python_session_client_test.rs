use goose::agents::extension_manager::ExtensionManager;
use goose::agents::mcp_client::McpClientTrait;
use goose::agents::platform_extensions::python_session::PythonSessionClient;
use goose::agents::ToolCallContext;
use goose::config::GooseMode;
use goose::session::SessionType;
use serde_json::json;
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

async fn setup() -> (PythonSessionClient, String, tempfile::TempDir) {
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
    let client = PythonSessionClient::new(context).unwrap();
    (client, session.id, temp_dir)
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
async fn cells_share_state_and_moim_waits_for_compaction() {
    if !python_available() {
        eprintln!("skipping: python3 not available");
        return;
    }
    let (client, session_id, _dir) = setup().await;

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
async fn kernel_death_is_reported_and_snapshot_restores_the_next_cell() {
    if !python_available() {
        eprintln!("skipping: python3 not available");
        return;
    }
    let (client, session_id, _dir) = setup().await;

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
