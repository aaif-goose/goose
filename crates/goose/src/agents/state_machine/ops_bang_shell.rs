//! Runs a kickoff message beginning with `!` as a direct shell tool call.

use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::CallToolRequestParams;

use crate::agents::state_machine::effects::GooseEffect;
use crate::agents::state_machine::{
    applied, last_effective_role, messages_since_kickoff, not_applicable, yielded, Emitter,
    Operation, OperationResult,
};
use crate::conversation::message::Message;
use crate::conversation::{Conversation, EffectiveRole};
use crate::session::Session;

const SHELL_TOOL_NAME: &str = "shell";

pub(crate) fn bang_shell_command(message: &str) -> Option<&str> {
    message
        .trim_start()
        .strip_prefix('!')
        .map(str::trim_start)
        .filter(|command| !command.is_empty())
}

pub struct BangShellOperation;

impl BangShellOperation {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Operation<Session, GooseEffect> for BangShellOperation {
    fn name(&self) -> &'static str {
        "bang_shell"
    }

    async fn run(
        &self,
        _session: &Session,
        conversation: &Conversation,
        emit: &Emitter,
    ) -> Result<OperationResult<GooseEffect>> {
        let messages = messages_since_kickoff(conversation)?;
        let Some(kickoff) = messages.first() else {
            return not_applicable();
        };
        let kickoff_text = kickoff.user_visible_content().as_concat_text();
        let Some(command) = bang_shell_command(&kickoff_text) else {
            return not_applicable();
        };

        if messages.len() > 1 {
            return if last_effective_role(messages)? == EffectiveRole::Tool {
                yielded()
            } else {
                not_applicable()
            };
        }

        let call = CallToolRequestParams::new(SHELL_TOOL_NAME.to_string()).with_arguments(
            serde_json::Map::from_iter([(
                "command".to_string(),
                serde_json::Value::String(command.to_string()),
            )]),
        );
        let request = Message::assistant()
            .with_tool_request(format!("bang_shell_{}", uuid::Uuid::now_v7()), Ok(call));
        let request = emit.message(request).await;

        applied([request.into()])
    }
}
