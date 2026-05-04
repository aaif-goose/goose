use goose::tool_inspection::{InspectionAction, ToolInspector};
use goose::tool_monitor::RepetitionInspector;
use rmcp::model::CallToolRequestParams;
use rmcp::object;

// This test targets RepetitionInspector::check_tool_call
// It verifies that:
// - consecutive identical tool calls are allowed up to max_repetitions times
// - the (max_repetitions + 1)th identical call is denied (returns false)
// - changing the parameters resets the repetition count and allows the call
#[test]
fn test_repetition_inspector_denies_after_exceeding_and_resets_on_param_change() {
    // Allow at most 2 consecutive identical calls
    let mut inspector = RepetitionInspector::new(Some(2));

    // First identical call → allowed
    let call_v1 = CallToolRequestParams::new("fetch_user").with_arguments(object!({"id": 123}));
    assert!(inspector.check_tool_call(call_v1.clone()));

    // Second identical call → still allowed (at limit)
    assert!(inspector.check_tool_call(call_v1.clone()));

    // Third identical call → should be denied (exceeds limit)
    assert!(!inspector.check_tool_call(call_v1.clone()));

    // Change parameters; this should reset the consecutive counter
    let call_v2 = CallToolRequestParams::new("fetch_user").with_arguments(object!({"id": 456}));

    assert!(inspector.check_tool_call(call_v2.clone()));

    // Another identical call with new params → allowed (second in a row for this variant)
    assert!(inspector.check_tool_call(call_v2.clone()));

    // One more identical call with new params → denied again
    assert!(!inspector.check_tool_call(call_v2));
}

fn make_tool_request(tool_name: &'static str) -> goose::conversation::message::ToolRequest {
    goose::conversation::message::ToolRequest {
        id: format!("req_{}", tool_name),
        tool_call: Ok(CallToolRequestParams::new(tool_name).with_arguments(object!({}))),
        metadata: None,
        tool_meta: None,
    }
}

#[tokio::test]
async fn test_error_pattern_fires_after_threshold() {
    let inspector = RepetitionInspector::new(None);
    for _ in 0..3 {
        inspector.record_error("my_tool", "404 Not Found");
    }
    let requests = vec![make_tool_request("my_tool")];
    let results: Vec<_> = ToolInspector::inspect(
        &inspector,
        "session",
        &requests,
        &[],
        goose::config::GooseMode::Auto,
    )
    .await
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].action, InspectionAction::Deny);
    assert_eq!(results[0].finding_id.as_deref(), Some("REP-002"));
}

#[tokio::test]
async fn test_error_pattern_does_not_fire_before_threshold() {
    let inspector = RepetitionInspector::new(None);
    for _ in 0..2 {
        inspector.record_error("my_tool", "404 Not Found");
    }
    let requests = vec![make_tool_request("my_tool")];
    let results: Vec<_> = ToolInspector::inspect(
        &inspector,
        "session",
        &requests,
        &[],
        goose::config::GooseMode::Auto,
    )
    .await
    .unwrap();
    assert!(results
        .iter()
        .all(|r| r.finding_id.as_deref() != Some("REP-002")));
}

#[tokio::test]
async fn test_error_pattern_resets_on_success() {
    let inspector = RepetitionInspector::new(None);
    inspector.record_error("my_tool", "404 Not Found");
    inspector.record_error("my_tool", "404 Not Found");
    inspector.record_success();
    inspector.record_error("my_tool", "404 Not Found");
    let requests = vec![make_tool_request("my_tool")];
    let results: Vec<_> = ToolInspector::inspect(
        &inspector,
        "session",
        &requests,
        &[],
        goose::config::GooseMode::Auto,
    )
    .await
    .unwrap();
    assert!(results
        .iter()
        .all(|r| r.finding_id.as_deref() != Some("REP-002")));
}

#[tokio::test]
async fn test_error_pattern_does_not_cross_tool_names() {
    let inspector = RepetitionInspector::new(None);
    for _ in 0..3 {
        inspector.record_error("tool_a", "same error");
    }
    let requests = vec![make_tool_request("tool_b")];
    let results: Vec<_> = ToolInspector::inspect(
        &inspector,
        "session",
        &requests,
        &[],
        goose::config::GooseMode::Auto,
    )
    .await
    .unwrap();
    assert!(results
        .iter()
        .all(|r| r.finding_id.as_deref() != Some("REP-002")));
}
