use goose::conversation::message::Message;
use goose::session::session_manager::SessionStorage;
use goose::session::thread_manager::ThreadManager;
use rmcp::model::CallToolRequestParams;
use std::sync::Arc;
use tempfile::TempDir;

async fn setup() -> (ThreadManager, TempDir) {
    let tmp = TempDir::new().unwrap();
    let storage = SessionStorage::create(tmp.path()).await.unwrap();
    let tm = ThreadManager::new(Arc::new(storage));
    (tm, tmp)
}

#[tokio::test]
async fn consecutive_text_chunks_are_coalesced() {
    let (tm, _tmp) = setup().await;
    let thread = tm.create_thread(None, None, None).await.unwrap();

    // Simulate streaming: three consecutive assistant text chunks.
    tm.append_message(
        &thread.id,
        Some("s1"),
        &Message::assistant().with_text("Hello"),
    )
    .await
    .unwrap();
    tm.append_message(
        &thread.id,
        Some("s1"),
        &Message::assistant().with_text(" world"),
    )
    .await
    .unwrap();
    tm.append_message(&thread.id, Some("s1"), &Message::assistant().with_text("!"))
        .await
        .unwrap();

    let messages = tm.list_messages(&thread.id).await.unwrap();
    assert_eq!(messages.len(), 1, "should coalesce into a single row");
    assert_eq!(messages[0].as_concat_text(), "Hello world!");
}

#[tokio::test]
async fn role_change_prevents_coalescing() {
    let (tm, _tmp) = setup().await;
    let thread = tm.create_thread(None, None, None).await.unwrap();

    tm.append_message(&thread.id, Some("s1"), &Message::user().with_text("Hi"))
        .await
        .unwrap();
    tm.append_message(
        &thread.id,
        Some("s1"),
        &Message::assistant().with_text("Hey"),
    )
    .await
    .unwrap();

    let messages = tm.list_messages(&thread.id).await.unwrap();
    assert_eq!(messages.len(), 2, "different roles should not coalesce");
    assert_eq!(messages[0].as_concat_text(), "Hi");
    assert_eq!(messages[1].as_concat_text(), "Hey");
}

#[tokio::test]
async fn non_text_content_breaks_coalescing() {
    let (tm, _tmp) = setup().await;
    let thread = tm.create_thread(None, None, None).await.unwrap();

    // Text, then tool request, then more text — should be 3 rows.
    tm.append_message(
        &thread.id,
        Some("s1"),
        &Message::assistant().with_text("Let me check"),
    )
    .await
    .unwrap();

    let tool_msg = Message::assistant().with_tool_request(
        "call_1",
        Ok(CallToolRequestParams::new("shell").with_arguments(
            serde_json::json!({"command": "ls"})
                .as_object()
                .unwrap()
                .clone(),
        )),
    );
    tm.append_message(&thread.id, Some("s1"), &tool_msg)
        .await
        .unwrap();

    tm.append_message(
        &thread.id,
        Some("s1"),
        &Message::assistant().with_text("Done"),
    )
    .await
    .unwrap();

    let messages = tm.list_messages(&thread.id).await.unwrap();
    assert_eq!(messages.len(), 3, "tool request should break coalescing");
    assert_eq!(messages[0].as_concat_text(), "Let me check");
    assert_eq!(messages[2].as_concat_text(), "Done");
}

#[tokio::test]
async fn text_after_tool_response_not_coalesced_with_tool() {
    let (tm, _tmp) = setup().await;
    let thread = tm.create_thread(None, None, None).await.unwrap();

    // A tool request message (non-text) followed by text — should not coalesce.
    let tool_msg = Message::assistant().with_tool_request(
        "call_1",
        Ok(CallToolRequestParams::new("shell").with_arguments(
            serde_json::json!({"command": "ls"})
                .as_object()
                .unwrap()
                .clone(),
        )),
    );
    tm.append_message(&thread.id, Some("s1"), &tool_msg)
        .await
        .unwrap();

    tm.append_message(
        &thread.id,
        Some("s1"),
        &Message::assistant().with_text("Result"),
    )
    .await
    .unwrap();

    let messages = tm.list_messages(&thread.id).await.unwrap();
    assert_eq!(
        messages.len(),
        2,
        "text should not coalesce with non-text predecessor"
    );
}

#[tokio::test]
async fn empty_message_not_coalesced() {
    let (tm, _tmp) = setup().await;
    let thread = tm.create_thread(None, None, None).await.unwrap();

    tm.append_message(
        &thread.id,
        Some("s1"),
        &Message::assistant().with_text("Hello"),
    )
    .await
    .unwrap();

    // An empty assistant message (no content items).
    let empty = Message::assistant();
    tm.append_message(&thread.id, Some("s1"), &empty)
        .await
        .unwrap();

    let messages = tm.list_messages(&thread.id).await.unwrap();
    // Empty message should be inserted as a new row (not coalesced).
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].as_concat_text(), "Hello");
}

#[tokio::test]
async fn metadata_change_prevents_coalescing() {
    let (tm, _tmp) = setup().await;
    let thread = tm.create_thread(None, None, None).await.unwrap();

    tm.append_message(
        &thread.id,
        Some("s1"),
        &Message::assistant().with_text("Visible"),
    )
    .await
    .unwrap();
    tm.append_message(
        &thread.id,
        Some("s1"),
        &Message::assistant().with_text(" hidden").agent_only(),
    )
    .await
    .unwrap();

    let messages = tm.list_messages(&thread.id).await.unwrap();
    assert_eq!(
        messages.len(),
        2,
        "metadata boundary should break coalescing"
    );
    assert!(messages[0].metadata.user_visible);
    assert!(!messages[1].metadata.user_visible);
    assert_eq!(messages[0].as_concat_text(), "Visible");
    assert_eq!(messages[1].as_concat_text(), " hidden");
}
