use goose::conversation::message::ToolRequest;
use goose::permission::permission_judge::PermissionCheckResult;
use goose::tool_inspection::{
    apply_inspection_results_to_permissions, InspectionAction, InspectionResult,
};
use rmcp::model::CallToolRequestParams;
use rmcp::object;

fn request(id: &str) -> ToolRequest {
    ToolRequest {
        id: id.to_string(),
        tool_call: Ok(CallToolRequestParams::new("test_tool").with_arguments(object!({}))),
        metadata: None,
        tool_meta: None,
    }
}

fn inspection(id: &str, action: InspectionAction) -> InspectionResult {
    InspectionResult {
        tool_request_id: id.to_string(),
        action,
        reason: "test decision".to_string(),
        confidence: 1.0,
        inspector_name: "test_inspector".to_string(),
        finding_id: None,
    }
}

fn assert_only_denied(result: PermissionCheckResult, id: &str) {
    assert!(result.approved.is_empty());
    assert!(result.needs_approval.is_empty());
    assert_eq!(result.denied, vec![request(id)]);
}

#[test]
fn require_approval_does_not_resurrect_a_denied_request() {
    let result = apply_inspection_results_to_permissions(
        PermissionCheckResult {
            approved: vec![],
            needs_approval: vec![],
            denied: vec![request("request-1")],
        },
        &[inspection(
            "request-1",
            InspectionAction::RequireApproval(Some("security warning".to_string())),
        )],
    );

    assert_only_denied(result, "request-1");
}

#[test]
fn denial_dominates_regardless_of_inspection_result_order() {
    for inspection_results in [
        vec![
            inspection("request-1", InspectionAction::Deny),
            inspection(
                "request-1",
                InspectionAction::RequireApproval(Some("security warning".to_string())),
            ),
        ],
        vec![
            inspection(
                "request-1",
                InspectionAction::RequireApproval(Some("security warning".to_string())),
            ),
            inspection("request-1", InspectionAction::Deny),
        ],
    ] {
        let result = apply_inspection_results_to_permissions(
            PermissionCheckResult {
                approved: vec![request("request-1")],
                needs_approval: vec![],
                denied: vec![],
            },
            &inspection_results,
        );

        assert_only_denied(result, "request-1");
    }
}

#[test]
fn require_approval_still_moves_an_approved_request_to_needs_approval() {
    let result = apply_inspection_results_to_permissions(
        PermissionCheckResult {
            approved: vec![request("request-1")],
            needs_approval: vec![],
            denied: vec![],
        },
        &[inspection(
            "request-1",
            InspectionAction::RequireApproval(Some("security warning".to_string())),
        )],
    );

    assert!(result.approved.is_empty());
    assert_eq!(result.needs_approval, vec![request("request-1")]);
    assert!(result.denied.is_empty());
}
