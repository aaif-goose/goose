use goose::config::GooseMode;
use goose::conversation::message::{Message, MessageContent, ToolRequest};
use goose::security::adversary_inspector::AdversaryInspector;
use goose::tool_inspection::ToolInspector;
use rmcp::model::CallToolRequestParams;
use rmcp::object;
use std::sync::Arc;
use tokio::sync::Mutex;

fn make_request(
    id: &str,
    tool: &str,
    args: serde_json::Map<String, serde_json::Value>,
) -> ToolRequest {
    ToolRequest {
        id: id.into(),
        tool_call: Ok(CallToolRequestParams::new(tool.to_string()).with_arguments(args)),
        metadata: None,
        tool_meta: None,
    }
}

fn write_adversary_md(dir: &std::path::Path, content: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("adversary.md"), content).unwrap();
}

#[tokio::test]
async fn test_adversary_disabled_without_config_file() {
    let tmp = tempfile::tempdir().unwrap();

    let provider = Arc::new(Mutex::new(None));
    let inspector = AdversaryInspector::with_config_dir(
        provider,
        Arc::new(goose::session::SessionManager::new(
            tmp.path().to_path_buf(),
        )),
        tmp.path().to_path_buf(),
    );

    assert_eq!(inspector.name(), "adversary");
    assert!(!inspector.is_enabled());

    let results = inspector
        .inspect(
            "test-session",
            &[make_request(
                "r1",
                "shell",
                object!({"command": "rm -rf /"}),
            )],
            &[],
            GooseMode::SmartApprove,
        )
        .await
        .unwrap();

    assert!(results.is_empty());
}

#[tokio::test]
async fn test_adversary_enabled_default_tools() {
    let tmp = tempfile::tempdir().unwrap();
    write_adversary_md(tmp.path(), "BLOCK everything for testing");

    let provider = Arc::new(Mutex::new(None));
    let inspector = AdversaryInspector::with_config_dir(
        provider,
        Arc::new(goose::session::SessionManager::new(
            tmp.path().to_path_buf(),
        )),
        tmp.path().to_path_buf(),
    );

    assert!(inspector.is_enabled());

    let messages = vec![Message::new(
        rmcp::model::Role::User,
        chrono::Utc::now().timestamp(),
        vec![MessageContent::text("build the project")],
    )];

    // shell is reviewed by default — no provider means fail-open (Allow)
    let results = inspector
        .inspect(
            "test-session",
            &[make_request(
                "r1",
                "shell",
                object!({"command": "cargo build"}),
            )],
            &messages,
            GooseMode::SmartApprove,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert!(matches!(
        results[0].action,
        goose::tool_inspection::InspectionAction::Allow
    ));

    // write is NOT reviewed by default — skipped entirely
    let results = inspector
        .inspect(
            "test-session",
            &[make_request(
                "r1",
                "write",
                object!({"path": "foo.txt", "content": "hi"}),
            )],
            &messages,
            GooseMode::SmartApprove,
        )
        .await
        .unwrap();

    assert!(results.is_empty());
}

#[tokio::test]
async fn test_adversary_custom_tool_filter() {
    let tmp = tempfile::tempdir().unwrap();
    write_adversary_md(
        tmp.path(),
        "tools: shell, developer__shell\n---\nBLOCK bad stuff",
    );

    let provider = Arc::new(Mutex::new(None));
    let inspector = AdversaryInspector::with_config_dir(
        provider,
        Arc::new(goose::session::SessionManager::new(
            tmp.path().to_path_buf(),
        )),
        tmp.path().to_path_buf(),
    );

    assert!(inspector.is_enabled());

    let messages = vec![Message::new(
        rmcp::model::Role::User,
        chrono::Utc::now().timestamp(),
        vec![MessageContent::text("do something")],
    )];

    // shell — reviewed
    let results = inspector
        .inspect(
            "test",
            &[make_request("r1", "shell", object!({"command": "ls"}))],
            &messages,
            GooseMode::Auto,
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 1);

    // developer__shell — reviewed
    let results = inspector
        .inspect(
            "test",
            &[make_request(
                "r2",
                "developer__shell",
                object!({"script": "echo hi", "language": "shell"}),
            )],
            &messages,
            GooseMode::Auto,
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 1);

    // write — NOT reviewed
    let results = inspector
        .inspect(
            "test",
            &[make_request(
                "r3",
                "write",
                object!({"path": "x.txt", "content": "y"}),
            )],
            &messages,
            GooseMode::Auto,
        )
        .await
        .unwrap();
    assert!(results.is_empty());
}
