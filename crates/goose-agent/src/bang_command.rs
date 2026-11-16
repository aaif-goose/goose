//! Runs a kickoff message beginning with `!` as a direct tool call.

use anyhow::Result;
use async_trait::async_trait;
use goose_provider_types::conversation::message::Message;
use goose_provider_types::conversation::{Conversation, EffectiveRole};
use goose_provider_types::maybe_send::{MaybeSend, MaybeSync};
use rmcp::model::CallToolRequestParams;

use crate::operation::{
    applied, last_effective_role, messages_since_kickoff, not_applicable, yielded, Emitter,
    Operation, OperationResult,
};

fn bang_command(message: &str) -> Option<&str> {
    message
        .trim_start()
        .strip_prefix('!')
        .map(str::trim_start)
        .filter(|command| !command.is_empty())
}

/// Calls `tool_name` with the text after `!` as `argument_name`, and ends the
/// turn after its response. The host registers that tool with its normal tool
/// steps.
pub struct BangCommandOperation {
    tool_name: String,
    argument_name: String,
}

impl BangCommandOperation {
    pub fn new(tool_name: impl Into<String>, argument_name: impl Into<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            argument_name: argument_name.into(),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<S, E> Operation<S, E> for BangCommandOperation
where
    S: MaybeSend + MaybeSync + 'static,
    E: From<Message> + MaybeSend + 'static,
{
    fn name(&self) -> &'static str {
        "bang_command"
    }

    async fn run(
        &self,
        _session: &S,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<E>> {
        let messages = messages_since_kickoff(conversation)?;
        let Some(kickoff) = messages.first() else {
            return not_applicable();
        };
        let kickoff_text = kickoff.user_visible_content().as_concat_text();
        let Some(command) = bang_command(&kickoff_text) else {
            return not_applicable();
        };

        if messages.len() > 1 {
            return if last_effective_role(messages)? == EffectiveRole::Tool {
                yielded()
            } else {
                not_applicable()
            };
        }

        let call = CallToolRequestParams::new(self.tool_name.clone()).with_arguments(
            serde_json::Map::from_iter([(
                self.argument_name.clone(),
                serde_json::Value::String(command.to_string()),
            )]),
        );
        let request = Message::assistant()
            .with_tool_request(format!("bang_command_{}", uuid::Uuid::new_v4()), Ok(call));

        applied([E::from(emit.message(request))])
    }
}
