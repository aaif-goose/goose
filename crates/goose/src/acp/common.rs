use std::str::FromStr;

use crate::permission::Permission;
use sacp::schema::{
    PermissionOption, PermissionOptionKind, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome,
};
use strum::{Display, EnumString};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Display, EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum PermissionDecision {
    AllowAlways,
    AllowOnce,
    RejectAlways,
    RejectOnce,
    Cancel,
}

impl PermissionDecision {
    pub fn should_record_rejection(self) -> bool {
        matches!(
            self,
            PermissionDecision::RejectAlways
                | PermissionDecision::RejectOnce
                | PermissionDecision::Cancel
        )
    }
}

impl From<Permission> for PermissionDecision {
    fn from(p: Permission) -> Self {
        match p {
            Permission::AlwaysAllow => Self::AllowAlways,
            Permission::AllowOnce => Self::AllowOnce,
            Permission::DenyOnce => Self::RejectOnce,
            Permission::AlwaysDeny => Self::RejectAlways,
            Permission::Cancel => Self::Cancel,
        }
    }
}

impl From<PermissionDecision> for Permission {
    fn from(d: PermissionDecision) -> Self {
        match d {
            PermissionDecision::AllowAlways => Self::AlwaysAllow,
            PermissionDecision::AllowOnce => Self::AllowOnce,
            PermissionDecision::RejectOnce => Self::DenyOnce,
            PermissionDecision::RejectAlways => Self::AlwaysDeny,
            PermissionDecision::Cancel => Self::Cancel,
        }
    }
}

impl From<&RequestPermissionOutcome> for PermissionDecision {
    fn from(outcome: &RequestPermissionOutcome) -> Self {
        match outcome {
            RequestPermissionOutcome::Cancelled => Self::Cancel,
            RequestPermissionOutcome::Selected(selected) => {
                Self::from_str(&selected.option_id.0).unwrap_or(Self::Cancel)
            }
            _ => Self::Cancel,
        }
    }
}

/// Map a permission decision to a response by matching the option kind from the
/// request. Each decision tries its preferred kind first, then falls back to the
/// closest alternative (e.g. AllowAlways falls back to AllowOnce).
pub fn map_permission_response(
    request: &RequestPermissionRequest,
    decision: PermissionDecision,
) -> RequestPermissionResponse {
    let selected_id = match decision {
        PermissionDecision::AllowAlways => {
            find_option(&request.options, PermissionOptionKind::AllowAlways)
                .or_else(|| find_option(&request.options, PermissionOptionKind::AllowOnce))
        }
        PermissionDecision::AllowOnce => {
            find_option(&request.options, PermissionOptionKind::AllowOnce)
                .or_else(|| find_option(&request.options, PermissionOptionKind::AllowAlways))
        }
        PermissionDecision::RejectAlways => {
            find_option(&request.options, PermissionOptionKind::RejectAlways)
                .or_else(|| find_option(&request.options, PermissionOptionKind::RejectOnce))
        }
        PermissionDecision::RejectOnce => {
            find_option(&request.options, PermissionOptionKind::RejectOnce)
                .or_else(|| find_option(&request.options, PermissionOptionKind::RejectAlways))
        }
        PermissionDecision::Cancel => None,
    };

    if let Some(option_id) = selected_id {
        RequestPermissionResponse::new(RequestPermissionOutcome::Selected(
            SelectedPermissionOutcome::new(option_id),
        ))
    } else {
        RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)
    }
}

fn find_option(options: &[PermissionOption], kind: PermissionOptionKind) -> Option<String> {
    options
        .iter()
        .find(|opt| opt.kind == kind)
        .map(|opt| opt.option_id.0.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sacp::schema::{PermissionOptionId, ToolCallId, ToolCallUpdate, ToolCallUpdateFields};
    use test_case::test_case;

    fn make_request(options: Vec<PermissionOption>) -> RequestPermissionRequest {
        let tool_call =
            ToolCallUpdate::new(ToolCallId::new("tool-1"), ToolCallUpdateFields::default());
        RequestPermissionRequest::new("session-1", tool_call, options)
    }

    fn option(id: &str, kind: PermissionOptionKind) -> PermissionOption {
        PermissionOption::new(
            PermissionOptionId::new(id.to_string()),
            id.to_string(),
            kind,
        )
    }

    #[test_case(
        PermissionDecision::AllowAlways,
        "allow_always";
        "allow_always_matches_kind"
    )]
    #[test_case(
        PermissionDecision::AllowOnce,
        "allow_once";
        "allow_once_matches_kind"
    )]
    #[test_case(
        PermissionDecision::RejectOnce,
        "reject_once";
        "reject_once_matches_kind"
    )]
    #[test_case(
        PermissionDecision::RejectAlways,
        "reject_always";
        "reject_always_matches_kind"
    )]
    fn test_permission_response(decision: PermissionDecision, expected_id: &str) {
        let options = vec![
            option("allow_once", PermissionOptionKind::AllowOnce),
            option("allow_always", PermissionOptionKind::AllowAlways),
            option("reject_once", PermissionOptionKind::RejectOnce),
            option("reject_always", PermissionOptionKind::RejectAlways),
        ];
        let request = make_request(options);
        let response = map_permission_response(&request, decision);
        match response.outcome {
            RequestPermissionOutcome::Selected(selected) => {
                assert_eq!(selected.option_id.0.as_ref(), expected_id);
            }
            _ => panic!("expected selected outcome"),
        }
    }

    #[test]
    fn test_allow_always_falls_back_to_allow_once() {
        let request = make_request(vec![option("allow_once", PermissionOptionKind::AllowOnce)]);
        let response = map_permission_response(&request, PermissionDecision::AllowAlways);
        match response.outcome {
            RequestPermissionOutcome::Selected(selected) => {
                assert_eq!(selected.option_id.0.as_ref(), "allow_once");
            }
            _ => panic!("expected selected outcome"),
        }
    }

    #[test_case(PermissionDecision::Cancel; "cancelled")]
    fn test_permission_cancelled(decision: PermissionDecision) {
        let request = make_request(vec![option("allow_once", PermissionOptionKind::AllowOnce)]);
        let response = map_permission_response(&request, decision);
        assert!(matches!(
            response.outcome,
            RequestPermissionOutcome::Cancelled
        ));
    }

    #[test_case(Permission::AlwaysAllow, PermissionDecision::AllowAlways; "always_allow")]
    #[test_case(Permission::AllowOnce, PermissionDecision::AllowOnce; "allow_once")]
    #[test_case(Permission::DenyOnce, PermissionDecision::RejectOnce; "deny_once")]
    #[test_case(Permission::AlwaysDeny, PermissionDecision::RejectAlways; "always_deny")]
    #[test_case(Permission::Cancel, PermissionDecision::Cancel; "cancel")]
    fn test_permission_to_decision(input: Permission, expected: PermissionDecision) {
        assert_eq!(PermissionDecision::from(input), expected);
    }

    #[test_case(PermissionDecision::AllowAlways, Permission::AlwaysAllow; "allow_always")]
    #[test_case(PermissionDecision::AllowOnce, Permission::AllowOnce; "allow_once")]
    #[test_case(PermissionDecision::RejectOnce, Permission::DenyOnce; "reject_once")]
    #[test_case(PermissionDecision::RejectAlways, Permission::AlwaysDeny; "reject_always")]
    #[test_case(PermissionDecision::Cancel, Permission::Cancel; "cancel")]
    fn test_decision_to_permission(input: PermissionDecision, expected: Permission) {
        assert_eq!(Permission::from(input), expected);
    }

    #[test_case("allow_once", PermissionDecision::AllowOnce; "allow_once")]
    #[test_case("allow_always", PermissionDecision::AllowAlways; "allow_always")]
    #[test_case("reject_once", PermissionDecision::RejectOnce; "reject_once")]
    #[test_case("reject_always", PermissionDecision::RejectAlways; "reject_always")]
    #[test_case("unknown", PermissionDecision::Cancel; "unknown_maps_to_cancel")]
    fn test_outcome_to_decision(option_id: &str, expected: PermissionDecision) {
        let outcome = RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
            PermissionOptionId::new(option_id.to_string()),
        ));
        assert_eq!(PermissionDecision::from(&outcome), expected);
    }

    #[test]
    fn test_cancelled_outcome_to_decision() {
        assert_eq!(
            PermissionDecision::from(&RequestPermissionOutcome::Cancelled),
            PermissionDecision::Cancel
        );
    }
}
