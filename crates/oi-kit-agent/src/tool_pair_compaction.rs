//! Replaces old tool results with short summaries in the model's view.
//!
//! The summary is stored as a note on the tool response message and shown to
//! the model where the result was. Nothing is appended or reordered, and the
//! user keeps seeing the original result.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use indoc::indoc;
use oi_kit_context_management::{format_message_for_compacting, CompactionModel};
use oi_kit_provider_types::conversation::message::{
    summarized_tool_result_text, Message, MessageContent, ToolRequest, ToolResponse,
    TOOL_RESULT_SUMMARY_OPERATION,
};
use oi_kit_provider_types::conversation::token_usage::ProviderUsage;
use oi_kit_provider_types::conversation::Conversation;
use rmcp::model::Role;

use crate::inference::InferenceEffect;
use crate::machine::MachineSession;
use crate::operation::{
    applied, messages_since_kickoff, not_applicable, ConversationEffect, Emitter, Operation,
    OperationResult,
};

pub const BATCH_SIZE: usize = 10;

const SYSTEM_PROMPT: &str = indoc! {r#"
    You shorten one tool call and its result from a coding agent's history to save context.
    The agent will see your text in place of the result and can no longer read the original.

    Write what the call did and what the agent learned from it, in under 150 words.
    Keep the facts later work depends on: paths, identifiers, values, counts, errors, and conclusions.
    Do not reproduce the output and do not address the agent.
"#};

pub struct ToolPairCompactionOperation {
    model: Arc<dyn CompactionModel>,
    cutoff: usize,
    failed: Mutex<HashSet<String>>,
}

struct ToolResult<'a> {
    message_id: &'a str,
    response: &'a ToolResponse,
}

enum Outcome {
    Summarized(String, ProviderUsage),
    Kept(Option<ProviderUsage>),
    Failed,
}

impl ToolPairCompactionOperation {
    /// Once more than `cutoff + BATCH_SIZE` results from earlier turns are
    /// still in full, the oldest `BATCH_SIZE` are summarized.
    pub fn new(model: Arc<dyn CompactionModel>, cutoff: usize) -> Self {
        Self {
            model,
            cutoff,
            failed: Mutex::default(),
        }
    }

    fn full_results<'a>(&self, messages: &'a [Message]) -> Vec<ToolResult<'a>> {
        let failed = self.failed.lock().unwrap();
        let mut results = Vec::new();
        for message in messages.iter().filter(|message| message.is_agent_visible()) {
            let Some(message_id) = message.id.as_deref() else {
                continue;
            };
            for content in &message.content {
                let MessageContent::ToolResponse(response) = content else {
                    continue;
                };
                if is_text_result(response)
                    && !failed.contains(&response.id)
                    && message
                        .metadata
                        .operation_note(TOOL_RESULT_SUMMARY_OPERATION, &response.id)
                        .is_none()
                {
                    results.push(ToolResult {
                        message_id,
                        response,
                    });
                }
            }
        }
        results
    }

    async fn summarize(&self, messages: &[Message], result: &ToolResult<'_>) -> Outcome {
        let Some(request) = tool_request(messages, &result.response.id) else {
            return Outcome::Failed;
        };
        let response = format_response(result.response);
        if response.len() <= summarized_tool_result_text("").len() {
            return Outcome::Kept(None);
        }
        let pair = format!(
            "{}\n{response}",
            format_message_for_compacting(&Message::new(
                Role::Assistant,
                0,
                vec![MessageContent::ToolRequest(request.clone())]
            )),
        );
        let reply = self
            .model
            .complete(SYSTEM_PROMPT, &[Message::user().with_text(pair)])
            .await;
        let (message, usage) = match reply {
            Ok(reply) => reply,
            Err(error) => {
                tracing::warn!("Failed to summarize tool result: {error}");
                return Outcome::Failed;
            }
        };
        let summary = message.as_concat_text().trim().to_string();
        if summary.is_empty() {
            return Outcome::Failed;
        }
        if summarized_tool_result_text(&summary).len() >= response.len() {
            return Outcome::Kept(Some(usage));
        }
        Outcome::Summarized(summary, usage)
    }
}

fn is_text_result(response: &ToolResponse) -> bool {
    response
        .tool_result
        .as_ref()
        .is_ok_and(|result| result.content.iter().all(|item| item.as_text().is_some()))
}

fn format_response(response: &ToolResponse) -> String {
    let content = MessageContent::ToolResponse(response.clone())
        .filter_for_audience(Role::Assistant)
        .into_iter()
        .collect();
    format_message_for_compacting(&Message::new(Role::User, 0, content))
}

fn tool_request<'a>(messages: &'a [Message], tool_call_id: &str) -> Option<&'a ToolRequest> {
    messages
        .iter()
        .flat_map(|message| &message.content)
        .find_map(|content| match content {
            MessageContent::ToolRequest(request) if request.id == tool_call_id => Some(request),
            _ => None,
        })
}

#[async_trait]
impl<S, E> Operation<S, E> for ToolPairCompactionOperation
where
    S: MachineSession,
    E: InferenceEffect + From<ConversationEffect>,
{
    fn name(&self) -> &'static str {
        TOOL_RESULT_SUMMARY_OPERATION
    }

    async fn run(
        &self,
        _session: &S,
        conversation: &Conversation,
        _emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let messages = conversation.messages();
        let earlier_turns = messages.len() - messages_since_kickoff(conversation)?.len();
        let mut batch = self.full_results(&messages[..earlier_turns]);
        if batch.len() <= self.cutoff + BATCH_SIZE {
            return not_applicable();
        }
        batch.truncate(BATCH_SIZE);

        let calls: Vec<_> = batch
            .iter()
            .map(|result| self.summarize(messages, result))
            .collect();
        let outcomes: Vec<_> = futures::stream::iter(calls)
            .buffered(BATCH_SIZE)
            .collect()
            .await;

        let mut effects = Vec::new();
        for (result, outcome) in batch.iter().zip(outcomes) {
            let (note, usage) = match outcome {
                Outcome::Summarized(summary, usage) => {
                    (serde_json::Value::String(summary), Some(usage))
                }
                Outcome::Kept(usage) => (serde_json::Value::Null, usage),
                Outcome::Failed => {
                    self.failed
                        .lock()
                        .unwrap()
                        .insert(result.response.id.clone());
                    continue;
                }
            };
            effects.extend(usage.map(E::record_usage));
            effects.push(E::from(ConversationEffect::SetMessageOperationNote {
                message_id: result.message_id.to_string(),
                operation: TOOL_RESULT_SUMMARY_OPERATION.to_string(),
                key: result.response.id.clone(),
                value: note,
            }));
        }
        if effects.is_empty() {
            return not_applicable();
        }
        applied(effects)
    }
}
