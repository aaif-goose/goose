use std::collections::HashSet;

use anyhow::Result;
use async_trait::async_trait;
use goose_agent::tool_approval::{ApprovalDecision, ToolApproval};

use crate::agents::state_machine::ops_toolcalling::call_will_not_run;
use crate::agents::tool_execution::DECLINED_RESPONSE;
use crate::config::permission::PermissionLevel;
use crate::conversation::message::ToolRequest;
use crate::conversation::Conversation;
use crate::permission::Permission;
use crate::session::Session;
use crate::tool_inspection::{
    get_security_finding_id_from_results, InspectionAction, ToolInspectionManager,
};

const SECURITY_FINDING_ID_KEY: &str = "goose.security_finding_id";

pub struct GooseToolApproval<'a> {
    tool_inspection_manager: &'a ToolInspectionManager,
}

impl<'a> GooseToolApproval<'a> {
    pub fn new(tool_inspection_manager: &'a ToolInspectionManager) -> Self {
        Self {
            tool_inspection_manager,
        }
    }
}

#[async_trait]
impl ToolApproval<Session> for GooseToolApproval<'_> {
    async fn check(
        &self,
        session: &Session,
        conversation: &Conversation,
        requests: &[&ToolRequest],
    ) -> Result<Vec<ApprovalDecision>> {
        let mut to_inspect = Vec::new();
        for request in requests {
            if !call_will_not_run(session, conversation, request)? {
                to_inspect.push((*request).clone());
            }
        }
        if to_inspect.is_empty() {
            return Ok(requests.iter().map(|_| ApprovalDecision::Allow).collect());
        }

        let inspection_results = self
            .tool_inspection_manager
            .inspect_tools(
                &session.id,
                &to_inspect,
                conversation.messages(),
                session.goose_mode,
            )
            .await?;
        let Some(check) = self
            .tool_inspection_manager
            .process_inspection_results_with_permission_inspector(&to_inspect, &inspection_results)
        else {
            return Ok(requests.iter().map(|_| ApprovalDecision::Allow).collect());
        };
        let denied: HashSet<&str> = check
            .denied
            .iter()
            .map(|request| request.id.as_str())
            .collect();
        let needs_approval: HashSet<&str> = check
            .needs_approval
            .iter()
            .map(|request| request.id.as_str())
            .collect();

        Ok(requests
            .iter()
            .map(|request| {
                if denied.contains(request.id.as_str()) {
                    return ApprovalDecision::Deny {
                        reason: DECLINED_RESPONSE.to_string(),
                    };
                }
                if !needs_approval.contains(request.id.as_str()) {
                    return ApprovalDecision::Allow;
                }
                let prompt = inspection_results
                    .iter()
                    .find(|result| result.tool_request_id == request.id)
                    .and_then(|result| match &result.action {
                        InspectionAction::RequireApproval(Some(message)) => Some(message.clone()),
                        _ => None,
                    });
                let mut tool_meta = serde_json::Map::new();
                if let Some(finding_id) =
                    get_security_finding_id_from_results(&request.id, &inspection_results)
                {
                    tracing::info!(
                        security.event_type = "approval_request",
                        security.finding_id = %finding_id,
                        tool.request_id = %request.id,
                        "security finding: approval requested"
                    );
                    tool_meta.insert(SECURITY_FINDING_ID_KEY.to_string(), finding_id.into());
                }
                ApprovalDecision::Ask { prompt, tool_meta }
            })
            .collect())
    }

    async fn on_confirmation(
        &self,
        _session: &Session,
        request: &ToolRequest,
        permission: &Permission,
    ) -> Result<()> {
        if let Some(finding_id) = request
            .tool_meta
            .as_ref()
            .and_then(|meta| meta.get(SECURITY_FINDING_ID_KEY))
            .and_then(serde_json::Value::as_str)
        {
            let action = match permission {
                Permission::AllowOnce | Permission::AlwaysAllow => "ALLOW",
                _ => "BLOCK",
            };
            tracing::info!(
                monotonic_counter.goose.prompt_injection_user_decisions = 1,
                security.event_type = "user_decision",
                security.action = action,
                security.finding_id = %finding_id,
                tool.request_id = %request.id,
                user.decision = ?permission,
                "security finding: user decision"
            );
        }
        let level = match permission {
            Permission::AlwaysAllow => PermissionLevel::AlwaysAllow,
            Permission::AlwaysDeny => PermissionLevel::NeverAllow,
            _ => return Ok(()),
        };
        let Ok(tool_call) = &request.tool_call else {
            return Ok(());
        };
        self.tool_inspection_manager
            .update_permission_manager(&tool_call.name, level)
            .await;
        Ok(())
    }
}
