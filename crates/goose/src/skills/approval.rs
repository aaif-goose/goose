use anyhow::Result;
use async_trait::async_trait;

use crate::config::GooseMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};

pub const LOAD_SKILL_TOOL_NAME: &str = "load_skill";

fn requests_mcp_skill(request: &ToolRequest) -> bool {
    let Ok(tool_call) = &request.tool_call else {
        return false;
    };
    if tool_call.name.as_ref() != LOAD_SKILL_TOOL_NAME {
        return false;
    }
    tool_call
        .arguments
        .as_ref()
        .and_then(|arguments| arguments.get("name"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|name| {
            super::mcp::direct_skill_reference(name).is_some()
                || name
                    .split_once("::")
                    .is_some_and(|(extension, skill)| !extension.is_empty() && !skill.is_empty())
        })
}

pub struct McpSkillApprovalInspector;

#[async_trait]
impl ToolInspector for McpSkillApprovalInspector {
    fn name(&self) -> &'static str {
        "mcp_skill_approval"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn inspect(
        &self,
        _session_id: &str,
        tool_requests: &[ToolRequest],
        _messages: &[Message],
        _goose_mode: GooseMode,
    ) -> Result<Vec<InspectionResult>> {
        let origins: Vec<String> = super::mcp::active_origins(_session_id)
            .into_iter()
            .map(|origin| super::mcp::context_label(&origin))
            .collect();
        let acting_on_mcp_skill = !origins.is_empty();
        Ok(tool_requests
            .iter()
            .filter_map(|request| {
                let loading = requests_mcp_skill(request);
                if !loading && !acting_on_mcp_skill {
                    return None;
                }
                let prompt = if loading {
                    "This skill is untrusted instructional content from an MCP server. Approve loading it into the model context. This approval does not grant tool, filesystem, or code-execution permissions."
                        .to_string()
                } else {
                    format!(
                        "This tool call was issued while untrusted MCP skill instructions from [{}] are active in model context. Approve this call explicitly; prior tool permissions do not authorize instructions from the remote skill.",
                        origins.join(", ")
                    )
                };
                Some(InspectionResult {
                    tool_request_id: request.id.clone(),
                    action: InspectionAction::RequireApproval(Some(prompt)),
                    reason: if loading {
                        "MCP skills require explicit per-load approval".to_string()
                    } else {
                        "tool call occurred while acting on an MCP skill".to_string()
                    },
                    confidence: 1.0,
                    inspector_name: self.name().to_string(),
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

    fn request(name: &str) -> ToolRequest {
        ToolRequest {
            id: "request".to_string(),
            tool_call: Ok(CallToolRequestParams::new(LOAD_SKILL_TOOL_NAME)
                .with_arguments(object!({ "name": name }))),
            metadata: None,
            tool_meta: None,
        }
    }

    #[tokio::test]
    async fn qualified_mcp_skill_always_requires_approval() {
        let results = McpSkillApprovalInspector
            .inspect(
                "session",
                &[request("billing-server::refunds")],
                &[],
                GooseMode::Auto,
            )
            .await
            .unwrap();
        assert!(matches!(
            results[0].action,
            InspectionAction::RequireApproval(Some(_))
        ));
    }

    #[tokio::test]
    async fn tool_call_while_remote_skill_is_active_requires_approval() {
        let mut tool = request("refunds");
        tool.tool_call = Ok(CallToolRequestParams::new("shell").with_arguments(object!({
            "command": "echo unsafe"
        })));
        crate::skills::mcp::hold_skill(
            "session",
            crate::skills::mcp::SkillRecord {
                uri: "skill://billing/SKILL.md".to_string(),
                name: "billing".to_string(),
                description: "Billing".to_string(),
                frontmatter: serde_json::Map::new(),
                extension_name: "billing".to_string(),
                resources: crate::skills::mcp::SkillResources::Dynamic,
            },
        );
        let results = McpSkillApprovalInspector
            .inspect("session", &[tool], &[], GooseMode::Auto)
            .await
            .unwrap();
        assert!(matches!(
            results[0].action,
            InspectionAction::RequireApproval(Some(_))
        ));
        assert!(matches!(
            &results[0].action,
            InspectionAction::RequireApproval(Some(prompt)) if prompt.contains("billing")
        ));
        crate::skills::mcp::clear_held_skills("session");
    }

    #[tokio::test]
    async fn local_skill_is_not_intercepted() {
        let results = McpSkillApprovalInspector
            .inspect("session", &[request("refunds")], &[], GooseMode::Auto)
            .await
            .unwrap();
        assert!(results.is_empty());
    }
}
