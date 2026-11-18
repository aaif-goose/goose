//! Approval for skills served over MCP. Every load is approved explicitly, and
//! while loaded MCP skill instructions remain in a session's context every tool
//! call is approved explicitly too: those instructions are server content, so
//! standing permissions granted for the user's own work do not cover them.

use anyhow::Result;
use async_trait::async_trait;
use oi_kit_agent::skills::LOAD_SKILL_TOOL_NAME;

use super::mcp;
use crate::config::GooseMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};

pub const INSPECTOR_NAME: &str = "mcp_skill_approval";

/// Names an MCP skill (`extension::skill`, `extension::URI`, or a supporting
/// file of either) rather than a local one.
pub(crate) fn requested_mcp_skill(request: &ToolRequest) -> Option<&str> {
    let tool_call = request.tool_call.as_ref().ok()?;
    if tool_call.name.as_ref() != LOAD_SKILL_TOOL_NAME {
        return None;
    }
    let name = tool_call.arguments.as_ref()?.get("name")?.as_str()?;
    name.split_once("::")
        .is_some_and(|(extension, skill)| !extension.is_empty() && !skill.is_empty())
        .then_some(name)
}

const LOAD_NOTICE: &str = "Skills are instructions written by the server. Approving this load does not grant tool, filesystem, or code-execution permissions, and while the skill is loaded every tool call asks for approval.";

pub struct McpSkillApprovalInspector;

impl McpSkillApprovalInspector {
    fn load_prompt(&self, name: &str) -> String {
        format!(
            "Load MCP skill `{}`?\n\n{LOAD_NOTICE}",
            mcp::context_label(name)
        )
    }
}

#[async_trait]
impl ToolInspector for McpSkillApprovalInspector {
    fn name(&self) -> &'static str {
        INSPECTOR_NAME
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn inspect(
        &self,
        session_id: &str,
        tool_requests: &[ToolRequest],
        _messages: &[Message],
        _goose_mode: GooseMode,
    ) -> Result<Vec<InspectionResult>> {
        let origins: Vec<String> = mcp::active_origins(session_id)
            .iter()
            .map(|origin| mcp::context_label(origin))
            .collect();
        Ok(tool_requests
            .iter()
            .filter_map(|request| {
                let (prompt, reason) = match requested_mcp_skill(request) {
                    Some(name) => (
                        self.load_prompt(name),
                        "MCP skills require explicit per-load approval",
                    ),
                    None if !origins.is_empty() => (
                        format!(
                            "This tool call was issued while untrusted MCP skill instructions from [{}] are active in model context. Approve this call explicitly; prior tool permissions do not authorize instructions from the remote skill.",
                            origins.join(", ")
                        ),
                        "tool call occurred while acting on an MCP skill",
                    ),
                    None => return None,
                };
                Some(InspectionResult {
                    tool_request_id: request.id.clone(),
                    action: InspectionAction::RequireApproval(Some(prompt)),
                    reason: reason.to_string(),
                    confidence: 1.0,
                    inspector_name: INSPECTOR_NAME.to_string(),
                    finding_id: None,
                })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::{model::CallToolRequestParams, object};

    fn load(name: &str) -> ToolRequest {
        ToolRequest {
            id: "request".to_string(),
            tool_call: Ok(CallToolRequestParams::new(LOAD_SKILL_TOOL_NAME)
                .with_arguments(object!({ "name": name }))),
            metadata: None,
            tool_meta: None,
        }
    }

    fn hold(session_id: &str) {
        mcp::hold_skill(
            session_id,
            mcp::SkillRecord {
                uri: "skill://billing/SKILL.md".to_string(),
                name: "billing".to_string(),
                description: "Billing".to_string(),
                frontmatter: serde_json::Map::new(),
                extension_name: "billing".to_string(),
                resources: mcp::SkillResources::Dynamic,
            },
        );
    }

    #[tokio::test]
    async fn mcp_skill_loads_require_approval_even_in_auto_mode() {
        for name in [
            "billing-server::refunds",
            "billing-server::skill://acme/refunds/SKILL.md",
            "billing-server::refunds/references/GUIDE.md",
        ] {
            let results = McpSkillApprovalInspector
                .inspect("approval-load", &[load(name)], &[], GooseMode::Auto)
                .await
                .unwrap();
            assert!(
                matches!(
                    results[0].action,
                    InspectionAction::RequireApproval(Some(_))
                ),
                "{name}"
            );
        }
    }

    #[tokio::test]
    async fn every_tool_call_requires_approval_while_an_mcp_skill_is_held() {
        let mut shell = load("refunds");
        shell.tool_call = Ok(CallToolRequestParams::new("shell")
            .with_arguments(object!({ "command": "echo unsafe" })));
        hold("approval-active");
        let results = McpSkillApprovalInspector
            .inspect("approval-active", &[shell], &[], GooseMode::Auto)
            .await
            .unwrap();
        mcp::clear_held_skills("approval-active");
        assert!(matches!(
            &results[0].action,
            InspectionAction::RequireApproval(Some(prompt)) if prompt.contains("[billing]")
        ));
    }

    #[tokio::test]
    async fn local_skills_and_tools_are_not_intercepted_without_a_held_skill() {
        let mut shell = load("refunds");
        shell.tool_call = Ok(CallToolRequestParams::new("shell"));
        let results = McpSkillApprovalInspector
            .inspect(
                "approval-local",
                &[load("refunds"), shell],
                &[],
                GooseMode::Auto,
            )
            .await
            .unwrap();
        assert!(results.is_empty());
    }
}
