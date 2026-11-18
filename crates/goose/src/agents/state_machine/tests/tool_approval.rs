use std::io::Write;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::tool_approval::{confirmation_response_message, ToolApprovalOperation};
use rmcp::model::CallToolRequestParams;
use serde_json::Value;
use test_case::test_case;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::instrument::WithSubscriber;

use crate::agents::state_machine::tool_approval::GooseToolApproval;
use crate::agents::state_machine::{Emitter, GooseEffect, StateMachine, Step};
use crate::config::{GooseMode, PermissionManager};
use crate::conversation::message::{Message, ToolRequest};
use crate::permission::permission_inspector::PermissionInspector;
use crate::permission::Permission;
use crate::session::{Session, SessionManager, SessionType};
use crate::tool_inspection::{
    InspectionAction, InspectionResult, ToolInspectionManager, ToolInspector,
};

#[derive(Clone, Default)]
struct TraceCapture(Arc<Mutex<Vec<u8>>>);

impl Write for TraceCapture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl TraceCapture {
    fn events(&self) -> Vec<Value> {
        String::from_utf8(self.0.lock().unwrap().clone())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap()["fields"].clone())
            .filter(|fields| fields["security.event_type"] != Value::Null)
            .collect()
    }
}

struct SecurityFinding;

#[async_trait]
impl ToolInspector for SecurityFinding {
    fn name(&self) -> &'static str {
        "security"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn inspect(
        &self,
        session_id: &str,
        requests: &[ToolRequest],
        _messages: &[Message],
        _mode: GooseMode,
    ) -> Result<Vec<InspectionResult>> {
        Ok(requests
            .iter()
            .filter(|request| request.id != "ordinary")
            .map(|request| InspectionResult {
                tool_request_id: request.id.clone(),
                action: InspectionAction::RequireApproval(Some("security warning".into())),
                reason: "test security finding".into(),
                confidence: 1.0,
                inspector_name: self.name().into(),
                finding_id: Some(format!("SEC-{session_id}-{}", request.id)),
            })
            .collect())
    }
}

fn inspection_manager(
    permissions: &Arc<PermissionManager>,
    sessions: &Arc<SessionManager>,
) -> ToolInspectionManager {
    let mut manager = ToolInspectionManager::new();
    manager.add_inspector(Box::new(PermissionInspector::new(
        permissions.clone(),
        Default::default(),
        sessions.clone(),
    )));
    manager.add_inspector(Box::new(SecurityFinding));
    manager
}

async fn approval_step(
    manager: &ToolInspectionManager,
    sessions: &SessionManager,
    session_id: &str,
) -> Result<Option<&'static str>> {
    let cancel = CancellationToken::new();
    let machine: StateMachine<'_, Session, GooseEffect> = StateMachine::new(
        vec![Step::Operation(Arc::new(ToolApprovalOperation::new(
            Arc::new(GooseToolApproval::new(manager)),
        )))],
        cancel.clone(),
    );
    let (tx, _rx) = mpsc::unbounded_channel();
    let emit = Emitter::new(tx, cancel);
    let session = sessions.get_session(session_id, true).await?;
    let mut result = machine.step(&session, &emit).await?;
    machine
        .apply(sessions, &session, &mut result.effects, &emit)
        .await?;
    Ok(result.applied_step)
}

#[test_case(Permission::AllowOnce, "ALLOW"; "allow_once")]
#[test_case(Permission::AlwaysAllow, "ALLOW"; "always_allow")]
#[test_case(Permission::DenyOnce, "BLOCK"; "deny_once")]
#[test_case(Permission::AlwaysDeny, "BLOCK"; "always_deny")]
#[test_case(Permission::Cancel, "BLOCK"; "cancel")]
#[tokio::test]
async fn security_decision_correlates_with_approval(
    permission: Permission,
    action: &str,
) -> Result<()> {
    let capture = TraceCapture::default();
    let writer = capture.clone();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_writer(move || writer.clone())
        .finish();
    async {
        let directory = tempfile::tempdir()?;
        let permissions = Arc::new(PermissionManager::new(directory.path().into()));
        let sessions = Arc::new(SessionManager::new(directory.path().into()));
        let manager = inspection_manager(&permissions, &sessions);
        for (session_id, request_id, mode) in [
            ("first", "flagged", GooseMode::Auto),
            ("second", "flagged", GooseMode::Auto),
            ("ordinary", "ordinary", GooseMode::Approve),
        ] {
            sessions
                .create_session_with_id(
                    session_id,
                    directory.path().into(),
                    session_id.into(),
                    SessionType::User,
                    mode,
                )
                .await?;
            sessions
                .add_message(session_id, &Message::user().with_text("run tool"))
                .await?;
            sessions
                .add_message(
                    session_id,
                    &Message::assistant()
                        .with_tool_request(request_id, Ok(CallToolRequestParams::new("test_tool"))),
                )
                .await?;
            assert_eq!(
                approval_step(&manager, &sessions, session_id).await?,
                Some("tool_approval")
            );
            assert_eq!(approval_step(&manager, &sessions, session_id).await?, None);
        }
        let requests = capture.events();
        assert_eq!(
            requests
                .iter()
                .filter(|event| event["security.event_type"] == "approval_request")
                .count(),
            2
        );
        assert!(requests.iter().all(|event| event
            ["monotonic_counter.goose.prompt_injection_user_decisions"]
            .is_null()));

        drop(manager);
        drop(sessions);
        let sessions = Arc::new(SessionManager::new(directory.path().into()));
        let manager = inspection_manager(&permissions, &sessions);
        for (session_id, request_id) in [
            ("ordinary", "ordinary"),
            ("second", "flagged"),
            ("first", "flagged"),
        ] {
            sessions
                .add_message(
                    session_id,
                    &confirmation_response_message(request_id, permission.clone()),
                )
                .await?;
            assert_eq!(
                approval_step(&manager, &sessions, session_id).await?,
                Some("tool_approval")
            );
            assert_eq!(approval_step(&manager, &sessions, session_id).await?, None);
        }

        let events = capture.events();
        let decisions: Vec<_> = events
            .iter()
            .filter(|event| event["security.event_type"] == "user_decision")
            .collect();
        assert_eq!(decisions.len(), 2);
        for (decision, session_id) in decisions.iter().zip(["second", "first"]) {
            let finding_id = format!("SEC-{session_id}-flagged");
            assert_eq!(decision["security.action"], action);
            assert_eq!(decision["security.finding_id"], finding_id);
            assert_eq!(decision["tool.request_id"], "flagged");
            assert_eq!(decision["user.decision"], format!("{permission:?}"));
            assert_eq!(
                decision["monotonic_counter.goose.prompt_injection_user_decisions"],
                1
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|event| event["security.event_type"] == "approval_request"
                        && event["security.finding_id"] == finding_id
                        && event["tool.request_id"] == decision["tool.request_id"])
                    .count(),
                1
            );
        }
        let expected_permission = match permission {
            Permission::AlwaysAllow => {
                Some(crate::config::permission::PermissionLevel::AlwaysAllow)
            }
            Permission::AlwaysDeny => Some(crate::config::permission::PermissionLevel::NeverAllow),
            _ => None,
        };
        assert_eq!(
            permissions.get_user_permission("test_tool"),
            expected_permission
        );
        Ok(())
    }
    .with_subscriber(subscriber)
    .await
}
