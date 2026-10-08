use futures::StreamExt;
use goose::agents::extension::PlatformExtensionContext;
use goose::agents::extension_manager::ExtensionManager;
use goose::agents::mcp_client::McpClientTrait;
use goose::agents::platform_extensions::developer::python_session::PythonSessionClient;
use goose::agents::{Agent, AgentEvent, ExtensionConfig, SessionConfig, ToolCallContext};
use goose::config::GooseMode;
use goose::conversation::message::Message;
use goose::providers::base::{stream_from_single_message, MessageStream, Provider};
use goose::session::SessionType;
use goose_providers::conversation::token_usage::{ProviderUsage, Usage};
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};
use serde_json::json;
use serial_test::serial;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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
async fn saved_namespace_is_listed_without_restoring_it() {
    if !python_available() {
        eprintln!("skipping: python3 not available");
        return;
    }
    let (client, session_id, dir, context) = setup().await;
    let marker = dir.path().join("class-body-ran");
    let cell = format!(
        "import socket\nclass Tracked:\n    open({:?}, 'a').write('x')\nkept = Tracked()\nsock = socket.socket()",
        marker.display().to_string()
    );
    run_cell(&client, &session_id, &cell).await;
    hide_python_call(&context, &session_id).await;
    drop(client);

    // A new process: no kernel, only the snapshot and its listing on disk.
    let client = PythonSessionClient::new(context.clone()).unwrap();
    let listing = client
        .get_moim(&session_id)
        .await
        .expect("the saved listing is shown");
    assert!(
        listing.contains("kept") && listing.contains("Tracked"),
        "got: {listing}"
    );
    assert!(
        !listing.contains("sock"),
        "a variable the snapshot cannot restore is not listed: {listing}"
    );
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap(),
        "x",
        "listing the namespace must not restore it, which re-runs definitions"
    );

    let result = run_cell(&client, &session_id, "type(kept).__name__").await;
    assert!(
        result_text(&result).contains("=> 'Tracked'"),
        "got: {}",
        result_text(&result)
    );
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "xx");
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

#[tokio::test]
#[serial]
async fn restore_notice_bounds_the_names_it_lists() {
    if !python_available() {
        eprintln!("skipping: python3 not available");
        return;
    }
    let (client, session_id, _dir, _) = setup().await;

    run_cell(
        &client,
        &session_id,
        "globals().update({'v%d' % i: i for i in range(200)})",
    )
    .await;
    run_cell(&client, &session_id, "import os; os._exit(3)").await;

    let text = result_text(&run_cell(&client, &session_id, "v199").await);
    assert!(text.contains("=> 199"), "got: {text}");
    assert!(text.contains("(+150 more)"), "got: {text}");
    assert!(
        !text.contains("v120,"),
        "names past the cap are counted, not listed"
    );
}

const TURN_PROMPT: &str = "Sort the orders";

/// Fails the first request of the turn with a context overflow, then records what the retry
/// after recovery compaction sends.
struct OverflowOnceProvider {
    overflowed: AtomicBool,
    requests: Mutex<Vec<Vec<Message>>>,
}

#[async_trait::async_trait]
impl Provider for OverflowOnceProvider {
    fn get_name(&self) -> &str {
        "overflow-once"
    }

    async fn stream(
        &self,
        _model_config: &ModelConfig,
        _system_prompt: &str,
        messages: &[Message],
        _tools: &[rmcp::model::Tool],
    ) -> Result<MessageStream, ProviderError> {
        // Side requests (the session title, the compaction summary) send a single
        // message and carry the conversation, if at all, in their prompt.
        let is_turn_request = messages.len() > 1;
        let reply = if !is_turn_request {
            "<summary of the conversation>"
        } else if !self.overflowed.swap(true, Ordering::SeqCst) {
            return Err(ProviderError::ContextLengthExceeded("too long".to_string()));
        } else {
            self.requests.lock().unwrap().push(messages.to_vec());
            "done"
        };
        Ok(stream_from_single_message(
            Message::assistant().with_text(reply),
            ProviderUsage::new(
                "mock-model".to_string(),
                Usage::new(Some(100), Some(10), Some(110)),
            ),
        ))
    }
}

#[tokio::test]
#[serial]
async fn retry_after_recovery_compaction_lists_the_namespace() {
    if !python_available() {
        eprintln!("skipping: python3 not available");
        return;
    }
    isolate_data_dir();
    std::env::set_var("GOOSE_DEVELOPER_MODE", "python_session");
    for use_state_machine in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let agent = Agent::new();
        let session_manager = agent.config.session_manager.clone();
        let session = session_manager
            .create_session(
                dir.path().to_path_buf(),
                "recovery-compaction".to_string(),
                SessionType::Hidden,
                GooseMode::Auto,
            )
            .await
            .unwrap();

        let writer =
            PythonSessionClient::new(agent.extension_manager.get_context().clone()).unwrap();
        run_cell(&writer, &session.id, "orders = [3, 1, 2]").await;
        drop(writer);
        let call = CallToolRequestParams::new("developer__python");
        for message in [
            Message::user().with_text("Load the orders"),
            Message::assistant().with_tool_request("call_1", Ok(call)),
            Message::user().with_tool_response(
                "call_1",
                Ok(CallToolResult::success(vec![ContentBlock::text("ok")])),
            ),
            Message::assistant().with_text("Loaded them into `orders`."),
        ] {
            session_manager
                .add_message(&session.id, &message)
                .await
                .unwrap();
        }

        agent
            .extension_manager
            .add_extension(
                ExtensionConfig::Platform {
                    name: "developer".to_string(),
                    description: String::new(),
                    display_name: None,
                    bundled: Some(true),
                    available_tools: vec![],
                },
                None,
                None,
                None,
            )
            .await
            .unwrap();
        let provider = Arc::new(OverflowOnceProvider {
            overflowed: AtomicBool::new(false),
            requests: Mutex::new(Vec::new()),
        });
        agent
            .update_provider(
                provider.clone(),
                ModelConfig::new("mock-model"),
                &session.id,
            )
            .await
            .unwrap();

        let session_config = SessionConfig {
            id: session.id.clone(),
            schedule_id: None,
            max_turns: None,
            retry_config: None,
        };
        let stream = agent
            .reply(
                Message::user().with_text(TURN_PROMPT),
                session_config,
                use_state_machine,
                None,
            )
            .await
            .unwrap();
        let events: Vec<_> = stream.collect().await;
        assert!(
            events
                .iter()
                .any(|event| matches!(event, Ok(AgentEvent::HistoryReplaced(_)))),
            "the overflow should trigger recovery compaction"
        );

        let requests = provider.requests.lock().unwrap();
        let retry = requests.first().expect("the turn should be retried");
        let sent: Vec<String> = retry.iter().map(Message::as_concat_text).collect();
        assert!(
            sent.iter()
                .any(|text| text.contains("<python-session>") && text.contains("orders")),
            "use_state_machine={use_state_machine}: the retry should list the namespace, got: {sent:#?}"
        );
    }
    std::env::remove_var("GOOSE_DEVELOPER_MODE");
}
