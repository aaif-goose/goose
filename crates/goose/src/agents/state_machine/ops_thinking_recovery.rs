use anyhow::Result;
use async_trait::async_trait;
use oi_kit_agent::empty_response::{
    is_answer_content, response_recovery_attempts, RESPONSE_RECOVERY_OPERATION,
    RESPONSE_RECOVERY_RETRIED,
};
use rmcp::model::Role;
use tracing::warn;

use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::{
    applied, ends_turn, messages_since_kickoff, not_applicable, Emitter, Operation, OperationResult,
};
use crate::conversation::message::{Message, MessageContent};
use crate::conversation::Conversation;
use crate::session::Session;

pub(crate) const THINKING_ONLY_CONTINUATION_MESSAGE: &str =
    "You did some thinking but did not produce an answer. Please continue.";
pub(crate) const THINKING_ONLY_TURN_MESSAGE: &str =
    "The model finished thinking but did not produce an answer. Please resend your message to continue.";
const MAX_THINKING_RECOVERY_ATTEMPTS: usize = 3;

pub(crate) fn is_thinking_content(content: &MessageContent) -> bool {
    match content {
        MessageContent::Thinking(thinking) => {
            !thinking.thinking.is_empty() || !thinking.signature.is_empty()
        }
        MessageContent::RedactedThinking(thinking) => !thinking.data.is_empty(),
        _ => false,
    }
}

fn is_thinking_only_turn(messages: &[Message]) -> bool {
    let mut produced_thinking = false;
    for content in messages
        .iter()
        .rev()
        .take_while(|message| message.role == Role::Assistant)
        .flat_map(|message| message.content.iter())
    {
        if is_answer_content(content) {
            return false;
        }
        produced_thinking |= is_thinking_content(content);
    }
    produced_thinking
}

/// Asking the model to continue cannot help once the provider says the output
/// budget ran out; clients warn instead and leave resending or compacting to
/// the user.
fn reached_output_token_limit(messages: &[Message]) -> bool {
    messages
        .iter()
        .rev()
        .take_while(|message| message.role == Role::Assistant)
        .any(|message| message.metadata.output_token_limit_reached)
}

pub struct ThinkingRecoveryOperation;

#[async_trait]
impl Operation<Session, GooseEffect> for ThinkingRecoveryOperation {
    fn name(&self) -> &'static str {
        "thinking_recovery"
    }

    async fn run(
        &self,
        _session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        if !ends_turn(messages)
            || !is_thinking_only_turn(messages)
            || reached_output_token_limit(messages)
        {
            return not_applicable();
        }

        let attempts = response_recovery_attempts(conversation);

        if attempts < MAX_THINKING_RECOVERY_ATTEMPTS {
            warn!(
                "Provider returned a thinking-only response; prompting it to continue ({}/{})",
                attempts + 1,
                MAX_THINKING_RECOVERY_ATTEMPTS
            );
            let mut continuation = Message::user()
                .with_text(THINKING_ONLY_CONTINUATION_MESSAGE)
                .with_visibility(false, true);
            continuation.metadata.set_operation_note(
                RESPONSE_RECOVERY_OPERATION,
                RESPONSE_RECOVERY_RETRIED,
                serde_json::json!(true),
            );
            applied([continuation.into()])
        } else {
            warn!("Provider returned a thinking-only response after retries; ending turn");
            let message = Message::assistant()
                .with_text(THINKING_ONLY_TURN_MESSAGE)
                .with_visibility(true, false);
            let message = emit.message(message);
            applied([message.into()])
        }
    }
}
