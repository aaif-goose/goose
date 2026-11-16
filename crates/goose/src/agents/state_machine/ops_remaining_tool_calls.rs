//! Answers tool requests that no other operation handled.

use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::{CallToolResult, ContentBlock};

use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::ops_toolcalling::{
    pending_tool_requests, tool_span, ToolDisposition,
};
use crate::agents::state_machine::{
    applied, messages_since_kickoff, not_applicable, Emitter, Operation, OperationResult,
};
use crate::conversation::message::Message;
use crate::conversation::Conversation;
use crate::session::Session;

pub(crate) const UNCLAIMED_TOOL_ERROR: &str = "goose.unclaimed_tool";

pub struct RemainingToolCallsOperation;

#[async_trait]
impl Operation<Session, GooseEffect> for RemainingToolCallsOperation {
    fn name(&self) -> &'static str {
        "remaining_tool_calls"
    }

    async fn run(
        &self,
        session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        let pending = pending_tool_requests(messages);
        if pending.is_empty() {
            return not_applicable();
        }

        let mut response = Message::user();
        for (request, disposition) in pending {
            let tool_name = request
                .tool_call
                .as_ref()
                .map(|tool_call| tool_call.name.as_ref())
                .unwrap_or("unknown");
            let span = tool_span(tool_name, &request.id, &session.id);
            let (result, unclaimed) = match disposition {
                ToolDisposition::ParseError(error) => (
                    Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                        "The tool call could not be parsed: {error}. Correct the arguments and try again."
                    ))])),
                    false,
                ),
                ToolDisposition::Execute => {
                    span.record("error.type", "tool_not_available");
                    (
                        Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                            "Tool '{tool_name}' is not available."
                        ))])),
                        true,
                    )
                }
            };
            let mut metadata = request.metadata.clone();
            if unclaimed {
                metadata
                    .get_or_insert_default()
                    .insert(UNCLAIMED_TOOL_ERROR.to_string(), true.into());
            }
            response.add_tool_response_with_metadata(request.id, result, metadata.as_ref());
        }

        let response = emit.message(response);
        applied([response.into()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose_agent::tool_call_state::EXECUTABLE_KEY;
    use rmcp::model::CallToolRequestParams;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    async fn run(
        session: &Session,
        messages: Vec<Message>,
    ) -> Result<OperationResult<GooseEffect>> {
        let (tx, _rx) = mpsc::unbounded_channel();
        let emit = Emitter::new(tx, CancellationToken::new());
        RemainingToolCallsOperation
            .run(session, &Conversation::new_unvalidated(messages), &emit)
            .await
    }

    #[tokio::test]
    async fn a_call_awaiting_approval_is_left_to_the_unhandled_tool_calls_step() -> Result<()> {
        let messages = vec![
            Message::user().with_text("run it"),
            Message::assistant().with_tool_request_with_metadata(
                "call",
                Ok(CallToolRequestParams::new("shell")),
                None,
                Some(serde_json::json!({ EXECUTABLE_KEY: false })),
            ),
            Message::assistant()
                .with_action_required("call", "shell".to_string(), Default::default(), None)
                .user_only(),
        ];

        assert!(matches!(
            run(&Session::default(), messages).await?,
            OperationResult::NotApplicable
        ));
        Ok(())
    }
}
