use futures::StreamExt;
use std::time::Duration;
use tokio::time::timeout;

/// End-to-end test: share session → connect SSE → publish suggestion → verify SSE delivery
///
/// This test:
/// 1. Starts a real goosed server
/// 2. Creates a session and shares it via Nostr (saving owner channel)
/// 3. Connects to the SSE event stream for that session
/// 4. Publishes a suggestion to the Nostr relay
/// 5. Verifies the suggestion arrives on the SSE stream
#[tokio::test]
#[ignore] // requires network access to Nostr relays
async fn test_suggestion_arrives_via_sse() {
    // 1. Start server
    let state = goose_server::state::AppState::new(false)
        .await
        .expect("Failed to create AppState");
    let secret_key = "test-secret-key".to_string();
    let app = goose_server::routes::configure(state.clone(), secret_key.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let addr = listener.local_addr().unwrap();
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let base = format!("http://{}", addr);
    let client = reqwest::Client::new();

    let sm = state.session_manager();
    use goose::config::GooseMode;
    use goose::session::session_manager::SessionType;
    use std::path::PathBuf;

    let session = sm
        .create_session(
            PathBuf::from("/tmp"),
            "test_sse_session".to_string(),
            SessionType::User,
            GooseMode::Auto,
        )
        .await
        .expect("Failed to create session");
    let session_id = session.id.as_str();

    // 3. Share via Nostr to create the channel
    let share_resp = client
        .post(format!("{}/sessions/{}/share/nostr", base, session_id))
        .header("Content-Type", "application/json")
        .header("X-Secret-Key", &secret_key)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("Share request failed");

    assert!(
        share_resp.status().is_success(),
        "Share failed: {} - {}",
        share_resp.status(),
        share_resp.text().await.unwrap_or_default()
    );

    // Re-fetch to get the deeplink
    let share_resp = client
        .post(format!("{}/sessions/{}/share/nostr", base, session_id))
        .header("Content-Type", "application/json")
        .header("X-Secret-Key", &secret_key)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("Share request failed");

    let share_body: serde_json::Value = share_resp
        .json()
        .await
        .expect("Failed to parse share response");
    let deeplink = share_body["deeplink"]
        .as_str()
        .expect("No deeplink in response");
    println!("Deeplink: {}", deeplink);

    // Verify channel was saved
    let channel = sm
        .get_nostr_channel(session_id)
        .await
        .expect("DB error")
        .expect("Channel not saved");
    assert_eq!(
        channel.role,
        goose::session::nostr_channel::ChannelRole::Owner
    );
    println!(
        "Channel saved: event_id={}, role={:?}",
        channel.event_id, channel.role
    );

    // 4. Connect to SSE
    let sse_resp = client
        .get(format!("{}/sessions/{}/events", base, session_id))
        .header("X-Secret-Key", &secret_key)
        .send()
        .await
        .expect("SSE connect failed");

    assert!(
        sse_resp.status().is_success(),
        "SSE connect failed: {}",
        sse_resp.status()
    );

    // Give the subscription time to connect to relays
    tokio::time::sleep(Duration::from_secs(6)).await;

    // 5. Publish a suggestion
    let parsed =
        goose::session::nostr_share::parse_deeplink(deeplink).expect("Failed to parse deeplink");
    let (event_id, relays) =
        goose::session::nostr_share::parse_nevent(&parsed.nevent).expect("Failed to parse nevent");

    println!("Publishing suggestion to event {}...", &event_id);
    goose::session::nostr_share::publish_suggestion(
        &parsed.decryption_key,
        &event_id,
        "E2E test suggestion",
        Some("TestBot"),
        relays,
    )
    .await
    .expect("Failed to publish suggestion");
    println!("Published suggestion");

    // 6. Read from SSE stream and look for the Suggestion event
    let mut stream = sse_resp.bytes_stream();

    let result = timeout(Duration::from_secs(20), async {
        let mut buffer = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk: bytes::Bytes = chunk.expect("Stream error");
            let text = String::from_utf8_lossy(&chunk);
            println!("SSE chunk: {:?}", text);
            buffer.push_str(&text);

            // Look for suggestion event in the buffer
            for line in buffer.lines() {
                if line.starts_with("data:") {
                    let data = line.trim_start_matches("data:").trim();
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                        println!(
                            "Parsed SSE event: type={}",
                            json.get("type").unwrap_or(&serde_json::Value::Null)
                        );
                        if json.get("type").and_then(|t| t.as_str()) == Some("Suggestion") {
                            return json;
                        }
                    }
                }
            }
        }
        panic!("Stream ended without suggestion");
    })
    .await;

    match result {
        Ok(json) => {
            println!("\n=== SUGGESTION RECEIVED VIA SSE ===");
            println!("  type: {}", json["type"]);
            println!("  text: {}", json["text"]);
            println!("  sender_name: {}", json["sender_name"]);
            println!("  event_id: {}", json["event_id"]);
            assert_eq!(json["type"], "Suggestion");
            assert_eq!(json["text"], "E2E test suggestion");
            assert_eq!(json["sender_name"], "TestBot");
        }
        Err(_) => {
            panic!("Timed out waiting for suggestion on SSE stream");
        }
    }

    server_handle.abort();
}
