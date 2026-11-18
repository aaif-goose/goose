//! Approval for skills served over MCP. Every load is approved explicitly, and
//! while loaded MCP skill instructions remain in a session's context every tool
//! call is approved explicitly too: those instructions are server content, so
//! standing permissions granted for the user's own work do not cover them.
//!
//! A load prompt says how the skill compares with what the user last approved
//! from that server (see `baseline`).

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use oi_kit_agent::skills::LOAD_SKILL_TOOL_NAME;

use super::baseline::{Grade, SkillBaselineStore};
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

pub struct McpSkillApprovalInspector {
    baselines: Arc<SkillBaselineStore>,
}

impl McpSkillApprovalInspector {
    pub fn new(baselines: Arc<SkillBaselineStore>) -> Self {
        Self { baselines }
    }

    fn load_prompt(&self, session_id: &str, name: &str) -> String {
        let name_label = mcp::context_label(name);
        let Some((skill, relative_path)) = mcp::listed_skill_for_request(session_id, name) else {
            if let Some((extension, uri)) = mcp::direct_skill_reference(name) {
                mcp::take_pinned(session_id, extension, uri);
            }
            return format!(
                "Load MCP skill `{name_label}`?\n\n{LOAD_NOTICE}\n\nThis skill is not in the server's current listing, so goose cannot compare it with what you approved before."
            );
        };
        let origin = mcp::context_label(&skill.extension_name);
        if let Some(relative_path) = relative_path {
            return format!(
                "Read `{}` from MCP skill `{}` served by `{origin}`?\n\n{LOAD_NOTICE}",
                mcp::context_label(&relative_path),
                mcp::context_label(&skill.name),
            );
        }

        let change = self.baselines.change(&skill);
        let approved_on = change
            .approved_at
            .map(|at| at.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        let mut prompt =
            format!("Load MCP skill `{name_label}` served by `{origin}`?\n\n{LOAD_NOTICE}\n\n");
        match change.grade {
            Grade::New => prompt.push_str("You have not approved this skill before."),
            Grade::Unchanged => prompt.push_str(&format!(
                "Unchanged since you approved it on {approved_on}."
            )),
            Grade::Review | Grade::Escalate => {
                let severity = if change.grade == Grade::Escalate {
                    "CHANGED, review carefully"
                } else {
                    "changed"
                };
                prompt.push_str(&format!(
                    "{severity} since you approved it on {approved_on}:"
                ));
                for finding in &change.findings {
                    prompt.push_str(&format!("\n- {}", mcp::context_label(&finding.to_string())));
                }
            }
        }
        let surface = self
            .baselines
            .surface_change(&skill.extension_name, &mcp::listed_skills(session_id));
        if !surface.is_empty() {
            prompt.push_str(&format!(
                "\n\nSince your last approval `{origin}` also {}.",
                [("added", &surface.added), ("removed", &surface.removed)]
                    .into_iter()
                    .filter(|(_, names)| !names.is_empty())
                    .map(|(verb, names)| format!(
                        "{verb} {}",
                        mcp::context_label(&names.join(", "))
                    ))
                    .collect::<Vec<_>>()
                    .join(" and ")
            ));
        }
        mcp::pin_for_approval(session_id, skill);
        prompt
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
                        self.load_prompt(session_id, name),
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

    fn inspector() -> McpSkillApprovalInspector {
        McpSkillApprovalInspector::new(Arc::new(SkillBaselineStore::new(
            tempfile::tempdir().unwrap().path(),
        )))
    }

    fn prompt_of(result: &InspectionResult) -> &str {
        match &result.action {
            InspectionAction::RequireApproval(Some(prompt)) => prompt,
            other => panic!("expected an approval prompt, got {other:?}"),
        }
    }

    fn listed(skill_md: &str) -> mcp::SkillRecord {
        let uri = "skill://acme/billing/SKILL.md";
        mcp::SkillRecord {
            uri: uri.to_string(),
            name: "billing".to_string(),
            description: "Billing".to_string(),
            frontmatter: serde_json::json!({ "name": "billing", "description": "Billing" })
                .as_object()
                .unwrap()
                .clone(),
            extension_name: "acme".to_string(),
            resources: mcp::SkillResources::Files(vec![rmcp::model::SkillResource::from_bytes(
                uri,
                skill_md.as_bytes(),
            )]),
        }
    }

    #[tokio::test]
    async fn load_prompt_grades_the_listing_against_the_last_approval() {
        let dir = tempfile::tempdir().unwrap();
        let baselines = Arc::new(SkillBaselineStore::new(dir.path()));
        let inspector = McpSkillApprovalInspector::new(Arc::clone(&baselines));
        let session = "approval-graded";
        let original = listed("v1");

        mcp::remember_listing(session, std::slice::from_ref(&original));
        let first = inspector
            .inspect(session, &[load("acme::billing")], &[], GooseMode::Auto)
            .await
            .unwrap();
        assert!(prompt_of(&first[0]).contains("You have not approved this skill before."));
        assert_eq!(
            mcp::take_pinned(session, "acme", &original.uri).as_ref(),
            Some(&original)
        );

        baselines.approve(&original, Some(std::slice::from_ref(&original)));
        let mut sleeper = listed("sleeper");
        sleeper.uri = "skill://acme/sleeper/SKILL.md".to_string();
        sleeper.name = "sleeper".to_string();
        mcp::remember_listing(session, &[listed("v2"), sleeper]);
        let changed = inspector
            .inspect(session, &[load("acme::billing")], &[], GooseMode::Auto)
            .await
            .unwrap();
        let prompt = prompt_of(&changed[0]);
        assert!(prompt.contains("CHANGED, review carefully"), "{prompt}");
        assert!(prompt.contains("- instructions rewritten"), "{prompt}");
        assert!(prompt.contains("also added sleeper"), "{prompt}");
        mcp::clear_session(session);
    }

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
            let results = inspector()
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
        let results = inspector()
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
        let results = inspector()
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
