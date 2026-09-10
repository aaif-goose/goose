use std::collections::HashSet;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ContentBlock, ErrorCode, ErrorData, Tool};
use serde_json::Value;

use crate::agents::state_machine::ops_toolcalling::{pending_tool_requests, ToolDisposition};
use crate::agents::state_machine::{
    applied, ends_turn, messages_since_kickoff, not_applicable, yielded, Emitter, GooseEffect,
    Operation, OperationResult,
};
use crate::conversation::message::{Message, MessageContent};
use crate::conversation::Conversation;
use crate::session::Session;

pub const SUBMIT_PLAN_TOOL_NAME: &str = "submit_plan";
pub const SUBMIT_FEEDBACK_TOOL_NAME: &str = "submit_feedback";
pub const SUBMIT_IMPLEMENTATION_TOOL_NAME: &str = "submit_implementation";
const PLAN_CONTINUATION: &str = "You MUST call the `submit_plan` tool NOW with your findings and complete implementation plan. Do not provide the report directly in your response.";
const FEEDBACK_CONTINUATION: &str = "You MUST call the `submit_feedback` tool NOW with your assessment. Do not provide the assessment directly in your response.";
const IMPLEMENTATION_CONTINUATION: &str = "The task is not complete until you call `submit_implementation` with a summary of the completed work and the verification you ran. Continue implementing or call the tool now if the work is complete.";

pub struct PlanOperation;

pub struct SupervisorOperation;

pub struct ImplementPlanOperation {
    findings: String,
    plan: String,
}

impl ImplementPlanOperation {
    pub fn new(findings: String, plan: String) -> Self {
        Self { findings, plan }
    }
}

fn successful_report(messages: &[Message], tool_name: &str) -> Option<Value> {
    let successful_responses: HashSet<&str> = messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|content| match content {
            MessageContent::ToolResponse(response)
                if response
                    .tool_result
                    .as_ref()
                    .is_ok_and(|result| result.is_error != Some(true)) =>
            {
                Some(response.id.as_str())
            }
            _ => None,
        })
        .collect();

    messages
        .iter()
        .rev()
        .flat_map(|message| message.content.iter().rev())
        .find_map(|content| match content {
            MessageContent::ToolRequest(request)
                if successful_responses.contains(request.id.as_str()) =>
            {
                request.tool_call.as_ref().ok().and_then(|tool_call| {
                    (tool_call.name == tool_name)
                        .then(|| Value::Object(tool_call.arguments.clone().unwrap_or_default()))
                })
            }
            _ => None,
        })
}

pub fn submitted_report(conversation: &Conversation, tool_name: &str) -> Result<Option<Value>> {
    Ok(successful_report(
        messages_since_kickoff(conversation)?,
        tool_name,
    ))
}

fn report_tool(name: &str, description: &str, schema: Value) -> Tool {
    Tool::new(
        name.to_string(),
        description.to_string(),
        schema
            .as_object()
            .expect("report tool schema is an object")
            .clone(),
    )
}

async fn handle_report(
    conversation: &Conversation,
    emit: &Emitter,
    tool_name: &str,
    required_fields: &[&str],
    continuation: &str,
) -> Result<OperationResult<GooseEffect>> {
    let messages = messages_since_kickoff(conversation)?;
    let pending = pending_tool_requests(messages)
        .into_iter()
        .find(|(request, disposition)| {
            *disposition == ToolDisposition::Execute
                && request
                    .tool_call
                    .as_ref()
                    .is_ok_and(|tool_call| tool_call.name == tool_name)
        });

    if let Some((request, _)) = pending {
        let tool_call = request
            .tool_call
            .map_err(|error| anyhow!("report tool call could not be parsed: {error}"))?;
        let arguments = tool_call.arguments.unwrap_or_default();
        let missing = required_fields
            .iter()
            .find(|field| !arguments.contains_key(**field));
        let result = match missing {
            Some(field) => Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!("Missing required field: {field}"),
                None,
            )),
            None => Ok(CallToolResult::success(vec![ContentBlock::text(
                "Report submitted.",
            )])),
        };
        let mut response = Message::user();
        response.add_tool_response_with_metadata(request.id, result, request.metadata.as_ref());
        let response = emit.message(response).await;
        return applied([response.into()]);
    }

    if successful_report(messages, tool_name).is_some() {
        return yielded();
    }

    if ends_turn(messages) {
        let message = emit.message(Message::user().with_text(continuation)).await;
        return applied([message.into()]);
    }

    not_applicable()
}

#[async_trait]
impl Operation<Session, GooseEffect> for PlanOperation {
    fn name(&self) -> &'static str {
        "plan"
    }

    async fn inference_tools(&self, _session: &Session) -> Result<Vec<Tool>> {
        Ok(vec![report_tool(
            SUBMIT_PLAN_TOOL_NAME,
            "This tool MUST be called to submit the complete implementation plan after investigating the task.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "findings": { "type": "string" },
                    "plan": { "type": "string" }
                },
                "required": ["findings", "plan"]
            }),
        )])
    }

    async fn prompt_parts(
        &self,
        _session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        Ok(vec![(
            "planner".to_string(),
            "# Plan Submission Instructions\n\nYou are producing an implementation-ready report for a separate implementer. Inspect the repository without editing it. Your findings must identify the relevant files and behavior, tests and acceptance criteria, environment constraints, and evidence for important claims. Your plan must name exact files, commands, and behavior; give complete ordered steps; resolve implementation choices; and state exact verification with expected results. Do not defer core investigation, propose trying several approaches, or ask the user to decide. You MUST use `submit_plan` with both `findings` and `plan` rather than returning the report as prose. On a later turn, submit a complete replacement report with the same tool."
                .to_string(),
        )])
    }

    async fn run(
        &self,
        _session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        handle_report(
            conversation,
            emit,
            SUBMIT_PLAN_TOOL_NAME,
            &["findings", "plan"],
            PLAN_CONTINUATION,
        )
        .await
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for SupervisorOperation {
    fn name(&self) -> &'static str {
        "supervisor"
    }

    async fn inference_tools(&self, _session: &Session) -> Result<Vec<Tool>> {
        Ok(vec![report_tool(
            SUBMIT_FEEDBACK_TOOL_NAME,
            "This tool MUST be called to submit criticism or implementation steering.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "requires_action": { "type": "boolean" },
                    "feedback": { "type": "string" }
                },
                "required": ["requires_action", "feedback"]
            }),
        )])
    }

    async fn prompt_parts(
        &self,
        _session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        Ok(vec![(
            "supervisor".to_string(),
            "# Feedback Submission Instructions\n\nReview only the task, planner report, and implementation progress supplied in the prompt. Do not inspect or modify the repository. Reject a plan when a requirement is missing, a claim lacks supporting findings, choices or investigation are deferred, the core solution is left to the implementer, verification is vague, or the report asks the user to decide. Be concise and give concrete corrections. You MUST use `submit_feedback` rather than returning the assessment as prose. Set `requires_action` when the planner or implementer must respond."
                .to_string(),
        )])
    }

    async fn run(
        &self,
        _session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        handle_report(
            conversation,
            emit,
            SUBMIT_FEEDBACK_TOOL_NAME,
            &["requires_action", "feedback"],
            FEEDBACK_CONTINUATION,
        )
        .await
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for ImplementPlanOperation {
    fn name(&self) -> &'static str {
        "implement_plan"
    }

    async fn inference_tools(&self, _session: &Session) -> Result<Vec<Tool>> {
        Ok(vec![report_tool(
            SUBMIT_IMPLEMENTATION_TOOL_NAME,
            "This tool MUST be called after the implementation and verification are complete.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "summary": { "type": "string" },
                    "verification": { "type": "string" }
                },
                "required": ["summary", "verification"]
            }),
        )])
    }

    async fn prompt_parts(
        &self,
        _session: &Session,
        _conversation: &Conversation,
    ) -> Result<Vec<(String, String)>> {
        Ok(vec![(
            "implementation_plan".to_string(),
            format!(
                "# Implementation Contract\n\nExecute the supplied plan now. The repository and tests are authoritative; adapt if they contradict the report. Make all required code and environment changes, run the verification, and fix failures. Do not stop after writing a script that still needs to be run, after merely issuing commands, to ask the user a question, or without verification. The task is complete only after you call `submit_implementation` with the completed work and verification evidence.\n\n## Planner findings\n\n{}\n\n## Selected plan\n\n{}",
                self.findings, self.plan
            ),
        )])
    }

    async fn run(
        &self,
        _session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        handle_report(
            conversation,
            emit,
            SUBMIT_IMPLEMENTATION_TOOL_NAME,
            &["summary", "verification"],
            IMPLEMENTATION_CONTINUATION,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use rmcp::model::CallToolRequestParams;

    use super::*;

    fn conversation_with_report(tool_name: &str, arguments: Value) -> Conversation {
        let request_id = "report_1";
        let request = Message::assistant().with_tool_request(
            request_id,
            Ok(
                CallToolRequestParams::new(tool_name.to_string()).with_arguments(
                    arguments
                        .as_object()
                        .expect("test arguments are an object")
                        .clone(),
                ),
            ),
        );
        let mut response = Message::user();
        response.add_tool_response_with_metadata(
            request_id,
            Ok(CallToolResult::success(vec![ContentBlock::text(
                "Report submitted.",
            )])),
            None,
        );
        Conversation::new_unvalidated(vec![Message::user().with_text("start"), request, response])
    }

    #[test]
    fn extracts_successful_report_from_current_turn() {
        let conversation = conversation_with_report(
            SUBMIT_PLAN_TOOL_NAME,
            serde_json::json!({
                "findings": "the parser is in parser.rs",
                "plan": "change the parser"
            }),
        );

        assert_eq!(
            submitted_report(&conversation, SUBMIT_PLAN_TOOL_NAME).unwrap(),
            Some(serde_json::json!({
                "findings": "the parser is in parser.rs",
                "plan": "change the parser"
            }))
        );
    }

    #[test]
    fn does_not_reuse_report_from_previous_turn() {
        let mut conversation = conversation_with_report(
            SUBMIT_PLAN_TOOL_NAME,
            serde_json::json!({ "findings": "old findings", "plan": "old plan" }),
        );
        conversation.push(Message::user().with_text("revise the plan"));

        assert_eq!(
            submitted_report(&conversation, SUBMIT_PLAN_TOOL_NAME).unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn planner_requires_findings_and_plan() {
        let tools = PlanOperation
            .inference_tools(&Session::default())
            .await
            .expect("planner tools");

        assert_eq!(
            tools[0].input_schema["required"],
            serde_json::json!(["findings", "plan"])
        );
    }

    #[tokio::test]
    async fn implementation_contract_contains_plan_and_requires_evidence() {
        let operation = ImplementPlanOperation::new(
            "parser lives in parser.rs".to_string(),
            "edit parser.rs and run parser tests".to_string(),
        );
        let tools = operation
            .inference_tools(&Session::default())
            .await
            .expect("implementation tools");
        let prompts = operation
            .prompt_parts(
                &Session::default(),
                &Conversation::new_unvalidated(Vec::new()),
            )
            .await
            .expect("implementation prompt");

        assert_eq!(
            tools[0].input_schema["required"],
            serde_json::json!(["summary", "verification"])
        );
        assert!(prompts[0].1.contains("parser lives in parser.rs"));
        assert!(prompts[0].1.contains("edit parser.rs and run parser tests"));
    }
}
