use crate::config::GooseMode;
use crate::conversation::message::{Message, ToolRequest};
use crate::tool_inspection::{InspectionAction, InspectionResult, ToolInspector};
use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::CallToolRequestParams;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

pub const FINDING_ID_REPEATED_CALLS: &str = "REP-001";
pub const FINDING_ID_REPEATED_ERROR: &str = "REP-002";
const MAX_CONSECUTIVE_ERROR_FINGERPRINTS: u32 = 3;

// Helper struct for internal tracking
#[derive(Debug, Clone)]
struct InternalToolCall {
    name: String,
    parameters: Value,
}

impl InternalToolCall {
    fn matches(&self, other: &InternalToolCall) -> bool {
        self.name == other.name && self.parameters == other.parameters
    }

    fn from_tool_call(tool_call: &CallToolRequestParams) -> Self {
        let name = tool_call.name.to_string();
        let parameters = tool_call
            .arguments
            .as_ref()
            .map(|obj| Value::Object(obj.clone()))
            .unwrap_or(Value::Null);
        Self { name, parameters }
    }
}

#[derive(Debug)]
struct ErrorState {
    last_tool_name: Option<String>,
    last_error_text: Option<String>,
    consecutive_count: u32,
}

#[derive(Debug)]
pub struct RepetitionInspector {
    max_repetitions: Option<u32>,
    last_call: Option<InternalToolCall>,
    repeat_count: u32,
    call_counts: HashMap<String, u32>,
    error_state: Mutex<ErrorState>,
}

impl RepetitionInspector {
    pub fn new(max_repetitions: Option<u32>) -> Self {
        Self {
            max_repetitions,
            last_call: None,
            repeat_count: 0,
            call_counts: HashMap::new(),
            error_state: Mutex::new(ErrorState {
                last_tool_name: None,
                last_error_text: None,
                consecutive_count: 0,
            }),
        }
    }

    pub fn record_error(&self, tool_name: &str, error_text: &str) {
        let truncated: String = error_text.chars().take(100).collect();
        let mut state = self.error_state.lock().unwrap();
        if state.last_tool_name.as_deref() == Some(tool_name)
            && state.last_error_text.as_deref() == Some(truncated.as_str())
        {
            state.consecutive_count += 1;
        } else {
            state.last_tool_name = Some(tool_name.to_string());
            state.last_error_text = Some(truncated);
            state.consecutive_count = 1;
        }
    }

    pub fn record_success(&self) {
        let mut state = self.error_state.lock().unwrap();
        state.last_tool_name = None;
        state.last_error_text = None;
        state.consecutive_count = 0;
    }

    pub fn check_tool_call(&mut self, tool_call: CallToolRequestParams) -> bool {
        let internal_call = InternalToolCall::from_tool_call(&tool_call);
        let total_calls = self
            .call_counts
            .entry(internal_call.name.clone())
            .or_insert(0);
        *total_calls += 1;

        if self.max_repetitions.is_none() {
            self.last_call = Some(internal_call);
            self.repeat_count = 1;
            return true;
        }

        if let Some(last) = &self.last_call {
            if last.matches(&internal_call) {
                self.repeat_count += 1;
                if self.repeat_count > self.max_repetitions.unwrap() {
                    return false;
                }
            } else {
                self.repeat_count = 1;
            }
        } else {
            self.repeat_count = 1;
        }

        self.last_call = Some(internal_call);
        true
    }

    pub fn reset(&mut self) {
        self.last_call = None;
        self.repeat_count = 0;
        self.call_counts.clear();
        let mut state = self.error_state.lock().unwrap();
        state.last_tool_name = None;
        state.last_error_text = None;
        state.consecutive_count = 0;
    }
}

#[async_trait]
impl ToolInspector for RepetitionInspector {
    fn name(&self) -> &'static str {
        "repetition"
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
        let mut results = Vec::new();

        // Check call-repetition limits for each tool request
        for tool_request in tool_requests {
            if let Ok(tool_call) = &tool_request.tool_call {
                // Create a temporary clone to check without modifying state
                let mut temp_inspector = RepetitionInspector::new(self.max_repetitions);
                temp_inspector.last_call = self.last_call.clone();
                temp_inspector.repeat_count = self.repeat_count;
                temp_inspector.call_counts = self.call_counts.clone();

                if !temp_inspector.check_tool_call(tool_call.clone()) {
                    results.push(InspectionResult {
                        tool_request_id: tool_request.id.clone(),
                        action: InspectionAction::Deny,
                        reason: format!(
                            "Tool '{}' has exceeded maximum repetitions",
                            tool_call.name
                        ),
                        confidence: 1.0,
                        inspector_name: "repetition".to_string(),
                        finding_id: Some(FINDING_ID_REPEATED_CALLS.to_string()),
                    });
                }
            }
        }

        // Deny a tool that has returned the same error N consecutive times, regardless
        // of whether call parameters changed — catches retry loops with varying inputs.
        {
            let mut state = self.error_state.lock().unwrap();
            if state.consecutive_count >= MAX_CONSECUTIVE_ERROR_FINGERPRINTS {
                if let Some(ref last_tool) = state.last_tool_name {
                    let error_text = state.last_error_text.as_deref().unwrap_or("");
                    for tool_request in tool_requests {
                        if let Ok(tool_call) = &tool_request.tool_call {
                            if tool_call.name.as_ref() == last_tool.as_str() {
                                results.push(InspectionResult {
                                    tool_request_id: tool_request.id.clone(),
                                    action: InspectionAction::Deny,
                                    reason: format!(
                                        "Tool '{}' has returned the same error {} consecutive times: '{}'",
                                        tool_call.name, state.consecutive_count, error_text
                                    ),
                                    confidence: 1.0,
                                    inspector_name: "repetition".to_string(),
                                    finding_id: Some(FINDING_ID_REPEATED_ERROR.to_string()),
                                });
                            }
                        }
                    }
                }
                // Reset so the model can retry after adapting strategy
                state.consecutive_count = 0;
            }
        }

        Ok(results)
    }
}
