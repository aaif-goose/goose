use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use agent_client_protocol::ContentBlock as AcpContentBlock;

use super::{
    needs_provider_update, prepared_session_for_key, prompt_ops::build_content_blocks,
    register_prepared_session_keys, wait_for_replay_drain, ManagerState, PreparedSession,
    MAX_DRAIN_ITERATIONS,
};
use crate::services::acp::split_composite_key;

#[test]
fn provider_update_detects_switch_back_to_goose() {
    assert!(needs_provider_update(Some("openai"), "goose"));
    assert!(needs_provider_update(Some("claude-acp"), "goose"));
    assert!(!needs_provider_update(Some("goose"), "goose"));
    assert!(needs_provider_update(None, "goose"));
}

#[test]
fn pending_cancel_is_consumed_once() {
    let mut state = ManagerState {
        sessions: HashMap::new(),
        op_locks: HashMap::new(),
        pending_cancels: HashSet::new(),
        preparing_sessions: HashSet::new(),
    };

    state.mark_cancel_requested("session-1");

    assert!(state.take_cancel_requested("session-1"));
    assert!(!state.take_cancel_requested("session-1"));
}

#[test]
fn split_composite_key_extracts_local_session_id() {
    assert_eq!(
        split_composite_key("session-1__persona-1"),
        ("session-1", Some("persona-1"))
    );
    assert_eq!(split_composite_key("session-1"), ("session-1", None));
}

#[test]
fn prepared_session_lookup_falls_back_to_local_session_id() {
    let mut sessions = HashMap::new();
    sessions.insert(
        "session-1".to_string(),
        PreparedSession {
            goose_session_id: "goose-1".to_string(),
            provider_id: "goose".to_string(),
            working_dir: PathBuf::from("/tmp/project"),
        },
    );

    let prepared = prepared_session_for_key(&sessions, "session-1__persona-1", "session-1")
        .expect("prepared session");

    assert_eq!(prepared.goose_session_id, "goose-1");
    assert_eq!(prepared.provider_id, "goose");
    assert_eq!(prepared.working_dir, PathBuf::from("/tmp/project"));
}

#[test]
fn register_prepared_session_keys_preserves_composite_and_local_entries() {
    let mut sessions = HashMap::new();
    let prepared = PreparedSession {
        goose_session_id: "goose-1".to_string(),
        provider_id: "goose".to_string(),
        working_dir: PathBuf::from("/tmp/project"),
    };

    register_prepared_session_keys(&mut sessions, "session-1__persona-1", "session-1", prepared);

    assert!(sessions.contains_key("session-1__persona-1"));
    assert!(sessions.contains_key("session-1"));
    assert_eq!(
        sessions
            .get("session-1__persona-1")
            .expect("composite session")
            .goose_session_id,
        "goose-1"
    );
}

#[tokio::test]
async fn replay_drain_returns_immediately_when_count_is_zero() {
    let final_count = wait_for_replay_drain(|| async { 0u32 }).await;
    assert_eq!(final_count, 0);
}

#[tokio::test]
async fn replay_drain_returns_stable_count() {
    let counter = Arc::new(AtomicU32::new(42));
    let c = counter.clone();
    let final_count = wait_for_replay_drain(|| {
        let c = c.clone();
        async move { c.load(Ordering::SeqCst) }
    })
    .await;
    assert_eq!(final_count, 42);
}

#[tokio::test]
async fn replay_drain_waits_for_spawned_notifications() {
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();

    // Simulate async notifications arriving over multiple yields, like
    // the real ACP RPC layer does after load_session returns.
    tokio::spawn(async move {
        for i in 1..=5 {
            tokio::task::yield_now().await;
            c.store(i, Ordering::SeqCst);
        }
    });

    let c2 = counter.clone();
    let final_count = wait_for_replay_drain(|| {
        let c = c2.clone();
        async move { c.load(Ordering::SeqCst) }
    })
    .await;

    assert_eq!(final_count, 5);
}

#[tokio::test]
async fn replay_drain_resets_stability_on_late_arrival() {
    // Simulate: counter jumps to 3, stabilises for 2 rounds, then a late
    // notification bumps it to 4. The drain must NOT stop at 3.
    let poll_count = Arc::new(AtomicU32::new(0));

    let pc = poll_count.clone();
    let final_count = wait_for_replay_drain(|| {
        let pc = pc.clone();
        async move {
            let poll = pc.fetch_add(1, Ordering::SeqCst);
            // Polls 0..2 return 3 (2 stable rounds), then poll 3 bumps
            // to 4 — simulating a late notification just before the drain
            // would have declared stability. The drain must reset and
            // wait for 4 to stabilise.
            if poll < 3 {
                3
            } else {
                4
            }
        }
    })
    .await;

    // Must see the late arrival, not stop at 3
    assert_eq!(final_count, 4);
    // Verify the stability window truly reset: 3 polls to see 3, 1 poll
    // to see the bump to 4, then 3 more polls for 4 to stabilise = 7 min.
    assert!(
        poll_count.load(Ordering::SeqCst) >= 7,
        "expected at least 7 polls to confirm stability window reset, got {}",
        poll_count.load(Ordering::SeqCst)
    );
}

#[tokio::test]
async fn replay_drain_caps_iterations_on_runaway_counter() {
    // Simulate a counter that never stabilises — increments every poll.
    let poll_count = Arc::new(AtomicU32::new(0));
    let pc = poll_count.clone();
    let final_count = wait_for_replay_drain(|| {
        let pc = pc.clone();
        async move { pc.fetch_add(1, Ordering::SeqCst) + 1 }
    })
    .await;

    // Should have stopped at the cap rather than spinning forever.
    assert_eq!(final_count, MAX_DRAIN_ITERATIONS);
    assert_eq!(poll_count.load(Ordering::SeqCst), MAX_DRAIN_ITERATIONS);
}

#[test]
fn build_content_blocks_places_images_before_prompt_text() {
    let blocks = build_content_blocks(
        "Please inspect all three attachments".to_string(),
        vec![("abc123".to_string(), "image/png".to_string())],
    );

    assert_eq!(blocks.len(), 2);
    assert!(matches!(blocks[0], AcpContentBlock::Image(_)));
    assert!(matches!(blocks[1], AcpContentBlock::Text(_)));
}
