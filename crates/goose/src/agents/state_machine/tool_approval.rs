use std::collections::HashSet;

use anyhow::Result;
use async_trait::async_trait;
use oi_kit_agent::tool_approval::{ApprovalDecision, ToolApproval};

use crate::agents::tool_execution::DECLINED_RESPONSE;
use crate::config::permission::PermissionLevel;
use crate::conversation::message::ToolRequest;
use crate::conversation::Conversation;
use crate::permission::Permission;
use crate::session::Session;
use crate::tool_inspection::{
    get_security_finding_id_from_results, InspectionAction, ToolInspectionManager,
};

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
        let to_inspect: Vec<ToolRequest> =
            requests.iter().map(|request| (*request).clone()).collect();
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
                if let Some(finding_id) =
                    get_security_finding_id_from_results(&request.id, &inspection_results)
                {
                    tracing::info!(
                        monotonic_counter.goose.prompt_injection_user_decisions = 1,
                        security.event_type = "approval_request",
                        security.finding_id = %finding_id,
                        tool.request_id = %request.id,
                        "security finding: approval requested"
                    );
                }
                ApprovalDecision::Ask { prompt }
            })
            .collect())
    }

    async fn on_confirmation(
        &self,
        session: &Session,
        request: &ToolRequest,
        permission: &Permission,
    ) -> Result<()> {
        // A standing allow granted for an MCP skill, or while its instructions
        // steer the model, would outlive the content that asked for it.
        let acting_on_mcp_skill = crate::skills::approval::requested_mcp_skill(request).is_some()
            || !crate::skills::mcp::active_origins(&session.id).is_empty();
        let level = match permission {
            Permission::AlwaysAllow if acting_on_mcp_skill => return Ok(()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::StateMachineServices;
    use rmcp::model::CallToolRequestParams;
    use rmcp::object;

    fn request(name: &str, arguments: rmcp::model::JsonObject) -> ToolRequest {
        ToolRequest {
            id: "request".to_string(),
            tool_call: Ok(CallToolRequestParams::new(name.to_string()).with_arguments(arguments)),
            metadata: None,
            tool_meta: None,
        }
    }

    #[tokio::test]
    async fn standing_allows_are_not_saved_while_acting_on_mcp_skills() {
        let data_dir = tempfile::tempdir().unwrap();
        let services = StateMachineServices::for_tool_tests(data_dir.path().to_path_buf());
        let approval = GooseToolApproval::new(&services.tool_inspection_manager);
        let session = Session {
            id: "approval-standing".to_string(),
            ..Session::default()
        };

        approval
            .on_confirmation(
                &session,
                &request(
                    oi_kit_agent::skills::LOAD_SKILL_TOOL_NAME,
                    object!({ "name": "server::billing" }),
                ),
                &Permission::AlwaysAllow,
            )
            .await
            .unwrap();
        assert!(services
            .permission_manager
            .get_user_permission(oi_kit_agent::skills::LOAD_SKILL_TOOL_NAME)
            .is_none());

        crate::skills::mcp::hold_skill(
            &session.id,
            crate::skills::mcp::SkillRecord {
                uri: "skill://billing/SKILL.md".to_string(),
                name: "billing".to_string(),
                description: "Billing".to_string(),
                frontmatter: serde_json::Map::new(),
                extension_name: "server".to_string(),
                resources: crate::skills::mcp::SkillResources::Dynamic,
            },
        );
        let shell = request("shell", object!({ "command": "ls" }));
        approval
            .on_confirmation(&session, &shell, &Permission::AlwaysAllow)
            .await
            .unwrap();
        assert!(services
            .permission_manager
            .get_user_permission("shell")
            .is_none());
        approval
            .on_confirmation(&session, &shell, &Permission::AlwaysDeny)
            .await
            .unwrap();
        assert_eq!(
            services.permission_manager.get_user_permission("shell"),
            Some(PermissionLevel::NeverAllow)
        );

        crate::skills::mcp::clear_held_skills(&session.id);
        approval
            .on_confirmation(&session, &shell, &Permission::AlwaysAllow)
            .await
            .unwrap();
        assert_eq!(
            services.permission_manager.get_user_permission("shell"),
            Some(PermissionLevel::AlwaysAllow)
        );
    }

    #[tokio::test]
    async fn a_resumed_confirmation_does_not_save_a_standing_allow_for_a_loaded_skill() {
        let data_dir = tempfile::tempdir().unwrap();
        let services = StateMachineServices::for_tool_tests(data_dir.path().to_path_buf());
        let approval = GooseToolApproval::new(&services.tool_inspection_manager);
        let session = Session {
            id: "approval-resumed".to_string(),
            ..Session::default()
        };
        let load = request(
            oi_kit_agent::skills::LOAD_SKILL_TOOL_NAME,
            object!({ "name": "server::billing" }),
        );
        let conversation = [
            crate::conversation::message::Message::assistant()
                .with_tool_request(&load.id, Ok(load.tool_call.clone().unwrap())),
            crate::conversation::message::Message::user().with_tool_response(
                &load.id,
                Ok(rmcp::model::CallToolResult::success(vec![
                    rmcp::model::ContentBlock::text(crate::skills::mcp::origin_tagged_context(
                        "server",
                        "skill://billing/SKILL.md",
                        "Do the work.",
                    )),
                ])),
            ),
        ];

        crate::skills::approval::restore_loaded_origins(&session.id, &conversation);
        approval
            .on_confirmation(
                &session,
                &request("shell", object!({ "command": "ls" })),
                &Permission::AlwaysAllow,
            )
            .await
            .unwrap();
        assert!(services
            .permission_manager
            .get_user_permission("shell")
            .is_none());
        crate::skills::mcp::clear_session(&session.id);
    }
}
