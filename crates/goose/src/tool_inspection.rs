use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;

use crate::config::GooseMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::permission::permission_inspector::PermissionInspector;
use crate::permission::permission_judge::PermissionCheckResult;
use crate::tool_monitor::RepetitionInspector;

/// Result of inspecting a tool call
#[derive(Debug, Clone)]
pub struct InspectionResult {
    pub tool_request_id: String,
    pub action: InspectionAction,
    pub reason: String,
    pub confidence: f32,
    pub inspector_name: String,
    pub finding_id: Option<String>,
}

/// Action to take based on inspection result
#[derive(Debug, Clone, PartialEq)]
pub enum InspectionAction {
    /// Allow the tool to execute without user intervention
    Allow,
    /// Deny the tool execution completely
    Deny,
    /// Require user approval before execution (with optional warning message)
    RequireApproval(Option<String>),
}

/// Trait for all tool inspectors
#[async_trait]
pub trait ToolInspector: Send + Sync {
    /// Name of this inspector (for logging/debugging)
    fn name(&self) -> &'static str;

    /// Inspect tool requests and return results
    async fn inspect(
        &self,
        session_id: &str,
        tool_requests: &[ToolRequest],
        messages: &[Message],
        goose_mode: GooseMode,
    ) -> Result<Vec<InspectionResult>>;

    /// Whether this inspector is enabled
    fn is_enabled(&self) -> bool {
        true
    }

    /// Allow downcasting to concrete types
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Manages all tool inspectors and coordinates their results
pub struct ToolInspectionManager {
    inspectors: Vec<Box<dyn ToolInspector>>,
}

impl ToolInspectionManager {
    pub fn new() -> Self {
        Self {
            inspectors: Vec::new(),
        }
    }

    /// Add an inspector to the manager
    /// Inspectors run in the order they are added
    pub fn add_inspector(&mut self, inspector: Box<dyn ToolInspector>) {
        self.inspectors.push(inspector);
    }

    /// Run all inspectors on the tool requests
    pub async fn inspect_tools(
        &self,
        session_id: &str,
        tool_requests: &[ToolRequest],
        messages: &[Message],
        goose_mode: GooseMode,
    ) -> Result<Vec<InspectionResult>> {
        let mut all_results = Vec::new();

        for inspector in &self.inspectors {
            if !inspector.is_enabled() {
                continue;
            }

            tracing::debug!(
                inspector_name = inspector.name(),
                tool_count = tool_requests.len(),
                "Running tool inspector"
            );

            match inspector
                .inspect(session_id, tool_requests, messages, goose_mode)
                .await
            {
                Ok(results) => {
                    tracing::debug!(
                        inspector_name = inspector.name(),
                        result_count = results.len(),
                        "Tool inspector completed"
                    );
                    all_results.extend(results);
                }
                Err(e) => {
                    tracing::error!(
                        inspector_name = inspector.name(),
                        error = %e,
                        "Tool inspector failed"
                    );
                    // Continue with other inspectors even if one fails
                }
            }
        }

        Ok(all_results)
    }

    /// Get list of registered inspector names
    pub fn inspector_names(&self) -> Vec<&'static str> {
        self.inspectors.iter().map(|i| i.name()).collect()
    }

    fn get_permission_inspector(&self) -> Option<&PermissionInspector> {
        self.inspectors
            .iter()
            .find(|i| i.name() == "permission")
            .and_then(|i| i.as_any().downcast_ref::<PermissionInspector>())
    }

    fn get_repetition_inspector(&self) -> Option<&RepetitionInspector> {
        self.inspectors
            .iter()
            .find(|i| i.name() == "repetition")
            .and_then(|i| i.as_any().downcast_ref::<RepetitionInspector>())
    }

    pub fn record_tool_error(&self, tool_name: &str, error_text: &str) {
        if let Some(inspector) = self.get_repetition_inspector() {
            inspector.record_error(tool_name, error_text);
        }
    }

    pub fn record_tool_success(&self) {
        if let Some(inspector) = self.get_repetition_inspector() {
            inspector.record_success();
        }
    }

    pub fn apply_tool_annotations(&self, tools: &[rmcp::model::Tool]) {
        if let Some(inspector) = self.get_permission_inspector() {
            inspector.apply_tool_annotations(tools);
        }
    }

    pub async fn update_permission_manager(
        &self,
        tool_name: &str,
        permission_level: crate::config::permission::PermissionLevel,
    ) {
        if let Some(inspector) = self.get_permission_inspector() {
            inspector
                .permission_manager
                .update_user_permission(tool_name, permission_level);
        }
    }

    pub fn process_inspection_results_with_permission_inspector(
        &self,
        remaining_requests: &[ToolRequest],
        inspection_results: &[InspectionResult],
    ) -> Option<PermissionCheckResult> {
        self.get_permission_inspector().map(|inspector| {
            inspector.process_inspection_results(remaining_requests, inspection_results)
        })
    }
}

impl Default for ToolInspectionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Apply inspection results to permission check results
/// This is the generic permission-mixing logic that works for all inspector types
pub fn apply_inspection_results_to_permissions(
    mut permission_result: PermissionCheckResult,
    inspection_results: &[InspectionResult],
) -> PermissionCheckResult {
    if inspection_results.is_empty() {
        return permission_result;
    }

    // Create a map of tool requests by ID for easy lookup
    let mut all_requests: HashMap<String, ToolRequest> = HashMap::new();

    // Collect all tool requests
    for req in &permission_result.approved {
        all_requests.insert(req.id.clone(), req.clone());
    }
    for req in &permission_result.needs_approval {
        all_requests.insert(req.id.clone(), req.clone());
    }
    for req in &permission_result.denied {
        all_requests.insert(req.id.clone(), req.clone());
    }

    // Process inspection results
    for result in inspection_results {
        let request_id = &result.tool_request_id;

        tracing::info!(
            inspector_name = result.inspector_name,
            tool_request_id = %request_id,
            action = ?result.action,
            confidence = result.confidence,
            reason = %result.reason,
            finding_id = ?result.finding_id,
            "Applying inspection result"
        );

        match result.action {
            InspectionAction::Deny => {
                // Remove from approved and needs_approval, add to denied
                permission_result
                    .approved
                    .retain(|req| req.id != *request_id);
                permission_result
                    .needs_approval
                    .retain(|req| req.id != *request_id);

                if let Some(request) = all_requests.get(request_id) {
                    if !permission_result
                        .denied
                        .iter()
                        .any(|req| req.id == *request_id)
                    {
                        permission_result.denied.push(request.clone());
                    }
                }
            }
            InspectionAction::RequireApproval(_) => {
                // Remove from approved, add to needs_approval if not already there
                permission_result
                    .approved
                    .retain(|req| req.id != *request_id);

                if let Some(request) = all_requests.get(request_id) {
                    if !permission_result
                        .needs_approval
                        .iter()
                        .any(|req| req.id == *request_id)
                    {
                        permission_result.needs_approval.push(request.clone());
                    }
                }
            }
            InspectionAction::Allow => {
                // This inspector allows it, but don't override other inspectors' decisions
                // If it's already denied or needs approval, leave it that way
            }
        }
    }

    permission_result
}

pub fn get_security_finding_id_from_results(
    tool_request_id: &str,
    inspection_results: &[InspectionResult],
) -> Option<String> {
    inspection_results
        .iter()
        .find(|result| {
            result.tool_request_id == tool_request_id && result.inspector_name == "security"
        })
        .and_then(|result| result.finding_id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::message::ToolRequest;
    use rmcp::model::CallToolRequestParams;
    use rmcp::object;

    #[test]
    fn test_apply_inspection_results() {
        let tool_request = ToolRequest {
            id: "req_1".to_string(),
            tool_call: Ok(CallToolRequestParams::new("test_tool").with_arguments(object!({}))),
            metadata: None,
            tool_meta: None,
        };

        let permission_result = PermissionCheckResult {
            approved: vec![tool_request.clone()],
            needs_approval: vec![],
            denied: vec![],
        };

        let inspection_results = vec![InspectionResult {
            tool_request_id: "req_1".to_string(),
            action: InspectionAction::Deny,
            reason: "Test denial".to_string(),
            confidence: 0.9,
            inspector_name: "test_inspector".to_string(),
            finding_id: Some("TEST-001".to_string()),
        }];

        let updated_result =
            apply_inspection_results_to_permissions(permission_result, &inspection_results);

        assert_eq!(updated_result.approved.len(), 0);
        assert_eq!(updated_result.denied.len(), 1);
        assert_eq!(updated_result.denied[0].id, "req_1");
    }
}
